// Neuro semantic type to LLVM type mapping

use std::collections::HashMap;

use inkwell::context::Context as LLVMContext;
use inkwell::types::{BasicType, BasicTypeEnum};

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// How deep struct nesting may go before the mapper gives up. Field types must be
/// declared before use, so a cycle is impossible today; the limit turns any future
/// self-referential layout into a diagnostic instead of a stack overflow.
const MAX_STRUCT_DEPTH: u32 = 64;

/// Maps Neuro semantic types to LLVM types
pub(crate) struct TypeMapper<'ctx> {
    context: &'ctx LLVMContext,
    /// Enum name → payload word count `W`: the number of 64-bit slots a value of
    /// that enum reserves for variant data, sized to its largest variant.
    /// Populated before code generation so every enum type maps to a single,
    /// consistent `{ i32, [W x i64] }` aggregate.
    enum_words: HashMap<String, u32>,
    /// Struct name → its field types in declaration order. A struct's layout is not
    /// carried by [`Type::Struct`] (which holds only the name), so the mapper needs
    /// this table to build the LLVM aggregate for one — as a function parameter, a
    /// return type, or a field of another struct.
    struct_fields: HashMap<String, Vec<Type>>,
}

impl<'ctx> TypeMapper<'ctx> {
    pub(crate) fn new(context: &'ctx LLVMContext) -> Self {
        Self {
            context,
            enum_words: HashMap::new(),
            struct_fields: HashMap::new(),
        }
    }

    /// Record each enum's payload word count before code generation begins.
    pub(crate) fn set_enum_words(&mut self, enum_words: HashMap<String, u32>) {
        self.enum_words = enum_words;
    }

    /// Record every struct's field types before code generation begins.
    pub(crate) fn set_struct_fields(&mut self, struct_fields: HashMap<String, Vec<Type>>) {
        self.struct_fields = struct_fields;
    }

    /// The LLVM aggregate for a named struct: its field types in declaration order.
    ///
    /// LLVM deduplicates anonymous struct types structurally, so rebuilding the type
    /// on each call yields the same type and no cache is needed.
    pub(crate) fn struct_type(
        &self,
        name: &str,
    ) -> CodegenResult<inkwell::types::StructType<'ctx>> {
        self.struct_type_at_depth(name, 0)
    }

    fn struct_type_at_depth(
        &self,
        name: &str,
        depth: u32,
    ) -> CodegenResult<inkwell::types::StructType<'ctx>> {
        if depth >= MAX_STRUCT_DEPTH {
            return Err(CodegenError::UnsupportedType(format!(
                "struct '{}' nests more than {} levels deep, or refers to itself",
                name, MAX_STRUCT_DEPTH
            )));
        }
        let fields = self.struct_fields.get(name).ok_or_else(|| {
            CodegenError::UnsupportedType(format!("unknown struct type '{}'", name))
        })?;
        let mut field_llvm_types = Vec::with_capacity(fields.len());
        for field_ty in fields {
            field_llvm_types.push(self.map_type_at_depth(field_ty, depth + 1)?);
        }
        Ok(self.context.struct_type(&field_llvm_types, false))
    }

    /// The LLVM tagged-union type for a named enum: `{ i32 tag, [W x i64] payload }`
    /// The tag is the variant discriminant; the payload reserves `W` 64-bit
    /// slots — one per field of the widest variant — into which scalar payload
    /// values are packed. `W == 0` (an all-unit enum) yields a zero-length array.
    pub(crate) fn enum_struct_type(
        &self,
        name: &str,
    ) -> CodegenResult<inkwell::types::StructType<'ctx>> {
        let words = *self.enum_words.get(name).ok_or_else(|| {
            CodegenError::UnsupportedType(format!("unknown enum type '{}'", name))
        })?;
        let tag_ty = self.context.i32_type();
        let payload_ty = self.context.i64_type().array_type(words);
        Ok(self
            .context
            .struct_type(&[tag_ty.into(), payload_ty.into()], false))
    }

    /// The LLVM layout of a trait-object reference `&dyn Trait`:
    /// `{ ptr data, ptr vtable }`. The data pointer addresses the concrete value's
    /// storage; the vtable pointer addresses that concrete type's method table for the
    /// trait, so a call indexes a fixed slot regardless of which type is behind it.
    pub(crate) fn dyn_ref_type(&self) -> inkwell::types::StructType<'ctx> {
        let ptr = self.context.ptr_type(inkwell::AddressSpace::default());
        self.context.struct_type(&[ptr.into(), ptr.into()], false)
    }

    /// The LLVM header shared by every standard collection:
    /// `{ ptr buffer, i64 len, i64 cap, i64 used }`.
    ///
    /// `len` counts live elements/entries and `cap` the allocated slots. `used` counts
    /// occupied *slots* — for the hash map that includes tombstones, which is what the
    /// load factor must be measured against; the other kinds leave it zero.
    pub(crate) fn collection_header_type(&self) -> inkwell::types::StructType<'ctx> {
        let ptr = self.context.ptr_type(inkwell::AddressSpace::default());
        let i64_ty = self.context.i64_type();
        self.context.struct_type(
            &[ptr.into(), i64_ty.into(), i64_ty.into(), i64_ty.into()],
            false,
        )
    }

    /// Convert a Neuro semantic type to an LLVM type
    pub(crate) fn map_type(&self, ty: &Type) -> CodegenResult<BasicTypeEnum<'ctx>> {
        self.map_type_at_depth(ty, 0)
    }

    /// `map_type` carrying the struct-nesting depth, so a struct field that is itself
    /// a struct is bounded by [`MAX_STRUCT_DEPTH`].
    fn map_type_at_depth(&self, ty: &Type, depth: u32) -> CodegenResult<BasicTypeEnum<'ctx>> {
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
            // Floating point. `f16`/`bf16` lower to LLVM `half` / `bfloat`.
            Type::F16 => Ok(self.context.f16_type().into()),
            Type::BF16 => Ok(self.context.bf16_type().into()),
            Type::F32 => Ok(self.context.f32_type().into()),
            Type::F64 => Ok(self.context.f64_type().into()),
            // Other types
            Type::Bool => Ok(self.context.bool_type().into()),
            // `char` is a 32-bit Unicode scalar value.
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
            // A reference to a trait object is a fat pointer `{ data ptr, vtable ptr }`
            // `dyn Trait` is unsized, so the reference must additionally carry
            // the method table that selects the concrete implementation at runtime.
            Type::Reference(inner) if matches!(**inner, Type::DynObject(_)) => {
                Ok(self.dyn_ref_type().into())
            }
            // An immutable borrow `&T` is an opaque pointer to the referent's storage.
            // LLVM 20 pointers are untyped, so every reference maps to the same `ptr`.
            Type::Reference(_) => Ok(self
                .context
                .ptr_type(inkwell::AddressSpace::default())
                .into()),
            // A bare `dyn Trait` has no size; only `&dyn Trait` is representable.
            Type::DynObject(name) => Err(CodegenError::UnsupportedType(format!(
                "`dyn {}` is unsized and must be used behind a reference",
                name
            ))),
            // Fixed-size array `[T; N]` → LLVM `[N x T]` aggregate.
            Type::Array { element, size } => {
                let elem_llvm = self.map_type_at_depth(element, depth)?;
                Ok(elem_llvm.array_type(*size as u32).into())
            }
            // Tuple `(T1, T2, ...)` → anonymous LLVM struct `{ T1, T2, ... }`.
            Type::Tuple(elements) => {
                let mut field_tys = Vec::with_capacity(elements.len());
                for el in elements {
                    field_tys.push(self.map_type_at_depth(el, depth)?);
                }
                Ok(self.context.struct_type(&field_tys, false).into())
            }
            Type::Void => Err(CodegenError::UnsupportedType(
                "void type cannot be used as a value".to_string(),
            )),
            // A closure / function value is a `{ fn_ptr, env_ptr }` fat pointer.
            // Every closure shares this uniform two-pointer representation, so a
            // `(T) -> U` parameter can accept any closure regardless of its captures.
            Type::Function { .. } => {
                let ptr = self.context.ptr_type(inkwell::AddressSpace::default());
                Ok(self
                    .context
                    .struct_type(&[ptr.into(), ptr.into()], false)
                    .into())
            }
            // A named struct is its field aggregate, built from the layout table. It is
            // passed and returned by value like any other first-class aggregate.
            Type::Struct(name) => Ok(self.struct_type_at_depth(name, depth)?.into()),
            // Every standard collection is a `{ buffer, len, cap, used }` header
            // held by value; the elements live in the heap buffer it points at.
            Type::Collection { .. } => Ok(self.collection_header_type().into()),
            // Enum `{ i32 tag, [W x i64] payload }`. Unlike structs, the enum
            // layout is self-contained (the word count comes from `enum_words`), so an
            // enum maps directly here and works as a parameter, return, or field type.
            Type::Enum(name) => Ok(self.enum_struct_type(name)?.into()),
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
