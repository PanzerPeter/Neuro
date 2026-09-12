// Codegen for tensor indexing and slicing `t[i, j]` / `t[1..3, ..]`.
//
// A tensor's elements sit in one flat, row-major run behind its DLPack handle's `data`
// pointer, so an index is arithmetic on that run: the position given to axis `k`
// contributes `position * stride[k]`, where `stride[k]` is the product of the extents
// below it. Every stride is a compile-time constant, because every extent is part of
// the type.
//
// Reading one element is therefore one `getelementptr` and one `load`. A slice is a
// COPY into a fresh tensor rather than a view onto the source: a tensor owns its buffer
// and releases it through its own DLPack deleter, so two values sharing one buffer would
// be a double free. It also keeps the DLPack field contract — contiguous row-major
// `strides`, `byte_offset` of zero — true of every tensor value.

use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use inkwell::IntPredicate;
use neuro_hir::{HirExpr, HirTensorAxis};

use super::row_major_strides;
use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// What an out-of-range position reports. A slice's bounds are constants the type
/// checker has already rejected, so only a position can fail here.
const INDEX_OUT_OF_BOUNDS: &str = "tensor index out of bounds";

impl<'ctx> CodegenContext<'ctx> {
    /// Lower `object[a0, a1, ...]`: a load when every axis is a position, a fresh
    /// tensor when at least one survives.
    ///
    /// `result_ty` is the expression's own type, which is what says which of the two
    /// this is — the checker decided it by dropping every axis given a position.
    pub(crate) fn codegen_tensor_index(
        &mut self,
        object: &HirExpr,
        axes: &[HirTensorAxis],
        result_ty: &Type,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let source_ty = Type::from_hir(&object.ty);
        let Type::Tensor { element, shape } = source_ty.referent().clone() else {
            return Err(CodegenError::InternalError(
                "a tensor index node does not carry a tensor receiver".to_string(),
            ));
        };
        let shape = crate::types::static_extents(&shape)?;
        let data = self.tensor_index_data(object, &source_ty)?;
        let strides = row_major_strides(&shape);
        let base = self.tensor_index_base(axes, &shape, &strides, offset)?;
        let elem_llvm = self.get_any_llvm_type(&element)?;

        let Type::Tensor {
            shape: result_shape,
            ..
        } = result_ty
        else {
            let slot = self.tensor_element_ptr(elem_llvm, data, base)?;
            return self
                .builder
                .build_load(elem_llvm, slot, "tensor.elem")
                .map_err(CodegenError::from);
        };
        let result_shape = crate::types::static_extents(result_shape)?;
        self.copy_tensor_slice(result_ty, &result_shape, axes, &strides, data, base)
    }

    /// The element buffer of the indexed tensor. A borrowed receiver lowers to the
    /// address of the handle pointer, an owned one to the handle pointer itself.
    pub(super) fn tensor_index_data(
        &mut self,
        object: &HirExpr,
        source_ty: &Type,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let value = self.codegen_expr(object)?;
        let BasicValueEnum::PointerValue(ptr) = value else {
            return Err(CodegenError::InternalError(
                "a tensor receiver does not lower to a pointer".to_string(),
            ));
        };
        let handle = if matches!(source_ty, Type::Reference { .. }) {
            self.builder
                .build_load(
                    self.context.ptr_type(inkwell::AddressSpace::default()),
                    ptr,
                    "tensor.index.recv",
                )?
                .into_pointer_value()
        } else {
            ptr
        };
        self.load_dlpack_data(handle)
    }

    /// The flat element offset the index starts at: every position's contribution plus
    /// the first element of every surviving range.
    fn tensor_index_base(
        &mut self,
        axes: &[HirTensorAxis],
        shape: &[usize],
        strides: &[usize],
        offset: usize,
    ) -> CodegenResult<IntValue<'ctx>> {
        let i64_type = self.context.i64_type();
        let mut base = i64_type.const_zero();
        for (axis, index) in axes.iter().enumerate() {
            let stride = i64_type.const_int(strides[axis] as u64, false);
            let contribution = match index {
                HirTensorAxis::Range { start, .. } => {
                    i64_type.const_int((start * strides[axis]) as u64, false)
                }
                HirTensorAxis::Position(expr) => {
                    let position = self.codegen_expr(expr)?.into_int_value();
                    let position = self.widen_index_to_i64(position, &Type::from_hir(&expr.ty))?;
                    self.guard_tensor_position(position, shape[axis], offset)?;
                    self.builder
                        .build_int_mul(position, stride, "tensor.index.axis")?
                }
            };
            base = self
                .builder
                .build_int_add(base, contribution, "tensor.index.base")?;
        }
        Ok(base)
    }

    /// Trap a position outside its axis on the debug tier, where an array index's
    /// bounds check also lives. A negative signed index sign-extends to a large
    /// unsigned value and so fails the same unsigned test.
    fn guard_tensor_position(
        &mut self,
        position: IntValue<'ctx>,
        extent: usize,
        offset: usize,
    ) -> CodegenResult<()> {
        if !self.overflow_checks {
            return Ok(());
        }
        let extent = self.context.i64_type().const_int(extent as u64, false);
        let ok = self.builder.build_int_compare(
            IntPredicate::ULT,
            position,
            extent,
            "tensor.index.ok",
        )?;
        self.codegen_guard_or_panic(ok, INDEX_OUT_OF_BOUNDS, offset)
    }

    /// Build the sliced tensor: one counted loop over the result's own elements, each
    /// mapped back to the source element it copies.
    ///
    /// The loop walks the RESULT rather than the source because the result is
    /// contiguous — its linear index is the buffer index — while the source positions
    /// it reads are strided and, for an inner axis, not even monotonic in one run.
    fn copy_tensor_slice(
        &mut self,
        result_ty: &Type,
        result_shape: &[usize],
        axes: &[HirTensorAxis],
        strides: &[usize],
        source: PointerValue<'ctx>,
        base: IntValue<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let handle = self.alloc_dlpack_tensor(result_ty, "tensor.slice")?;
        let target = self.load_dlpack_data(handle)?;
        let Type::Tensor { element, .. } = result_ty else {
            return Err(CodegenError::InternalError(
                "a tensor slice does not carry a tensor type".to_string(),
            ));
        };
        let elem_llvm = self.get_any_llvm_type(element)?;

        // The surviving axes, paired with the source stride each one steps by.
        let kept: Vec<usize> = axes
            .iter()
            .enumerate()
            .filter(|(_, index)| matches!(index, HirTensorAxis::Range { .. }))
            .map(|(axis, _)| strides[axis])
            .collect();
        let result_strides = row_major_strides(result_shape);
        let count: usize = result_shape.iter().product();

        let i64_type = self.context.i64_type();
        let cursor = self.entry_alloca(i64_type, "tensor.slice.i")?;
        self.builder.build_store(cursor, i64_type.const_zero())?;
        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("a tensor slice outside a function".to_string())
        })?;
        let head = self
            .context
            .append_basic_block(function, "tensor.slice.head");
        let body = self
            .context
            .append_basic_block(function, "tensor.slice.body");
        let done = self
            .context
            .append_basic_block(function, "tensor.slice.done");

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let i = self
            .builder
            .build_load(i64_type, cursor, "tensor.slice.idx")?
            .into_int_value();
        let more = self.builder.build_int_compare(
            IntPredicate::ULT,
            i,
            i64_type.const_int(count as u64, false),
            "tensor.slice.more",
        )?;
        self.builder.build_conditional_branch(more, body, done)?;

        self.builder.position_at_end(body);
        let mut source_index = base;
        for (position, (extent, source_stride)) in result_shape.iter().zip(kept.iter()).enumerate()
        {
            let coordinate = self.slice_coordinate(i, result_strides[position], *extent)?;
            let stepped = self.builder.build_int_mul(
                coordinate,
                i64_type.const_int(*source_stride as u64, false),
                "tensor.slice.step",
            )?;
            source_index = self
                .builder
                .build_int_add(source_index, stepped, "tensor.slice.src")?;
        }
        let read = self.tensor_element_ptr(elem_llvm, source, source_index)?;
        let value = self
            .builder
            .build_load(elem_llvm, read, "tensor.slice.value")?;
        let write = self.tensor_element_ptr(elem_llvm, target, i)?;
        self.builder.build_store(write, value)?;
        let next =
            self.builder
                .build_int_add(i, i64_type.const_int(1, false), "tensor.slice.next")?;
        self.builder.build_store(cursor, next)?;
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(done);
        Ok(handle.into())
    }

    /// The coordinate one surviving axis holds at the result's linear index `i`.
    ///
    /// Both divisors are compile-time constants, so this is the standard
    /// `(i / stride) % extent` decomposition and LLVM lowers each division to a
    /// multiply and a shift.
    fn slice_coordinate(
        &self,
        i: IntValue<'ctx>,
        result_stride: usize,
        extent: usize,
    ) -> CodegenResult<IntValue<'ctx>> {
        let i64_type = self.context.i64_type();
        let divided = self.builder.build_int_unsigned_div(
            i,
            i64_type.const_int(result_stride as u64, false),
            "tensor.slice.div",
        )?;
        self.builder
            .build_int_unsigned_rem(
                divided,
                i64_type.const_int(extent as u64, false),
                "tensor.slice.coord",
            )
            .map_err(CodegenError::from)
    }

    /// The address of one element of a tensor buffer, `index` elements in.
    fn tensor_element_ptr(
        &self,
        elem_llvm: inkwell::types::BasicTypeEnum<'ctx>,
        buffer: PointerValue<'ctx>,
        index: IntValue<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        // SAFETY: every position is bounds-guarded on the debug tier and every range was
        // proved to lie inside its axis at compile time, so the offset stays inside the
        // buffer under the same policy an array index carries.
        unsafe {
            self.builder
                .build_in_bounds_gep(elem_llvm, buffer, &[index], "tensor.index.ptr")
                .map_err(CodegenError::from)
        }
    }
}
