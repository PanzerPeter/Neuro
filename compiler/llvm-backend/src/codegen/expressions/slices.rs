// Codegen for borrowed slices `&[T]` / `&mut [T]`: the unsizing coercion from a
// sized container, `.slice(range)`, `.len()`, indexing, element assignment, and
// `for x in xs` iteration.
//
// A slice is a `{ ptr, i64 }` fat pointer held by value — the buffer address of the
// borrowed run and its element count. Every operation here re-derives the element
// stride from the slice's semantic element type, because LLVM 20 pointers are untyped.

use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use inkwell::IntPredicate;
use neuro_hir::{HirExpr, HirExprKind, HirStmt};

use crate::codegen::context::{CodegenContext, LoopTargets};
use crate::errors::{CodegenError, CodegenResult};
use crate::types::{CollectionKind, Type};

/// Fat-pointer field indices, matching `TypeMapper::slice_ref_type`.
const FIELD_PTR: u32 = 0;
const FIELD_LEN: u32 = 1;

impl<'ctx> CodegenContext<'ctx> {
    /// Lower the unsizing coercion `&[T; N]` / `&Vec<T>` → `&[T]`: read the container's
    /// buffer address and element count and pair them into the fat pointer.
    ///
    /// A `&[T]` source is already that pair and passes through unchanged, which is what
    /// lets a slice parameter be forwarded to another slice parameter.
    pub(crate) fn codegen_slice_coerce(
        &mut self,
        value: &HirExpr,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let source_ty = Type::from_hir(&value.ty);
        let (base, _, len) = self.slice_source(value, &source_ty)?;
        self.slice_fat_pointer(base, len)
    }

    /// Lower `seq.slice(a..b)` / `seq.slice(a..=b)` to a `&[T]` view into the receiver's
    /// buffer — zero copy, whether the receiver is an array, a `Vec`, or another slice.
    ///
    /// The bounds are validated in every build, not only in debug ones: unlike an index,
    /// an out-of-range slice hands back a *view* that would outlive the check and read
    /// past the buffer on every later access, so there is no point at which a release
    /// build could still notice. This matches `string.slice`.
    pub(crate) fn codegen_sequence_slice(
        &mut self,
        receiver: &HirExpr,
        args: &[HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let (start_expr, end_expr, inclusive, offset) = match args.first() {
            Some(HirExpr {
                kind:
                    HirExprKind::Range {
                        start,
                        end,
                        inclusive,
                    },
                span,
                ..
            }) => (start.as_ref(), end.as_ref(), *inclusive, span.start),
            _ => {
                return Err(CodegenError::InternalError(
                    "sequence .slice reached codegen without a range argument".into(),
                ))
            }
        };

        let recv_ty = Type::from_hir(&receiver.ty);
        let (base, element_ty, len) = self.slice_source(receiver, &recv_ty)?;
        let i64_ty = self.context.i64_type();

        let start = self.slice_index_to_i64(start_expr)?;
        let raw_end = self.slice_index_to_i64(end_expr)?;
        let end = if inclusive {
            self.builder
                .build_int_add(raw_end, i64_ty.const_int(1, false), "sl.incl.end")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
        } else {
            raw_end
        };

        // `0 <= start <= end <= len` in one conjunction. The comparisons are signed so a
        // negative bound fails the first test rather than wrapping to a huge unsigned
        // value that would then pass the upper-bound test.
        let zero = i64_ty.const_zero();
        let start_nonneg = self
            .builder
            .build_int_compare(IntPredicate::SGE, start, zero, "sl.start.nonneg")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let start_le_end = self
            .builder
            .build_int_compare(IntPredicate::SLE, start, end, "sl.start.le.end")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let end_le_len = self
            .builder
            .build_int_compare(IntPredicate::SLE, end, len, "sl.end.le.len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let lower_ok = self
            .builder
            .build_and(start_nonneg, start_le_end, "sl.lower.ok")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let in_bounds = self
            .builder
            .build_and(lower_ok, end_le_len, "sl.in.bounds")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.codegen_guard_or_panic(in_bounds, "slice range out of bounds", offset)?;

        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        // SAFETY: the guard above panics unless `0 <= start <= end <= len`, so the
        // element at `start` is inside the borrowed run (or one past its end, which is
        // the empty slice and a legal GEP result).
        let new_ptr = unsafe {
            self.builder
                .build_in_bounds_gep(elem_llvm, base, &[start], "sl.ptr")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
        };
        let new_len = self
            .builder
            .build_int_sub(end, start, "sl.newlen")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.slice_fat_pointer(new_ptr, new_len)
    }

    /// Lower `slice.len()`: field 1 of the fat pointer. O(1), no walk.
    pub(crate) fn codegen_slice_len(
        &mut self,
        receiver: &HirExpr,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let fat = self.codegen_expr(receiver)?.into_struct_value();
        self.builder
            .build_extract_value(fat, FIELD_LEN, "sl.len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// Lower a slice index read `xs[i]`: bounds-check (debug), then load the element.
    pub(crate) fn codegen_slice_index(
        &mut self,
        object: &HirExpr,
        index: &HirExpr,
        obj_ty: &Type,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let (base, element_ty, len) = self.slice_source(object, obj_ty)?;
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        let slot = self.slice_element_ptr(base, &element_ty, len, index, offset)?;
        self.builder
            .build_load(elem_llvm, slot, "sl.idx")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// Lower a slice element assignment `xs[i] = value` through a `&mut [T]`. The write
    /// reaches the borrowed buffer, so the owning array or `Vec` sees it.
    pub(crate) fn codegen_slice_index_assignment(
        &mut self,
        target: &str,
        target_ty: &Type,
        index: &HirExpr,
        value: &HirExpr,
    ) -> CodegenResult<()> {
        let fat = self
            .variables
            .get(target)
            .copied()
            .ok_or_else(|| CodegenError::UndefinedVariable(target.to_string()))?;
        let element_ty = match target_ty.referent() {
            Type::Slice(element) => (**element).clone(),
            other => {
                return Err(CodegenError::InternalError(format!(
                    "slice index assignment target is not a slice: {:?}",
                    other
                )))
            }
        };
        let (base, len) = self.load_slice_parts(fat)?;
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        let slot = self.slice_element_ptr(base, &element_ty, len, index, index.span.start)?;
        let val = self.codegen_expr(value)?;
        let val = self.coerce_if_needed(val, elem_llvm, &element_ty)?;
        self.builder
            .build_store(slot, val)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Lower `for x in xs` over a slice: a counted loop to the slice's runtime length,
    /// binding `iterator` to a copy of each element. Mirrors the array and `Vec` forms;
    /// only the source of the bound differs.
    pub(crate) fn codegen_slice_for_each(
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
        let (base, element_ty, len) = self.slice_source(iterable, obj_ty)?;
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        let i64_ty = self.context.i64_type();

        let idx_alloca = self.entry_alloca(i64_ty, "sleach.i")?;
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

        let cond_bb = self.context.append_basic_block(parent_fn, "sleach.cond");
        let body_bb = self.context.append_basic_block(parent_fn, "sleach.body");
        let step_bb = self.context.append_basic_block(parent_fn, "sleach.step");
        let exit_bb = self.context.append_basic_block(parent_fn, "sleach.exit");

        if !self.current_block_terminated() {
            self.builder
                .build_unconditional_branch(cond_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        }

        self.builder.position_at_end(cond_bb);
        let i_val = self
            .builder
            .build_load(i64_ty, idx_alloca, "sleach.iv")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let cond = self
            .builder
            .build_int_compare(IntPredicate::ULT, i_val, len, "sleach.cmp")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(cond, body_bb, exit_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(body_bb);
        // SAFETY: this block is reached only when the loop condition proved
        // `i_val < len`, so the addressed element is inside the borrowed run.
        let slot = unsafe {
            self.builder
                .build_in_bounds_gep(elem_llvm, base, &[i_val], "sleach.slot")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
        };
        let elem_val = self
            .builder
            .build_load(elem_llvm, slot, "sleach.elem")
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
            .build_load(i64_ty, idx_alloca, "sleach.iv")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let next = self
            .builder
            .build_int_add(cur, i64_ty.const_int(1, false), "sleach.next")
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

    /// The `(buffer address, element type, element count)` a slice operation reads its
    /// receiver as. The three contiguous receivers differ only in where the count comes
    /// from: an array's is its static `N`, a `Vec`'s is a header field, and a slice's
    /// already travels beside the pointer.
    fn slice_source(
        &mut self,
        object: &HirExpr,
        obj_ty: &Type,
    ) -> CodegenResult<(PointerValue<'ctx>, Type, IntValue<'ctx>)> {
        match obj_ty.referent().clone() {
            Type::Array { element, size } => {
                let (base, element_ty, _) = self.array_place_ptr(object, obj_ty)?;
                let len = self.context.i64_type().const_int(size as u64, false);
                let _ = element;
                Ok((base, element_ty, len))
            }
            Type::Collection {
                kind: CollectionKind::Vec,
                args,
            } => {
                let element_ty = args.first().cloned().ok_or_else(|| {
                    CodegenError::InternalError("Vec slice source has no element type".into())
                })?;
                let header = self.collection_place_ptr(object, obj_ty)?;
                let len = self.load_header_field(
                    header,
                    crate::codegen::collections::FIELD_LEN,
                    "sl.src.len",
                )?;
                let base = self.load_header_buffer(header)?;
                Ok((base, element_ty, len))
            }
            Type::Slice(element) => {
                let fat = self.codegen_expr(object)?.into_struct_value();
                let base = self
                    .builder
                    .build_extract_value(fat, FIELD_PTR, "sl.src.ptr")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                    .into_pointer_value();
                let len = self
                    .builder
                    .build_extract_value(fat, FIELD_LEN, "sl.src.len")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                    .into_int_value();
                Ok((base, (*element).clone(), len))
            }
            other => Err(CodegenError::InternalError(format!(
                "slice operation on a non-contiguous receiver: {:?}",
                other
            ))),
        }
    }

    /// Split a slice value held in a stack slot back into its pointer and length.
    /// Used by element assignment, whose target is a named `&mut [T]` binding.
    fn load_slice_parts(
        &mut self,
        slot: PointerValue<'ctx>,
    ) -> CodegenResult<(PointerValue<'ctx>, IntValue<'ctx>)> {
        let fat_ty = self.slice_ref_type();
        let fat = self
            .builder
            .build_load(fat_ty, slot, "sl.load")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_struct_value();
        let base = self
            .builder
            .build_extract_value(fat, FIELD_PTR, "sl.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let len = self
            .builder
            .build_extract_value(fat, FIELD_LEN, "sl.len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        Ok((base, len))
    }

    /// Address of element `index` in a borrowed run, emitting the same debug-build
    /// bounds guard the owning container gets — only the bound is a runtime length.
    fn slice_element_ptr(
        &mut self,
        base: PointerValue<'ctx>,
        element_ty: &Type,
        len: IntValue<'ctx>,
        index: &HirExpr,
        offset: usize,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let idx_sem = Type::from_hir(&index.ty);
        let idx_val = self.codegen_expr(index)?.into_int_value();
        let idx64 = self.widen_index_to_i64(idx_val, &idx_sem)?;

        if self.overflow_checks {
            let ok = self
                .builder
                .build_int_compare(IntPredicate::ULT, idx64, len, "sl.bounds")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.codegen_guard_or_panic(ok, "slice index out of bounds", offset)?;
        }

        let elem_llvm = self.get_any_llvm_type(element_ty)?;
        // SAFETY: in debug builds the guard above panics unless `idx64 < len`; in
        // release builds an out-of-range index is the documented behaviour of the
        // bounds policy, matching arrays and the integer-overflow policy.
        unsafe {
            self.builder
                .build_in_bounds_gep(elem_llvm, base, &[idx64], "sl.slot")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))
        }
    }

    /// Assemble a `{ ptr, len }` slice value.
    fn slice_fat_pointer(
        &mut self,
        base: PointerValue<'ctx>,
        len: IntValue<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let fat_ty = self.slice_ref_type();
        let with_ptr = self
            .builder
            .build_insert_value(fat_ty.get_undef(), base, FIELD_PTR, "sl.res.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_struct_value();
        let full = self
            .builder
            .build_insert_value(with_ptr, len, FIELD_LEN, "sl.res")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_struct_value();
        Ok(full.into())
    }

    /// The `{ ptr, i64 }` layout a borrowed slice is held in.
    fn slice_ref_type(&self) -> inkwell::types::StructType<'ctx> {
        self.type_mapper.slice_ref_type()
    }
}
