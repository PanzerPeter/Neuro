// Codegen for one tensor element `t[i, j]`, read or written. A slice is computed in MLIR.
//
// A tensor's elements sit in one flat, row-major run behind its DLPack handle's `data`
// pointer, so an index is arithmetic on that run: the position given to axis `k`
// contributes `position * stride[k]`, where `stride[k]` is the product of the extents
// below it. Every stride is a compile-time constant, because every extent is part of
// the type. Reading one element is therefore one `getelementptr` and one `load`.

use inkwell::IntPredicate;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use neuro_hir::{HirExpr, HirTensorAxis};

use super::row_major_strides;
use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// What an out-of-range position reports. A slice's bounds are constants the type
/// checker has already rejected, so only a position can fail here.
const INDEX_OUT_OF_BOUNDS: &str = "tensor index out of bounds";

impl<'ctx> CodegenContext<'ctx> {
    /// Lower `object[i0, i1, ...]` with every axis given a position: one element's load.
    pub(crate) fn codegen_tensor_element(
        &mut self,
        object: &HirExpr,
        axes: &[HirTensorAxis],
        result_ty: &Type,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let source_ty = Type::from_hir(&object.ty);
        let Type::Tensor { shape, .. } = source_ty.referent().clone() else {
            return Err(CodegenError::InternalError(
                "a tensor index node does not carry a tensor receiver".to_string(),
            ));
        };
        let shape = crate::types::static_extents(&shape)?;
        let handle = self.tensor_receiver_handle(object, &source_ty)?;
        let strides = row_major_strides(&shape);
        let base = self.tensor_index_base(axes, &shape, &strides, offset)?;
        let elem_llvm = self.get_any_llvm_type(result_ty)?;
        let value = self.read_tensor_element(handle, elem_llvm, base)?;
        // The element is independent of the receiver's buffer once read, so a receiver no
        // binding owns (`v.sum(axis: 0).item()`) is released here or it leaks.
        self.release_receiver_temporary(object, handle)?;
        Ok(value)
    }

    /// Element `base` of `handle`'s buffer. A device tensor's element is copied to the
    /// host on its own, after every kernel queued before it.
    fn read_tensor_element(
        &mut self,
        handle: PointerValue<'ctx>,
        elem_llvm: BasicTypeEnum<'ctx>,
        base: IntValue<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let value = self.split_on_home(
            handle,
            |this| {
                let data = this.load_dlpack_data(handle)?;
                let slot = this.tensor_element_ptr(elem_llvm, data, base)?;
                Ok(Some(this.builder.build_load(
                    elem_llvm,
                    slot,
                    "tensor.elem",
                )?))
            },
            |this, index| {
                let data = this.load_dlpack_data(handle)?;
                let slot = this.tensor_element_ptr(elem_llvm, data, base)?;
                let local = this.entry_alloca(elem_llvm, "tensor.elem.copy")?;
                let bytes = this.element_bytes(elem_llvm)?;
                this.device_copy(local, slot, bytes, index)?;
                Ok(Some(this.builder.build_load(
                    elem_llvm,
                    local,
                    "tensor.elem",
                )?))
            },
        )?;
        value.ok_or_else(|| CodegenError::InternalError("an element read produced no value".into()))
    }

    /// Store `value` into element `base` of `handle`'s buffer, copying it over on its own
    /// for a device tensor.
    fn write_tensor_element(
        &mut self,
        handle: PointerValue<'ctx>,
        elem_llvm: BasicTypeEnum<'ctx>,
        base: IntValue<'ctx>,
        value: BasicValueEnum<'ctx>,
    ) -> CodegenResult<()> {
        self.split_on_home(
            handle,
            |this| {
                let data = this.load_dlpack_data(handle)?;
                let slot = this.tensor_element_ptr(elem_llvm, data, base)?;
                this.builder.build_store(slot, value)?;
                Ok(None)
            },
            |this, index| {
                let data = this.load_dlpack_data(handle)?;
                let slot = this.tensor_element_ptr(elem_llvm, data, base)?;
                let local = this.entry_alloca(elem_llvm, "tensor.elem.copy")?;
                this.builder.build_store(local, value)?;
                let bytes = this.element_bytes(elem_llvm)?;
                this.device_copy(slot, local, bytes, index)?;
                Ok(None)
            },
        )?;
        Ok(())
    }

    fn element_bytes(&self, elem_llvm: BasicTypeEnum<'ctx>) -> CodegenResult<IntValue<'ctx>> {
        elem_llvm
            .size_of()
            .ok_or_else(|| CodegenError::InternalError("a tensor element has no size".into()))
    }

    /// The DLPack handle a tensor receiver lowers to, evaluated exactly once, so a caller
    /// that has to release the receiver afterwards does not re-lower the expression to
    /// recover it.
    pub(super) fn tensor_receiver_handle(
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
        Ok(handle)
    }

    /// Lower `place[i, j] = value`, a store into one element of a tensor's buffer.
    ///
    /// The checker has already refused an index that leaves an axis standing, so every
    /// axis here names a position and the address is the same `getelementptr` a read
    /// computes. The DLPack handle is untouched: the write goes into the buffer it
    /// already addresses, which is what keeps a raw pointer held elsewhere valid.
    pub(crate) fn codegen_tensor_index_assignment(
        &mut self,
        object: &HirExpr,
        axes: &[HirTensorAxis],
        value: &HirExpr,
        offset: usize,
    ) -> CodegenResult<()> {
        let source_ty = Type::from_hir(&object.ty);
        let Type::Tensor { element, shape } = source_ty.referent().clone() else {
            return Err(CodegenError::InternalError(
                "a tensor index assignment does not carry a tensor receiver".to_string(),
            ));
        };
        let shape = crate::types::static_extents(&shape)?;
        let elem_llvm = self.get_any_llvm_type(&element)?;
        // The value first (the language evaluates it before the place): it may reassign the tensor, releasing the buffer a
        // data pointer read before it would still address.
        let val = self.codegen_expr(value)?;
        let val = self.coerce_if_needed(val, elem_llvm, &element)?;
        let handle = self.tensor_receiver_handle(object, &source_ty)?;
        let strides = row_major_strides(&shape);
        let base = self.tensor_index_base(axes, &shape, &strides, offset)?;
        self.write_tensor_element(handle, elem_llvm, base, val)?;
        self.mark_moved_for_drop(value);
        Ok(())
    }

    /// The flat element offset every position's contribution adds up to.
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
            let HirTensorAxis::Position(expr) = index else {
                return Err(super::computed_in_mlir("a tensor slice"));
            };
            let position = self.codegen_expr(expr)?.into_int_value();
            let position = self.widen_index_to_i64(position, &Type::from_hir(&expr.ty))?;
            self.guard_tensor_position(position, shape[axis], offset)?;
            let contribution = self
                .builder
                .build_int_mul(position, stride, "tensor.index.axis")?;
            base = self
                .builder
                .build_int_add(base, contribution, "tensor.index.base")?;
        }
        Ok(base)
    }

    /// Trap a position outside its axis, in every build, as an array index is. A
    /// negative signed index sign-extends to a large unsigned value and so fails the
    /// same unsigned test.
    fn guard_tensor_position(
        &mut self,
        position: IntValue<'ctx>,
        extent: usize,
        offset: usize,
    ) -> CodegenResult<()> {
        let extent = self.context.i64_type().const_int(extent as u64, false);
        let ok = self.builder.build_int_compare(
            IntPredicate::ULT,
            position,
            extent,
            "tensor.index.ok",
        )?;
        self.codegen_guard_or_panic(ok, INDEX_OUT_OF_BOUNDS, offset)
    }

    /// The address of one element of a tensor buffer, `index` elements in.
    fn tensor_element_ptr(
        &self,
        elem_llvm: inkwell::types::BasicTypeEnum<'ctx>,
        buffer: PointerValue<'ctx>,
        index: IntValue<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        // SAFETY: every position is bounds-guarded, so the offset stays inside the buffer.
        unsafe {
            self.builder
                .build_in_bounds_gep(elem_llvm, buffer, &[index], "tensor.index.ptr")
                .map_err(CodegenError::from)
        }
    }
}
