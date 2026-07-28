// Probe-run primitives and raw slot access shared by every generated helper.
//
// One of the map-codegen modules under `maps`; each adds methods to the same
// `impl CodegenContext` block.

use inkwell::values::{BasicValueEnum, FunctionValue, IntValue, PointerValue};

use super::STATE_FULL;
use crate::codegen::collections::FIELD_LEN;
use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::{CollectionKind, Type};

/// A probe cursor: the stack slot holding the current bucket index, and the capacity it
/// wraps at (always a power of two, so wrapping is a mask).
pub(super) struct ProbeCursor<'ctx> {
    pub(super) cursor: PointerValue<'ctx>,
    pub(super) capacity: IntValue<'ctx>,
}

/// The `(header, key)` parameters shared by every generated map helper.
pub(super) fn helper_params<'ctx>(
    func: FunctionValue<'ctx>,
) -> CodegenResult<(PointerValue<'ctx>, BasicValueEnum<'ctx>)> {
    let header = func
        .get_nth_param(0)
        .ok_or_else(|| CodegenError::InternalError("map helper arity".into()))?
        .into_pointer_value();
    let key = func
        .get_nth_param(1)
        .ok_or_else(|| CodegenError::InternalError("map helper arity".into()))?;
    Ok((header, key))
}

impl<'ctx> CodegenContext<'ctx> {
    /// The starting bucket of `key`: `hash & (capacity - 1)`, valid because capacity is
    /// always a power of two.
    pub(super) fn bucket_of(
        &mut self,
        key_ty: &Type,
        key: BasicValueEnum<'ctx>,
        capacity: IntValue<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        let hash = self.emit_key_hash(key_ty, key)?;
        let mask = self
            .builder
            .build_int_sub(
                capacity,
                self.context.i64_type().const_int(1, false),
                "mask",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_and(hash, mask, "bucket")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// Step the probe cursor one slot forward, wrapping at capacity.
    pub(super) fn advance_probe(&mut self, state: &ProbeCursor<'ctx>) -> CodegenResult<()> {
        let i64_ty = self.context.i64_type();
        let current = self
            .builder
            .build_load(i64_ty, state.cursor, "cur")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let stepped = self
            .builder
            .build_int_add(current, i64_ty.const_int(1, false), "cur.next")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let mask = self
            .builder
            .build_int_sub(state.capacity, i64_ty.const_int(1, false), "mask")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let wrapped = self
            .builder
            .build_and(stepped, mask, "cur.wrap")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(state.cursor, wrapped)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Read a hashed slot's state byte.
    pub(super) fn load_slot_state(
        &mut self,
        header: PointerValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
        slot: IntValue<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        let state_ptr = self.hashed_slot_state_ptr(header, key_ty, value_ty, slot)?;
        Ok(self
            .builder
            .build_load(self.context.i8_type(), state_ptr, "state")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value())
    }

    /// Read a slot's key.
    pub(super) fn load_slot_key(
        &mut self,
        kind: CollectionKind,
        header: PointerValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
        slot: IntValue<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let key_ptr = self.map_slot_field_ptr(kind, header, key_ty, value_ty, slot, true)?;
        let key_llvm = self.collection_value_type(key_ty)?;
        self.builder
            .build_load(key_llvm, key_ptr, "slot.key")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// Write a full entry into a hashed slot.
    pub(super) fn write_hashed_slot(
        &mut self,
        header: PointerValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
        slot: IntValue<'ctx>,
        key: BasicValueEnum<'ctx>,
        value: BasicValueEnum<'ctx>,
    ) -> CodegenResult<()> {
        let state_ptr = self.hashed_slot_state_ptr(header, key_ty, value_ty, slot)?;
        self.builder
            .build_store(
                state_ptr,
                self.context.i8_type().const_int(STATE_FULL, false),
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let key_ptr = self.map_slot_field_ptr(
            CollectionKind::HashMap,
            header,
            key_ty,
            value_ty,
            slot,
            true,
        )?;
        self.builder
            .build_store(key_ptr, key)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let value_ptr = self.map_slot_field_ptr(
            CollectionKind::HashMap,
            header,
            key_ty,
            value_ty,
            slot,
            false,
        )?;
        self.builder
            .build_store(value_ptr, value)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Increment the live-entry count.
    pub(super) fn bump_len(&mut self, header: PointerValue<'ctx>) -> CodegenResult<()> {
        let len = self.load_header_field(header, FIELD_LEN, "len")?;
        let next = self
            .builder
            .build_int_add(len, self.context.i64_type().const_int(1, false), "len.next")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.store_header_field(header, FIELD_LEN, next)
    }
}
