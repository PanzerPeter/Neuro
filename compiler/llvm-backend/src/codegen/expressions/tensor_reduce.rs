// Codegen for the tensor reductions `.sum()`, `.mean()`, `.max()`, and `.min()`.
//
// A tensor's elements sit in one flat, row-major run, so reducing along axis `k` splits
// that run into three constant factors: `outer` elements above the axis, `mid` along it,
// and `inner` below. Result slot `r` then gathers the source elements
// `(r / inner) * mid * inner + j * inner + (r % inner)` for `j` in `0..mid`, and a
// whole-tensor reduction is the same walk with `outer` and `inner` both 1.
//
// Two counted loops rather than a nest of `rank` of them: every factor is a compile-time
// constant, so the IR is the same size whatever the receiver's rank is, exactly as the
// slice and permute copies are built.
//
// The receiver is READ. Nothing is moved and nothing is released here: a reduction
// summarises a buffer its owner keeps.

use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use inkwell::IntPredicate;
use neuro_hir::{HirExpr, HirReduceOp};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// How one reduced run is laid out in the receiver's flat buffer.
struct ReduceLayout {
    /// Elements between two neighbouring values of the reduced axis.
    inner: usize,
    /// The length of the reduced run.
    mid: usize,
    /// How many independent runs there are, which is the result's element count.
    runs: usize,
}

impl<'ctx> CodegenContext<'ctx> {
    /// Lower one reduction: a scalar when `axis` is `None`, a tensor of the surviving
    /// axes otherwise.
    pub(crate) fn codegen_tensor_reduce(
        &mut self,
        receiver: &HirExpr,
        op: HirReduceOp,
        axis: Option<usize>,
        result_ty: &Type,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let source_ty = Type::from_hir(&receiver.ty);
        let Type::Tensor { element, shape } = source_ty.referent().clone() else {
            return Err(CodegenError::InternalError(
                "a tensor reduction does not carry a tensor receiver".to_string(),
            ));
        };
        let shape = crate::types::static_extents(&shape)?;
        let layout = reduce_layout(&shape, axis)?;
        let element = (*element).clone();
        let elem_llvm = self.get_any_llvm_type(&element)?;
        let receiver_handle = self.tensor_receiver_handle(receiver, &source_ty)?;
        let source = self.load_dlpack_data(receiver_handle)?;

        // The accumulator doubles as the result of a whole-tensor reduction, which has
        // exactly one run and so leaves its finished value here. A half-precision run is
        // folded in `f32` and rounded once at the end, so a long sum does not stall at the
        // point where one more element no longer changes a 16-bit total.
        let half =
            elem_llvm.is_float_type() && self.is_half_float_type(elem_llvm.into_float_type());
        let (acc_llvm, acc_element) = if half {
            (self.context.f32_type().into(), Type::F32)
        } else {
            (elem_llvm, element.clone())
        };
        let accumulator = self.entry_alloca(acc_llvm, "tensor.reduce.acc")?;
        let target = match result_ty {
            Type::Tensor { .. } => {
                let handle = self.alloc_dlpack_tensor(result_ty, "tensor.reduce")?;
                Some((handle, self.load_dlpack_data(handle)?))
            }
            _ => None,
        };

        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("a tensor reduction outside a function".to_string())
        })?;
        let i64_type = self.context.i64_type();
        let run = self.entry_alloca(i64_type, "tensor.reduce.run")?;
        self.builder.build_store(run, i64_type.const_zero())?;
        let head = self
            .context
            .append_basic_block(function, "tensor.reduce.head");
        let body = self
            .context
            .append_basic_block(function, "tensor.reduce.body");
        let done = self
            .context
            .append_basic_block(function, "tensor.reduce.done");

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let r = self
            .builder
            .build_load(i64_type, run, "tensor.reduce.r")?
            .into_int_value();
        let more = self.builder.build_int_compare(
            IntPredicate::ULT,
            r,
            i64_type.const_int(layout.runs as u64, false),
            "tensor.reduce.more",
        )?;
        self.builder.build_conditional_branch(more, body, done)?;

        self.builder.position_at_end(body);
        let base = self.run_base(r, &layout)?;
        let first = self.load_element(elem_llvm, source, base, "tensor.reduce.first")?;
        let first = self.widen_element(first)?;
        self.builder.build_store(accumulator, first)?;
        self.fold_run(
            (elem_llvm, acc_llvm),
            source,
            base,
            accumulator,
            &layout,
            op,
            &acc_element,
            offset,
        )?;

        let mut value = self
            .builder
            .build_load(acc_llvm, accumulator, "tensor.reduce.value")?;
        if op == HirReduceOp::Mean {
            value = self.divide_by_run_length(value, layout.mid)?;
        }
        if half {
            value = self
                .narrow_float(value.into_float_value(), elem_llvm.into_float_type())?
                .into();
        }
        // The finished value, at the element's own width, is the result of a whole-tensor
        // reduction.
        let result_slot = self.entry_alloca(elem_llvm, "tensor.reduce.out")?;
        self.builder.build_store(result_slot, value)?;
        if let Some((_, data)) = target {
            let slot = self.tensor_buffer_slot(elem_llvm, data, r)?;
            self.builder.build_store(slot, value)?;
        }
        let next =
            self.builder
                .build_int_add(r, i64_type.const_int(1, false), "tensor.reduce.next")?;
        self.builder.build_store(run, next)?;
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(done);
        // Every read of the receiver is behind us, so a receiver that owns its buffer and
        // has no binding to release it can be freed here instead of leaking.
        self.release_receiver_temporary(receiver, receiver_handle)?;
        match target {
            Some((handle, _)) => Ok(handle.into()),
            None => self
                .builder
                .build_load(elem_llvm, result_slot, "tensor.reduce.result")
                .map_err(CodegenError::from),
        }
    }

    /// The source index result slot `r` starts at: the run's first element.
    fn run_base(&self, r: IntValue<'ctx>, layout: &ReduceLayout) -> CodegenResult<IntValue<'ctx>> {
        let i64_type = self.context.i64_type();
        let inner = i64_type.const_int(layout.inner as u64, false);
        let above = self
            .builder
            .build_int_unsigned_div(r, inner, "tensor.reduce.above")?;
        let below = self
            .builder
            .build_int_unsigned_rem(r, inner, "tensor.reduce.below")?;
        let scaled = self.builder.build_int_mul(
            above,
            i64_type.const_int((layout.mid * layout.inner) as u64, false),
            "tensor.reduce.scaled",
        )?;
        self.builder
            .build_int_add(scaled, below, "tensor.reduce.base")
            .map_err(CodegenError::from)
    }

    /// Fold the run's remaining `mid - 1` elements into the accumulator. The first is
    /// already there, which is what gives `.max()` and `.min()` a starting value without
    /// a per-dtype sentinel.
    #[allow(clippy::too_many_arguments)]
    fn fold_run(
        &mut self,
        (elem_llvm, acc_llvm): (
            inkwell::types::BasicTypeEnum<'ctx>,
            inkwell::types::BasicTypeEnum<'ctx>,
        ),
        source: PointerValue<'ctx>,
        base: IntValue<'ctx>,
        accumulator: PointerValue<'ctx>,
        layout: &ReduceLayout,
        op: HirReduceOp,
        element: &Type,
        offset: usize,
    ) -> CodegenResult<()> {
        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("a tensor reduction outside a function".to_string())
        })?;
        let i64_type = self.context.i64_type();
        let step = self.entry_alloca(i64_type, "tensor.reduce.j")?;
        self.builder
            .build_store(step, i64_type.const_int(1, false))?;
        let head = self
            .context
            .append_basic_block(function, "tensor.reduce.fold.head");
        let body = self
            .context
            .append_basic_block(function, "tensor.reduce.fold.body");
        let done = self
            .context
            .append_basic_block(function, "tensor.reduce.fold.done");

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let j = self
            .builder
            .build_load(i64_type, step, "tensor.reduce.jdx")?
            .into_int_value();
        let more = self.builder.build_int_compare(
            IntPredicate::ULT,
            j,
            i64_type.const_int(layout.mid as u64, false),
            "tensor.reduce.fold.more",
        )?;
        self.builder.build_conditional_branch(more, body, done)?;

        self.builder.position_at_end(body);
        let stepped = self.builder.build_int_mul(
            j,
            i64_type.const_int(layout.inner as u64, false),
            "tensor.reduce.step",
        )?;
        let index = self
            .builder
            .build_int_add(base, stepped, "tensor.reduce.index")?;
        let value = self.load_element(elem_llvm, source, index, "tensor.reduce.elem")?;
        let value = self.widen_element(value)?;
        let carried = self
            .builder
            .build_load(acc_llvm, accumulator, "tensor.reduce.carried")?;
        let folded = self.fold_element(op, carried, value, element, offset)?;
        self.builder.build_store(accumulator, folded)?;
        let next =
            self.builder
                .build_int_add(j, i64_type.const_int(1, false), "tensor.reduce.jnext")?;
        self.builder.build_store(step, next)?;
        // An integer fold may have split the body around its overflow guard, so the back
        // edge leaves whichever block is current now.
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(done);
        Ok(())
    }

    /// One fold step. A sum carries the same overflow guard the scalar `+` does: a
    /// tensor's arithmetic is its element's arithmetic. `.max()` / `.min()` keep the
    /// candidate the sorting comparator would put first, descending or ascending.
    fn fold_element(
        &mut self,
        op: HirReduceOp,
        carried: BasicValueEnum<'ctx>,
        value: BasicValueEnum<'ctx>,
        element: &Type,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        match (op, carried, value) {
            (HirReduceOp::Max | HirReduceOp::Min, _, _) => {
                let descending = op == HirReduceOp::Max;
                let wins = self.precedes(value, carried, element, descending)?;
                Ok(self
                    .builder
                    .build_select(wins, value, carried, "tensor.reduce.sel")?)
            }
            (_, BasicValueEnum::FloatValue(a), BasicValueEnum::FloatValue(b)) => Ok(self
                .builder
                .build_float_add(a, b, "tensor.reduce.add")?
                .into()),
            (HirReduceOp::Sum, BasicValueEnum::IntValue(a), BasicValueEnum::IntValue(b)) => {
                let unsigned = crate::type_mapping::TypeMapper::is_unsigned_int(element);
                Ok(self
                    .codegen_int_arith(
                        ast_types::BinaryOp::Add,
                        a,
                        b,
                        unsigned,
                        offset,
                        "tensor.reduce.add",
                    )?
                    .into())
            }
            (HirReduceOp::Mean, BasicValueEnum::IntValue(_), _) => {
                Err(CodegenError::InternalError(
                    "`.mean()` reached codegen on an integer tensor".into(),
                ))
            }
            _ => Err(CodegenError::InternalError(
                "a tensor element is an integer or a float".to_string(),
            )),
        }
    }

    /// The mean's division by the run length, which the checker has already restricted
    /// to a floating-point element type.
    fn divide_by_run_length(
        &self,
        total: BasicValueEnum<'ctx>,
        length: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let BasicValueEnum::FloatValue(total) = total else {
            return Err(CodegenError::InternalError(
                "`.mean()` reached codegen on a non-float accumulator".to_string(),
            ));
        };
        let divisor = total.get_type().const_float(length as f64);
        Ok(self
            .builder
            .build_float_div(total, divisor, "tensor.reduce.mean")?
            .into())
    }

    /// A loaded element at the width it is folded at: `f32` for a half-precision one.
    fn widen_element(&self, value: BasicValueEnum<'ctx>) -> CodegenResult<BasicValueEnum<'ctx>> {
        match value {
            BasicValueEnum::FloatValue(v) => Ok(self.widen_half(v)?.into()),
            other => Ok(other),
        }
    }

    fn load_element(
        &self,
        elem_llvm: inkwell::types::BasicTypeEnum<'ctx>,
        buffer: PointerValue<'ctx>,
        index: IntValue<'ctx>,
        name: &str,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let slot = self.tensor_buffer_slot(elem_llvm, buffer, index)?;
        self.builder
            .build_load(elem_llvm, slot, name)
            .map_err(CodegenError::from)
    }

    /// The address of one element of a tensor buffer, `index` elements in.
    fn tensor_buffer_slot(
        &self,
        elem_llvm: inkwell::types::BasicTypeEnum<'ctx>,
        buffer: PointerValue<'ctx>,
        index: IntValue<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        // SAFETY: every index is `base + j * inner` with `base` below the buffer's
        // element count and `j` below the reduced extent, so the offset is inside the
        // allocation by the same arithmetic that laid the buffer out.
        unsafe {
            self.builder
                .build_in_bounds_gep(elem_llvm, buffer, &[index], "tensor.reduce.ptr")
                .map_err(CodegenError::from)
        }
    }
}

/// Split the receiver's flat run into the three factors the reduction walks.
fn reduce_layout(shape: &[usize], axis: Option<usize>) -> CodegenResult<ReduceLayout> {
    let Some(axis) = axis else {
        return Ok(ReduceLayout {
            inner: 1,
            mid: shape.iter().product(),
            runs: 1,
        });
    };
    if axis >= shape.len() {
        return Err(CodegenError::InternalError(
            "a tensor reduction names an axis its receiver does not have".to_string(),
        ));
    }
    let outer: usize = shape[..axis].iter().product();
    let inner: usize = shape[axis + 1..].iter().product();
    Ok(ReduceLayout {
        inner,
        mid: shape[axis],
        runs: outer * inner,
    })
}
