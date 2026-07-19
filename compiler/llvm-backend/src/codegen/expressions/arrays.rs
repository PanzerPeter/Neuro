// Codegen for fixed-size arrays `[T; N]`: literals, indexing, element
// assignment, and `for x in arr` iteration. Arrays lower to LLVM `[N x T]`
// aggregates stored in an alloca; indexing is a `getelementptr` + load/store with
// a debug-build bounds guard routed through the panic runtime.

use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use inkwell::IntPredicate;
use neuro_hir::{HirExpr, HirExprKind, HirStmt};

use crate::codegen::context::{CodegenContext, LoopTargets};
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

impl<'ctx> CodegenContext<'ctx> {
    /// Lower an array literal `[e0, e1, ...]` to an `[N x T]` aggregate value.
    /// Each element is built at the array's element type recorded by the type pass;
    /// a surrounding `val a: [T; N] = ...` retargets the element width via
    /// `coerce_if_needed`, mirroring the scalar initializer path.
    pub(crate) fn codegen_array_literal(
        &mut self,
        elements: &[HirExpr],
        array_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let (element_ty, size) = match array_ty {
            Type::Array { element, size } => ((**element).clone(), *size),
            _ => {
                return Err(CodegenError::InternalError(
                    "array literal type is not an array".to_string(),
                ))
            }
        };
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        let arr_llvm = elem_llvm.array_type(size as u32);

        let mut agg = arr_llvm.get_undef();
        for (i, el) in elements.iter().enumerate() {
            let val = self.codegen_expr(el)?;
            let val = self.coerce_if_needed(val, elem_llvm, &element_ty)?;
            agg = self
                .builder
                .build_insert_value(agg, val, i as u32, "arr.elem")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_array_value();
        }
        Ok(agg.into())
    }

    /// Lower an array rest remainder `..rest`: build a fresh `[T; N - start]`
    /// aggregate holding elements `start..N` of the source array. `rest_ty` is the
    /// already-sized remainder type carried by the HIR node. Emitted from the
    /// `val [a, b, ..rest] = arr` desugar; arity was validated upstream.
    pub(crate) fn codegen_array_rest(
        &mut self,
        array: &HirExpr,
        start: usize,
        rest_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let rest_len = match rest_ty {
            Type::Array { size, .. } => *size,
            _ => {
                return Err(CodegenError::InternalError(
                    "array rest type is not an array".to_string(),
                ))
            }
        };
        let obj_ty = Type::from_hir(&array.ty);
        let (base_ptr, element_ty, size) = self.array_place_ptr(array, &obj_ty)?;
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        let src_arr_llvm = elem_llvm.array_type(size as u32);
        let rest_arr_llvm = elem_llvm.array_type(rest_len as u32);
        let i64t = self.context.i64_type();

        let mut agg = rest_arr_llvm.get_undef();
        for offset in 0..rest_len {
            let src_index = (start + offset) as u64;
            let elem_ptr = unsafe {
                self.builder
                    .build_in_bounds_gep(
                        src_arr_llvm,
                        base_ptr,
                        &[i64t.const_zero(), i64t.const_int(src_index, false)],
                        "arr.rest.src",
                    )
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            };
            let elem_val = self
                .builder
                .build_load(elem_llvm, elem_ptr, "arr.rest.elem")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            agg = self
                .builder
                .build_insert_value(agg, elem_val, offset as u32, "arr.rest.ins")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_array_value();
        }
        Ok(agg.into())
    }

    /// Lower an array index read `object[index]`: bounds-check (debug), then
    /// `getelementptr` + load of the element.
    pub(crate) fn codegen_index(
        &mut self,
        object: &HirExpr,
        index: &HirExpr,
        obj_ty: &Type,
        span: &shared_types::Span,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let (base_ptr, element_ty, size) = self.array_place_ptr(object, obj_ty)?;
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        let elem_ptr = self.array_element_ptr(base_ptr, elem_llvm, size, index, span.start)?;
        self.builder
            .build_load(elem_llvm, elem_ptr, "arr.idx")
            .map_err(|e| CodegenError::LlvmError(format!("failed to load array element: {}", e)))
    }

    /// Lower an array element assignment `target[index] = value`: bounds-check
    /// (debug), then `getelementptr` + store.
    pub(crate) fn codegen_index_assignment(
        &mut self,
        target: &str,
        index: &HirExpr,
        value: &HirExpr,
    ) -> CodegenResult<()> {
        // The target is an owned array binding; recover its array type from the
        // codegen-time local environment and its storage from `variables`.
        let obj_ty = self.type_env.get(target).cloned().ok_or_else(|| {
            CodegenError::InternalError(
                "missing array type for index assignment target".to_string(),
            )
        })?;
        let (element_ty, size) = match obj_ty.referent() {
            Type::Array { element, size } => ((**element).clone(), *size),
            other => {
                return Err(CodegenError::InternalError(format!(
                    "index assignment target is not an array: {:?}",
                    other
                )))
            }
        };
        let base_ptr = self
            .variables
            .get(target)
            .copied()
            .ok_or_else(|| CodegenError::UndefinedVariable(target.to_string()))?;
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        let elem_ptr =
            self.array_element_ptr(base_ptr, elem_llvm, size, index, index.span.start)?;
        let val = self.codegen_expr(value)?;
        let val = self.coerce_if_needed(val, elem_llvm, &element_ty)?;
        self.builder.build_store(elem_ptr, val).map_err(|e| {
            CodegenError::LlvmError(format!("failed to store array element: {}", e))
        })?;
        Ok(())
    }

    /// Lower `for x in arr` / `for x in &arr`: a counted loop over the array
    /// storage, binding `iterator` to a copy of each element in turn. Mirrors
    /// `codegen_for_range` minus the user-visible bound expressions.
    pub(crate) fn codegen_for_each(
        &mut self,
        label: Option<&str>,
        iterator: &str,
        iterable: &HirExpr,
        body: &[HirStmt],
    ) -> CodegenResult<()> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;

        let obj_ty = Type::from_hir(&iterable.ty);
        let (base_ptr, element_ty, size) = self.array_place_ptr(iterable, &obj_ty)?;
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        let arr_llvm = elem_llvm.array_type(size as u32);
        let i64t = self.context.i64_type();

        // Induction variable `i` and the element binding `x`, both stack slots.
        let idx_alloca = self
            .builder
            .build_alloca(i64t, "foreach.i")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(idx_alloca, i64t.const_zero())
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let elem_alloca = self
            .builder
            .build_alloca(elem_llvm, iterator)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        // Record the element binding's resolved type so a place statement in the body
        // (e.g. indexing a struct/array element) can recover its nominal type.
        self.type_env
            .insert(iterator.to_string(), element_ty.clone());
        let iter_name = iterator.to_string();
        let previous_var = self.variables.insert(iter_name.clone(), elem_alloca);
        let previous_var_type = self.variable_types.insert(iter_name.clone(), elem_llvm);

        let cond_bb = self.context.append_basic_block(parent_fn, "foreach.cond");
        let body_bb = self.context.append_basic_block(parent_fn, "foreach.body");
        let step_bb = self.context.append_basic_block(parent_fn, "foreach.step");
        let exit_bb = self.context.append_basic_block(parent_fn, "foreach.exit");

        if !self.current_block_terminated() {
            self.builder
                .build_unconditional_branch(cond_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        }

        self.builder.position_at_end(cond_bb);
        let i_val = self
            .builder
            .build_load(i64t, idx_alloca, "foreach.iv")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let size_c = i64t.const_int(size as u64, false);
        let cond = self
            .builder
            .build_int_compare(IntPredicate::ULT, i_val, size_c, "foreach.cmp")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(cond, body_bb, exit_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(body_bb);
        // Load the current element into the binding slot, then run the body.
        let elem_ptr = unsafe {
            self.builder
                .build_in_bounds_gep(
                    arr_llvm,
                    base_ptr,
                    &[i64t.const_zero(), i_val],
                    "foreach.elem.ptr",
                )
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
        };
        let elem_val = self
            .builder
            .build_load(elem_llvm, elem_ptr, "foreach.elem")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(elem_alloca, elem_val)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

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
            .build_load(i64t, idx_alloca, "foreach.iv")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let next = self
            .builder
            .build_int_add(cur, i64t.const_int(1, false), "foreach.next")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(idx_alloca, next)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(exit_bb);

        // Restore any shadowed outer binding of the iterator name.
        match previous_var {
            Some(p) => {
                self.variables.insert(iter_name.clone(), p);
            }
            None => {
                self.variables.remove(&iter_name);
            }
        }
        match previous_var_type {
            Some(p) => {
                self.variable_types.insert(iter_name, p);
            }
            None => {
                self.variable_types.remove(&iter_name);
            }
        }

        Ok(())
    }

    /// Resolve the storage pointer to an array place plus its element type and length.
    ///
    /// An owned-array identifier uses its alloca directly; a borrow of an array
    /// (`&[T; N]`) evaluates the operand to the array's address; any other
    /// array-valued expression is materialised into a fresh temporary.
    fn array_place_ptr(
        &mut self,
        object: &HirExpr,
        obj_ty: &Type,
    ) -> CodegenResult<(PointerValue<'ctx>, Type, usize)> {
        let (element_ty, size) = match obj_ty.referent() {
            Type::Array { element, size } => ((**element).clone(), *size),
            other => {
                return Err(CodegenError::InternalError(format!(
                    "array place expression is not an array: {:?}",
                    other
                )))
            }
        };

        // A borrow yields the array's address directly; an owned identifier uses its
        // alloca; any other owned array value is stored into a temporary first.
        if matches!(obj_ty, Type::Reference(_)) {
            let ptr = self.codegen_expr(object)?.into_pointer_value();
            return Ok((ptr, element_ty, size));
        }

        if let HirExprKind::Variable(name) = &object.kind {
            if let Some(ptr) = self.variables.get(name).copied() {
                return Ok((ptr, element_ty, size));
            }
        }

        let val = self.codegen_expr(object)?;
        let tmp = self
            .builder
            .build_alloca(val.get_type(), "arr.tmp")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(tmp, val)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok((tmp, element_ty, size))
    }

    /// Compute the address of element `index` within an array at `base_ptr`, emitting
    /// a debug-build bounds guard (`index < size`, unsigned) that panics on violation
    /// `offset` keys the panic diagnostic's source location.
    fn array_element_ptr(
        &mut self,
        base_ptr: PointerValue<'ctx>,
        elem_llvm: BasicTypeEnum<'ctx>,
        size: usize,
        index: &HirExpr,
        offset: usize,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let i64t = self.context.i64_type();
        let idx_sem = Type::from_hir(&index.ty);
        let idx_val = self.codegen_expr(index)?.into_int_value();
        let idx64 = self.widen_index_to_i64(idx_val, &idx_sem)?;

        // Debug builds trap an out-of-bounds access; release builds omit the check
        // (matching the integer-overflow policy). A negative signed index
        // sign-extends to a large unsigned value and so fails the `< size` test.
        if self.overflow_checks {
            let size_c = i64t.const_int(size as u64, false);
            let ok = self
                .builder
                .build_int_compare(IntPredicate::ULT, idx64, size_c, "arr.bounds")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.codegen_guard_or_panic(ok, "array index out of bounds", offset)?;
        }

        let arr_llvm = elem_llvm.array_type(size as u32);
        unsafe {
            self.builder
                .build_in_bounds_gep(
                    arr_llvm,
                    base_ptr,
                    &[i64t.const_zero(), idx64],
                    "arr.elem.ptr",
                )
                .map_err(|e| CodegenError::LlvmError(e.to_string()))
        }
    }

    /// Widen an integer index value to `i64` for GEP and the bounds compare, using
    /// zero-extension for unsigned index types and sign-extension for signed ones.
    fn widen_index_to_i64(
        &self,
        idx: IntValue<'ctx>,
        idx_sem: &Type,
    ) -> CodegenResult<IntValue<'ctx>> {
        let i64t = self.context.i64_type();
        if idx.get_type().get_bit_width() >= 64 {
            return Ok(idx);
        }
        if idx_sem.is_unsigned_int() {
            self.builder
                .build_int_z_extend(idx, i64t, "idx.zext")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))
        } else {
            self.builder
                .build_int_s_extend(idx, i64t, "idx.sext")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))
        }
    }
}
