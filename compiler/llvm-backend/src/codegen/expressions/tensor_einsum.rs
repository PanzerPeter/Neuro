// Codegen for the Einstein-notation contraction `einsum("bij,bjk->bik", a, b)`.
//
// The notation is already gone: HIR hands over one extent per subscript letter and, per
// operand, which letter each of its axes carries. That reduces the contraction to flat
// index arithmetic over row-major buffers, with every factor a compile-time constant.
//
// Two counted loops rather than a nest of one per letter: the outer walks the result's
// elements and the inner the contracted letters' product, so the IR is the same size
// whatever the ranks are, exactly as the reductions and the slice copies are built. The
// accumulator starts at the additive identity because the loop sums products — unlike a
// `.max()` fold there is no first element to seed it with, and a contraction over an
// empty run is genuinely zero rather than undefined.
//
// A letter's index is recovered from a loop counter by dividing out the letters below it
// and taking the remainder. An operand's offset is then the sum of that index times a
// per-letter COEFFICIENT: the strides of every axis the letter sits on, added together.
// Summing them is what makes a letter repeated within one operand walk its diagonal,
// which is the whole of `"ii->"`.
//
// Every operand is READ. A contraction allocates its own result, so an operand that a
// binding owns is left alone and only a temporary built for the call is released here.

use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use inkwell::IntPredicate;
use neuro_hir::HirExpr;

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// How one operand is addressed: its element buffer, and the coefficient each subscript
/// letter contributes to a flat index into it.
struct OperandPlan<'ctx> {
    data: PointerValue<'ctx>,
    /// `coefficients[letter]`, zero for a letter the operand's subscript never names.
    coefficients: Vec<usize>,
}

/// How one loop counter decomposes into per-letter indices: the letters it walks, and
/// the divisor that isolates each one.
struct CounterPlan {
    /// `(letter, divisor)` pairs, in the order the counter's digits run.
    digits: Vec<(usize, usize)>,
    /// The counter's length: the product of the digits' extents.
    count: usize,
}

impl<'ctx> CodegenContext<'ctx> {
    /// Lower one contraction: a scalar when the output subscript is empty, and a tensor
    /// of the output letters' extents otherwise.
    pub(crate) fn codegen_tensor_einsum(
        &mut self,
        operands: &[HirExpr],
        inputs: &[Vec<usize>],
        output: &[usize],
        extents: &[usize],
        result_ty: &Type,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let element = match result_ty {
            Type::Tensor { element, .. } => (**element).clone(),
            scalar => scalar.clone(),
        };
        let elem_llvm = self.get_any_llvm_type(&element)?;
        let out_plan = counter_plan(output, extents)?;
        let contracted = contracted_letters(inputs, output, extents);
        let sum_plan = counter_plan(&contracted, extents)?;

        // Handles are kept alongside the data pointers: releasing a temporary operand at
        // the end needs the handle, and re-lowering the expression to recover it would
        // emit the whole computation a second time.
        let mut handles = Vec::with_capacity(operands.len());
        let mut plans = Vec::with_capacity(operands.len());
        for (operand, subscript) in operands.iter().zip(inputs.iter()) {
            let operand_ty = Type::from_hir(&operand.ty);
            let handle = self.tensor_receiver_handle(operand, &operand_ty)?;
            let data = self.load_dlpack_data(handle)?;
            handles.push(handle);
            plans.push(OperandPlan {
                data,
                coefficients: coefficients(subscript, extents),
            });
        }

        // The accumulator doubles as the result of a fully contracted `einsum`, which has
        // exactly one output slot and so leaves its finished value here.
        let accumulator = self.entry_alloca(elem_llvm, "tensor.einsum.acc")?;
        let target = match result_ty {
            Type::Tensor { .. } => {
                let handle = self.alloc_dlpack_tensor(result_ty, "tensor.einsum")?;
                Some((handle, self.load_dlpack_data(handle)?))
            }
            _ => None,
        };

        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("a tensor contraction outside a function".to_string())
        })?;
        let i64_type = self.context.i64_type();
        let slot = self.entry_alloca(i64_type, "tensor.einsum.out")?;
        self.builder.build_store(slot, i64_type.const_zero())?;
        let head = self
            .context
            .append_basic_block(function, "tensor.einsum.head");
        let body = self
            .context
            .append_basic_block(function, "tensor.einsum.body");
        let done = self
            .context
            .append_basic_block(function, "tensor.einsum.done");

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let o = self
            .builder
            .build_load(i64_type, slot, "tensor.einsum.o")?
            .into_int_value();
        let more = self.builder.build_int_compare(
            IntPredicate::ULT,
            o,
            i64_type.const_int(out_plan.count as u64, false),
            "tensor.einsum.more",
        )?;
        self.builder.build_conditional_branch(more, body, done)?;

        self.builder.position_at_end(body);
        let zero = zero_of(elem_llvm)?;
        self.builder.build_store(accumulator, zero)?;
        self.accumulate_contraction(
            elem_llvm,
            &element,
            accumulator,
            o,
            &out_plan,
            &sum_plan,
            &plans,
            extents,
            offset,
        )?;
        let value = self
            .builder
            .build_load(elem_llvm, accumulator, "tensor.einsum.value")?;
        if let Some((_, data)) = target {
            let address = self.einsum_slot(elem_llvm, data, o)?;
            self.builder.build_store(address, value)?;
        }
        let next =
            self.builder
                .build_int_add(o, i64_type.const_int(1, false), "tensor.einsum.next")?;
        self.builder.build_store(slot, next)?;
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(done);
        // Every read of every operand is behind us, so an operand that owns its buffer
        // and has no binding to release it can be freed here instead of leaking.
        for (operand, handle) in operands.iter().zip(handles) {
            self.release_receiver_temporary(operand, handle)?;
        }
        match target {
            Some((handle, _)) => Ok(handle.into()),
            None => self
                .builder
                .build_load(elem_llvm, accumulator, "tensor.einsum.result")
                .map_err(CodegenError::from),
        }
    }

    /// Sum the products of the operands' elements over every value of the contracted
    /// letters, for the one output slot `o` names.
    #[allow(clippy::too_many_arguments)]
    fn accumulate_contraction(
        &mut self,
        elem_llvm: BasicTypeEnum<'ctx>,
        element: &Type,
        accumulator: PointerValue<'ctx>,
        o: IntValue<'ctx>,
        out_plan: &CounterPlan,
        sum_plan: &CounterPlan,
        plans: &[OperandPlan<'ctx>],
        extents: &[usize],
        offset: usize,
    ) -> CodegenResult<()> {
        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("a tensor contraction outside a function".to_string())
        })?;
        let i64_type = self.context.i64_type();
        let step = self.entry_alloca(i64_type, "tensor.einsum.s")?;
        self.builder.build_store(step, i64_type.const_zero())?;
        let head = self
            .context
            .append_basic_block(function, "tensor.einsum.sum.head");
        let body = self
            .context
            .append_basic_block(function, "tensor.einsum.sum.body");
        let done = self
            .context
            .append_basic_block(function, "tensor.einsum.sum.done");

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let s = self
            .builder
            .build_load(i64_type, step, "tensor.einsum.sdx")?
            .into_int_value();
        let more = self.builder.build_int_compare(
            IntPredicate::ULT,
            s,
            i64_type.const_int(sum_plan.count as u64, false),
            "tensor.einsum.sum.more",
        )?;
        self.builder.build_conditional_branch(more, body, done)?;

        self.builder.position_at_end(body);
        // Both counters are decoded here rather than once per output slot: the output
        // digits are loop-invariant and LLVM hoists them, and computing them in one place
        // keeps the two decodings identical.
        let mut indices: Vec<Option<IntValue<'ctx>>> = vec![None; extents.len()];
        self.decode_counter(o, out_plan, extents, &mut indices, "out")?;
        self.decode_counter(s, sum_plan, extents, &mut indices, "sum")?;

        let mut product: Option<BasicValueEnum<'ctx>> = None;
        for plan in plans {
            let index = self.operand_index(plan, &indices)?;
            let address = self.einsum_slot(elem_llvm, plan.data, index)?;
            let value = self
                .builder
                .build_load(elem_llvm, address, "tensor.einsum.elem")?;
            product = Some(match product {
                None => value,
                Some(carried) => self.multiply(carried, value, element, offset)?,
            });
        }
        let product = product.ok_or_else(|| {
            CodegenError::InternalError("a tensor contraction with no operands".to_string())
        })?;
        let carried = self
            .builder
            .build_load(elem_llvm, accumulator, "tensor.einsum.carried")?;
        let total = self.add(carried, product, element, offset)?;
        self.builder.build_store(accumulator, total)?;

        // A checked multiply or add may have split the body around its overflow guard, so
        // the back edge leaves whichever block is current now. `s` is reloaded for the
        // same reason: the increment has to be built where the body actually ends.
        let s = self
            .builder
            .build_load(i64_type, step, "tensor.einsum.snow")?
            .into_int_value();
        let next =
            self.builder
                .build_int_add(s, i64_type.const_int(1, false), "tensor.einsum.snext")?;
        self.builder.build_store(step, next)?;
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(done);
        Ok(())
    }

    /// Recover each of a counter's letters from its value, writing them into `indices`.
    fn decode_counter(
        &self,
        counter: IntValue<'ctx>,
        plan: &CounterPlan,
        extents: &[usize],
        indices: &mut [Option<IntValue<'ctx>>],
        label: &str,
    ) -> CodegenResult<()> {
        let i64_type = self.context.i64_type();
        for (letter, divisor) in &plan.digits {
            let above = self.builder.build_int_unsigned_div(
                counter,
                i64_type.const_int(*divisor as u64, false),
                &format!("tensor.einsum.{label}.div"),
            )?;
            let index = self.builder.build_int_unsigned_rem(
                above,
                i64_type.const_int(extents[*letter] as u64, false),
                &format!("tensor.einsum.{label}.idx"),
            )?;
            indices[*letter] = Some(index);
        }
        Ok(())
    }

    /// The flat index into one operand's buffer: every letter's index scaled by the
    /// coefficient that letter contributes, summed.
    fn operand_index(
        &self,
        plan: &OperandPlan<'ctx>,
        indices: &[Option<IntValue<'ctx>>],
    ) -> CodegenResult<IntValue<'ctx>> {
        let i64_type = self.context.i64_type();
        let mut total = i64_type.const_zero();
        for (letter, coefficient) in plan.coefficients.iter().enumerate() {
            if *coefficient == 0 {
                continue;
            }
            let Some(index) = indices[letter] else {
                return Err(CodegenError::InternalError(
                    "a tensor contraction reads a subscript letter no counter walks".to_string(),
                ));
            };
            let scaled = self.builder.build_int_mul(
                index,
                i64_type.const_int(*coefficient as u64, false),
                "tensor.einsum.scaled",
            )?;
            total = self
                .builder
                .build_int_add(total, scaled, "tensor.einsum.index")?;
        }
        Ok(total)
    }

    /// One product step, carrying the same overflow guard the scalar `*` does: a
    /// tensor's arithmetic is its element's arithmetic.
    fn multiply(
        &mut self,
        lhs: BasicValueEnum<'ctx>,
        rhs: BasicValueEnum<'ctx>,
        element: &Type,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        if let (BasicValueEnum::FloatValue(a), BasicValueEnum::FloatValue(b)) = (lhs, rhs) {
            return Ok(self
                .builder
                .build_float_mul(a, b, "tensor.einsum.mul")?
                .into());
        }
        let (BasicValueEnum::IntValue(a), BasicValueEnum::IntValue(b)) = (lhs, rhs) else {
            return Err(CodegenError::InternalError(
                "a tensor element is an integer or a float".to_string(),
            ));
        };
        let unsigned = crate::type_mapping::TypeMapper::is_unsigned_int(element);
        Ok(self
            .codegen_int_arith(
                ast_types::BinaryOp::Multiply,
                a,
                b,
                unsigned,
                offset,
                "tensor.einsum.mul",
            )?
            .into())
    }

    /// One accumulation step, guarded the same way.
    fn add(
        &mut self,
        lhs: BasicValueEnum<'ctx>,
        rhs: BasicValueEnum<'ctx>,
        element: &Type,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        if let (BasicValueEnum::FloatValue(a), BasicValueEnum::FloatValue(b)) = (lhs, rhs) {
            return Ok(self
                .builder
                .build_float_add(a, b, "tensor.einsum.add")?
                .into());
        }
        let (BasicValueEnum::IntValue(a), BasicValueEnum::IntValue(b)) = (lhs, rhs) else {
            return Err(CodegenError::InternalError(
                "a tensor element is an integer or a float".to_string(),
            ));
        };
        let unsigned = crate::type_mapping::TypeMapper::is_unsigned_int(element);
        Ok(self
            .codegen_int_arith(
                ast_types::BinaryOp::Add,
                a,
                b,
                unsigned,
                offset,
                "tensor.einsum.add",
            )?
            .into())
    }

    /// The address of one element of a tensor buffer, `index` elements in.
    fn einsum_slot(
        &self,
        elem_llvm: BasicTypeEnum<'ctx>,
        buffer: PointerValue<'ctx>,
        index: IntValue<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        // SAFETY: every letter's index is a remainder below its extent, and an operand's
        // coefficients are the row-major strides of the axes that letter sits on, so the
        // sum is inside the allocation by the same arithmetic that laid the buffer out.
        unsafe {
            self.builder
                .build_in_bounds_gep(elem_llvm, buffer, &[index], "tensor.einsum.ptr")
                .map_err(CodegenError::from)
        }
    }
}

/// The additive identity of an element type: what an empty contraction sums to.
fn zero_of(elem_llvm: BasicTypeEnum<'_>) -> CodegenResult<BasicValueEnum<'_>> {
    match elem_llvm {
        BasicTypeEnum::IntType(ty) => Ok(ty.const_zero().into()),
        BasicTypeEnum::FloatType(ty) => Ok(ty.const_zero().into()),
        _ => Err(CodegenError::InternalError(
            "a tensor contraction reached codegen on a non-numeric element".to_string(),
        )),
    }
}

/// The letters that appear in an input subscript but not in the output: the ones summed
/// over. Reported in letter order so the inner counter's digit layout is deterministic.
fn contracted_letters(inputs: &[Vec<usize>], output: &[usize], extents: &[usize]) -> Vec<usize> {
    (0..extents.len())
        .filter(|letter| !output.contains(letter))
        .filter(|letter| inputs.iter().any(|subscript| subscript.contains(letter)))
        .collect()
}

/// How a counter over `letters` decomposes: the divisor that isolates each digit, and
/// the counter's total length.
fn counter_plan(letters: &[usize], extents: &[usize]) -> CodegenResult<CounterPlan> {
    let mut digits = Vec::with_capacity(letters.len());
    let mut divisor: usize = 1;
    for letter in letters.iter().rev() {
        let extent = *extents.get(*letter).ok_or_else(|| {
            CodegenError::InternalError(
                "a tensor contraction names a subscript letter with no extent".to_string(),
            )
        })?;
        digits.push((*letter, divisor));
        divisor = divisor.saturating_mul(extent);
    }
    digits.reverse();
    Ok(CounterPlan {
        digits,
        count: divisor,
    })
}

/// What each letter contributes to one operand's flat index: the row-major strides of
/// every axis that letter sits on, added together. A letter the subscript never names
/// contributes nothing, and a letter it names twice contributes the diagonal step.
fn coefficients(subscript: &[usize], extents: &[usize]) -> Vec<usize> {
    let mut coefficients = vec![0usize; extents.len()];
    let mut stride: usize = 1;
    for letter in subscript.iter().rev() {
        coefficients[*letter] = coefficients[*letter].saturating_add(stride);
        stride = stride.saturating_mul(extents[*letter]);
    }
    coefficients
}

#[cfg(test)]
mod tests {
    use super::{coefficients, contracted_letters, counter_plan};

    #[test]
    fn a_matmul_contracts_only_the_shared_letter() {
        // "ij,jk->ik" with i=2, j=3, k=4, letters interned in first-appearance order.
        let inputs = vec![vec![0, 1], vec![1, 2]];
        assert_eq!(contracted_letters(&inputs, &[0, 2], &[2, 3, 4]), vec![1]);
    }

    #[test]
    fn row_major_strides_come_back_as_coefficients() {
        // Operand "jk" over j=3, k=4: k steps by 1 and j by 4.
        assert_eq!(coefficients(&[1, 2], &[2, 3, 4]), vec![0, 4, 1]);
    }

    #[test]
    fn a_repeated_letter_steps_the_diagonal() {
        // Operand "ii" over a 3x3: the diagonal step is 3 + 1.
        assert_eq!(coefficients(&[0, 0], &[3]), vec![4]);
    }

    #[test]
    fn a_counter_walks_the_product_of_its_extents() {
        let plan = match counter_plan(&[0, 2], &[2, 3, 4]) {
            Ok(plan) => plan,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(plan.count, 8);
        assert_eq!(plan.digits, vec![(0, 4), (2, 1)]);
    }

    #[test]
    fn an_empty_counter_runs_once() {
        let plan = match counter_plan(&[], &[2, 3]) {
            Ok(plan) => plan,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(plan.count, 1);
        assert!(plan.digits.is_empty());
    }
}
