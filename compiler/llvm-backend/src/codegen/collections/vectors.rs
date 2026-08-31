// `Vec<T>`: growable contiguous storage.
//
// The buffer is a plain `[T]` run of `cap` elements holding `len` live ones. Growth
// doubles the capacity through one shared byte-sized `realloc` helper — the element type
// only enters through its stride, so every `Vec<T>` in a module reuses the same helper.

use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use inkwell::IntPredicate;
use neuro_hir::{HirExpr, HirStmt};

use super::{collection_arg, initial_capacity, FIELD_CAP, FIELD_LEN};
use crate::codegen::context::{CodegenContext, LoopTargets};
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// The shared capacity-growth helper: `__neuro_vec_reserve(header, elem_size)`.
const RESERVE_HELPER: &str = "__neuro_vec_reserve";

impl<'ctx> CodegenContext<'ctx> {
    /// `v.push(x)` — grow if the buffer is full, then store at index `len` and bump it.
    pub(crate) fn codegen_vec_push(
        &mut self,
        header: PointerValue<'ctx>,
        element_ty: &Type,
        args: &[HirExpr],
    ) -> CodegenResult<()> {
        let value_expr = args.first().ok_or_else(|| {
            CodegenError::InternalError("Vec::push reached codegen without a value".into())
        })?;
        let value = self.codegen_expr(value_expr)?;
        let elem_llvm = self.collection_value_type(element_ty)?;
        let value = self.coerce_if_needed(value, elem_llvm, element_ty)?;

        self.emit_vec_reserve(header, element_ty)?;

        let len = self.load_header_field(header, FIELD_LEN, "vec.len")?;
        let slot = self.vec_slot_ptr(header, element_ty, len)?;
        self.builder
            .build_store(slot, value)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let next = self
            .builder
            .build_int_add(len, self.context.i64_type().const_int(1, false), "vec.len1")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.store_header_field(header, FIELD_LEN, next)?;
        // Pushing a `string` transfers it into the buffer, so the source binding must
        // not also be dropped by its own scope.
        self.mark_moved_for_drop(value_expr);
        Ok(())
    }

    /// `v.pop()` — `Some(last)` after shrinking by one, or `None` when empty.
    pub(crate) fn codegen_vec_pop(
        &mut self,
        header: PointerValue<'ctx>,
        element_ty: &Type,
        result_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let i64_ty = self.context.i64_type();
        let len = self.load_header_field(header, FIELD_LEN, "vec.len")?;
        let present = self
            .builder
            .build_int_compare(IntPredicate::UGT, len, i64_ty.const_zero(), "vec.any")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        // Read the last element unconditionally under a clamped index: with `len == 0`
        // the index saturates to 0, which is in bounds whenever a buffer exists, and the
        // value is discarded by the `None` select. This keeps `pop` branch-free.
        let last = self
            .builder
            .build_int_sub(len, i64_ty.const_int(1, false), "vec.last")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let index = self
            .builder
            .build_select(present, last, i64_ty.const_zero(), "vec.pop.idx")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let value = self.load_vec_element_or_zero(header, element_ty, index, present)?;

        let shrunk = self
            .builder
            .build_select(present, last, i64_ty.const_zero(), "vec.len.new")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        self.store_header_field(header, FIELD_LEN, shrunk)?;

        self.build_option_value(result_ty, present, value, element_ty)
    }

    /// `v.get(i)` — `Some(element)` when `i < len`, else `None`. The checked
    /// counterpart to `v[i]`, which panics instead.
    pub(crate) fn codegen_vec_get(
        &mut self,
        header: PointerValue<'ctx>,
        element_ty: &Type,
        result_ty: &Type,
        args: &[HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let index_expr = args.first().ok_or_else(|| {
            CodegenError::InternalError("Vec::get reached codegen without an index".into())
        })?;
        let index_sem = Type::from_hir(&index_expr.ty);
        let raw = self.codegen_expr(index_expr)?.into_int_value();
        let index = self.widen_collection_index(raw, &index_sem)?;

        let len = self.load_header_field(header, FIELD_LEN, "vec.len")?;
        let present = self
            .builder
            .build_int_compare(IntPredicate::ULT, index, len, "vec.in.bounds")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let clamped = self
            .builder
            .build_select(
                present,
                index,
                self.context.i64_type().const_zero(),
                "vec.get.idx",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let value = self.load_vec_element_or_zero(header, element_ty, clamped, present)?;
        self.build_option_value(result_ty, present, value, element_ty)
    }

    /// `v[i]` read — bounds-checked in every build, panicking on violation.
    ///
    /// Unlike `[T; N]`, whose length is a compile-time constant the optimizer can fold,
    /// a `Vec`'s length is only known at run time, so the check is never elided.
    pub(crate) fn codegen_vec_index(
        &mut self,
        object: &HirExpr,
        obj_ty: &Type,
        index: &HirExpr,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let element_ty = collection_arg(collection_args(obj_ty)?, 0)?;
        let header = self.collection_place_ptr(object, obj_ty)?;
        let slot = self.checked_vec_slot(header, &element_ty, index, offset)?;
        let elem_llvm = self.collection_value_type(&element_ty)?;
        self.builder
            .build_load(elem_llvm, slot, "vec.idx")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// `v[i] = x` — bounds-checked element store into an owned `Vec` binding.
    pub(crate) fn codegen_vec_index_assignment(
        &mut self,
        target: &str,
        target_ty: &Type,
        index: &HirExpr,
        value: &HirExpr,
    ) -> CodegenResult<()> {
        let element_ty = collection_arg(collection_args(target_ty)?, 0)?;
        let header = self
            .variables
            .get(target)
            .copied()
            .ok_or_else(|| CodegenError::UndefinedVariable(target.to_string()))?;
        let slot = self.checked_vec_slot(header, &element_ty, index, index.span.start)?;
        let elem_llvm = self.collection_value_type(&element_ty)?;
        let val = self.codegen_expr(value)?;
        let val = self.coerce_if_needed(val, elem_llvm, &element_ty)?;
        self.builder
            .build_store(slot, val)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// `for x in v` / `for x in &v` — a counted loop over the live elements, binding
    /// `iterator` to a copy of each in turn. The bound is re-read every iteration, so a
    /// body that pushes or pops observes the current length.
    pub(crate) fn codegen_vec_for_each(
        &mut self,
        label: Option<&str>,
        index: Option<&str>,
        iterator: &str,
        iterable: &HirExpr,
        obj_ty: &Type,
        body: &[HirStmt],
    ) -> CodegenResult<()> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;
        let element_ty = collection_arg(collection_args(obj_ty)?, 0)?;
        let header = self.collection_place_ptr(iterable, obj_ty)?;
        let elem_llvm = self.collection_value_type(&element_ty)?;
        let i64_ty = self.context.i64_type();

        let idx_alloca = self.entry_alloca(i64_ty, "veach.i")?;
        self.builder
            .build_store(idx_alloca, i64_ty.const_zero())
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let elem_alloca = self.entry_alloca(elem_llvm, iterator)?;

        self.type_env
            .insert(iterator.to_string(), element_ty.clone());
        let iter_name = iterator.to_string();
        let previous_var = self.variables.insert(iter_name.clone(), elem_alloca);
        let previous_ty = self.variable_types.insert(iter_name.clone(), elem_llvm);
        let index_binding = self.bind_loop_index(index)?;

        let cond_bb = self.context.append_basic_block(parent_fn, "veach.cond");
        let body_bb = self.context.append_basic_block(parent_fn, "veach.body");
        let step_bb = self.context.append_basic_block(parent_fn, "veach.step");
        let exit_bb = self.context.append_basic_block(parent_fn, "veach.exit");

        if !self.current_block_terminated() {
            self.builder
                .build_unconditional_branch(cond_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        }

        self.builder.position_at_end(cond_bb);
        let i_val = self
            .builder
            .build_load(i64_ty, idx_alloca, "veach.iv")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let len = self.load_header_field(header, FIELD_LEN, "veach.len")?;
        let cond = self
            .builder
            .build_int_compare(IntPredicate::ULT, i_val, len, "veach.cmp")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(cond, body_bb, exit_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(body_bb);
        let slot = self.vec_slot_ptr(header, &element_ty, i_val)?;
        let elem_val = self
            .builder
            .build_load(elem_llvm, slot, "veach.elem")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(elem_alloca, elem_val)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.store_loop_index(&index_binding, i_val)?;

        let body_scope_index = self.drop_scopes.len();
        self.push_drop_scope();
        self.loop_targets.push(LoopTargets {
            label: label.map(str::to_string),
            continue_bb: step_bb,
            break_bb: exit_bb,
            break_slot: None,
            drop_scope_depth: body_scope_index,
        });
        for stmt in body {
            if self.current_block_terminated() {
                break;
            }
            self.codegen_stmt(stmt)?;
        }
        let _ = self.loop_targets.pop();
        if !self.current_block_terminated() {
            self.emit_top_scope_drops()?;
        }
        self.pop_drop_scope();

        if let Some(tail_bb) = self.builder.get_insert_block() {
            if tail_bb.get_terminator().is_none() {
                self.builder
                    .build_unconditional_branch(step_bb)
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            }
        }

        self.builder.position_at_end(step_bb);
        let cur = self
            .builder
            .build_load(i64_ty, idx_alloca, "veach.iv")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let next = self
            .builder
            .build_int_add(cur, i64_ty.const_int(1, false), "veach.next")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(idx_alloca, next)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(exit_bb);
        self.unbind_loop_index(index_binding);

        match previous_var {
            Some(p) => {
                self.variables.insert(iter_name.clone(), p);
            }
            None => {
                self.variables.remove(&iter_name);
            }
        }
        match previous_ty {
            Some(p) => {
                self.variable_types.insert(iter_name, p);
            }
            None => {
                self.variable_types.remove(&iter_name);
            }
        }
        Ok(())
    }

    /// Build an owned `Vec<T>` header over `count` elements already written into
    /// `buffer`. Used by the maps' `keys()`, which allocates and fills the buffer itself.
    pub(super) fn vec_header_value(
        &mut self,
        buffer: PointerValue<'ctx>,
        count: IntValue<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let header_ty = self.collection_header_type();
        let mut agg = header_ty.get_undef();
        let fields: [(u32, BasicValueEnum<'ctx>); 4] = [
            (super::FIELD_BUFFER, buffer.into()),
            (FIELD_LEN, count.into()),
            (FIELD_CAP, count.into()),
            (
                super::FIELD_USED,
                self.context.i64_type().const_zero().into(),
            ),
        ];
        for (field, value) in fields {
            agg = self
                .builder
                .build_insert_value(agg, value, field, "vec.hdr")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_struct_value();
        }
        Ok(agg.into())
    }

    /// Address of element `index` in the buffer, with no bounds check.
    pub(super) fn vec_slot_ptr(
        &mut self,
        header: PointerValue<'ctx>,
        element_ty: &Type,
        index: IntValue<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let buffer = self.load_header_buffer(header)?;
        let elem_llvm = self.collection_value_type(element_ty)?;
        // SAFETY: unchecked by contract — every caller either bounds-checks `index`
        // first (`checked_vec_slot`) or derives it from the vector's own `len`/`cap`,
        // so it addresses a slot inside the allocated buffer.
        unsafe {
            self.builder
                .build_in_bounds_gep(elem_llvm, buffer, &[index], "vec.slot")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))
        }
    }

    /// Address of element `index`, panicking first when it is outside `0..len`.
    fn checked_vec_slot(
        &mut self,
        header: PointerValue<'ctx>,
        element_ty: &Type,
        index: &HirExpr,
        offset: usize,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let index_sem = Type::from_hir(&index.ty);
        let raw = self.codegen_expr(index)?.into_int_value();
        let widened = self.widen_collection_index(raw, &index_sem)?;
        let len = self.load_header_field(header, FIELD_LEN, "vec.len")?;
        let ok = self
            .builder
            .build_int_compare(IntPredicate::ULT, widened, len, "vec.bounds")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.codegen_guard_or_panic(ok, "Vec index out of bounds", offset)?;
        self.vec_slot_ptr(header, element_ty, widened)
    }

    /// Load element `index`, or a zero value when `present` is false — the buffer may be
    /// null on an empty collection, so the load itself is predicated.
    fn load_vec_element_or_zero(
        &mut self,
        header: PointerValue<'ctx>,
        element_ty: &Type,
        index: IntValue<'ctx>,
        present: IntValue<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;
        let elem_llvm = self.collection_value_type(element_ty)?;
        let slot_alloca = self.entry_alloca(elem_llvm, "vec.read")?;
        self.builder
            .build_store(slot_alloca, zero_of(elem_llvm))
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let read_bb = self.context.append_basic_block(parent_fn, "vec.read.do");
        let done_bb = self.context.append_basic_block(parent_fn, "vec.read.done");
        self.builder
            .build_conditional_branch(present, read_bb, done_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(read_bb);
        let slot = self.vec_slot_ptr(header, element_ty, index)?;
        let value = self
            .builder
            .build_load(elem_llvm, slot, "vec.elem")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(slot_alloca, value)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unconditional_branch(done_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(done_bb);
        self.builder
            .build_load(elem_llvm, slot_alloca, "vec.read.val")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// Ensure the buffer has room for one more element, doubling it if not.
    fn emit_vec_reserve(
        &mut self,
        header: PointerValue<'ctx>,
        element_ty: &Type,
    ) -> CodegenResult<()> {
        let elem_llvm = self.collection_value_type(element_ty)?;
        let stride = self.size_of_type(elem_llvm)?;
        let reserve = self.build_reserve_helper()?;
        self.builder
            .build_call(reserve, &[header.into(), stride.into()], "")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Emit (once per module) `__neuro_vec_reserve(header, elem_size)`: if `len == cap`,
    /// reallocate to `max(INITIAL, cap * 2)` elements and record the new capacity.
    ///
    /// Byte-sized rather than element-typed, so every `Vec<T>` in the module shares it.
    pub(super) fn build_reserve_helper(
        &mut self,
    ) -> CodegenResult<inkwell::values::FunctionValue<'ctx>> {
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        let i64_ty = self.context.i64_type();
        let fn_type = self
            .context
            .void_type()
            .fn_type(&[ptr_ty.into(), i64_ty.into()], false);

        self.get_or_build_helper(RESERVE_HELPER, fn_type, |ctx, func| {
            let entry = ctx.context.append_basic_block(func, "entry");
            let grow_bb = ctx.context.append_basic_block(func, "grow");
            let done_bb = ctx.context.append_basic_block(func, "done");
            ctx.builder.position_at_end(entry);

            let header = func
                .get_nth_param(0)
                .ok_or_else(|| CodegenError::InternalError("reserve helper arity".into()))?
                .into_pointer_value();
            let stride = func
                .get_nth_param(1)
                .ok_or_else(|| CodegenError::InternalError("reserve helper arity".into()))?
                .into_int_value();

            let len = ctx.load_header_field(header, FIELD_LEN, "len")?;
            let cap = ctx.load_header_field(header, FIELD_CAP, "cap")?;
            let full = ctx
                .builder
                .build_int_compare(IntPredicate::UGE, len, cap, "full")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.builder
                .build_conditional_branch(full, grow_bb, done_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            ctx.builder.position_at_end(grow_bb);
            let doubled = ctx
                .builder
                .build_int_mul(cap, i64_ty.const_int(2, false), "double")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            let min_cap = i64_ty.const_int(initial_capacity(), false);
            let use_min = ctx
                .builder
                .build_int_compare(IntPredicate::ULT, doubled, min_cap, "too.small")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            let new_cap = ctx
                .builder
                .build_select(use_min, min_cap, doubled, "new.cap")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_int_value();
            let bytes = ctx
                .builder
                .build_int_mul(new_cap, stride, "new.bytes")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            let old = ctx.load_header_buffer(header)?;
            let realloc = ctx.get_or_declare_realloc();
            let grown = ctx
                .builder
                .build_call(realloc, &[old.into(), bytes.into()], "grown")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| CodegenError::InternalError("realloc returned void".into()))?
                .into_pointer_value();
            ctx.store_header_buffer(header, grown)?;
            ctx.store_header_field(header, FIELD_CAP, new_cap)?;
            ctx.builder
                .build_unconditional_branch(done_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            ctx.builder.position_at_end(done_bb);
            ctx.builder
                .build_return(None)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            Ok(())
        })
    }

    /// Widen an index to `i64` for the bounds compare and GEP, zero-extending an
    /// unsigned index and sign-extending a signed one (a negative index then reads as a
    /// huge unsigned value and fails the bounds test).
    pub(super) fn widen_collection_index(
        &mut self,
        index: IntValue<'ctx>,
        index_ty: &Type,
    ) -> CodegenResult<IntValue<'ctx>> {
        let i64_ty = self.context.i64_type();
        if index.get_type().get_bit_width() >= 64 {
            return Ok(index);
        }
        if index_ty.is_unsigned_int() {
            self.builder
                .build_int_z_extend(index, i64_ty, "idx.zext")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))
        } else {
            self.builder
                .build_int_s_extend(index, i64_ty, "idx.sext")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))
        }
    }
}

/// The type arguments of a collection type, seen through a reference.
pub(super) fn collection_args(ty: &Type) -> CodegenResult<&[Type]> {
    match ty.referent() {
        Type::Collection { args, .. } => Ok(args),
        other => Err(CodegenError::InternalError(format!(
            "expected a collection type, found {:?}",
            other
        ))),
    }
}

/// The all-zero value of an LLVM type, used as the discarded payload of a `None`.
fn zero_of(ty: inkwell::types::BasicTypeEnum<'_>) -> BasicValueEnum<'_> {
    match ty {
        inkwell::types::BasicTypeEnum::IntType(t) => t.const_zero().into(),
        inkwell::types::BasicTypeEnum::FloatType(t) => t.const_zero().into(),
        inkwell::types::BasicTypeEnum::PointerType(t) => t.const_null().into(),
        inkwell::types::BasicTypeEnum::StructType(t) => t.const_zero().into(),
        inkwell::types::BasicTypeEnum::ArrayType(t) => t.const_zero().into(),
        inkwell::types::BasicTypeEnum::VectorType(t) => t.const_zero().into(),
        inkwell::types::BasicTypeEnum::ScalableVectorType(t) => t.const_zero().into(),
    }
}
