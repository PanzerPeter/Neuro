// Code generation for the standard collections `Vec<T>`, `HashMap<K, V>`, and
// `BTreeMap<K, V>`.
//
// All three are values of one header type — `{ ptr buffer, i64 len, i64 cap, i64 used }`
// — held in the owner's stack slot, with the elements in a single heap buffer. The
// buffer's layout is per kind: a plain element array for `Vec`, an array of
// `{ i8 state, K key, V value }` probe slots for `HashMap`, and a key-sorted array of
// `{ K key, V value }` slots for `BTreeMap`.
//
// Operations that need a loop (probing, binary search, growth) are emitted once per
// concrete instantiation as a private helper function rather than inlined at every call
// site, so a program with many `map.get(k)` calls emits one probe loop, not one per call.

mod keys;
mod maps;
mod vectors;

use inkwell::types::{BasicType, BasicTypeEnum, StructType};
use inkwell::values::{BasicValueEnum, FunctionValue, IntValue, PointerValue};
use neuro_hir::HirExpr;

use crate::codegen::context::{CodegenContext, DropTarget};
use crate::errors::{CodegenError, CodegenResult};
use crate::types::{CollectionKind, Type};

/// Header field indices. The four fields are described on
/// [`crate::type_mapping::TypeMapper::collection_header_type`].
pub(super) const FIELD_BUFFER: u32 = 0;
pub(super) const FIELD_LEN: u32 = 1;
pub(super) const FIELD_CAP: u32 = 2;
pub(super) const FIELD_USED: u32 = 3;

/// The drop-scope name given to an unnamed collection temporary. It contains `__`,
/// which semantic analysis rejects in any declared name, so it can never be mistaken for
/// a source binding when a move clears drop flags by name.
const TEMPORARY_BINDING: &str = "__collection_temporary";

/// The capacity a collection jumps to on its first growth. Small enough not to
/// over-allocate for the short collections that dominate, large enough that the early
/// pushes do not each trigger a `realloc`.
const INITIAL_CAPACITY: u64 = 8;

impl<'ctx> CodegenContext<'ctx> {
    /// Lower `Vec::new()` / `HashMap::new()` / `BTreeMap::new()`: a header with a null
    /// buffer and zero counts. Nothing is allocated until the first insertion, so an
    /// empty collection costs no heap traffic.
    pub(crate) fn codegen_collection_new(&mut self) -> CodegenResult<BasicValueEnum<'ctx>> {
        Ok(self.collection_header_type().const_zero().into())
    }

    /// Dispatch a method call on a collection receiver.
    ///
    /// `recv_ty` is the receiver's type (possibly a reference, which is auto-dereferenced
    /// to the header) and `result_ty` the call's resolved result type, which supplies the
    /// `Option<T>` instance the fallible readers build.
    pub(crate) fn codegen_collection_method(
        &mut self,
        method: &str,
        recv_ty: &Type,
        result_ty: &Type,
        receiver: &HirExpr,
        args: &[HirExpr],
    ) -> CodegenResult<Option<BasicValueEnum<'ctx>>> {
        let Type::Collection { kind, args: params } = recv_ty.referent().clone() else {
            return Err(CodegenError::InternalError(format!(
                "collection method '{}' on non-collection receiver {:?}",
                method, recv_ty
            )));
        };
        let header = self.collection_place_ptr(receiver, recv_ty)?;

        match (kind, method) {
            (_, "len") => Ok(Some(
                self.load_header_field(header, FIELD_LEN, "col.len")?.into(),
            )),
            (_, "clear") => {
                let stride = self.collection_slot_stride(kind, &params)?;
                self.codegen_collection_clear(header, stride)?;
                Ok(None)
            }
            (CollectionKind::Vec, "push") => {
                let element_ty = collection_arg(&params, 0)?;
                self.codegen_vec_push(header, &element_ty, args)?;
                Ok(None)
            }
            (CollectionKind::Vec, "pop") => Ok(Some(self.codegen_vec_pop(
                header,
                &collection_arg(&params, 0)?,
                result_ty,
            )?)),
            (CollectionKind::Vec, "get") => Ok(Some(self.codegen_vec_get(
                header,
                &collection_arg(&params, 0)?,
                result_ty,
                args,
            )?)),
            (CollectionKind::HashMap | CollectionKind::BTreeMap, _) => {
                self.codegen_map_method(kind, method, header, &params, result_ty, args)
            }
            _ => Err(CodegenError::InternalError(format!(
                "unknown collection method '{}' reached codegen",
                method
            ))),
        }
    }

    /// Release a collection's heap buffer. Emitted at the owner's scope exit; a
    /// never-allocated collection holds a null buffer, which `free` accepts.
    pub(crate) fn emit_collection_free(&mut self, header: PointerValue<'ctx>) -> CodegenResult<()> {
        let buffer = self.load_header_buffer(header)?;
        let free_fn = self.get_or_declare_free();
        self.builder
            .build_call(free_fn, &[buffer.into()], "")
            .map_err(|e| CodegenError::LlvmError(format!("failed to free buffer: {}", e)))?;
        Ok(())
    }

    /// `collection.clear()` — reset the counts and wipe the allocated slots. The buffer
    /// is retained so refilling does not reallocate, and the elements themselves are
    /// `Copy`-or-`string` values with nothing of their own to release.
    ///
    /// The wipe matters for the hash map: a stale `FULL` slot state would still answer
    /// lookups after the length reached zero. `EMPTY` is state zero, so zeroing the
    /// buffer is exactly the reset, and it is harmless for the other kinds.
    fn codegen_collection_clear(
        &mut self,
        header: PointerValue<'ctx>,
        slot_stride: IntValue<'ctx>,
    ) -> CodegenResult<()> {
        let zero = self.context.i64_type().const_zero();
        self.store_header_field(header, FIELD_LEN, zero)?;
        self.store_header_field(header, FIELD_USED, zero)?;

        let buffer = self.load_header_buffer(header)?;
        let cap = self.load_header_field(header, FIELD_CAP, "col.cap")?;
        let bytes = self
            .builder
            .build_int_mul(cap, slot_stride, "col.wipe.bytes")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let memset = self.get_or_declare_memset();
        self.builder
            .build_call(
                memset,
                &[
                    buffer.into(),
                    self.context.i32_type().const_zero().into(),
                    bytes.into(),
                ],
                "",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// The byte size of one buffer slot: the element for a `Vec`, the whole
    /// key/value/state record for a map.
    fn collection_slot_stride(
        &mut self,
        kind: CollectionKind,
        params: &[Type],
    ) -> CodegenResult<IntValue<'ctx>> {
        let slot_ty = match kind {
            CollectionKind::Vec => self.collection_value_type(&collection_arg(params, 0)?)?,
            CollectionKind::HashMap | CollectionKind::BTreeMap => self
                .map_slot_type(
                    kind,
                    &collection_arg(params, 0)?,
                    &collection_arg(params, 1)?,
                )?
                .into(),
        };
        self.size_of_type(slot_ty)
    }

    /// Resolve the storage pointer of a collection place: the binding's stack slot for
    /// an owned collection, the pointee for a `&`/`&mut` receiver, and a fresh temporary
    /// for any other collection-valued expression (`for k in m.keys()`, `m.keys().len()`).
    ///
    /// A temporary owns its buffer with no binding to free it, so it is registered in the
    /// enclosing drop scope like a named collection — that is what keeps map iteration,
    /// whose only route is the `Vec` `keys()` builds, from leaking.
    pub(super) fn collection_place_ptr(
        &mut self,
        object: &HirExpr,
        obj_ty: &Type,
    ) -> CodegenResult<PointerValue<'ctx>> {
        if matches!(obj_ty, Type::Reference(_)) {
            return Ok(self.codegen_expr(object)?.into_pointer_value());
        }
        if let neuro_hir::HirExprKind::Variable(name) = &object.kind {
            if let Some(ptr) = self.variables.get(name).copied() {
                return Ok(ptr);
            }
        }
        let value = self.codegen_expr(object)?;
        let tmp = self.entry_alloca(value.get_type(), "col.tmp")?;
        self.builder
            .build_store(tmp, value)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        // The synthetic name cannot collide with a source binding: `__` is rejected in
        // every declared name, so no move site will ever clear this entry's drop flag.
        self.register_local_drop(TEMPORARY_BINDING, tmp, DropTarget::Collection)?;
        Ok(tmp)
    }

    /// The `{ buffer, len, cap, used }` header type.
    pub(super) fn collection_header_type(&self) -> StructType<'ctx> {
        self.type_mapper.collection_header_type()
    }

    /// Load one `i64` header field.
    pub(super) fn load_header_field(
        &mut self,
        header: PointerValue<'ctx>,
        field: u32,
        name: &str,
    ) -> CodegenResult<IntValue<'ctx>> {
        let header_ty = self.collection_header_type();
        let field_ptr = self
            .builder
            .build_struct_gep(header_ty, header, field, "col.field")
            .map_err(|_| CodegenError::InternalError("collection header GEP failed".into()))?;
        Ok(self
            .builder
            .build_load(self.context.i64_type(), field_ptr, name)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value())
    }

    /// Store one `i64` header field.
    pub(super) fn store_header_field(
        &mut self,
        header: PointerValue<'ctx>,
        field: u32,
        value: IntValue<'ctx>,
    ) -> CodegenResult<()> {
        let header_ty = self.collection_header_type();
        let field_ptr = self
            .builder
            .build_struct_gep(header_ty, header, field, "col.field")
            .map_err(|_| CodegenError::InternalError("collection header GEP failed".into()))?;
        self.builder
            .build_store(field_ptr, value)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Load the heap buffer pointer out of the header.
    pub(super) fn load_header_buffer(
        &mut self,
        header: PointerValue<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let header_ty = self.collection_header_type();
        let field_ptr = self
            .builder
            .build_struct_gep(header_ty, header, FIELD_BUFFER, "col.buf.ptr")
            .map_err(|_| CodegenError::InternalError("collection header GEP failed".into()))?;
        Ok(self
            .builder
            .build_load(
                self.context.ptr_type(inkwell::AddressSpace::default()),
                field_ptr,
                "col.buf",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value())
    }

    /// Store the heap buffer pointer into the header.
    pub(super) fn store_header_buffer(
        &mut self,
        header: PointerValue<'ctx>,
        buffer: PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        let header_ty = self.collection_header_type();
        let field_ptr = self
            .builder
            .build_struct_gep(header_ty, header, FIELD_BUFFER, "col.buf.ptr")
            .map_err(|_| CodegenError::InternalError("collection header GEP failed".into()))?;
        self.builder
            .build_store(field_ptr, buffer)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Build an `Option<T>` value: `Some(payload)` when `present`, else `None`.
    ///
    /// The instance comes from the call's resolved result type, so the enum layout and
    /// both variant tags are looked up rather than assumed.
    pub(super) fn build_option_value(
        &mut self,
        result_ty: &Type,
        present: IntValue<'ctx>,
        payload: BasicValueEnum<'ctx>,
        payload_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let Type::Enum(enum_name) = result_ty else {
            return Err(CodegenError::InternalError(format!(
                "fallible collection reader has non-enum result type {:?}",
                result_ty
            )));
        };
        let some_tag = self.enum_variant_tag(enum_name, "Some")?;
        let none_tag = self.enum_variant_tag(enum_name, "None")?;

        let some_val = self.codegen_enum_value(enum_name, some_tag, &[(payload, payload_ty)])?;
        let none_val = self.codegen_enum_value(enum_name, none_tag, &[])?;
        self.builder
            .build_select(present, some_val, none_val, "opt")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// Get (creating on first use) a private helper function for a collection
    /// instantiation. `build` emits the body; the function is reused afterwards.
    pub(super) fn get_or_build_helper(
        &mut self,
        name: &str,
        fn_type: inkwell::types::FunctionType<'ctx>,
        build: impl FnOnce(&mut Self, FunctionValue<'ctx>) -> CodegenResult<()>,
    ) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(name) {
            return Ok(existing);
        }
        let func = self.module.add_function(name, fn_type, None);
        func.set_linkage(inkwell::module::Linkage::Private);

        // Helpers are emitted mid-function, so the caller's insertion point and
        // enclosing-function bookkeeping must be restored once the body is complete.
        let saved_block = self.builder.get_insert_block();
        let saved_fn = self.current_function;
        self.current_function = Some(func);
        build(self, func)?;
        self.current_function = saved_fn;
        if let Some(block) = saved_block {
            self.builder.position_at_end(block);
        }
        Ok(func)
    }

    /// The LLVM type of a collection's element/key/value, resolving struct types the
    /// plain type mapper cannot.
    pub(super) fn collection_value_type(&self, ty: &Type) -> CodegenResult<BasicTypeEnum<'ctx>> {
        self.get_any_llvm_type(ty)
    }

    /// The byte size of `llvm_ty`, as an `i64`.
    pub(super) fn size_of_type(
        &self,
        llvm_ty: BasicTypeEnum<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        llvm_ty
            .size_of()
            .ok_or_else(|| CodegenError::InternalError("collection element type is unsized".into()))
    }
}

/// The `index`-th type argument of a collection, or an internal error when the frontend
/// produced a shape the backend cannot read.
pub(super) fn collection_arg(args: &[Type], index: usize) -> CodegenResult<Type> {
    args.get(index).cloned().ok_or_else(|| {
        CodegenError::InternalError(format!("collection type argument {} is missing", index))
    })
}

/// The capacity an empty collection grows to on its first insertion.
pub(super) fn initial_capacity() -> u64 {
    INITIAL_CAPACITY
}
