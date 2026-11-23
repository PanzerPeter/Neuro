// The two intrinsics `std::safetensors`'s compiler-known methods lower to. A read copies a
// tensor's stored bytes into a fresh tensor of the annotated type, its `?` axes sized by the
// stored extents, and a write copies a tensor's elements out into a `Vec<u8>`, reading a device
// tensor back first, with its extents beside them. The library has already checked that the
// stored range is exactly the tensor's size, and every target stores elements little-endian,
// as the format does, so both are one `memcpy`.

use inkwell::AddressSpace;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue, StructValue};
use neuro_hir::HirExpr;

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::{Type, is_dynamic};

/// `__tensor_from_bytes(stored: (&[u8], &[u64])) -> Tensor<T, S>`, typed by its result: the
/// bytes and every axis's stored extent, as `locate` hands them back.
const FROM_BYTES: &str = "__tensor_from_bytes";
/// `__tensor_bytes(tensor: &Tensor<T, S>) -> (Vec<u8>, Vec<u64>)`: the elements and every
/// axis's extent, as `append` takes them.
const TO_BYTES: &str = "__tensor_bytes";
/// The bytes of one extent in a `&[u64]` or a `Vec<u64>`.
const EXTENT_BYTES: u64 = 8;

impl<'ctx> CodegenContext<'ctx> {
    pub(crate) fn is_tensor_bytes_intrinsic(name: &str) -> bool {
        name == FROM_BYTES || name == TO_BYTES
    }

    pub(crate) fn codegen_tensor_bytes_intrinsic(
        &mut self,
        name: &str,
        args: &[HirExpr],
        result: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let [arg] = args else {
            return Err(CodegenError::InternalError(format!(
                "`{name}` takes one argument, given {}",
                args.len()
            )));
        };
        if name == FROM_BYTES {
            return self.tensor_from_bytes(arg, result);
        }
        self.tensor_bytes(arg, result)
    }

    fn tensor_from_bytes(
        &mut self,
        stored: &HirExpr,
        tensor_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let BasicValueEnum::StructValue(stored_value) = self.codegen_expr(stored)? else {
            return Err(CodegenError::InternalError(
                "`__tensor_from_bytes` was not handed its bytes and extents".to_string(),
            ));
        };
        let source = self.slice_data(stored_value, 0, "tensor.bytes.src")?;
        let Type::Tensor { shape, .. } = tensor_ty else {
            return Err(CodegenError::InternalError(format!(
                "`__tensor_from_bytes` builds `{}`, which is not a tensor",
                tensor_ty.mangle()
            )));
        };
        // `locate` matched the stored shape against this type, so the stored extents have its
        // rank and agree with every static axis: only the `?` ones are read.
        let mut dims = Vec::new();
        if is_dynamic(shape) {
            let extents = self.slice_data(stored_value, 1, "tensor.read.extents")?;
            let i64_type = self.context.i64_type();
            for (axis, _) in shape.iter().enumerate().filter(|(_, e)| e.is_none()) {
                let index = i64_type.const_int(axis as u64, false);
                // SAFETY: `locate` checked the stored rank equals `shape.len()`, so `axis`
                // lies inside the extents slice.
                let at = unsafe {
                    self.builder
                        .build_gep(i64_type, extents, &[index], "tensor.read.extent.at")?
                };
                dims.push(
                    self.builder
                        .build_load(i64_type, at, "tensor.read.extent")?
                        .into_int_value(),
                );
            }
        }
        let size = self.element_run(tensor_ty, &dims, stored.span.start)?.bytes;
        let (handle, data) = self.alloc_tensor(tensor_ty, "tensor.read", &dims, size)?;
        self.build_memcpy_call(data, source, size)?;
        Ok(handle.into())
    }

    /// The data pointer of the slice in field `field` of `value`.
    fn slice_data(
        &self,
        value: StructValue<'ctx>,
        field: u32,
        name: &str,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let BasicValueEnum::StructValue(slice) =
            self.builder.build_extract_value(value, field, name)?
        else {
            return Err(CodegenError::InternalError(format!(
                "field {field} of a tensor's stored bytes and extents is not a slice"
            )));
        };
        Ok(self
            .builder
            .build_extract_value(slice, 0, name)?
            .into_pointer_value())
    }

    fn tensor_bytes(
        &mut self,
        tensor: &HirExpr,
        result: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let BasicValueEnum::PointerValue(address) = self.codegen_expr(tensor)? else {
            return Err(CodegenError::InternalError(
                "`__tensor_bytes` was not handed a tensor".to_string(),
            ));
        };
        // A `&Tensor` is the address of the slot holding the handle, as for `.clone()`.
        let handle: PointerValue<'ctx> = self
            .builder
            .build_load(ptr_type, address, "tensor.bytes.handle")?
            .into_pointer_value();
        let tensor_ty = Type::from_hir(&tensor.ty).referent().clone();
        let size = self.dlpack_copy_length(handle, &tensor_ty)?;
        // `malloc`, because a `Vec` grows with `realloc` and releases with `free`.
        let malloc = self.get_or_declare_malloc();
        let buffer = self
            .builder
            .build_call(malloc, &[size.into()], "tensor.bytes.buf")?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("malloc returned void".to_string()))?
            .into_pointer_value();
        self.split_on_home(
            handle,
            |this| {
                let data = this.load_dlpack_data(handle)?;
                this.build_memcpy_call(buffer, data, size)?;
                Ok(None)
            },
            |this, index| {
                let data = this.load_dlpack_data(handle)?;
                this.device_download(buffer, data, index, size)?;
                Ok(None)
            },
        )?;
        let bytes = self.vec_header(buffer, size)?;
        let Type::Tensor { shape, .. } = &tensor_ty else {
            return Err(CodegenError::InternalError(
                "`__tensor_bytes` was not handed a tensor".to_string(),
            ));
        };
        let extents = self.extents_vec(handle, shape)?;
        let BasicTypeEnum::StructType(pair_ty) = self.get_any_llvm_type(result)? else {
            return Err(CodegenError::InternalError(
                "`__tensor_bytes` does not return a pair".to_string(),
            ));
        };
        let mut pair = pair_ty.get_undef();
        for (field, value) in [(0, bytes), (1, extents)] {
            pair = self
                .builder
                .build_insert_value(pair, value, field, "tensor.bytes.pair")?
                .into_struct_value();
        }
        Ok(pair.into())
    }

    /// A `Vec<u64>` of every axis's extent, constant for a static axis and read from `handle`
    /// for a dynamic one. A scalar's is empty and allocates nothing, as `Vec::new()`.
    fn extents_vec(
        &mut self,
        handle: PointerValue<'ctx>,
        shape: &[Option<usize>],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        if shape.is_empty() {
            return Ok(self
                .type_mapper
                .collection_header_type()
                .const_zero()
                .into());
        }
        let i64_type = self.context.i64_type();
        let rank = i64_type.const_int(shape.len() as u64, false);
        let size = i64_type.const_int(shape.len() as u64 * EXTENT_BYTES, false);
        let malloc = self.get_or_declare_malloc();
        let buffer = self
            .builder
            .build_call(malloc, &[size.into()], "tensor.extents.buf")?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("malloc returned void".to_string()))?
            .into_pointer_value();
        for axis in 0..shape.len() {
            let extent = self.handle_extent(handle, shape, axis)?;
            let index = i64_type.const_int(axis as u64, false);
            // SAFETY: the buffer was allocated above with room for `shape.len()` extents.
            let at = unsafe {
                self.builder
                    .build_gep(i64_type, buffer, &[index], "tensor.extents.at")?
            };
            self.builder.build_store(at, extent)?;
        }
        self.vec_header(buffer, rank)
    }

    /// A `Vec` header owning `buffer`, `count` elements long and as large.
    fn vec_header(
        &self,
        buffer: PointerValue<'ctx>,
        count: IntValue<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let mut header = self.type_mapper.collection_header_type().const_zero();
        for (field, value) in [
            (0, BasicValueEnum::from(buffer)),
            (1, count.into()),
            (2, count.into()),
        ] {
            header = self
                .builder
                .build_insert_value(header, value, field, "tensor.bytes")?
                .into_struct_value();
        }
        Ok(header.into())
    }
}
