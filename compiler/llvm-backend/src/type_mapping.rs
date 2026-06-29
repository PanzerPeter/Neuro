// Neuro semantic type to LLVM type mapping

use inkwell::context::Context as LLVMContext;
use inkwell::types::{BasicType, BasicTypeEnum};

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// Maps Neuro semantic types to LLVM types
pub(crate) struct TypeMapper<'ctx> {
    context: &'ctx LLVMContext,
}

impl<'ctx> TypeMapper<'ctx> {
    pub(crate) fn new(context: &'ctx LLVMContext) -> Self {
        Self { context }
    }

    /// Convert a Neuro semantic type to an LLVM type
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
            // Floating point. `f16`/`bf16` lower to LLVM `half` / `bfloat` (§1.2).
            Type::F16 => Ok(self.context.f16_type().into()),
            Type::BF16 => Ok(self.context.bf16_type().into()),
            Type::F32 => Ok(self.context.f32_type().into()),
            Type::F64 => Ok(self.context.f64_type().into()),
            // Other types
            Type::Bool => Ok(self.context.bool_type().into()),
            // `char` is a 32-bit Unicode scalar value (§1.2).
            Type::Char => Ok(self.context.i32_type().into()),
            // String fat pointer: { ptr, i64 } where ptr points to null-terminated UTF-8
            // bytes in read-only memory and i64 holds the byte count excluding the null.
            // O(1) length access without scanning; prerequisite for the ownership system.
            Type::String => {
                let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
                let len_type = self.context.i64_type();
                Ok(self
                    .context
                    .struct_type(&[ptr_type.into(), len_type.into()], false)
                    .into())
            }
            // An immutable borrow `&T` is an opaque pointer to the referent's storage (§2.4).
            // LLVM 20 pointers are untyped, so every reference maps to the same `ptr`.
            Type::Reference(_) => Ok(self
                .context
                .ptr_type(inkwell::AddressSpace::default())
                .into()),
            // Fixed-size array `[T; N]` → LLVM `[N x T]` aggregate (§3.1).
            Type::Array { element, size } => {
                let elem_llvm = self.map_type(element)?;
                Ok(elem_llvm.array_type(*size as u32).into())
            }
            // Tuple `(T1, T2, ...)` → anonymous LLVM struct `{ T1, T2, ... }` (§3.2).
            // Elements are restricted to Copy non-struct types at resolution, so each
            // maps directly here (a struct element would need field definitions and is
            // not yet permitted — same restriction as arrays).
            Type::Tuple(elements) => {
                let mut field_tys = Vec::with_capacity(elements.len());
                for el in elements {
                    field_tys.push(self.map_type(el)?);
                }
                Ok(self.context.struct_type(&field_tys, false).into())
            }
            Type::Void => Err(CodegenError::UnsupportedType(
                "void type cannot be used as a value".to_string(),
            )),
            Type::Function { .. } => Err(CodegenError::UnsupportedType(
                "function types as values not yet supported".to_string(),
            )),
            // Struct types must be built via CodegenContext::get_struct_llvm_type,
            // which has access to field definitions. Calling map_type directly on
            // a struct (e.g. for a function parameter) is not supported in Phase 2.
            Type::Struct(name) => Err(CodegenError::UnsupportedType(format!(
                "struct '{}' as a function parameter or return type is not yet supported",
                name
            ))),
        }
    }

    /// Return the LLVM integer type for a Neuro integer type (signed or unsigned).
    /// Panics if called on a non-integer type.
    pub(crate) fn map_int_type(&self, ty: &Type) -> inkwell::types::IntType<'ctx> {
        match ty {
            Type::I8 | Type::U8 => self.context.i8_type(),
            Type::I16 | Type::U16 => self.context.i16_type(),
            Type::I32 | Type::U32 | Type::Char => self.context.i32_type(),
            Type::I64 | Type::U64 => self.context.i64_type(),
            _ => panic!("map_int_type called on non-integer type {:?}", ty),
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
