// The two intrinsics `std::safetensors`'s compiler-known methods lower to. A read copies a
// tensor's stored bytes into a fresh tensor of the annotated type, and a write copies a
// tensor's elements out into a `Vec<u8>`, reading a device tensor back first. The library
// has already checked that the stored range is exactly the tensor's size, and every target
// stores elements little-endian, as the format does, so both are one `memcpy`.

use inkwell::AddressSpace;
use inkwell::values::{BasicValueEnum, PointerValue};
use neuro_hir::HirExpr;

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::{Type, is_dynamic};

/// `__tensor_from_bytes(bytes: &[u8]) -> Tensor<T, S>`, typed by its result.
const FROM_BYTES: &str = "__tensor_from_bytes";
/// `__tensor_bytes(tensor: &Tensor<T, S>) -> Vec<u8>`.
const TO_BYTES: &str = "__tensor_bytes";

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
        self.tensor_bytes(arg)
    }

    fn tensor_from_bytes(
        &mut self,
        bytes: &HirExpr,
        tensor_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let BasicValueEnum::StructValue(slice) = self.codegen_expr(bytes)? else {
            return Err(CodegenError::InternalError(
                "`__tensor_from_bytes` was not handed a slice".to_string(),
            ));
        };
        let source = self
            .builder
            .build_extract_value(slice, 0, "tensor.bytes.src")?
            .into_pointer_value();
        // A read has no run-time extents to size a `?` axis from: the file the bytes come from
        // is what would bind them, and that source does not exist yet.
        if matches!(tensor_ty, Type::Tensor { shape, .. } if is_dynamic(shape)) {
            return Err(CodegenError::UnsupportedType(format!(
                "a tensor read from bytes cannot bind the `?` axes of `{}`",
                tensor_ty.mangle()
            )));
        }
        // The read builds a static tensor: its bytes are the type's own, so no run-time extent
        // is needed to size it.
        let size = self.element_run(tensor_ty, &[], 0)?.bytes;
        let (handle, data) = self.alloc_tensor(tensor_ty, "tensor.read", &[], size)?;
        self.build_memcpy_call(data, source, size)?;
        Ok(handle.into())
    }

    fn tensor_bytes(&mut self, tensor: &HirExpr) -> CodegenResult<BasicValueEnum<'ctx>> {
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
        let mut header = self.type_mapper.collection_header_type().const_zero();
        for (field, value) in [
            (0, BasicValueEnum::from(buffer)),
            (1, size.into()),
            (2, size.into()),
        ] {
            header = self
                .builder
                .build_insert_value(header, value, field, "tensor.bytes")?
                .into_struct_value();
        }
        Ok(header.into())
    }
}
