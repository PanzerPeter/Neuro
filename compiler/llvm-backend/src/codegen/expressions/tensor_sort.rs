// Codegen for the order-based tensor selections `.sort()`, `.argsort()`, and `.topk()`.
//
// A tensor's elements sit in one flat, row-major run, so ordering along axis `k` splits
// that run into the same three constant factors a reduction walks: `outer` elements above
// the axis, `mid` along it, and `inner` below. Run `r` covers the source indices
// `base(r) + j * inner` for `j` in `0..mid`, with `base(r) = (r / inner) * mid * inner +
// (r % inner)`.
//
// All three methods compute the same thing per run — a permutation of `0..mid` that puts
// the run's elements in order — and differ only in what they write out of it: the
// elements in that order, the permutation itself, or the leading `k` of both. So one
// ordering loop serves them, and the writer at the end is the only branch on the method.
//
// ponytail: the permutation is built by a stable insertion sort, which is O(mid^2) in the
// sorted extent. The radix / Timsort guarantee is a kernel-level promise that arrives
// with 2C's MLIR lowering, where the sort becomes a dialect op rather than IR emitted
// here; until then the honest small-extent implementation is the one that is easy to read
// and impossible to get subtly wrong.
//
// The receiver is READ. Nothing is moved and nothing is released here: a selection orders
// a buffer its owner keeps.

use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use inkwell::{FloatPredicate, IntPredicate};
use neuro_hir::{HirExpr, HirSortKind};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// How one ordered run is laid out in the receiver's flat buffer.
struct SortLayout {
    /// Elements between two neighbouring positions along the sorted axis.
    inner: usize,
    /// The length of the sorted run.
    mid: usize,
    /// How many independent runs there are.
    runs: usize,
}

/// Where one selection writes its answer: the values tensor, the index tensor, or both.
struct SortTargets<'ctx> {
    values: Option<(PointerValue<'ctx>, PointerValue<'ctx>)>,
    indices: Option<(PointerValue<'ctx>, PointerValue<'ctx>)>,
    /// How many of each run's ordered positions are written, which is `mid` for a full
    /// sort and `k` for a top-k.
    width: usize,
}

impl<'ctx> CodegenContext<'ctx> {
    /// Lower one selection: a tensor for `.sort` and `.argsort`, and a two-tensor tuple
    /// for `.topk`.
    pub(crate) fn codegen_tensor_sort(
        &mut self,
        receiver: &HirExpr,
        kind: HirSortKind,
        axis: usize,
        descending: bool,
        result_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let source_ty = Type::from_hir(&receiver.ty);
        let Type::Tensor { element, shape } = source_ty.referent().clone() else {
            return Err(CodegenError::InternalError(
                "a tensor selection does not carry a tensor receiver".to_string(),
            ));
        };
        let shape = crate::types::static_extents(&shape)?;
        let layout = sort_layout(&shape, axis)?;
        let element = (*element).clone();
        let elem_llvm = self.get_any_llvm_type(&element)?;
        let source = self.tensor_index_data(receiver, &source_ty)?;
        let targets = self.allocate_targets(kind, &layout, result_ty)?;

        let i64_type = self.context.i64_type();
        let order =
            self.entry_alloca(i64_type.array_type(layout.mid as u32), "tensor.sort.order")?;

        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("a tensor selection outside a function".to_string())
        })?;
        let run = self.entry_alloca(i64_type, "tensor.sort.run")?;
        self.builder.build_store(run, i64_type.const_zero())?;
        let head = self
            .context
            .append_basic_block(function, "tensor.sort.head");
        let body = self
            .context
            .append_basic_block(function, "tensor.sort.body");
        let done = self
            .context
            .append_basic_block(function, "tensor.sort.done");

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let r = self
            .builder
            .build_load(i64_type, run, "tensor.sort.r")?
            .into_int_value();
        let more = self.builder.build_int_compare(
            IntPredicate::ULT,
            r,
            i64_type.const_int(layout.runs as u64, false),
            "tensor.sort.more",
        )?;
        self.builder.build_conditional_branch(more, body, done)?;

        self.builder.position_at_end(body);
        let base = self.sort_run_base(r, layout.inner, layout.mid)?;
        self.fill_identity(order, layout.mid)?;
        self.insertion_sort(
            order, elem_llvm, source, base, &layout, &element, descending,
        )?;
        self.write_run(order, elem_llvm, source, base, r, &layout, &targets)?;
        let next =
            self.builder
                .build_int_add(r, i64_type.const_int(1, false), "tensor.sort.next")?;
        self.builder.build_store(run, next)?;
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(done);
        self.finish(kind, &targets, result_ty)
    }

    /// Allocate the result tensors the selection writes into.
    fn allocate_targets(
        &mut self,
        kind: HirSortKind,
        layout: &SortLayout,
        result_ty: &Type,
    ) -> CodegenResult<SortTargets<'ctx>> {
        let open = |this: &mut Self, ty: &Type| -> CodegenResult<_> {
            let handle = this.alloc_dlpack_tensor(ty, "tensor.sort")?;
            Ok((handle, this.load_dlpack_data(handle)?))
        };
        match kind {
            HirSortKind::Values => Ok(SortTargets {
                values: Some(open(self, result_ty)?),
                indices: None,
                width: layout.mid,
            }),
            HirSortKind::Indices => Ok(SortTargets {
                values: None,
                indices: Some(open(self, result_ty)?),
                width: layout.mid,
            }),
            HirSortKind::TopK(k) => {
                let Type::Tuple(parts) = result_ty else {
                    return Err(CodegenError::InternalError(
                        "`.topk` does not carry a tuple result type".to_string(),
                    ));
                };
                let [values_ty, indices_ty] = parts.as_slice() else {
                    return Err(CodegenError::InternalError(
                        "`.topk` carries a tuple that is not a values/indices pair".to_string(),
                    ));
                };
                Ok(SortTargets {
                    values: Some(open(self, values_ty)?),
                    indices: Some(open(self, indices_ty)?),
                    width: k,
                })
            }
        }
    }

    /// The selection's value: the one tensor handle, or the pair packed into a tuple.
    fn finish(
        &mut self,
        kind: HirSortKind,
        targets: &SortTargets<'ctx>,
        result_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let handle = |slot: &Option<(PointerValue<'ctx>, PointerValue<'ctx>)>| {
            slot.map(|(handle, _)| handle).ok_or_else(|| {
                CodegenError::InternalError("a tensor selection allocated no result".to_string())
            })
        };
        match kind {
            HirSortKind::Values => Ok(handle(&targets.values)?.into()),
            HirSortKind::Indices => Ok(handle(&targets.indices)?.into()),
            HirSortKind::TopK(_) => {
                let struct_ty = match self.get_any_llvm_type(result_ty)? {
                    BasicTypeEnum::StructType(ty) => ty,
                    _ => {
                        return Err(CodegenError::InternalError(
                            "`.topk` result type is not a tuple".to_string(),
                        ))
                    }
                };
                let mut agg = struct_ty.get_undef();
                for (index, slot) in [&targets.values, &targets.indices].into_iter().enumerate() {
                    agg = self
                        .builder
                        .build_insert_value(agg, handle(slot)?, index as u32, "tensor.sort.pair")?
                        .into_struct_value();
                }
                Ok(agg.into())
            }
        }
    }

    /// Seed the permutation with `0..mid`, which is what makes the sort below stable:
    /// two equal elements keep the relative order they start in.
    fn fill_identity(&mut self, order: PointerValue<'ctx>, mid: usize) -> CodegenResult<()> {
        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("a tensor selection outside a function".to_string())
        })?;
        let i64_type = self.context.i64_type();
        let step = self.entry_alloca(i64_type, "tensor.sort.seed")?;
        self.builder.build_store(step, i64_type.const_zero())?;
        let head = self
            .context
            .append_basic_block(function, "tensor.sort.seed.head");
        let body = self
            .context
            .append_basic_block(function, "tensor.sort.seed.body");
        let done = self
            .context
            .append_basic_block(function, "tensor.sort.seed.done");

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let i = self
            .builder
            .build_load(i64_type, step, "tensor.sort.seed.i")?
            .into_int_value();
        let more = self.builder.build_int_compare(
            IntPredicate::ULT,
            i,
            i64_type.const_int(mid as u64, false),
            "tensor.sort.seed.more",
        )?;
        self.builder.build_conditional_branch(more, body, done)?;

        self.builder.position_at_end(body);
        let slot = self.slot_of(i64_type.into(), order, i, "tensor.sort.seed.slot")?;
        self.builder.build_store(slot, i)?;
        let next =
            self.builder
                .build_int_add(i, i64_type.const_int(1, false), "tensor.sort.seed.next")?;
        self.builder.build_store(step, next)?;
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(done);
        Ok(())
    }

    /// Sort `order` so that the run's elements are visited in order through it.
    ///
    /// A stable insertion sort: each position's index is carried left past every earlier
    /// index whose element it strictly precedes, and stops at the first it does not, so
    /// equal elements never cross.
    #[allow(clippy::too_many_arguments)]
    fn insertion_sort(
        &mut self,
        order: PointerValue<'ctx>,
        elem_llvm: BasicTypeEnum<'ctx>,
        source: PointerValue<'ctx>,
        base: IntValue<'ctx>,
        layout: &SortLayout,
        element: &Type,
        descending: bool,
    ) -> CodegenResult<()> {
        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("a tensor selection outside a function".to_string())
        })?;
        let i64_type = self.context.i64_type();
        let outer = self.entry_alloca(i64_type, "tensor.sort.i")?;
        let hole = self.entry_alloca(i64_type, "tensor.sort.j")?;
        let carried = self.entry_alloca(i64_type, "tensor.sort.key")?;
        self.builder
            .build_store(outer, i64_type.const_int(1, false))?;

        let head = self
            .context
            .append_basic_block(function, "tensor.sort.pass.head");
        let body = self
            .context
            .append_basic_block(function, "tensor.sort.pass.body");
        let shift_test = self
            .context
            .append_basic_block(function, "tensor.sort.shift.test");
        let shift_cmp = self
            .context
            .append_basic_block(function, "tensor.sort.shift.cmp");
        let shift = self
            .context
            .append_basic_block(function, "tensor.sort.shift");
        let place = self
            .context
            .append_basic_block(function, "tensor.sort.place");
        let done = self
            .context
            .append_basic_block(function, "tensor.sort.pass.done");

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let i = self
            .builder
            .build_load(i64_type, outer, "tensor.sort.idx")?
            .into_int_value();
        let more = self.builder.build_int_compare(
            IntPredicate::ULT,
            i,
            i64_type.const_int(layout.mid as u64, false),
            "tensor.sort.pass.more",
        )?;
        self.builder.build_conditional_branch(more, body, done)?;

        self.builder.position_at_end(body);
        let key = self.load_index(order, i, "tensor.sort.key.val")?;
        self.builder.build_store(carried, key)?;
        self.builder.build_store(hole, i)?;
        self.builder.build_unconditional_branch(shift_test)?;

        // `j > 0` guards the load of `order[j - 1]`, so the comparison is a block of its
        // own rather than one leg of an `and`: the index has to be in range before it is
        // read, not after.
        self.builder.position_at_end(shift_test);
        let j = self
            .builder
            .build_load(i64_type, hole, "tensor.sort.hole")?
            .into_int_value();
        let room = self.builder.build_int_compare(
            IntPredicate::UGT,
            j,
            i64_type.const_zero(),
            "tensor.sort.room",
        )?;
        self.builder
            .build_conditional_branch(room, shift_cmp, place)?;

        self.builder.position_at_end(shift_cmp);
        let previous_at =
            self.builder
                .build_int_sub(j, i64_type.const_int(1, false), "tensor.sort.prev.at")?;
        let previous = self.load_index(order, previous_at, "tensor.sort.prev")?;
        let key = self
            .builder
            .build_load(i64_type, carried, "tensor.sort.key.now")?
            .into_int_value();
        let key_value = self.element_at(elem_llvm, source, base, key, layout.inner)?;
        let previous_value = self.element_at(elem_llvm, source, base, previous, layout.inner)?;
        let precedes = self.precedes(key_value, previous_value, element, descending)?;
        self.builder
            .build_conditional_branch(precedes, shift, place)?;

        self.builder.position_at_end(shift);
        let slot = self.slot_of(i64_type.into(), order, j, "tensor.sort.shift.slot")?;
        self.builder.build_store(slot, previous)?;
        self.builder.build_store(hole, previous_at)?;
        self.builder.build_unconditional_branch(shift_test)?;

        self.builder.position_at_end(place);
        let j = self
            .builder
            .build_load(i64_type, hole, "tensor.sort.place.at")?
            .into_int_value();
        let key = self
            .builder
            .build_load(i64_type, carried, "tensor.sort.place.key")?
            .into_int_value();
        let slot = self.slot_of(i64_type.into(), order, j, "tensor.sort.place.slot")?;
        self.builder.build_store(slot, key)?;
        let next =
            self.builder
                .build_int_add(i, i64_type.const_int(1, false), "tensor.sort.pass.next")?;
        self.builder.build_store(outer, next)?;
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(done);
        Ok(())
    }

    /// Write one ordered run into whichever results the selection allocated.
    #[allow(clippy::too_many_arguments)]
    fn write_run(
        &mut self,
        order: PointerValue<'ctx>,
        elem_llvm: BasicTypeEnum<'ctx>,
        source: PointerValue<'ctx>,
        base: IntValue<'ctx>,
        r: IntValue<'ctx>,
        layout: &SortLayout,
        targets: &SortTargets<'ctx>,
    ) -> CodegenResult<()> {
        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("a tensor selection outside a function".to_string())
        })?;
        let i64_type = self.context.i64_type();
        let i32_type = self.context.i32_type();
        let out_base = self.sort_run_base(r, layout.inner, targets.width)?;
        let step = self.entry_alloca(i64_type, "tensor.sort.out")?;
        self.builder.build_store(step, i64_type.const_zero())?;
        let head = self
            .context
            .append_basic_block(function, "tensor.sort.out.head");
        let body = self
            .context
            .append_basic_block(function, "tensor.sort.out.body");
        let done = self
            .context
            .append_basic_block(function, "tensor.sort.out.done");

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let p = self
            .builder
            .build_load(i64_type, step, "tensor.sort.out.p")?
            .into_int_value();
        let more = self.builder.build_int_compare(
            IntPredicate::ULT,
            p,
            i64_type.const_int(targets.width as u64, false),
            "tensor.sort.out.more",
        )?;
        self.builder.build_conditional_branch(more, body, done)?;

        self.builder.position_at_end(body);
        let picked = self.load_index(order, p, "tensor.sort.out.pick")?;
        let stride = self.builder.build_int_mul(
            p,
            i64_type.const_int(layout.inner as u64, false),
            "tensor.sort.out.step",
        )?;
        let at = self
            .builder
            .build_int_add(out_base, stride, "tensor.sort.out.at")?;
        if let Some((_, data)) = targets.values {
            let value = self.element_at(elem_llvm, source, base, picked, layout.inner)?;
            let slot = self.slot_of(elem_llvm, data, at, "tensor.sort.out.vslot")?;
            self.builder.build_store(slot, value)?;
        }
        if let Some((_, data)) = targets.indices {
            let narrowed =
                self.builder
                    .build_int_truncate(picked, i32_type, "tensor.sort.out.index")?;
            let slot = self.slot_of(i32_type.into(), data, at, "tensor.sort.out.islot")?;
            self.builder.build_store(slot, narrowed)?;
        }
        let next =
            self.builder
                .build_int_add(p, i64_type.const_int(1, false), "tensor.sort.out.next")?;
        self.builder.build_store(step, next)?;
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(done);
        Ok(())
    }

    /// Whether `a` strictly precedes `b` under the selection's comparator.
    ///
    /// For floats that comparator is IEEE-754-ordered with one single rule: `NaN`
    /// sorts to the END whatever the direction, so it never precedes anything and
    /// everything precedes it. Both facts fall out of the ordered predicates: a `NaN`
    /// operand makes `<` and `>` alike false, so the two `NaN` tests are all that has to
    /// be spelled out.
    fn precedes(
        &self,
        a: BasicValueEnum<'ctx>,
        b: BasicValueEnum<'ctx>,
        element: &Type,
        descending: bool,
    ) -> CodegenResult<IntValue<'ctx>> {
        if let (BasicValueEnum::FloatValue(a), BasicValueEnum::FloatValue(b)) = (a, b) {
            let predicate = if descending {
                FloatPredicate::OGT
            } else {
                FloatPredicate::OLT
            };
            let ordered = self
                .builder
                .build_float_compare(predicate, a, b, "tensor.sort.lt")?;
            let a_real = self.builder.build_float_compare(
                FloatPredicate::ORD,
                a,
                a,
                "tensor.sort.a.real",
            )?;
            let b_nan =
                self.builder
                    .build_float_compare(FloatPredicate::UNO, b, b, "tensor.sort.b.nan")?;
            let before = self
                .builder
                .build_or(ordered, b_nan, "tensor.sort.before")?;
            return self
                .builder
                .build_and(a_real, before, "tensor.sort.precedes")
                .map_err(CodegenError::from);
        }
        let (BasicValueEnum::IntValue(a), BasicValueEnum::IntValue(b)) = (a, b) else {
            return Err(CodegenError::InternalError(
                "a tensor element is an integer or a float".to_string(),
            ));
        };
        let unsigned = crate::type_mapping::TypeMapper::is_unsigned_int(element);
        let predicate = match (descending, unsigned) {
            (true, true) => IntPredicate::UGT,
            (true, false) => IntPredicate::SGT,
            (false, true) => IntPredicate::ULT,
            (false, false) => IntPredicate::SLT,
        };
        self.builder
            .build_int_compare(predicate, a, b, "tensor.sort.precedes")
            .map_err(CodegenError::from)
    }

    /// The flat index run `r` starts at, for a run of `mid` positions `inner` apart.
    fn sort_run_base(
        &self,
        r: IntValue<'ctx>,
        inner: usize,
        mid: usize,
    ) -> CodegenResult<IntValue<'ctx>> {
        let i64_type = self.context.i64_type();
        let inner_value = i64_type.const_int(inner as u64, false);
        let above = self
            .builder
            .build_int_unsigned_div(r, inner_value, "tensor.sort.above")?;
        let below = self
            .builder
            .build_int_unsigned_rem(r, inner_value, "tensor.sort.below")?;
        let scaled = self.builder.build_int_mul(
            above,
            i64_type.const_int((mid * inner) as u64, false),
            "tensor.sort.scaled",
        )?;
        self.builder
            .build_int_add(scaled, below, "tensor.sort.base")
            .map_err(CodegenError::from)
    }

    /// The run element at position `position`, which is `inner` strides past the base.
    fn element_at(
        &self,
        elem_llvm: BasicTypeEnum<'ctx>,
        source: PointerValue<'ctx>,
        base: IntValue<'ctx>,
        position: IntValue<'ctx>,
        inner: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let i64_type = self.context.i64_type();
        let stride = self.builder.build_int_mul(
            position,
            i64_type.const_int(inner as u64, false),
            "tensor.sort.stride",
        )?;
        let index = self
            .builder
            .build_int_add(base, stride, "tensor.sort.index")?;
        let slot = self.slot_of(elem_llvm, source, index, "tensor.sort.elem.ptr")?;
        self.builder
            .build_load(elem_llvm, slot, "tensor.sort.elem")
            .map_err(CodegenError::from)
    }

    /// One entry of the permutation being built.
    fn load_index(
        &self,
        order: PointerValue<'ctx>,
        at: IntValue<'ctx>,
        name: &str,
    ) -> CodegenResult<IntValue<'ctx>> {
        let i64_type = self.context.i64_type();
        let slot = self.slot_of(i64_type.into(), order, at, "tensor.sort.order.ptr")?;
        Ok(self
            .builder
            .build_load(i64_type, slot, name)?
            .into_int_value())
    }

    /// The address of one element of a flat buffer, `index` elements in.
    fn slot_of(
        &self,
        elem_llvm: BasicTypeEnum<'ctx>,
        buffer: PointerValue<'ctx>,
        index: IntValue<'ctx>,
        name: &str,
    ) -> CodegenResult<PointerValue<'ctx>> {
        // SAFETY: every index is a run base plus a stride below the run's extent, and the
        // permutation slots are the `mid` the scratch array was sized with, so each
        // offset is inside its allocation by the same arithmetic that laid it out.
        unsafe {
            self.builder
                .build_in_bounds_gep(elem_llvm, buffer, &[index], name)
                .map_err(CodegenError::from)
        }
    }
}

/// Split the receiver's flat run into the three factors the ordering walks.
fn sort_layout(shape: &[usize], axis: usize) -> CodegenResult<SortLayout> {
    if axis >= shape.len() {
        return Err(CodegenError::InternalError(
            "a tensor selection names an axis its receiver does not have".to_string(),
        ));
    }
    let outer: usize = shape[..axis].iter().product();
    let inner: usize = shape[axis + 1..].iter().product();
    Ok(SortLayout {
        inner,
        mid: shape[axis],
        runs: outer * inner,
    })
}

#[cfg(test)]
mod tests {
    use super::sort_layout;

    #[test]
    fn the_last_axis_walks_contiguous_runs() {
        let layout = sort_layout(&[2, 3], 1).expect("a rank-2 tensor has an axis 1");
        assert_eq!((layout.inner, layout.mid, layout.runs), (1, 3, 2));
    }

    #[test]
    fn an_inner_axis_walks_strided_runs() {
        let layout = sort_layout(&[2, 3, 4], 1).expect("a rank-3 tensor has an axis 1");
        assert_eq!((layout.inner, layout.mid, layout.runs), (4, 3, 8));
    }

    #[test]
    fn the_first_axis_of_a_matrix_walks_one_run_per_column() {
        let layout = sort_layout(&[2, 3], 0).expect("a rank-2 tensor has an axis 0");
        assert_eq!((layout.inner, layout.mid, layout.runs), (3, 2, 3));
    }

    #[test]
    fn an_axis_the_receiver_does_not_have_is_rejected() {
        assert!(sort_layout(&[2, 3], 2).is_err());
    }
}
