//! Function values: a closure's environment is a heap block shared by every copy of the
//! value, counted, and released with the last copy.
//!
//! The block starts with an `i64` count and the address of the closure's own release
//! (which frees what the environment owns, then the block). A function value with no
//! captures carries a null environment, which the retain and the release both skip.
//! A function value is copied freely, so every position that holds one holds a count:
//! a copy out of a place retains, a fresh value hands its count over, and each owner's
//! release gives one back.

use inkwell::AddressSpace;
use inkwell::basic_block::BasicBlock;
use inkwell::module::Linkage;
use inkwell::values::{BasicValueEnum, FunctionValue, IntValue, PointerValue};
use neuro_hir::{HirExpr, HirExprKind};
use shared_types::Literal;

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// Fields ahead of the captures in every environment: the count, then the release.
pub(crate) const ENV_HEADER_FIELDS: u32 = 2;

const RETAIN_FN: &str = "__neuro_fn_retain";
const RELEASE_FN: &str = "__neuro_fn_release";

impl<'ctx> CodegenContext<'ctx> {
    /// Whether a value of `ty` is, or holds, a function value.
    pub(crate) fn holds_function(&self, ty: &Type) -> bool {
        match ty {
            Type::Function { .. } => true,
            Type::Struct(name) => self
                .struct_defs
                .get(name)
                .is_some_and(|fields| fields.iter().any(|(_, f)| self.holds_function(f))),
            Type::Enum(name) => self
                .type_mapper
                .enum_payload_types(name)
                .is_some_and(|variants| variants.iter().flatten().any(|f| self.holds_function(f))),
            Type::Array { element, size } => *size > 0 && self.holds_function(element),
            Type::Tuple(elements) => elements.iter().any(|e| self.holds_function(e)),
            _ => false,
        }
    }

    /// Whether copying a value of `ty` shares what it owns rather than moving it: it
    /// holds function values and no other owner. The language copies such a value
    /// freely, so a copy takes counts of its own and the source keeps its own.
    pub(crate) fn shares_on_copy(&self, ty: &Type) -> bool {
        self.holds_function(ty) && !self.holds_owner_besides_functions(ty)
    }

    /// [`holds_owner`](CodegenContext::holds_owner), not counting function values.
    fn holds_owner_besides_functions(&self, ty: &Type) -> bool {
        match ty {
            Type::Function { .. } => false,
            Type::Collection { .. } | Type::Tensor { .. } => true,
            Type::Struct(name) => {
                self.drop_types.contains(name)
                    || self.struct_defs.get(name).is_some_and(|fields| {
                        fields
                            .iter()
                            .any(|(_, f)| self.holds_owner_besides_functions(f))
                    })
            }
            Type::Enum(name) => self
                .type_mapper
                .enum_payload_types(name)
                .is_some_and(|variants| {
                    variants
                        .iter()
                        .flatten()
                        .any(|f| self.holds_owner_besides_functions(f))
                }),
            Type::Array { element, size } => {
                *size > 0 && self.holds_owner_besides_functions(element)
            }
            Type::Tuple(elements) => elements
                .iter()
                .any(|e| self.holds_owner_besides_functions(e)),
            _ => false,
        }
    }

    /// Take a count of every function value the copy `expr` reads out of a place. A
    /// fresh value (a closure literal, a call's result, a literal built here) already
    /// carries its counts and hands them over, so it takes none.
    pub(crate) fn retain_copied(&mut self, expr: &HirExpr, ty: &Type) -> CodegenResult<()> {
        if !is_pure_place(expr) {
            return Ok(());
        }
        let value = self.codegen_expr(expr)?;
        let slot = self.entry_alloca(value.get_type(), "fn.copy")?;
        self.builder.build_store(slot, value)?;
        self.retain_held_functions(slot, ty)
    }

    /// Take a count of every function value the value of `ty` at `storage_ptr` holds.
    pub(crate) fn retain_held_functions(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
        ty: &Type,
    ) -> CodegenResult<()> {
        let retain = self.function_count_fn(RETAIN_FN)?;
        self.walk_functions(storage_ptr, ty, retain)
    }

    /// Give back the count the function value at `storage_ptr` holds.
    pub(crate) fn emit_function_release(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        let release = self.function_count_fn(RELEASE_FN)?;
        self.call_on_environment(storage_ptr, release)
    }

    /// Release `value`, the result of `expr`, when it is a fresh function value that a
    /// reader used and nothing took over: a closure literal called in place, or one a
    /// tensor traversal read.
    pub(crate) fn release_fresh_function(
        &mut self,
        expr: &HirExpr,
        value: BasicValueEnum<'ctx>,
    ) -> CodegenResult<()> {
        let fresh = matches!(
            expr.kind,
            HirExprKind::Closure { .. } | HirExprKind::Call { .. }
        );
        if !fresh || !matches!(Type::from_hir(&expr.ty), Type::Function { .. }) {
            return Ok(());
        }
        if self.current_block_terminated() {
            return Ok(());
        }
        let slot = self.entry_alloca(value.get_type(), "fn.temporary")?;
        self.builder.build_store(slot, value)?;
        self.emit_function_release(slot)
    }

    /// Call `count` on the environment of every function value inside the value of `ty`
    /// at `storage_ptr`. An enum is walked under a switch on its tag, since only the
    /// active variant's payload is live.
    fn walk_functions(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
        ty: &Type,
        count: FunctionValue<'ctx>,
    ) -> CodegenResult<()> {
        match ty {
            Type::Function { .. } => return self.call_on_environment(storage_ptr, count),
            Type::Enum(name) => return self.walk_enum_functions(storage_ptr, name, count),
            _ => {}
        }
        let positions = self.held_positions(ty);
        if positions.is_empty() {
            return Ok(());
        }
        let holder_llvm = self.get_any_llvm_type(ty)?;
        for (index, (segment, position_ty)) in positions.into_iter().enumerate() {
            if !self.holds_function(&position_ty) {
                continue;
            }
            let position_ptr = self.aggregate_position_ptr(
                holder_llvm,
                storage_ptr,
                index as u32,
                &format!("fn.held.{}.ptr", segment),
            )?;
            self.walk_functions(position_ptr, &position_ty, count)?;
        }
        Ok(())
    }

    fn walk_enum_functions(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
        enum_name: &str,
        count: FunctionValue<'ctx>,
    ) -> CodegenResult<()> {
        let variants = self
            .type_mapper
            .enum_payload_types(enum_name)
            .cloned()
            .unwrap_or_default();
        let parent_fn = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("enum payload walk emitted outside function".into())
        })?;
        let enum_llvm = self.type_mapper.enum_struct_type(enum_name)?;
        let payload_array_ty = enum_llvm
            .get_field_type_at_index(1)
            .ok_or_else(|| {
                CodegenError::InternalError(format!("enum '{}' has no payload field", enum_name))
            })?
            .into_array_type();
        let tag_ptr =
            self.builder
                .build_struct_gep(enum_llvm, storage_ptr, 0, "fn.enum.tag.ptr")?;
        let tag = self
            .builder
            .build_load(self.context.i32_type(), tag_ptr, "fn.enum.tag")?
            .into_int_value();
        let payload_ptr =
            self.builder
                .build_struct_gep(enum_llvm, storage_ptr, 1, "fn.enum.payload")?;

        let join_bb = self.context.append_basic_block(parent_fn, "fn.enum.cont");
        let holding: Vec<usize> = variants
            .iter()
            .enumerate()
            .filter(|(_, fields)| fields.iter().any(|field| self.holds_function(field)))
            .map(|(tag_value, _)| tag_value)
            .collect();
        let cases: Vec<(IntValue<'ctx>, BasicBlock<'ctx>)> = holding
            .iter()
            .map(|tag_value| {
                (
                    self.context.i32_type().const_int(*tag_value as u64, false),
                    self.context
                        .append_basic_block(parent_fn, &format!("fn.enum.v{}", tag_value)),
                )
            })
            .collect();
        self.builder.build_switch(tag, join_bb, &cases)?;
        for (tag_value, (_, case_bb)) in holding.iter().zip(cases.iter()) {
            self.builder.position_at_end(*case_bb);
            for (slot, field_ty) in variants[*tag_value].iter().enumerate() {
                if !self.holds_function(field_ty) {
                    continue;
                }
                let slot_ptr = self.aggregate_position_ptr(
                    payload_array_ty.into(),
                    payload_ptr,
                    slot as u32,
                    "fn.enum.slot",
                )?;
                self.walk_functions(slot_ptr, field_ty, count)?;
            }
            self.builder.build_unconditional_branch(join_bb)?;
        }
        self.builder.position_at_end(join_bb);
        Ok(())
    }

    /// Load the function value at `storage_ptr` and call `count` on its environment.
    fn call_on_environment(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
        count: FunctionValue<'ctx>,
    ) -> CodegenResult<()> {
        let ptr_ty = self.context.ptr_type(AddressSpace::default());
        let fat_ty = self
            .context
            .struct_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let env_ptr = self
            .builder
            .build_struct_gep(fat_ty, storage_ptr, 1, "fn.env.ptr")?;
        let env = self.builder.build_load(ptr_ty, env_ptr, "fn.env")?;
        self.builder.build_call(count, &[env.into()], "")?;
        Ok(())
    }

    /// The module's retain or release of an environment's count, built on first use.
    /// Both skip a null environment, which a function value with no captures carries.
    fn function_count_fn(&self, name: &str) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(name) {
            return Ok(existing);
        }
        let ptr_ty = self.context.ptr_type(AddressSpace::default());
        let i64_ty = self.context.i64_type();
        let function = self.module.add_function(
            name,
            self.context.void_type().fn_type(&[ptr_ty.into()], false),
            Some(Linkage::Internal),
        );
        let releasing = name == RELEASE_FN;
        self.detached(|| {
            let entry = self.context.append_basic_block(function, "entry");
            let counted = self.context.append_basic_block(function, "counted");
            let done = self.context.append_basic_block(function, "done");
            self.builder.position_at_end(entry);
            let env = function
                .get_first_param()
                .ok_or_else(|| CodegenError::InternalError("count lost its pointer".into()))?
                .into_pointer_value();
            let null = self.builder.build_is_null(env, "fn.env.null")?;
            self.builder.build_conditional_branch(null, done, counted)?;

            self.builder.position_at_end(counted);
            let count = self
                .builder
                .build_load(i64_ty, env, "fn.count")?
                .into_int_value();
            let one = i64_ty.const_int(1, false);
            let next = match releasing {
                true => self.builder.build_int_sub(count, one, "fn.count.dec")?,
                false => self.builder.build_int_add(count, one, "fn.count.inc")?,
            };
            self.builder.build_store(env, next)?;
            if releasing {
                let last = self.context.append_basic_block(function, "last");
                let zero = self.builder.build_int_compare(
                    inkwell::IntPredicate::EQ,
                    next,
                    i64_ty.const_zero(),
                    "fn.count.zero",
                )?;
                self.builder.build_conditional_branch(zero, last, done)?;
                self.builder.position_at_end(last);
                let header = self
                    .context
                    .struct_type(&[i64_ty.into(), ptr_ty.into()], false);
                let drop_ptr = self
                    .builder
                    .build_struct_gep(header, env, 1, "fn.env.drop.ptr")?;
                let drop = self
                    .builder
                    .build_load(ptr_ty, drop_ptr, "fn.env.drop")?
                    .into_pointer_value();
                let drop_ty = self.context.void_type().fn_type(&[ptr_ty.into()], false);
                self.builder
                    .build_indirect_call(drop_ty, drop, &[env.into()], "")?;
            }
            self.builder.build_unconditional_branch(done)?;
            self.builder.position_at_end(done);
            self.builder.build_return(None)?;
            Ok(())
        })?;
        Ok(function)
    }
}

/// Whether `expr` reads a place without side effects, so it can be read again and its
/// value is one some position already holds: a binding, or a field, element or
/// dereference of one, indexed by a constant or a binding.
pub(crate) fn is_pure_place(expr: &HirExpr) -> bool {
    match &expr.kind {
        HirExprKind::Variable(_) => true,
        HirExprKind::FieldAccess { object, .. }
        | HirExprKind::TupleIndex { object, .. }
        | HirExprKind::NewtypeAccess { object } => is_pure_place(object),
        HirExprKind::Deref { operand } => is_pure_place(operand),
        HirExprKind::Index { object, index } => {
            is_pure_place(object)
                && matches!(
                    index.kind,
                    HirExprKind::Literal(Literal::Integer(..)) | HirExprKind::Variable(_)
                )
        }
        _ => false,
    }
}
