// Neuro Programming Language - LLVM Backend
// Codegen for expressions: top-level dispatch and shared helpers.
// Category implementations live in the sibling submodules below; each adds
// methods to the same `impl CodegenContext` block (Rust allows split impls).

mod arrays;
mod binary;
mod control_flow;
mod literals;
mod methods;
mod tuples;
mod unary;

use inkwell::types::BasicTypeEnum;
use inkwell::values::*;
use neuro_hir::{HirExpr, HirExprKind};

use crate::codegen::context::{resolve_builtin_method, BuiltinMethod, CodegenContext};
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

impl<'ctx> CodegenContext<'ctx> {
    /// Generate code for an expression. The HIR carries the resolved type on every
    /// node (`expr.ty`), so the backend reads it directly instead of consulting a
    /// span-keyed side table.
    pub(crate) fn codegen_expr(&mut self, expr: &HirExpr) -> CodegenResult<BasicValueEnum<'ctx>> {
        match &expr.kind {
            HirExprKind::Cast { value } => {
                let target_ty = Type::from_hir(&expr.ty);
                self.codegen_cast(value, &target_ty)
            }
            HirExprKind::Literal(lit) => self.codegen_literal(lit),
            HirExprKind::Variable(name) => self.codegen_identifier(name),
            HirExprKind::Binary { op, left, right } => {
                // `codegen_binary` dispatches on the left-operand type (instruction
                // width / signedness), which is the operand's own type rather than the
                // expression's result type (e.g. `Bool` for a comparison).
                let left_ty = Type::from_hir(&left.ty);
                self.codegen_binary(left, *op, right, &left_ty)
            }
            HirExprKind::Unary { op, operand } => {
                let operand_ty = Type::from_hir(&operand.ty);
                self.codegen_unary(*op, operand, &operand_ty)
            }
            HirExprKind::Call { callee, args } => {
                // In value position a unit-returning call is an error — there is no
                // value to bind. Statement position discards the result instead
                // (see `codegen_call_dispatch` callers in `codegen_stmt`).
                self.codegen_call_dispatch(callee, args, &expr.span)?
                    .ok_or_else(|| {
                        CodegenError::InternalError(
                            "function call returned void when value expected".to_string(),
                        )
                    })
            }

            HirExprKind::Path { .. } => {
                // A path expression used outside of a call position has no value
                // representation at runtime; semantic analysis should have caught this.
                Err(CodegenError::InternalError(
                    "path expression used outside of call position".to_string(),
                ))
            }

            HirExprKind::StructLiteral { name, fields, base } => {
                self.codegen_struct_literal(name, fields, base.as_deref())
            }

            HirExprKind::FieldAccess { object, field } => {
                // A `&Struct` receiver auto-derefs to read a field of the referent (§2.4).
                let struct_name = match object.ty.referent() {
                    neuro_hir::HirType::Struct(n) => n.clone(),
                    other => {
                        return Err(CodegenError::InternalError(format!(
                            "field access on non-struct type: {}",
                            other
                        )))
                    }
                };
                self.codegen_field_access(object, field, &struct_name)
            }

            HirExprKind::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
            } => {
                let result_ty = Type::from_hir(&expr.ty);
                self.codegen_if_expr(
                    condition,
                    then_block,
                    else_if_blocks,
                    else_block,
                    &result_ty,
                )
            }

            HirExprKind::Block { stmts } => self.codegen_block_expr(stmts),

            // A `loop` in value position (§3.7) lowers like the statement form, but
            // its `break v` result is loaded and returned. A unit loop (no value
            // `break`) yields the placeholder used elsewhere for void positions.
            HirExprKind::Loop { label, body } => {
                let result_ty = Type::from_hir(&expr.ty);
                let value = self.codegen_loop(label.as_deref(), body, &result_ty)?;
                Ok(value.unwrap_or_else(|| self.context.i32_type().const_int(0, false).into()))
            }

            // `unsafe` is inert: lower its body identically to a bare block.
            HirExprKind::Unsafe { stmts } => self.codegen_block_expr(stmts),

            // Borrow `&place` (§2.4) / `&mut place` (§2.5): the value of the borrow is
            // the storage pointer of the place. Every local/parameter is an alloca, so
            // its address is exactly the pointer already held in `variables`. Mutability
            // is a compile-time-only distinction — both lower to the same pointer.
            HirExprKind::Reference { operand, .. } => self.codegen_reference(operand),

            // Dereference `*operand` (§2.5): load the referent through the reference.
            // The result type `T` is exactly this expression's type.
            HirExprKind::Deref { operand } => {
                let referent_ty = Type::from_hir(&expr.ty);
                self.codegen_deref(operand, &referent_ty)
            }

            // A range `a..b` is not a value (§2.7): it is consumed directly by
            // `string.slice`'s lowering. Semantic analysis rejects any other position,
            // so reaching the general expression path here is an internal inconsistency.
            HirExprKind::Range { .. } => Err(CodegenError::InternalError(
                "range expression reached codegen outside a slice argument".into(),
            )),

            // Array literal `[e0, ...]` and indexing `object[index]` (§3.1).
            HirExprKind::ArrayLiteral { elements } => {
                let array_ty = Type::from_hir(&expr.ty);
                self.codegen_array_literal(elements, &array_ty)
            }
            HirExprKind::Index { object, index } => {
                let obj_ty = Type::from_hir(&object.ty);
                self.codegen_index(object, index, &obj_ty, &expr.span)
            }

            // Tuple literal `(e0, ...)` and element access `object.N` (§3.2).
            HirExprKind::TupleLiteral { elements } => {
                let tuple_ty = Type::from_hir(&expr.ty);
                self.codegen_tuple_literal(elements, &tuple_ty)
            }
            HirExprKind::TupleIndex { object, index } => self.codegen_tuple_index(object, *index),
        }
    }

    /// Resolve and lower a call expression (free function, method, associated
    /// function, builtin intrinsic, or panic builtin), returning `None` when the
    /// callee returns unit `()`. Shared by value position (`codegen_expr`) and
    /// statement position (`codegen_stmt`), which discards a `None` result.
    pub(crate) fn codegen_call_dispatch(
        &mut self,
        callee: &HirExpr,
        args: &[HirExpr],
        span: &shared_types::Span,
    ) -> CodegenResult<Option<BasicValueEnum<'ctx>>> {
        match &callee.kind {
            HirExprKind::Variable(name) => {
                // Panic-family builtins (§1.2) lower to a diagnostic + `abort`. A user
                // function of the same name shadows the builtin, matching the semantic
                // resolver, so only intercept when none is registered.
                if CodegenContext::is_panic_builtin(name) && !self.functions.contains_key(name) {
                    return Ok(Some(self.codegen_panic_builtin(name, args, *span)?));
                }
                self.codegen_call(name, args)
            }

            // Method call: `instance.method(args)` — pass self as first arg.
            HirExprKind::FieldAccess { object, field } => {
                let recv_ty = Type::from_hir(&object.ty);
                if let Type::Struct(struct_name) = recv_ty.referent() {
                    let mangled = format!("{}__{}", struct_name, field);
                    // `struct.clone()` (§2.3) is a builtin deep copy when no user method
                    // named `clone` exists; semantic analysis has verified the struct
                    // derives Clone. Every other field call is a user struct method.
                    if field == "clone" && !self.functions.contains_key(&mangled) {
                        return Ok(Some(self.codegen_builtin_method(
                            BuiltinMethod::StructClone,
                            &recv_ty,
                            object,
                            args,
                        )?));
                    }
                    return self.codegen_method_call(&mangled, object, args);
                }

                // Non-struct receiver: a compiler-known intrinsic on a builtin type.
                match resolve_builtin_method(&recv_ty, field) {
                    Some((kind, _)) => Ok(Some(
                        self.codegen_builtin_method(kind, &recv_ty, object, args)?,
                    )),
                    None => Err(CodegenError::InternalError(format!(
                        "unresolved builtin method '{}' on {:?} reached codegen",
                        field, recv_ty
                    ))),
                }
            }

            // Associated function call: `TypeName::func(args)`.
            HirExprKind::Path { type_name, member } => {
                let mangled = format!("{}__{}", type_name, member);
                self.codegen_call(&mangled, args)
            }

            _ => Err(CodegenError::UnsupportedType(
                "unsupported call expression".to_string(),
            )),
        }
    }

    /// Lower a dereference `*operand` to a load of the referent (§2.5). The operand
    /// evaluates to a pointer (a reference); `referent_ty` (this deref's result type)
    /// selects the load type.
    fn codegen_deref(
        &mut self,
        operand: &HirExpr,
        referent_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let ptr_val = self.codegen_expr(operand)?;
        let ptr = ptr_val.into_pointer_value();
        let llvm_ty = self.get_any_llvm_type(referent_ty)?;
        self.builder.build_load(llvm_ty, ptr, "deref").map_err(|e| {
            CodegenError::LlvmError(format!("failed to load through reference: {}", e))
        })
    }

    /// Lower an immutable borrow `&place` to the storage pointer of the place (§2.4).
    /// Semantic analysis guarantees the operand is a live binding (identifier).
    fn codegen_reference(&self, operand: &HirExpr) -> CodegenResult<BasicValueEnum<'ctx>> {
        match &operand.kind {
            HirExprKind::Variable(name) => {
                let ptr = self
                    .variables
                    .get(name)
                    .ok_or_else(|| CodegenError::UndefinedVariable(name.clone()))?;
                Ok((*ptr).into())
            }
            other => Err(CodegenError::InternalError(format!(
                "borrow of a non-place expression reached codegen: {:?}",
                other
            ))),
        }
    }

    /// Map a semantic type to an LLVM type, including struct types.
    pub(crate) fn get_any_llvm_type(&self, ty: &Type) -> CodegenResult<BasicTypeEnum<'ctx>> {
        match ty {
            Type::Struct(name) => Ok(self.get_struct_llvm_type(name)?.into()),
            other => self.type_mapper.map_type(other),
        }
    }

    pub(crate) fn current_block_terminated(&self) -> bool {
        self.builder
            .get_insert_block()
            .and_then(|b| b.get_terminator())
            .is_some()
    }
}
