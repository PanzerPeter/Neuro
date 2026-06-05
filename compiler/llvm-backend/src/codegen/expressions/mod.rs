// Neuro Programming Language - LLVM Backend
// Codegen for expressions: top-level dispatch and shared helpers.
// Category implementations live in the sibling submodules below; each adds
// methods to the same `impl CodegenContext` block (Rust allows split impls).

mod binary;
mod control_flow;
mod literals;
mod methods;
mod unary;

use ast_types::*;
use inkwell::types::BasicTypeEnum;
use inkwell::values::*;

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

impl<'ctx> CodegenContext<'ctx> {
    /// Generate code for an expression
    pub(crate) fn codegen_expr(&mut self, expr: &Expr) -> CodegenResult<BasicValueEnum<'ctx>> {
        match expr {
            Expr::Cast {
                expr: inner_expr,
                target_type,
                span,
            } => {
                let llvm_ty = crate::types::Type::from_ast(target_type);
                self.codegen_cast(inner_expr, &llvm_ty, span)
            }
            Expr::Literal(lit, _) => self.codegen_literal(lit),
            Expr::Identifier(ident) => self.codegen_identifier(&ident.name),
            Expr::Binary {
                left,
                op,
                right,
                span,
            } => {
                // The left-operand type is stored in `binary_left_types`, keyed by this node's
                // full span, by visit_expr_for_types. `expr_types[span.start]` holds the result
                // type (e.g. Bool for comparisons), which is not what codegen_binary needs when
                // dispatching on the operand kind.
                let left_ty = self
                    .binary_left_types
                    .get(&(span.start, span.end))
                    .cloned()
                    .ok_or_else(|| {
                        CodegenError::InternalError(
                            "missing type information for expression".to_string(),
                        )
                    })?;
                self.codegen_binary(left, *op, right, &left_ty)
            }
            Expr::Unary { op, operand, span } => {
                let operand_ty = self.expr_types.get(&span.start).cloned().ok_or_else(|| {
                    CodegenError::InternalError(
                        "missing type information for expression".to_string(),
                    )
                })?;
                self.codegen_unary(*op, operand, &operand_ty)
            }
            Expr::Call { func, args, span } => {
                match &**func {
                    Expr::Identifier(ident) => {
                        // Panic-family builtins (§1.2) lower to a diagnostic + `abort`.
                        // A user function of the same name shadows the builtin, matching
                        // the semantic resolver, so only intercept when none is registered.
                        if CodegenContext::is_panic_builtin(&ident.name)
                            && !self.functions.contains_key(&ident.name)
                        {
                            return self.codegen_panic_builtin(&ident.name, args, *span);
                        }
                        self.codegen_call(&ident.name, args)
                    }

                    // Method call: `instance.method(args)` — pass self as first arg
                    Expr::FieldAccess { field, .. } => {
                        // `object` is the FieldAccess's own object sub-expression.
                        let object = if let Expr::FieldAccess { object, .. } = &**func {
                            object
                        } else {
                            unreachable!()
                        };
                        // Builtin intrinsics on primitive/string receivers are tagged by
                        // the type pass and lowered directly, bypassing struct mangling.
                        if let Some((kind, recv_ty)) =
                            self.builtin_methods.get(&(span.start, span.end)).cloned()
                        {
                            return self.codegen_builtin_method(kind, &recv_ty, object, args);
                        }
                        let struct_name = self
                            .fa_struct_names
                            .get(&span.start)
                            .ok_or_else(|| {
                                CodegenError::InternalError(
                                    "missing struct name for method call".to_string(),
                                )
                            })?
                            .clone();
                        let mangled = format!("{}__{}", struct_name, field.name);
                        self.codegen_method_call(&mangled, object, args)
                    }

                    // Associated function call: `TypeName::func(args)`
                    Expr::Path {
                        type_name, member, ..
                    } => {
                        let mangled = format!("{}__{}", type_name.name, member.name);
                        self.codegen_call(&mangled, args)
                    }

                    _ => Err(CodegenError::UnsupportedType(
                        "unsupported call expression".to_string(),
                    )),
                }
            }

            Expr::Path { .. } => {
                // A path expression used outside of a call position has no value
                // representation at runtime; semantic analysis should have caught this.
                Err(CodegenError::InternalError(
                    "path expression used outside of call position".to_string(),
                ))
            }
            Expr::Paren(inner, _) => self.codegen_expr(inner),

            Expr::StructLiteral {
                name, fields, base, ..
            } => self.codegen_struct_literal(&name.name, fields, base.as_deref()),

            Expr::FieldAccess {
                object,
                field,
                span,
            } => {
                // The struct name for this field access was stored in fa_struct_names during
                // type collection (keyed by the FieldAccess span.start). We cannot use
                // expr_types here because the FieldAccess and its first sub-expression
                // (the object Identifier) share the same span.start, and the later insert
                // of the field type overwrites the earlier insert of the struct type.
                let struct_name = self
                    .fa_struct_names
                    .get(&span.start)
                    .ok_or_else(|| {
                        CodegenError::InternalError(
                            "missing struct name for field access".to_string(),
                        )
                    })?
                    .clone();
                self.codegen_field_access(object, &field.name, &struct_name)
            }

            Expr::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                span,
            } => self.codegen_if_expr(condition, then_block, else_if_blocks, else_block, span),

            Expr::Block { stmts, .. } => self.codegen_block_expr(stmts),

            // `unsafe` is inert: lower its body identically to a bare block.
            Expr::Unsafe { stmts, .. } => self.codegen_block_expr(stmts),
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
