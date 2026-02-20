// NEURO Programming Language - LLVM Backend
// NEURO semantic type to LLVM type mapping

use inkwell::context::Context as LLVMContext;
use inkwell::types::BasicTypeEnum;

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// Maps NEURO semantic types to LLVM types
pub(crate) struct TypeMapper<'ctx> {
    context: &'ctx LLVMContext,
}

impl<'ctx> TypeMapper<'ctx> {
    pub(crate) fn new(context: &'ctx LLVMContext) -> Self {
        Self { context }
    }

    /// Convert a NEURO semantic type to an LLVM type
    pub(crate) fn map_type(&self, ty: &Type) -> CodegenResult<BasicTypeEnum<'ctx>> {
        match ty {
            // Signed integers
            Type::I8 => Ok(self.context.i8_type().into()),
            Type::I16 => Ok(self.context.i16_type().into()),
            Type::I32 => Ok(self.context.i32_type().into()),
            Type::I64 => Ok(self.context.i64_type().into()),
            // Unsigned integers (LLVM doesn't distinguish signed/unsigned at type level)
            Type::U8 => Ok(self.context.i8_type().into()),
            Type::U16 => Ok(self.context.i16_type().into()),
            Type::U32 => Ok(self.context.i32_type().into()),
            Type::U64 => Ok(self.context.i64_type().into()),
            // Floating point
            Type::F32 => Ok(self.context.f32_type().into()),
            Type::F64 => Ok(self.context.f64_type().into()),
            // Other types
            Type::Bool => Ok(self.context.bool_type().into()),
            // String type: represented as ptr (opaque pointer to null-terminated byte array)
            // Phase 1: C-style strings for simplicity (LLVM 15+ uses opaque pointers)
            // Phase 2: Upgrade to {ptr, i64} struct for length tracking
            Type::String => Ok(self
                .context
                .ptr_type(inkwell::AddressSpace::default())
                .into()),
            Type::Void => Err(CodegenError::UnsupportedType(
                "void type cannot be used as a value".to_string(),
            )),
            Type::Function { .. } => Err(CodegenError::UnsupportedType(
                "function types as values not yet supported".to_string(),
            )),
        }
    }

    /// Check if a type is a floating-point type
    pub(crate) fn is_float_type(ty: &Type) -> bool {
        ty.is_float()
    }

    /// Check if a type is an unsigned integer type
    pub(crate) fn is_unsigned_int(ty: &Type) -> bool {
        ty.is_unsigned_int()
    }
}
