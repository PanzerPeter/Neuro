// Codegen for tensor arithmetic: the by-value and borrowed binary operators, scalar
// broadcast, the `@` contraction, and the in-place compound assignment.
//
// Split from `tensors.rs`, which BUILDS and re-describes tensor values. Everything here
// reads buffers that already exist and writes a result, so the two files share the
// allocation helpers rather than each other's control flow.
//
// Broadcasting is arithmetic rather than a second loop shape: a stretched axis carries
// stride 0, so the same source element is read for every position along it. The compound
// assignment allocates nothing at all, which is the guarantee the language makes about a
// tensor's handle across an in-place update.

use ast_types::BinaryOp;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use inkwell::IntPredicate;
use neuro_hir::HirExpr;

use super::row_major_strides;
use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// A tensor operand as the emitted code reaches it: the DLPack handle that IS the tensor
/// value, the element buffer it addresses, and whether the operation consumes it.
struct TensorBuffer<'ctx> {
    handle: PointerValue<'ctx>,
    data: PointerValue<'ctx>,
    owned: bool,
}

/// One operand of an element-wise tensor operation, resolved to what the loop body reads.
enum ElementSource<'ctx> {
    /// A tensor buffer, plus the flat stride to advance per axis of the RESULT. A stride
    /// of zero is a stretched axis: every result coordinate along it reads the same
    /// element, which is the whole of broadcasting.
    Buffer {
        handle: PointerValue<'ctx>,
        data: PointerValue<'ctx>,
        strides: Vec<usize>,
        /// Whether the source has the result's own shape, so its slot index is the loop
        /// counter and no coordinate arithmetic is needed.
        contiguous: bool,
        /// Whether the operand is consumed, and so its buffer released once the result is
        /// built. A borrowed operand is only read.
        owned: bool,
    },
    /// A scalar broadcast across every element of the result.
    Scalar(BasicValueEnum<'ctx>),
}

/// The destination and extents of one matrix-product loop, bundled because emitting it
/// needs every one of them and they are meaningless apart.
struct Contraction<'a, 'ctx> {
    /// The destination's element count, `M * N`: the flat loop's trip count.
    count: usize,
    /// `N`, the destination's row length, which recovers a row and a column from the
    /// flat counter and is also the right operand's own row length.
    columns: usize,
    /// `K`, the axis read away by the product.
    contracted: usize,
    buffer_ty: BasicTypeEnum<'ctx>,
    elem_llvm: BasicTypeEnum<'ctx>,
    element_ty: &'a Type,
    destination: PointerValue<'ctx>,
    /// The source position keying the diagnostic of any run-time guard the element
    /// arithmetic needs.
    offset: usize,
}

/// The destination and shape of one element-wise loop, bundled because emitting it needs
/// every one of them and they are meaningless apart.
struct ElementwiseLoop<'a, 'ctx> {
    result_shape: &'a [usize],
    count: usize,
    buffer_ty: BasicTypeEnum<'ctx>,
    elem_llvm: BasicTypeEnum<'ctx>,
    element_ty: &'a Type,
    op: BinaryOp,
    destination: PointerValue<'ctx>,
    /// The source position keying the diagnostic of any run-time guard the element
    /// arithmetic needs.
    offset: usize,
    /// The prefix every block and value name in the loop carries, naming the node the
    /// loop was emitted for.
    name: &'a str,
}

/// The flat stride an operand advances by per step along each axis of the result.
///
/// Zero where the operand is stretched: an axis it does not have at all (a lower-rank
/// operand aligns at the trailing end), or one whose extent is 1 against a wider result.
fn broadcast_strides(result_shape: &[usize], operand_shape: &[usize]) -> Vec<usize> {
    let own = row_major_strides(operand_shape);
    let rank = result_shape.len();
    (0..rank)
        .map(|axis| {
            let depth = rank - 1 - axis;
            match operand_shape.len().checked_sub(depth + 1) {
                Some(source) if operand_shape[source] == result_shape[axis] => own[source],
                _ => 0,
            }
        })
        .collect()
}

impl<'ctx> CodegenContext<'ctx> {
    /// Lower `target OP= value` on a tensor: an element-wise update written straight
    /// into the buffer `target`'s handle already addresses.
    ///
    /// Nothing is allocated. That is the point of the node rather than an optimization
    /// of it: the DLPack handle and its `data` pointer are unchanged across the
    /// statement, so a raw pointer held by an optimizer, by the runtime, or by a foreign
    /// consumer stays valid. The desugaring this node exists to avoid would build a second
    /// tensor and rebind the name to it, invalidating both.
    ///
    /// The evaluation order is fixed by the language: the right-hand side is evaluated
    /// before the target is touched.
    ///
    /// The right-hand side broadcasts under the by-value operator's rule, with the
    /// one asymmetry the in-place write forces: it may be stretched UP to the target's
    /// shape and no further, because the result goes back into the target's own buffer.
    pub(crate) fn codegen_tensor_compound_assign(
        &mut self,
        receiver: &HirExpr,
        op: BinaryOp,
        value: &HirExpr,
        ty: &neuro_hir::HirType,
        offset: usize,
    ) -> CodegenResult<()> {
        let tensor_ty = Type::from_hir(ty);
        let (element_ty, count) = self.tensor_layout(&tensor_ty)?;
        let Type::Tensor { shape, .. } = &tensor_ty else {
            return Err(CodegenError::InternalError(
                "a tensor compound assignment does not carry a tensor type".to_string(),
            ));
        };
        let result_shape = crate::types::static_extents(shape)?;

        let rhs = self.codegen_operand_source(value, &result_shape)?;

        let lhs_handle = self.tensor_receiver_handle(receiver, &Type::from_hir(&receiver.ty))?;
        let lhs_data = self.load_dlpack_data(lhs_handle)?;
        // The target is both the left operand and the destination, so it is walked slot
        // for slot: it is the shape everything else broadcasts to.
        let lhs = ElementSource::Buffer {
            handle: lhs_handle,
            data: lhs_data,
            strides: broadcast_strides(&result_shape, &result_shape),
            contiguous: true,
            owned: false,
        };

        let buffer_ty = self.tensor_buffer_type(&tensor_ty)?;
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        self.emit_elementwise_loop(
            ElementwiseLoop {
                result_shape: &result_shape,
                count,
                buffer_ty,
                elem_llvm,
                element_ty: &element_ty,
                op,
                destination: lhs_data,
                offset,
                name: "tensor.op",
            },
            &lhs,
            &rhs,
        )?;

        // An owned operand is consumed by the update, so its buffer is released here:
        // it has no binding left to free it at scope exit.
        self.release_consumed_operand(value, &rhs)
    }

    /// Lower a by-value tensor operator: `a + b`, `&a + &b`, and the scalar broadcast
    ///
    /// Unlike the compound assignment above, this ALLOCATES: the operator's contract is a
    /// fresh tensor, which is what lets it read two borrows and what makes `w = w + g`
    /// observably different from `w -= g`. An owned operand is consumed, so its buffer is
    /// released once the result is built; a borrowed one is only read.
    pub(crate) fn codegen_tensor_binary(
        &mut self,
        left: &HirExpr,
        op: BinaryOp,
        right: &HirExpr,
        result_ty: &Type,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        // `@` contracts an axis instead of walking the result element for element, so it
        // is a different loop shape rather than a different body.
        if matches!(op, BinaryOp::MatMul) {
            return self.codegen_tensor_matmul(left, right, result_ty, offset);
        }
        let (element_ty, count) = self.tensor_layout(result_ty)?;
        let Type::Tensor { shape, .. } = result_ty else {
            return Err(CodegenError::InternalError(
                "a tensor operator does not carry a tensor result type".to_string(),
            ));
        };
        let result_shape = crate::types::static_extents(shape)?;

        // Left to right: the operands are two ordinary expressions and the language
        // evaluates them in source order.
        let lhs = self.codegen_operand_source(left, &result_shape)?;
        let rhs = self.codegen_operand_source(right, &result_shape)?;

        let (handle, data) = self.alloc_tensor(result_ty, "tensor.bin")?;
        let buffer_ty = self.tensor_buffer_type(result_ty)?;
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        self.emit_elementwise_loop(
            ElementwiseLoop {
                result_shape: &result_shape,
                count,
                buffer_ty,
                elem_llvm,
                element_ty: &element_ty,
                op,
                destination: data,
                offset,
                name: "tensor.bin",
            },
            &lhs,
            &rhs,
        )?;

        self.release_consumed_operand(left, &lhs)?;
        self.release_consumed_operand(right, &rhs)?;
        Ok(handle.into())
    }

    /// Lower `a @ b`: the matrix product of an `[M, K]` and a `[K, N]` into a fresh
    /// `[M, N]` tensor.
    ///
    /// The destination is walked flat, one iteration per output element, and the
    /// contraction is the inner loop over K — two loops rather than three, since the
    /// row and column are recovered from the flat counter the destination already needs.
    /// The accumulator lives in an entry `alloca` because the element arithmetic may
    /// split the body around an overflow guard, which a `phi` over the loop would then
    /// have to chase.
    fn codegen_tensor_matmul(
        &mut self,
        left: &HirExpr,
        right: &HirExpr,
        result_ty: &Type,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let (element_ty, count) = self.tensor_layout(result_ty)?;
        let Type::Tensor { shape, .. } = result_ty else {
            return Err(CodegenError::InternalError(
                "a tensor operator does not carry a tensor result type".to_string(),
            ));
        };
        let [_rows, columns] = crate::types::static_extents(shape)?[..] else {
            return Err(CodegenError::InternalError(
                "`@` produces a rank-2 tensor".to_string(),
            ));
        };

        // Left to right: the operands are two ordinary expressions and the language
        // evaluates them in source order.
        let Some((lhs, lhs_shape)) = self.codegen_tensor_operand(left)? else {
            return Err(CodegenError::InternalError(
                "`@` takes two tensor operands".to_string(),
            ));
        };
        let Some((rhs, _)) = self.codegen_tensor_operand(right)? else {
            return Err(CodegenError::InternalError(
                "`@` takes two tensor operands".to_string(),
            ));
        };
        // The contracted extent is the left operand's trailing axis; the checker has
        // already established the right operand's leading axis equals it.
        let [_, contracted] = lhs_shape[..] else {
            return Err(CodegenError::InternalError(
                "`@` takes two rank-2 tensor operands".to_string(),
            ));
        };

        let (handle, destination) = self.alloc_tensor(result_ty, "tensor.mm")?;
        let buffer_ty = self.tensor_buffer_type(result_ty)?;
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        self.emit_contraction_loop(
            Contraction {
                count,
                columns,
                contracted,
                buffer_ty,
                elem_llvm,
                element_ty: &element_ty,
                destination,
                offset,
            },
            &lhs,
            &rhs,
        )?;

        self.release_consumed_buffer(left, &lhs)?;
        self.release_consumed_buffer(right, &rhs)?;
        Ok(handle.into())
    }

    /// Walk every slot of the destination, accumulating the dot product of the left
    /// operand's row and the right operand's column that meet there.
    fn emit_contraction_loop(
        &mut self,
        spec: Contraction<'_, 'ctx>,
        lhs: &TensorBuffer<'ctx>,
        rhs: &TensorBuffer<'ctx>,
    ) -> CodegenResult<()> {
        let Contraction {
            count,
            columns,
            contracted,
            buffer_ty,
            elem_llvm,
            element_ty,
            destination,
            offset,
        } = spec;
        let i64_type = self.context.i64_type();
        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("a tensor operation outside a function".to_string())
        })?;

        let slot_index = self.entry_alloca(i64_type, "tensor.mm.i")?;
        let step = self.entry_alloca(i64_type, "tensor.mm.k")?;
        let total = self.entry_alloca(elem_llvm, "tensor.mm.acc")?;
        self.builder
            .build_store(slot_index, i64_type.const_zero())?;

        let head = self.context.append_basic_block(function, "tensor.mm.head");
        let body = self.context.append_basic_block(function, "tensor.mm.body");
        let inner_head = self
            .context
            .append_basic_block(function, "tensor.mm.inner.head");
        let inner_body = self
            .context
            .append_basic_block(function, "tensor.mm.inner.body");
        let store = self.context.append_basic_block(function, "tensor.mm.store");
        let done = self.context.append_basic_block(function, "tensor.mm.done");

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let i = self
            .builder
            .build_load(i64_type, slot_index, "tensor.mm.idx")?
            .into_int_value();
        let more = self.builder.build_int_compare(
            IntPredicate::ULT,
            i,
            i64_type.const_int(count as u64, false),
            "tensor.mm.more",
        )?;
        self.builder.build_conditional_branch(more, body, done)?;

        // The destination is row-major, so its flat counter carries both coordinates: the
        // row selects the left operand's row, the column the right operand's column.
        self.builder.position_at_end(body);
        let width = i64_type.const_int(columns as u64, false);
        let row = self
            .builder
            .build_int_unsigned_div(i, width, "tensor.mm.row")?;
        let column = self
            .builder
            .build_int_unsigned_rem(i, width, "tensor.mm.col")?;
        self.builder.build_store(total, elem_llvm.const_zero())?;
        self.builder.build_store(step, i64_type.const_zero())?;
        self.builder.build_unconditional_branch(inner_head)?;

        self.builder.position_at_end(inner_head);
        let k = self
            .builder
            .build_load(i64_type, step, "tensor.mm.step")?
            .into_int_value();
        let stepping = self.builder.build_int_compare(
            IntPredicate::ULT,
            k,
            i64_type.const_int(contracted as u64, false),
            "tensor.mm.stepping",
        )?;
        self.builder
            .build_conditional_branch(stepping, inner_body, store)?;

        self.builder.position_at_end(inner_body);
        // `row` and `k` are below their own extents on this edge, so both addresses stay
        // inside the operand buffers the checker sized them against.
        let left_at = self.builder.build_int_add(
            self.builder.build_int_mul(
                row,
                i64_type.const_int(contracted as u64, false),
                "tensor.mm.lrow",
            )?,
            k,
            "tensor.mm.lhs",
        )?;
        let right_at = self.builder.build_int_add(
            self.builder.build_int_mul(k, width, "tensor.mm.rrow")?,
            column,
            "tensor.mm.rhs",
        )?;
        let a = self.load_buffer_element(lhs.data, left_at, buffer_ty, elem_llvm, "tensor.mm.a")?;
        let b =
            self.load_buffer_element(rhs.data, right_at, buffer_ty, elem_llvm, "tensor.mm.b")?;
        let product = self.tensor_element_arith(BinaryOp::Multiply, a, b, element_ty, offset)?;
        let running = self
            .builder
            .build_load(elem_llvm, total, "tensor.mm.running")?;
        let summed =
            self.tensor_element_arith(BinaryOp::Add, running, product, element_ty, offset)?;
        self.builder.build_store(total, summed)?;
        let next_step =
            self.builder
                .build_int_add(k, i64_type.const_int(1, false), "tensor.mm.next.k")?;
        self.builder.build_store(step, next_step)?;
        // The element arithmetic may have split the body around an overflow guard, so the
        // back edge leaves whichever block is current now.
        self.builder.build_unconditional_branch(inner_head)?;

        self.builder.position_at_end(store);
        let value = self.builder.build_load(elem_llvm, total, "tensor.mm.sum")?;
        let slot = self.tensor_slot(buffer_ty, destination, i)?;
        self.builder.build_store(slot, value)?;
        let next = self
            .builder
            .build_int_add(i, i64_type.const_int(1, false), "tensor.mm.next")?;
        self.builder.build_store(slot_index, next)?;
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(done);
        Ok(())
    }

    /// One element read out of a tensor buffer at a flat index its caller has bounded.
    fn load_buffer_element(
        &self,
        buffer: PointerValue<'ctx>,
        index: IntValue<'ctx>,
        buffer_ty: BasicTypeEnum<'ctx>,
        elem_llvm: BasicTypeEnum<'ctx>,
        name: &str,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let slot = self.tensor_slot(buffer_ty, buffer, index)?;
        self.builder
            .build_load(elem_llvm, slot, name)
            .map_err(CodegenError::from)
    }

    /// Resolve one operand of an element-wise tensor operation to what the loop body
    /// reads from it: a buffer walked at broadcast strides, or a scalar.
    fn codegen_operand_source(
        &mut self,
        operand: &HirExpr,
        result_shape: &[usize],
    ) -> CodegenResult<ElementSource<'ctx>> {
        let Some((buffer, operand_shape)) = self.codegen_tensor_operand(operand)? else {
            return Ok(ElementSource::Scalar(self.codegen_expr(operand)?));
        };
        Ok(ElementSource::Buffer {
            handle: buffer.handle,
            data: buffer.data,
            strides: broadcast_strides(result_shape, &operand_shape),
            contiguous: operand_shape == result_shape,
            owned: buffer.owned,
        })
    }

    /// Lower one operand of a tensor operation to its handle, its element buffer and its
    /// own extents, or `None` when the operand is not a tensor at all.
    ///
    /// A scalar operand is deliberately NOT lowered here: answering `None` before emitting
    /// anything lets the caller decide what a non-tensor operand means for its operation,
    /// which is a broadcast for the element-wise family and an error for `@`.
    fn codegen_tensor_operand(
        &mut self,
        operand: &HirExpr,
    ) -> CodegenResult<Option<(TensorBuffer<'ctx>, Vec<usize>)>> {
        let operand_ty = Type::from_hir(&operand.ty);
        let Type::Tensor { shape, .. } = operand_ty.referent() else {
            return Ok(None);
        };
        let operand_shape = crate::types::static_extents(shape)?;
        let BasicValueEnum::PointerValue(ptr) = self.codegen_expr(operand)? else {
            return Err(CodegenError::InternalError(
                "a tensor operand does not lower to a pointer".to_string(),
            ));
        };
        // A borrowed operand lowers to the address of the handle pointer, an owned one to
        // the handle pointer itself; both are `ptr`, so the type is what tells them apart.
        let owned = !matches!(operand_ty, Type::Reference { .. });
        let handle = if owned {
            ptr
        } else {
            self.builder
                .build_load(
                    self.context.ptr_type(inkwell::AddressSpace::default()),
                    ptr,
                    "tensor.operand",
                )?
                .into_pointer_value()
        };
        Ok(Some((
            TensorBuffer {
                data: self.load_dlpack_data(handle)?,
                handle,
                owned,
            },
            operand_shape,
        )))
    }

    /// Release the buffer of an operand the operation consumed. A borrowed operand and a
    /// scalar own nothing, so both are left alone.
    fn release_consumed_operand(
        &mut self,
        operand: &HirExpr,
        source: &ElementSource<'ctx>,
    ) -> CodegenResult<()> {
        let ElementSource::Buffer {
            handle,
            owned: true,
            ..
        } = source
        else {
            return Ok(());
        };
        self.mark_moved_for_drop(operand);
        self.build_dlpack_release(*handle)
    }

    /// Release the buffer of a matmul operand the operator consumed.
    fn release_consumed_buffer(
        &mut self,
        operand: &HirExpr,
        buffer: &TensorBuffer<'ctx>,
    ) -> CodegenResult<()> {
        if !buffer.owned {
            return Ok(());
        }
        self.mark_moved_for_drop(operand);
        self.build_dlpack_release(buffer.handle)
    }

    /// Walk every slot of the destination buffer, applying `op` to the two sources at the
    /// broadcast position each one reads.
    ///
    /// The destination is written contiguously, so the loop counter IS its slot index; a
    /// source is addressed by decomposing that counter into result coordinates and
    /// recombining them at the source's own strides. A stretched axis carries stride 0,
    /// which is what makes broadcasting a matter of arithmetic rather than a second loop
    /// shape.
    fn emit_elementwise_loop(
        &mut self,
        loop_spec: ElementwiseLoop<'_, 'ctx>,
        lhs: &ElementSource<'ctx>,
        rhs: &ElementSource<'ctx>,
    ) -> CodegenResult<()> {
        let ElementwiseLoop {
            result_shape,
            count,
            buffer_ty,
            elem_llvm,
            element_ty,
            op,
            destination,
            offset,
            name,
        } = loop_spec;
        let i64_type = self.context.i64_type();
        let result_strides = row_major_strides(result_shape);
        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("a tensor operation outside a function".to_string())
        })?;
        let index = self.entry_alloca(i64_type, &format!("{name}.i"))?;
        self.builder.build_store(index, i64_type.const_zero())?;
        let head = self
            .context
            .append_basic_block(function, &format!("{name}.head"));
        let body = self
            .context
            .append_basic_block(function, &format!("{name}.body"));
        let done = self
            .context
            .append_basic_block(function, &format!("{name}.done"));

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let i = self
            .builder
            .build_load(i64_type, index, &format!("{name}.idx"))?
            .into_int_value();
        let more = self.builder.build_int_compare(
            IntPredicate::ULT,
            i,
            i64_type.const_int(count as u64, false),
            &format!("{name}.more"),
        )?;
        self.builder.build_conditional_branch(more, body, done)?;

        self.builder.position_at_end(body);
        // Computed once and shared by both sources: a rank-2 result decomposes its
        // counter twice, not once per operand.
        let stretched = |source: &ElementSource<'ctx>| {
            matches!(
                source,
                ElementSource::Buffer {
                    contiguous: false,
                    ..
                }
            )
        };
        let coords = if stretched(lhs) || stretched(rhs) {
            self.result_coordinates(i, result_shape, &result_strides, name)?
        } else {
            Vec::new()
        };

        let lhs_elem = self.load_source_element(lhs, i, &coords, buffer_ty, elem_llvm, name)?;
        let rhs_elem = self.load_source_element(rhs, i, &coords, buffer_ty, elem_llvm, name)?;
        let updated = self.tensor_element_arith(op, lhs_elem, rhs_elem, element_ty, offset)?;
        // `i` is below `count` on this edge, because the head's `ULT` test is what
        // branches here, so the slot stays inside the destination buffer.
        let slot = self.tensor_slot(buffer_ty, destination, i)?;
        self.builder.build_store(slot, updated)?;

        let next =
            self.builder
                .build_int_add(i, i64_type.const_int(1, false), &format!("{name}.next"))?;
        self.builder.build_store(index, next)?;
        // The element arithmetic may have split the body around an overflow or
        // divide-by-zero guard, so the back edge leaves whichever block is current now.
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(done);
        Ok(())
    }

    /// Decompose a flat result index into one coordinate per result axis.
    fn result_coordinates(
        &self,
        index: IntValue<'ctx>,
        result_shape: &[usize],
        result_strides: &[usize],
        name: &str,
    ) -> CodegenResult<Vec<IntValue<'ctx>>> {
        let i64_type = self.context.i64_type();
        result_shape
            .iter()
            .enumerate()
            .map(|(axis, extent)| {
                let scaled = self.builder.build_int_unsigned_div(
                    index,
                    i64_type.const_int(result_strides[axis] as u64, false),
                    &format!("{name}.div"),
                )?;
                self.builder
                    .build_int_unsigned_rem(
                        scaled,
                        i64_type.const_int(*extent as u64, false),
                        &format!("{name}.coord"),
                    )
                    .map_err(CodegenError::from)
            })
            .collect()
    }

    /// The element one source contributes at the current result position.
    fn load_source_element(
        &mut self,
        source: &ElementSource<'ctx>,
        index: IntValue<'ctx>,
        coords: &[IntValue<'ctx>],
        buffer_ty: BasicTypeEnum<'ctx>,
        elem_llvm: BasicTypeEnum<'ctx>,
        name: &str,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let ElementSource::Buffer {
            data,
            strides,
            contiguous,
            ..
        } = source
        else {
            let ElementSource::Scalar(value) = source else {
                return Err(CodegenError::InternalError(
                    "an element source is a buffer or a scalar".to_string(),
                ));
            };
            return Ok(*value);
        };
        let i64_type = self.context.i64_type();
        // A source of the result's own shape is walked slot for slot, which is both the
        // common case and the one where the coordinate arithmetic buys nothing.
        let source_index = if *contiguous {
            index
        } else {
            let mut address = i64_type.const_zero();
            for (coord, stride) in coords.iter().zip(strides) {
                let scaled = self.builder.build_int_mul(
                    *coord,
                    i64_type.const_int(*stride as u64, false),
                    &format!("{name}.scaled"),
                )?;
                address = self
                    .builder
                    .build_int_add(address, scaled, &format!("{name}.src"))?;
            }
            address
        };
        // Every stride above is the source's own and is zero on an axis the source does
        // not walk, so the address stays inside the source's own buffer.
        let slot = self.tensor_slot(buffer_ty, *data, source_index)?;
        self.builder
            .build_load(elem_llvm, slot, &format!("{name}.elem"))
            .map_err(CodegenError::from)
    }

    /// One element of a tensor compound assignment, with the same guards the scalar
    /// operator carries: a tensor's arithmetic is its element's arithmetic, so an
    /// overflowing element panics exactly where an overflowing scalar would.
    fn tensor_element_arith(
        &mut self,
        op: BinaryOp,
        lhs: BasicValueEnum<'ctx>,
        rhs: BasicValueEnum<'ctx>,
        element_ty: &Type,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        if let (BasicValueEnum::FloatValue(a), BasicValueEnum::FloatValue(b)) = (lhs, rhs) {
            let value = match op {
                BinaryOp::Add => self.builder.build_float_add(a, b, "tensor.op.add"),
                BinaryOp::Subtract => self.builder.build_float_sub(a, b, "tensor.op.sub"),
                BinaryOp::Multiply => self.builder.build_float_mul(a, b, "tensor.op.mul"),
                BinaryOp::Divide => self.builder.build_float_div(a, b, "tensor.op.div"),
                BinaryOp::Modulo => self.builder.build_float_rem(a, b, "tensor.op.rem"),
                _ => {
                    return Err(CodegenError::InternalError(
                        "a compound assignment carries an arithmetic operator".to_string(),
                    ))
                }
            };
            return Ok(value?.into());
        }
        let (BasicValueEnum::IntValue(a), BasicValueEnum::IntValue(b)) = (lhs, rhs) else {
            return Err(CodegenError::InternalError(
                "a tensor element is an integer or a float".to_string(),
            ));
        };
        let unsigned = crate::type_mapping::TypeMapper::is_unsigned_int(element_ty);
        let value = match op {
            BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply => {
                self.codegen_int_arith(op, a, b, unsigned, offset, "tensor.op.arith")?
            }
            BinaryOp::Divide | BinaryOp::Modulo => {
                self.codegen_int_div_rem(op, a, b, unsigned, offset, "tensor.op.divrem")?
            }
            _ => {
                return Err(CodegenError::InternalError(
                    "a compound assignment carries an arithmetic operator".to_string(),
                ))
            }
        };
        Ok(value.into())
    }
}
