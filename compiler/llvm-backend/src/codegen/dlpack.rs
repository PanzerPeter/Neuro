// The DLPack exchange structure a tensor value points at.
//
// A tensor value is not a buffer pointer with a conversion step waiting at the FFI
// boundary: it is a `DLManagedTensorVersioned*`, so the pointer a Neuro program moves
// around is the pointer NumPy, PyTorch, or JAX consumes. That also settles ownership
// once: the `deleter` field is the single release path, used by a tensor leaving scope
// and by a foreign consumer that took the handle, so there is no private free for a
// consumer to race with.

use inkwell::module::Linkage;
use inkwell::values::{FunctionValue, IntValue, PointerValue};

use crate::codegen::context::{CodegenContext, ALIGNED_ALLOC_FN};
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// DLPack version this compiler produces. 1.1 is the versioned structure; the
/// unversioned `DLManagedTensor` it replaced is deprecated.
const DLPACK_VERSION_MAJOR: u64 = 1;
const DLPACK_VERSION_MINOR: u64 = 1;

/// `kDLCPU`. Every buffer this backend can build is host memory; a device backend flips
/// this field rather than changing the layout.
const DLPACK_DEVICE_CPU: u64 = 1;

/// The alignment the SIMD and device-transfer paths want, and what this backend
/// guarantees. It is deliberately NOT what the DLPack header asks for: that header
/// specifies a `data` pointer "always aligned to 256 bytes as in CUDA", reached via
/// `byte_offset`. No major implementation honours it (PyTorch, CuPy, TensorFlow and TVM
/// all pass unaligned pointers with `byte_offset = 0`), so 64 interoperates with every
/// consumer that exists while 256 would only satisfy a rule the ecosystem abandoned.
const DLPACK_DATA_ALIGN: u64 = 64;

/// `lanes` is 1 for every Neuro element type: a vector element would be a language
/// feature of its own rather than an encoding of one.
const DLPACK_LANES: u64 = 1;

/// The release function every tensor handle carries. One definition serves every tensor
/// type, because the structure holds everything the free needs.
const DLPACK_DELETER_FN: &str = "__neuro_dlpack_deleter";

/// Field indices into `DLManagedTensorVersioned`, mirroring the C header's order.
const FIELD_VERSION: u32 = 0;
const FIELD_MANAGER_CTX: u32 = 1;
const FIELD_DELETER: u32 = 2;
const FIELD_FLAGS: u32 = 3;
const FIELD_DL_TENSOR: u32 = 4;

/// The control block's index in the storage block. The exchange structure is field 0, and
/// a struct's first field sits at offset 0, so the storage pointer needs no index at all.
const FIELD_CONTROL: u32 = 1;

/// Field indices into the control block.
const FIELD_DATA_BYTES: u32 = 0;

/// Field indices into the nested `DLTensor`.
const FIELD_DATA: u32 = 0;
const FIELD_DEVICE: u32 = 1;
const FIELD_NDIM: u32 = 2;
const FIELD_DTYPE: u32 = 3;
const FIELD_SHAPE: u32 = 4;
const FIELD_STRIDES: u32 = 5;
const FIELD_BYTE_OFFSET: u32 = 6;

impl<'ctx> CodegenContext<'ctx> {
    /// Allocate a tensor's DLPack handle and its element buffer, fill every field of the
    /// structure, and return the handle, the pointer that *is* the tensor value.
    ///
    /// The handle and the control block `manager_ctx` addresses come from one `malloc`,
    /// since neither has an alignment requirement the other does not. The ELEMENT buffer
    /// stays a second allocation: fusing it needs the structure's size rounded up to
    /// [`DLPACK_DATA_ALIGN`] as an IR constant expression, and LLVM 20 has been
    /// withdrawing constant-expression arithmetic. The element buffer's size is computable
    /// in Rust; the structure's is not.
    pub(crate) fn alloc_dlpack_tensor(
        &mut self,
        tensor_ty: &Type,
        name: &str,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let Type::Tensor { element, shape } = tensor_ty else {
            return Err(CodegenError::InternalError(
                "a DLPack handle is only built for a tensor type".to_string(),
            ));
        };

        let storage_ty = self.type_mapper.dlpack_tensor_storage_type();
        let i64_type = self.context.i64_type();
        let storage_size = storage_ty.size_of().ok_or_else(|| {
            CodegenError::InternalError("the DLPack storage block has no size".to_string())
        })?;
        // Field 0 of the storage block is the exchange structure, and a struct's first
        // field sits at offset 0, so this pointer is already the `DLManagedTensorVersioned*`
        // a foreign consumer takes: the control block trailing it is invisible to them.
        let handle = self.build_malloc(storage_size, name)?;

        // `aligned_alloc` wants a size that is a multiple of the alignment; a tensor
        // buffer is rounded up rather than passed through, since a small tensor's
        // element run is routinely shorter than one alignment unit. `_aligned_malloc`
        // has no such requirement, and the rounding is harmless there.
        let bytes = self.type_mapper.tensor_buffer_bytes(tensor_ty)?;
        let padded = bytes.div_ceil(DLPACK_DATA_ALIGN) * DLPACK_DATA_ALIGN;
        let aligned_alloc = self.aligned_alloc_fn()?;
        let alignment = i64_type.const_int(DLPACK_DATA_ALIGN, false);
        let size = i64_type.const_int(padded, false);
        // `aligned_alloc(alignment, size)` against `_aligned_malloc(size, alignment)`:
        // the two spellings take the same pair the other way round.
        let args = if cfg!(target_os = "windows") {
            [size.into(), alignment.into()]
        } else {
            [alignment.into(), size.into()]
        };
        let data = self
            .builder
            .build_call(aligned_alloc, &args, "tensor.data")?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| {
                CodegenError::InternalError(format!("{ALIGNED_ALLOC_FN} returned void"))
            })?
            .into_pointer_value();

        self.init_dlpack_handle(
            handle,
            data,
            bytes,
            element,
            &crate::types::static_extents(shape)?,
        )?;
        Ok(handle)
    }

    /// Write every field of a freshly allocated handle.
    fn init_dlpack_handle(
        &mut self,
        handle: PointerValue<'ctx>,
        data: PointerValue<'ctx>,
        data_bytes: u64,
        element: &Type,
        shape: &[usize],
    ) -> CodegenResult<()> {
        let handle_ty = self.type_mapper.dlpack_managed_tensor_type();
        let i8_type = self.context.i8_type();
        let i16_type = self.context.i16_type();
        let i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();

        let version = self
            .context
            .struct_type(&[i32_type.into(), i32_type.into()], false)
            .const_named_struct(&[
                i32_type.const_int(DLPACK_VERSION_MAJOR, false).into(),
                i32_type.const_int(DLPACK_VERSION_MINOR, false).into(),
            ]);
        self.store_handle_field(handle_ty, handle, &[FIELD_VERSION], version.into())?;
        let control = self.init_dlpack_control_block(handle, data_bytes)?;
        self.store_handle_field(handle_ty, handle, &[FIELD_MANAGER_CTX], control.into())?;
        let deleter = self.get_or_define_dlpack_deleter()?;
        self.store_handle_field(
            handle_ty,
            handle,
            &[FIELD_DELETER],
            deleter.as_global_value().as_pointer_value().into(),
        )?;
        // The buffer is writable, so DLPACK_FLAG_BITMASK_READ_ONLY stays clear.
        self.store_handle_field(
            handle_ty,
            handle,
            &[FIELD_FLAGS],
            i64_type.const_zero().into(),
        )?;

        self.store_handle_field(
            handle_ty,
            handle,
            &[FIELD_DL_TENSOR, FIELD_DATA],
            data.into(),
        )?;
        let device = self
            .context
            .struct_type(&[i32_type.into(), i32_type.into()], false)
            .const_named_struct(&[
                i32_type.const_int(DLPACK_DEVICE_CPU, false).into(),
                i32_type.const_zero().into(),
            ]);
        self.store_handle_field(
            handle_ty,
            handle,
            &[FIELD_DL_TENSOR, FIELD_DEVICE],
            device.into(),
        )?;
        self.store_handle_field(
            handle_ty,
            handle,
            &[FIELD_DL_TENSOR, FIELD_NDIM],
            i32_type.const_int(shape.len() as u64, false).into(),
        )?;
        let dtype_value = self.type_mapper.dlpack_dtype(element)?;
        let dtype = self
            .context
            .struct_type(&[i8_type.into(), i8_type.into(), i16_type.into()], false)
            .const_named_struct(&[
                i8_type.const_int(u64::from(dtype_value.code), false).into(),
                i8_type.const_int(u64::from(dtype_value.bits), false).into(),
                i16_type.const_int(DLPACK_LANES, false).into(),
            ]);
        self.store_handle_field(
            handle_ty,
            handle,
            &[FIELD_DL_TENSOR, FIELD_DTYPE],
            dtype.into(),
        )?;

        let (shape_global, strides_global) = self.dlpack_shape_globals(element, shape)?;
        self.store_handle_field(
            handle_ty,
            handle,
            &[FIELD_DL_TENSOR, FIELD_SHAPE],
            shape_global.into(),
        )?;
        self.store_handle_field(
            handle_ty,
            handle,
            &[FIELD_DL_TENSOR, FIELD_STRIDES],
            strides_global.into(),
        )?;
        self.store_handle_field(
            handle_ty,
            handle,
            &[FIELD_DL_TENSOR, FIELD_BYTE_OFFSET],
            i64_type.const_zero().into(),
        )?;
        Ok(())
    }

    /// Fill the control block trailing `handle` and return its address, the value
    /// `manager_ctx` carries.
    ///
    /// The block is the compiler's own per-tensor state, reserved here so that the arena
    /// registration a `pool` block needs and the gradient slot `@grad` needs are added as
    /// fields rather than as a second layout. DLPack itself says nothing about the target:
    /// a consumer passes the pointer back to the deleter and never reads through it.
    fn init_dlpack_control_block(
        &self,
        handle: PointerValue<'ctx>,
        data_bytes: u64,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let storage_ty = self.type_mapper.dlpack_tensor_storage_type();
        let control = self
            .builder
            .build_struct_gep(storage_ty, handle, FIELD_CONTROL, "dlpack.control")
            .map_err(|_| {
                CodegenError::InternalError(
                    "the DLPack storage block has no control field".to_string(),
                )
            })?;
        let control_ty = storage_ty
            .get_field_type_at_index(FIELD_CONTROL)
            .and_then(|field| field.try_into().ok())
            .ok_or_else(|| {
                CodegenError::InternalError("the DLPack control field is not a struct".to_string())
            })?;
        self.store_handle_field(
            control_ty,
            control,
            &[FIELD_DATA_BYTES],
            self.context.i64_type().const_int(data_bytes, false).into(),
        )?;
        Ok(control)
    }

    /// Re-describe an existing handle as a tensor of `tensor_ty`, leaving its `data`
    /// pointer and its deleter alone.
    ///
    /// This is the whole of an order-preserving reshape: the buffer already holds the
    /// result's elements in the result's order, so only the rank, extents, and strides a
    /// DLPack consumer reads have to change. Nothing is allocated and nothing is copied,
    /// which is why `.reshape` on a large tensor costs three stores.
    pub(crate) fn build_dlpack_redescribe(
        &self,
        handle: PointerValue<'ctx>,
        tensor_ty: &Type,
    ) -> CodegenResult<()> {
        let Type::Tensor { element, shape } = tensor_ty else {
            return Err(CodegenError::InternalError(
                "a DLPack handle is only re-described as a tensor type".to_string(),
            ));
        };
        let handle_ty = self.type_mapper.dlpack_managed_tensor_type();
        let i32_type = self.context.i32_type();
        self.store_handle_field(
            handle_ty,
            handle,
            &[FIELD_DL_TENSOR, FIELD_NDIM],
            i32_type.const_int(shape.len() as u64, false).into(),
        )?;
        let (shape_global, strides_global) =
            self.dlpack_shape_globals(element, &crate::types::static_extents(shape)?)?;
        self.store_handle_field(
            handle_ty,
            handle,
            &[FIELD_DL_TENSOR, FIELD_SHAPE],
            shape_global.into(),
        )?;
        self.store_handle_field(
            handle_ty,
            handle,
            &[FIELD_DL_TENSOR, FIELD_STRIDES],
            strides_global.into(),
        )
    }

    /// Store `value` into the handle field reached by walking `path` from the structure
    /// root: one index for a top-level field, two to reach into the nested `DLTensor`.
    fn store_handle_field(
        &self,
        handle_ty: inkwell::types::StructType<'ctx>,
        handle: PointerValue<'ctx>,
        path: &[u32],
        value: inkwell::values::BasicValueEnum<'ctx>,
    ) -> CodegenResult<()> {
        let mut current_ty = handle_ty;
        let mut ptr = handle;
        for (depth, index) in path.iter().enumerate() {
            ptr = self
                .builder
                .build_struct_gep(current_ty, ptr, *index, "dlpack.field")
                .map_err(|_| {
                    CodegenError::InternalError(format!(
                        "DLPack field {} is out of range for the structure",
                        index
                    ))
                })?;
            if depth + 1 < path.len() {
                current_ty = current_ty
                    .get_field_type_at_index(*index)
                    .and_then(|field| field.try_into().ok())
                    .ok_or_else(|| {
                        CodegenError::InternalError(
                            "a DLPack field path descends into a non-struct field".to_string(),
                        )
                    })?;
            }
        }
        self.builder.build_store(ptr, value)?;
        Ok(())
    }

    /// The `shape` and `strides` arrays for a statically shaped tensor, as private
    /// constants shared by every value of that tensor type.
    ///
    /// Strides are counted in elements, not bytes, which is what DLPack specifies. Rank 0
    /// has no axis to describe, so both fields are null, the spelling DLPack gives a
    /// scalar.
    fn dlpack_shape_globals(
        &self,
        element: &Type,
        shape: &[usize],
    ) -> CodegenResult<(PointerValue<'ctx>, PointerValue<'ctx>)> {
        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        if shape.is_empty() {
            return Ok((ptr_type.const_null(), ptr_type.const_null()));
        }

        let mut strides = vec![1u64; shape.len()];
        for axis in (0..shape.len() - 1).rev() {
            strides[axis] = strides[axis + 1] * shape[axis + 1] as u64;
        }

        let suffix = format!(
            "{}_{}",
            element.mangle(),
            shape
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("x")
        );
        let shape_ptr = self.dlpack_i64_array_global(
            &format!("__neuro_dlpack_shape_{}", suffix),
            &shape.iter().map(|d| *d as u64).collect::<Vec<_>>(),
        );
        let strides_ptr =
            self.dlpack_i64_array_global(&format!("__neuro_dlpack_strides_{}", suffix), &strides);
        Ok((shape_ptr, strides_ptr))
    }

    /// A private constant `[N x i64]`, reused when one of that name already exists.
    fn dlpack_i64_array_global(&self, name: &str, values: &[u64]) -> PointerValue<'ctx> {
        if let Some(existing) = self.module.get_global(name) {
            return existing.as_pointer_value();
        }
        let i64_type = self.context.i64_type();
        let elements: Vec<_> = values
            .iter()
            .map(|value| i64_type.const_int(*value, false))
            .collect();
        let initializer = i64_type.const_array(&elements);
        let global = self.module.add_global(initializer.get_type(), None, name);
        global.set_linkage(Linkage::Private);
        global.set_constant(true);
        global.set_initializer(&initializer);
        global.as_pointer_value()
    }

    /// Define the shared `deleter`, or return the existing definition.
    ///
    /// It frees the element buffer and then the structure, in that order, because reading
    /// `data` out of the block it is about to free would be a use-after-free. The control
    /// block needs no release of its own: it is part of the structure's allocation, so the
    /// one `free(self)` takes both. The deleter deliberately does NOT read `manager_ctx` —
    /// a handle built elsewhere carries a foreign context there.
    pub(crate) fn get_or_define_dlpack_deleter(&mut self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(DLPACK_DELETER_FN) {
            return Ok(existing);
        }
        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = self.context.void_type().fn_type(&[ptr_type.into()], false);
        let function =
            self.module
                .add_function(DLPACK_DELETER_FN, fn_type, Some(Linkage::Internal));
        let entry = self.context.append_basic_block(function, "entry");

        let saved_block = self.builder.get_insert_block();
        self.builder.position_at_end(entry);
        let handle = function
            .get_first_param()
            .ok_or_else(|| {
                CodegenError::InternalError("the DLPack deleter takes its handle".to_string())
            })?
            .into_pointer_value();
        let data = self.load_dlpack_data(handle)?;
        // The buffer goes back to the release paired with the over-aligned allocation and
        // the structure to plain `free`: the two blocks come from different allocators on
        // Windows, where crossing them corrupts the heap.
        let aligned_free_fn = self.aligned_release_fn()?;
        let free_fn = self.release_fn()?;
        self.builder
            .build_call(aligned_free_fn, &[data.into()], "")?;
        self.builder.build_call(free_fn, &[handle.into()], "")?;
        self.builder.build_return(None)?;
        if let Some(block) = saved_block {
            self.builder.position_at_end(block);
        }
        Ok(function)
    }

    /// Load a handle's `data` pointer, the address of the element buffer.
    pub(crate) fn load_dlpack_data(
        &self,
        handle: PointerValue<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let handle_ty = self.type_mapper.dlpack_managed_tensor_type();
        let dl_tensor = self
            .builder
            .build_struct_gep(handle_ty, handle, FIELD_DL_TENSOR, "dlpack.tensor")
            .map_err(|_| {
                CodegenError::InternalError("the DLPack structure has no tensor field".to_string())
            })?;
        let dl_tensor_ty: inkwell::types::StructType<'ctx> = handle_ty
            .get_field_type_at_index(FIELD_DL_TENSOR)
            .and_then(|field| field.try_into().ok())
            .ok_or_else(|| {
                CodegenError::InternalError("the DLPack tensor field is not a struct".to_string())
            })?;
        let data_ptr = self
            .builder
            .build_struct_gep(dl_tensor_ty, dl_tensor, FIELD_DATA, "dlpack.data.addr")
            .map_err(|_| {
                CodegenError::InternalError("the DLPack tensor has no data field".to_string())
            })?;
        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        self.builder
            .build_load(ptr_type, data_ptr, "dlpack.data")
            .map_err(CodegenError::from)
            .map(|value| value.into_pointer_value())
    }

    /// Call a handle's own `deleter`, releasing its buffer and the structure.
    ///
    /// Dispatched through the field rather than called by name so that the release a
    /// tensor performs at scope exit is provably the release a foreign owner performs.
    pub(crate) fn build_dlpack_release(&self, handle: PointerValue<'ctx>) -> CodegenResult<()> {
        let handle_ty = self.type_mapper.dlpack_managed_tensor_type();
        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        let deleter_addr = self
            .builder
            .build_struct_gep(handle_ty, handle, FIELD_DELETER, "dlpack.deleter.addr")
            .map_err(|_| {
                CodegenError::InternalError("the DLPack structure has no deleter".to_string())
            })?;
        let deleter = self
            .builder
            .build_load(ptr_type, deleter_addr, "dlpack.deleter")?
            .into_pointer_value();
        let fn_type = self.context.void_type().fn_type(&[ptr_type.into()], false);
        self.builder
            .build_indirect_call(fn_type, deleter, &[handle.into()], "")?;
        Ok(())
    }

    /// The byte length of a tensor's element buffer, as an `i64` for `memcpy`.
    ///
    /// This is the unpadded run of elements: the padding `aligned_alloc` receives exists
    /// to satisfy the allocator, not to be copied.
    pub(crate) fn dlpack_copy_length(&self, tensor_ty: &Type) -> CodegenResult<IntValue<'ctx>> {
        Ok(self
            .context
            .i64_type()
            .const_int(self.type_mapper.tensor_buffer_bytes(tensor_ty)?, false))
    }
}
