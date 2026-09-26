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

/// DLPack `DLDataTypeCode` values. Only the codes a Neuro element type can carry
/// are named; the rest of the enum has no Neuro spelling to reach it from.
const DLPACK_CODE_INT: u8 = 0;
const DLPACK_CODE_UINT: u8 = 1;
const DLPACK_CODE_FLOAT: u8 = 2;
const DLPACK_CODE_BFLOAT: u8 = 4;
const DLPACK_CODE_BOOL: u8 = 6;

/// A DLPack `DLDataType` minus its `lanes` field, which is always 1 here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DlpackDataType {
    pub(crate) code: u8,
    pub(crate) bits: u8,
}

/// Maps Neuro semantic types to LLVM types
pub(crate) struct TypeMapper<'ctx> {
    context: &'ctx LLVMContext,
    /// Enum name → each variant's payload field types, in declaration order.
    /// Populated before code generation, and the only input to the tagged-union
    /// layout: see [`TypeMapper::enum_payload_shape`].
    enum_payloads: HashMap<String, Vec<Vec<Type>>>,
    /// Struct name → its field types in declaration order. A struct's layout is not
    /// carried by [`Type::Struct`] (which holds only the name), so the mapper needs
    /// this table to build the LLVM aggregate for one, as a function parameter, a
    /// return type, or a field of another struct.
    struct_fields: HashMap<String, Vec<Type>>,
}

impl<'ctx> TypeMapper<'ctx> {
    pub(crate) fn new(context: &'ctx LLVMContext) -> Self {
        Self {
            context,
            enum_payloads: HashMap::new(),
            struct_fields: HashMap::new(),
        }
    }

    /// Record each enum's variant payload types before code generation begins.
    pub(crate) fn set_enum_payloads(&mut self, enum_payloads: HashMap<String, Vec<Vec<Type>>>) {
        self.enum_payloads = enum_payloads;
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

    /// The tagged-union payload shape of a named enum, as `(slots, words per slot)`.
    ///
    /// `slots` is the field count of the widest variant, so every variant's fields
    /// have somewhere to sit. `words` is the widest single field rounded up to whole
    /// 64-bit words, which makes one slot large enough for any payload the enum can
    /// hold and 8-byte aligned, as every representable value needs. An all-unit enum
    /// yields `(0, 1)`, a zero-length payload.
    pub(crate) fn enum_payload_shape(&self, name: &str) -> CodegenResult<(u32, u32)> {
        let variants = self.enum_payloads.get(name).ok_or_else(|| {
            CodegenError::UnsupportedType(format!("unknown enum type '{}'", name))
        })?;
        let slots = variants.iter().map(|v| v.len()).max().unwrap_or(0) as u32;
        let mut words = 1;
        for field in variants.iter().flatten() {
            words = words.max(Self::llvm_words(self.map_type(field)?));
        }
        Ok((slots, words))
    }

    /// The declared payload field types of a named enum, by variant in discriminant
    /// order. `None` when the name is not a declared enum.
    pub(crate) fn enum_payload_types(&self, name: &str) -> Option<&Vec<Vec<Type>>> {
        self.enum_payloads.get(name)
    }

    /// The payload slot type of a named enum: `[K x i64]`, one variant field's storage.
    pub(crate) fn enum_slot_type(
        &self,
        name: &str,
    ) -> CodegenResult<inkwell::types::ArrayType<'ctx>> {
        let (_, words) = self.enum_payload_shape(name)?;
        Ok(self.context.i64_type().array_type(words))
    }

    /// An upper bound on an LLVM type's storage, in whole 64-bit words.
    ///
    /// Every field and element is rounded up to a word before it is summed, which
    /// overshoots a real layout rather than under-counting it: the maximum alignment
    /// any type here carries is 8, so padding can never push a value past the rounded
    /// total. An enum slot only has to be big enough, so an upper bound is the answer.
    fn llvm_words(ty: BasicTypeEnum<'ctx>) -> u32 {
        const BITS_PER_WORD: u32 = 64;
        match ty {
            BasicTypeEnum::IntType(int_ty) => int_ty.get_bit_width().div_ceil(BITS_PER_WORD),
            BasicTypeEnum::FloatType(_) | BasicTypeEnum::PointerType(_) => 1,
            BasicTypeEnum::ArrayType(arr) => arr.len() * Self::llvm_words(arr.get_element_type()),
            BasicTypeEnum::StructType(st) => st
                .get_field_types()
                .into_iter()
                .map(Self::llvm_words)
                .sum::<u32>()
                .max(1),
            BasicTypeEnum::VectorType(_) | BasicTypeEnum::ScalableVectorType(_) => 1,
        }
    }

    /// The LLVM tagged-union type for a named enum: `{ i32 tag, [W x [K x i64]] payload }`.
    /// The tag is the variant discriminant; the payload reserves `W` slots of `K`
    /// 64-bit words each, one slot per field of the widest variant, and a payload
    /// field is stored into its slot as raw words.
    pub(crate) fn enum_struct_type(
        &self,
        name: &str,
    ) -> CodegenResult<inkwell::types::StructType<'ctx>> {
        let (slots, _) = self.enum_payload_shape(name)?;
        let tag_ty = self.context.i32_type();
        let payload_ty = self.enum_slot_type(name)?.array_type(slots);
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

    /// The LLVM layout of a borrowed slice `&[T]` / `&mut [T]`:
    /// `{ ptr buffer, i64 len }`. The buffer pointer addresses the first element of the
    /// borrowed run and `len` counts the elements in it; the element type is erased,
    /// since LLVM 20 pointers are untyped and every access re-derives the stride from
    /// the slice's semantic element type.
    pub(crate) fn slice_ref_type(&self) -> inkwell::types::StructType<'ctx> {
        let ptr = self.context.ptr_type(inkwell::AddressSpace::default());
        self.context
            .struct_type(&[ptr.into(), self.context.i64_type().into()], false)
    }

    /// The LLVM header shared by every standard collection:
    /// `{ ptr buffer, i64 len, i64 cap, i64 used }`.
    ///
    /// `len` counts live elements/entries and `cap` the allocated slots. `used` counts
    /// occupied *slots*. For the hash map that includes tombstones, which is what the
    /// load factor must be measured against; the other kinds leave it zero.
    pub(crate) fn collection_header_type(&self) -> inkwell::types::StructType<'ctx> {
        let ptr = self.context.ptr_type(inkwell::AddressSpace::default());
        let i64_ty = self.context.i64_type();
        self.context.struct_type(
            &[ptr.into(), i64_ty.into(), i64_ty.into(), i64_ty.into()],
            false,
        )
    }

    /// The LLVM layout of the buffer a `Tensor<T, [d0, ...]>` points at: a flat,
    /// row-major `[d0*d1*... x T]` array.
    ///
    /// The rank-0 tensor holds one element, the empty product, which is why
    /// `Tensor<f32, []>` is `[1 x float]` and not a zero-length array. Host memory only;
    /// device buffers arrive with the GPU backend.
    pub(crate) fn tensor_buffer_type(&self, ty: &Type) -> CodegenResult<BasicTypeEnum<'ctx>> {
        let Type::Tensor { element, shape } = ty else {
            return Err(CodegenError::UnsupportedType(format!(
                "`{}` is not a tensor and has no buffer layout",
                ty.mangle()
            )));
        };
        let elem_llvm = self.map_type(element)?;
        let count: usize = crate::types::static_extents(shape)?.iter().product();
        Ok(elem_llvm.array_type(count as u32).into())
    }

    /// The LLVM layout of the compiler's per-tensor control block, the structure
    /// `manager_ctx` addresses.
    ///
    /// It is compiler-private: DLPack fixes the field's width and nothing else, so a
    /// consumer may carry the pointer but may not read through it. Neuro reads it only on
    /// handles it built itself (a handle arriving from a foreign producer carries that
    /// producer's context there), and reaches it by its fixed offset in the storage block,
    /// never through `manager_ctx`.
    ///
    /// `grad` and `hessian` are the slots a `.backward()` fills, the second only under
    /// `@grad(order: 2)`: each null, or a handle the slot owns.
    pub(crate) fn dlpack_control_block_type(&self) -> inkwell::types::StructType<'ctx> {
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        self.context.struct_type(
            &[
                self.context.i64_type().into(), // data_bytes
                ptr_ty.into(),                  // grad
                ptr_ty.into(),                  // hessian
            ],
            false,
        )
    }

    /// The block a tensor's handle is allocated out of: the exchange structure with the
    /// control block trailing it.
    ///
    /// One allocation, not two. Field 0 of a struct sits at offset 0, so the block's
    /// address IS the `DLManagedTensorVersioned*` a foreign consumer takes, and the
    /// deleter's single `free(self)` releases the control block along with it.
    pub(crate) fn dlpack_tensor_storage_type(&self) -> inkwell::types::StructType<'ctx> {
        self.context.struct_type(
            &[
                self.dlpack_managed_tensor_type().into(),
                self.dlpack_control_block_type().into(),
            ],
            false,
        )
    }

    /// The LLVM layout of `DLManagedTensorVersioned`, the structure a tensor
    /// value points at.
    ///
    /// Field order and widths mirror the DLPack 1.1 C header exactly, because the
    /// pointer is handed to foreign consumers unmodified. The nested `DLDevice`
    /// (`{ i32, i32 }`), `DLDataType` (`{ i8, i8, i16 }`), and `DLPackVersion`
    /// (`{ i32, i32 }`) are spelled inline rather than named, since LLVM deduplicates
    /// anonymous structs structurally and nothing else refers to them by name.
    pub(crate) fn dlpack_managed_tensor_type(&self) -> inkwell::types::StructType<'ctx> {
        let i8_ty = self.context.i8_type();
        let i16_ty = self.context.i16_type();
        let i32_ty = self.context.i32_type();
        let i64_ty = self.context.i64_type();
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());

        let version = self
            .context
            .struct_type(&[i32_ty.into(), i32_ty.into()], false);
        let device = self
            .context
            .struct_type(&[i32_ty.into(), i32_ty.into()], false);
        let dtype = self
            .context
            .struct_type(&[i8_ty.into(), i8_ty.into(), i16_ty.into()], false);
        let dl_tensor = self.context.struct_type(
            &[
                ptr_ty.into(), // data
                device.into(), // device
                i32_ty.into(), // ndim
                dtype.into(),  // dtype
                ptr_ty.into(), // shape
                ptr_ty.into(), // strides
                i64_ty.into(), // byte_offset
            ],
            false,
        );
        self.context.struct_type(
            &[
                version.into(),   // version
                ptr_ty.into(),    // manager_ctx
                ptr_ty.into(),    // deleter
                i64_ty.into(),    // flags
                dl_tensor.into(), // dl_tensor
            ],
            false,
        )
    }

    /// The DLPack `dtype` of a tensor element type: its type code and its width in bits.
    ///
    /// `lanes` is not returned because it is 1 for every Neuro element type. A vector
    /// element would be a language feature rather than an encoding of one.
    pub(crate) fn dlpack_dtype(&self, element: &Type) -> CodegenResult<DlpackDataType> {
        let (code, bits) = match element {
            Type::I8 => (DLPACK_CODE_INT, 8),
            Type::I16 => (DLPACK_CODE_INT, 16),
            Type::I32 => (DLPACK_CODE_INT, 32),
            Type::I64 => (DLPACK_CODE_INT, 64),
            Type::U8 => (DLPACK_CODE_UINT, 8),
            Type::U16 => (DLPACK_CODE_UINT, 16),
            Type::U32 => (DLPACK_CODE_UINT, 32),
            Type::U64 => (DLPACK_CODE_UINT, 64),
            Type::F16 => (DLPACK_CODE_FLOAT, 16),
            Type::F32 => (DLPACK_CODE_FLOAT, 32),
            Type::F64 => (DLPACK_CODE_FLOAT, 64),
            Type::BF16 => (DLPACK_CODE_BFLOAT, 16),
            Type::Bool => (DLPACK_CODE_BOOL, 8),
            other => {
                return Err(CodegenError::UnsupportedType(format!(
                    "`{}` has no DLPack dtype and cannot be a tensor element",
                    other.mangle()
                )))
            }
        };
        Ok(DlpackDataType { code, bits })
    }

    /// The byte size of one tensor element.
    ///
    /// Computed in Rust rather than from `size_of()` because the element buffer's
    /// allocation size has to be rounded up to the DLPack alignment, and LLVM 20 has
    /// been withdrawing the constant-expression arithmetic that would take.
    pub(crate) fn tensor_element_bytes(&self, element: &Type) -> CodegenResult<u64> {
        Ok(u64::from(self.dlpack_dtype(element)?.bits) / 8)
    }

    /// The byte size of a tensor's element buffer: `d0 * d1 * ... * sizeof(T)`.
    ///
    /// The rank-0 tensor holds one element, the empty product, so its buffer is one
    /// element wide, not zero.
    pub(crate) fn tensor_buffer_bytes(&self, ty: &Type) -> CodegenResult<u64> {
        let Type::Tensor { element, shape } = ty else {
            return Err(CodegenError::UnsupportedType(format!(
                "`{}` is not a tensor and has no buffer size",
                ty.mangle()
            )));
        };
        let count: u64 = crate::types::static_extents(shape)?
            .iter()
            .map(|d| *d as u64)
            .product();
        Ok(count * self.tensor_element_bytes(element)?)
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
            Type::Reference { inner, .. } if matches!(**inner, Type::DynObject(_)) => {
                Ok(self.dyn_ref_type().into())
            }
            // An immutable borrow of a string is the `{ ptr, i64 }` fat pointer itself,
            // held by value: the string ABI, not a pointer to it.
            //
            // `string` is immutable, so the referent's address carries no information the
            // fat pointer does not, and requiring one forces every computed slice
            // (`s.slice(a..b)`, which has no home) into a stack slot whose address then
            // outlives the frame it was taken in. By value, a slice is returned like any
            // other aggregate and `.len()` is an `extractvalue`.
            //
            // `&mut string` is excluded: a store through it has to reach the referent, so
            // it stays the referent's address. `&&string` is excluded for the same reason
            // this arm matches one level only: the outer reference borrows a reference.
            Type::Reference {
                inner,
                mutable: false,
            } if matches!(**inner, Type::String) => self.map_type_at_depth(inner, depth),
            // A borrow of a slice, `&[T]` or `&mut [T]`, is the `{ ptr, i64 }` fat
            // pointer itself, held by value: the length is not recoverable from the
            // referent's address, so it has to travel with the pointer. Unlike
            // `&string` this includes the mutable form, because a write through a slice
            // goes to the buffer the pointer names, not to the fat pointer itself.
            Type::Reference { inner, .. } if matches!(**inner, Type::Slice(_)) => {
                Ok(self.slice_ref_type().into())
            }
            // Every other borrow `&T` / `&mut T` is an opaque pointer to the referent's
            // storage. LLVM 20 pointers are untyped, so they all map to the same `ptr`.
            Type::Reference { .. } => Ok(self
                .context
                .ptr_type(inkwell::AddressSpace::default())
                .into()),
            // A bare `dyn Trait` has no size; only `&dyn Trait` is representable.
            Type::DynObject(name) => Err(CodegenError::UnsupportedType(format!(
                "`dyn {}` is unsized and must be used behind a reference",
                name
            ))),
            // A bare `[T]` has no size either; only `&[T]` / `&mut [T]` are.
            Type::Slice(element) => Err(CodegenError::UnsupportedType(format!(
                "`[{}]` is unsized and must be used behind a reference",
                element.mangle()
            ))),
            // A tensor value is a pointer to its own `DLManagedTensorVersioned`,
            // so the value a Neuro program passes around is the handle a foreign consumer
            // takes: there is no wrap step at an FFI boundary. See
            // [`dlpack_managed_tensor_type`] for the structure and [`tensor_buffer_type`]
            // for the layout of the buffer its `data` field addresses.
            Type::Tensor { .. } => Ok(self
                .context
                .ptr_type(inkwell::AddressSpace::default())
                .into()),
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
            // Enum `{ i32 tag, [W x [K x i64]] payload }`. Unlike structs, the enum
            // layout is self-contained (it comes from `enum_payloads`), so an enum maps
            // directly here and works as a parameter, return, or field type.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every element type a tensor may hold maps to its DLPack code and width, and
    /// nothing else maps at all: an unmappable element is a diagnostic, not a guess.
    #[test]
    fn the_dlpack_dtype_table_covers_every_tensor_element() {
        let context = LLVMContext::create();
        let mapper = TypeMapper::new(&context);
        let cases = [
            (Type::I8, DLPACK_CODE_INT, 8),
            (Type::I16, DLPACK_CODE_INT, 16),
            (Type::I32, DLPACK_CODE_INT, 32),
            (Type::I64, DLPACK_CODE_INT, 64),
            (Type::U8, DLPACK_CODE_UINT, 8),
            (Type::U16, DLPACK_CODE_UINT, 16),
            (Type::U32, DLPACK_CODE_UINT, 32),
            (Type::U64, DLPACK_CODE_UINT, 64),
            (Type::F16, DLPACK_CODE_FLOAT, 16),
            (Type::F32, DLPACK_CODE_FLOAT, 32),
            (Type::F64, DLPACK_CODE_FLOAT, 64),
            (Type::BF16, DLPACK_CODE_BFLOAT, 16),
            (Type::Bool, DLPACK_CODE_BOOL, 8),
        ];
        for (element, code, bits) in cases {
            let dtype = mapper
                .dlpack_dtype(&element)
                .unwrap_or_else(|_| panic!("`{}` is a legal tensor element", element.mangle()));
            assert_eq!(dtype, DlpackDataType { code, bits });
        }
        assert!(mapper.dlpack_dtype(&Type::String).is_err());
    }

    /// The buffer size is the element count times the element width, with the rank-0
    /// tensor holding the empty product's one element rather than none.
    #[test]
    fn a_tensor_buffer_is_sized_from_its_shape() {
        let context = LLVMContext::create();
        let mapper = TypeMapper::new(&context);
        let tensor = |element: Type, shape: &[usize]| Type::Tensor {
            element: Box::new(element),
            shape: neuro_hir::static_shape(shape),
        };
        assert_eq!(
            mapper.tensor_buffer_bytes(&tensor(Type::F32, &[2, 3])).ok(),
            Some(24)
        );
        assert_eq!(
            mapper.tensor_buffer_bytes(&tensor(Type::F64, &[])).ok(),
            Some(8)
        );
        assert_eq!(
            mapper.tensor_buffer_bytes(&tensor(Type::Bool, &[7])).ok(),
            Some(7)
        );
        assert!(mapper.tensor_buffer_bytes(&Type::I32).is_err());
    }

    /// The exchange structure's field order and widths are the C header's, since the
    /// pointer is handed to foreign consumers unmodified.
    #[test]
    fn the_dlpack_structure_matches_the_c_header() {
        let context = LLVMContext::create();
        let mapper = TypeMapper::new(&context);
        let handle = mapper.dlpack_managed_tensor_type();
        assert_eq!(handle.count_fields(), 5);

        let dl_tensor: inkwell::types::StructType<'_> = handle
            .get_field_type_at_index(4)
            .and_then(|field| field.try_into().ok())
            .expect("the fifth field is the DLTensor");
        assert_eq!(dl_tensor.count_fields(), 7);
        assert!(dl_tensor
            .get_field_type_at_index(0)
            .is_some_and(|field| field.is_pointer_type()));
        assert!(dl_tensor
            .get_field_type_at_index(2)
            .is_some_and(|field| field.into_int_type().get_bit_width() == 32));
        assert!(dl_tensor
            .get_field_type_at_index(6)
            .is_some_and(|field| field.into_int_type().get_bit_width() == 64));
    }

    /// The control block trails the exchange structure in one allocation, so the storage
    /// block's address is the handle address a foreign consumer is handed.
    #[test]
    fn the_control_block_trails_the_exchange_structure() {
        let context = LLVMContext::create();
        let mapper = TypeMapper::new(&context);
        let storage = mapper.dlpack_tensor_storage_type();
        assert_eq!(storage.count_fields(), 2);
        assert_eq!(
            storage.get_field_type_at_index(0),
            Some(mapper.dlpack_managed_tensor_type().into())
        );

        let control: inkwell::types::StructType<'_> = storage
            .get_field_type_at_index(1)
            .and_then(|field| field.try_into().ok())
            .expect("the second field is the control block");
        // The element buffer's byte length, then the gradient and Hessian slots.
        assert_eq!(control.count_fields(), 3);
        assert!(control
            .get_field_type_at_index(0)
            .is_some_and(|field| field.into_int_type().get_bit_width() == 64));
        for slot in [1, 2] {
            assert!(control
                .get_field_type_at_index(slot)
                .is_some_and(|field| field.is_pointer_type()));
        }
    }
}
