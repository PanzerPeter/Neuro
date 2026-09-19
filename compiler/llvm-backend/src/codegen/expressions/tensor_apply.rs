// Codegen for the functional traversals `.map(f)`, `.zip(other, f)`, `.reduce(init, f)`.
//
// One counted loop over the flat, row-major buffer, whatever the receiver's rank: every
// extent is a compile-time number, so the element count is one constant and no axis
// arithmetic is needed. The three differ only inside the loop body — what the call is
// given, and whether its answer is stored or carried.
//
// The function value is evaluated ONCE, before the loop, and its `{ fn_ptr, env_ptr }`
// halves are reused per element. `t.map(make_rule())` must not rebuild its rule per
// element, the same rule an adapter chain in a `for` head follows.
//
// Every operand is READ. Nothing is moved here: a traversal allocates its own result, so
// the tensors it walked stay alive and usable.

use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use inkwell::IntPredicate;
use neuro_hir::{HirExpr, HirTensorApply};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// One buffer a traversal walks: where its elements are and how wide one is.
struct Walked<'ctx> {
    handle: PointerValue<'ctx>,
    data: PointerValue<'ctx>,
    elem_llvm: inkwell::types::BasicTypeEnum<'ctx>,
}

impl<'ctx> CodegenContext<'ctx> {
    /// Lower one traversal: a fresh tensor for `.map` and `.zip`, and the carried
    /// accumulator for `.reduce`.
    pub(crate) fn codegen_tensor_apply(
        &mut self,
        kind: HirTensorApply,
        receiver: &HirExpr,
        operand: Option<&HirExpr>,
        callee: &HirExpr,
        result_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let source = self.walk_tensor(receiver)?;
        let count = self.element_count(receiver)?;

        // Evaluated in source order: the second tensor or the seed, then the function.
        let (other, seed) = match (kind, operand) {
            (HirTensorApply::Zip, Some(entry)) => (Some(self.walk_tensor(entry)?), None),
            (HirTensorApply::Reduce, Some(entry)) => (None, Some(self.codegen_expr(entry)?)),
            (HirTensorApply::Map, None) => (None, None),
            _ => {
                return Err(CodegenError::InternalError(
                    "a tensor traversal carries the wrong operand for its kind".to_string(),
                ))
            }
        };
        let call_ret = match &callee.ty {
            neuro_hir::HirType::Function { ret, .. } => Type::from_hir(ret),
            _ => {
                return Err(CodegenError::InternalError(
                    "a tensor traversal's callee is not a function".to_string(),
                ))
            }
        };
        let value = self.codegen_expr(callee)?;
        let target = self.split_function_value(value)?;
        let call_llvm = self.get_any_llvm_type(&call_ret)?;

        // A fold carries its answer in a slot; the other two write theirs into a buffer.
        let accumulator = match seed {
            Some(seed) => {
                let slot = self.entry_alloca(call_llvm, "tensor.apply.acc")?;
                self.builder.build_store(slot, seed)?;
                Some(slot)
            }
            None => None,
        };
        let written = match kind {
            HirTensorApply::Reduce => None,
            _ => {
                let handle = self.alloc_dlpack_tensor(result_ty, "tensor.apply")?;
                Some((handle, self.load_dlpack_data(handle)?))
            }
        };

        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("a tensor traversal outside a function".to_string())
        })?;
        let i64_type = self.context.i64_type();
        let cursor = self.entry_alloca(i64_type, "tensor.apply.i")?;
        self.builder.build_store(cursor, i64_type.const_zero())?;
        let head = self
            .context
            .append_basic_block(function, "tensor.apply.head");
        let body = self
            .context
            .append_basic_block(function, "tensor.apply.body");
        let done = self
            .context
            .append_basic_block(function, "tensor.apply.done");

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let index = self
            .builder
            .build_load(i64_type, cursor, "tensor.apply.idx")?
            .into_int_value();
        let more = self.builder.build_int_compare(
            IntPredicate::ULT,
            index,
            i64_type.const_int(count as u64, false),
            "tensor.apply.more",
        )?;
        self.builder.build_conditional_branch(more, body, done)?;

        self.builder.position_at_end(body);
        let element = self.load_walked(&source, index)?;
        let args: Vec<BasicValueEnum<'ctx>> = match (kind, &other, accumulator) {
            (HirTensorApply::Zip, Some(other), _) => {
                vec![element, self.load_walked(other, index)?]
            }
            // The accumulator is handed over first, which is the order `|acc, x|` is
            // written in.
            (HirTensorApply::Reduce, _, Some(slot)) => {
                let carried = self
                    .builder
                    .build_load(call_llvm, slot, "tensor.apply.carried")?;
                vec![carried, element]
            }
            _ => vec![element],
        };
        let answer = self
            .call_function_value(&target, &args, &call_ret)?
            .ok_or_else(|| {
                CodegenError::InternalError(
                    "a tensor traversal's function answered no value".to_string(),
                )
            })?;
        match (accumulator, &written) {
            (Some(slot), _) => {
                self.builder.build_store(slot, answer)?;
            }
            (None, Some((_, data))) => {
                let slot = self.buffer_slot(call_llvm, *data, index)?;
                self.builder.build_store(slot, answer)?;
            }
            (None, None) => {
                return Err(CodegenError::InternalError(
                    "a tensor traversal has nowhere to put its answer".to_string(),
                ))
            }
        }
        let next =
            self.builder
                .build_int_add(index, i64_type.const_int(1, false), "tensor.apply.next")?;
        self.builder.build_store(cursor, next)?;
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(done);
        // Every read is behind us, so an operand that owns its buffer and has no binding
        // to release it can be freed here instead of leaking.
        self.release_receiver_temporary(receiver, source.handle)?;
        if let (Some(other), Some(entry)) = (&other, operand) {
            self.release_receiver_temporary(entry, other.handle)?;
        }
        match (written, accumulator) {
            (Some((handle, _)), _) => Ok(handle.into()),
            (None, Some(slot)) => self
                .builder
                .build_load(call_llvm, slot, "tensor.apply.result")
                .map_err(CodegenError::from),
            (None, None) => Err(CodegenError::InternalError(
                "a tensor traversal produced no value".to_string(),
            )),
        }
    }

    /// The handle, buffer, and element width of one tensor a traversal walks.
    fn walk_tensor(&mut self, expr: &HirExpr) -> CodegenResult<Walked<'ctx>> {
        let ty = Type::from_hir(&expr.ty);
        let Type::Tensor { element, .. } = ty.referent().clone() else {
            return Err(CodegenError::InternalError(
                "a tensor traversal walks a non-tensor operand".to_string(),
            ));
        };
        let elem_llvm = self.get_any_llvm_type(&element)?;
        let handle = self.tensor_receiver_handle(expr, &ty)?;
        let data = self.load_dlpack_data(handle)?;
        Ok(Walked {
            handle,
            data,
            elem_llvm,
        })
    }

    /// How many elements one traversal steps over, which is the receiver's whole buffer.
    fn element_count(&self, receiver: &HirExpr) -> CodegenResult<usize> {
        let Type::Tensor { shape, .. } = Type::from_hir(&receiver.ty).referent().clone() else {
            return Err(CodegenError::InternalError(
                "a tensor traversal does not carry a tensor receiver".to_string(),
            ));
        };
        Ok(crate::types::static_extents(&shape)?.iter().product())
    }

    fn load_walked(
        &self,
        walked: &Walked<'ctx>,
        index: IntValue<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let slot = self.buffer_slot(walked.elem_llvm, walked.data, index)?;
        self.builder
            .build_load(walked.elem_llvm, slot, "tensor.apply.elem")
            .map_err(CodegenError::from)
    }

    /// The address of one element of a tensor buffer, `index` elements in.
    fn buffer_slot(
        &self,
        elem_llvm: inkwell::types::BasicTypeEnum<'ctx>,
        buffer: PointerValue<'ctx>,
        index: IntValue<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        // SAFETY: the loop guard holds `index` below the receiver's element count, and
        // every buffer walked at that index was laid out from the same shape — the
        // checker rejects a `.zip` whose operand's extents differ.
        unsafe {
            self.builder
                .build_in_bounds_gep(elem_llvm, buffer, &[index], "tensor.apply.ptr")
                .map_err(CodegenError::from)
        }
    }
}
