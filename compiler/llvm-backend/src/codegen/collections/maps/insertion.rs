// Generated `insert(header, key, value)` helper for both map shapes.
//
// One of the map-codegen modules under `maps`; each adds methods to the same
// `impl CodegenContext` block.

use inkwell::values::FunctionValue;
use inkwell::IntPredicate;

use super::probing::{helper_params, ProbeCursor};
use super::{STATE_EMPTY, STATE_FULL};
use crate::codegen::collections::{FIELD_CAP, FIELD_LEN, FIELD_USED};
use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::{CollectionKind, Type};

impl<'ctx> CodegenContext<'ctx> {
    /// Emit the insertion helper: `insert(header, key, value)`. An existing key is
    /// overwritten in place; a new key grows the table first if the load factor demands.
    pub(super) fn build_map_insert_helper(
        &mut self,
        kind: CollectionKind,
        key_ty: &Type,
        value_ty: &Type,
    ) -> CodegenResult<FunctionValue<'ctx>> {
        let name = self.map_helper_name(kind, "insert", key_ty, value_ty);
        let key_llvm = self.collection_value_type(key_ty)?;
        let value_llvm = self.collection_value_type(value_ty)?;
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = self
            .context
            .void_type()
            .fn_type(&[ptr_ty.into(), key_llvm.into(), value_llvm.into()], false);
        let key_ty = key_ty.clone();
        let value_ty = value_ty.clone();

        self.get_or_build_helper(&name, fn_type, move |ctx, func| match kind {
            CollectionKind::HashMap => ctx.emit_hashed_insert_body(func, &key_ty, &value_ty),
            _ => ctx.emit_ordered_insert_body(func, &key_ty, &value_ty),
        })
    }

    /// Body of the hashed insert.
    pub(super) fn emit_hashed_insert_body(
        &mut self,
        func: FunctionValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
    ) -> CodegenResult<()> {
        let i64_ty = self.context.i64_type();
        let entry = self.context.append_basic_block(func, "entry");
        let update_bb = self.context.append_basic_block(func, "update");
        let fresh_bb = self.context.append_basic_block(func, "fresh");
        let probe_bb = self.context.append_basic_block(func, "probe");
        let place_bb = self.context.append_basic_block(func, "place");
        let advance_bb = self.context.append_basic_block(func, "advance");
        let exit_bb = self.context.append_basic_block(func, "exit");
        self.builder.position_at_end(entry);

        let (header, key) = helper_params(func)?;
        let value = func
            .get_nth_param(2)
            .ok_or_else(|| CodegenError::InternalError("map insert arity".into()))?;

        // An existing key keeps its slot, so neither the length nor the load factor moves.
        let find = self.build_map_find_helper(CollectionKind::HashMap, key_ty, value_ty)?;
        let existing = self
            .builder
            .build_call(find, &[header.into(), key.into()], "existing")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("map lookup returned void".into()))?
            .into_int_value();
        let hit = self.slot_found(existing)?;
        self.builder
            .build_conditional_branch(hit, update_bb, fresh_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(update_bb);
        let value_ptr = self.map_slot_field_ptr(
            CollectionKind::HashMap,
            header,
            key_ty,
            value_ty,
            existing,
            false,
        )?;
        self.builder
            .build_store(value_ptr, value)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unconditional_branch(exit_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(fresh_bb);
        self.emit_hashed_grow_if_needed(header, key_ty, value_ty)?;
        let capacity = self.load_header_field(header, FIELD_CAP, "cap")?;
        let cursor = self
            .builder
            .build_alloca(i64_ty, "cursor")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let start = self.bucket_of(key_ty, key, capacity)?;
        self.builder
            .build_store(cursor, start)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unconditional_branch(probe_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        // The first slot that is not FULL takes the entry — an EMPTY one, or a
        // tombstone whose run is thereby reused.
        self.builder.position_at_end(probe_bb);
        let slot = self
            .builder
            .build_load(i64_ty, cursor, "slot")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let state = self.load_slot_state(header, key_ty, value_ty, slot)?;
        let is_full = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                state,
                self.context.i8_type().const_int(STATE_FULL, false),
                "is.full",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(is_full, advance_bb, place_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(advance_bb);
        self.advance_probe(&ProbeCursor { cursor, capacity })?;
        self.builder
            .build_unconditional_branch(probe_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(place_bb);
        // Only an EMPTY slot consumes new capacity; reusing a tombstone does not.
        let was_empty = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                state,
                self.context.i8_type().const_int(STATE_EMPTY, false),
                "was.empty",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.write_hashed_slot(header, key_ty, value_ty, slot, key, value)?;
        let used = self.load_header_field(header, FIELD_USED, "used")?;
        let used_delta = self
            .builder
            .build_int_z_extend(was_empty, i64_ty, "used.delta")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let used_next = self
            .builder
            .build_int_add(used, used_delta, "used.next")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.store_header_field(header, FIELD_USED, used_next)?;
        self.bump_len(header)?;
        self.builder
            .build_unconditional_branch(exit_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(exit_bb);
        self.builder
            .build_return(None)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Body of the ordered insert: overwrite in place, or memmove the tail up and place
    /// the entry at its sorted position.
    pub(super) fn emit_ordered_insert_body(
        &mut self,
        func: FunctionValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
    ) -> CodegenResult<()> {
        let i64_ty = self.context.i64_type();
        let entry = self.context.append_basic_block(func, "entry");
        let update_bb = self.context.append_basic_block(func, "update");
        let fresh_bb = self.context.append_basic_block(func, "fresh");
        let exit_bb = self.context.append_basic_block(func, "exit");
        self.builder.position_at_end(entry);

        let (header, key) = helper_params(func)?;
        let value = func
            .get_nth_param(2)
            .ok_or_else(|| CodegenError::InternalError("map insert arity".into()))?;

        let find = self.build_map_find_helper(CollectionKind::BTreeMap, key_ty, value_ty)?;
        let existing = self
            .builder
            .build_call(find, &[header.into(), key.into()], "existing")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("map lookup returned void".into()))?
            .into_int_value();
        let hit = self.slot_found(existing)?;
        self.builder
            .build_conditional_branch(hit, update_bb, fresh_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(update_bb);
        let value_ptr = self.map_slot_field_ptr(
            CollectionKind::BTreeMap,
            header,
            key_ty,
            value_ty,
            existing,
            false,
        )?;
        self.builder
            .build_store(value_ptr, value)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unconditional_branch(exit_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(fresh_bb);
        self.emit_ordered_grow_if_needed(header, key_ty, value_ty)?;
        let at = self.emit_lower_bound(header, key_ty, value_ty, key)?;
        let len = self.load_header_field(header, FIELD_LEN, "len")?;
        let tail = self
            .builder
            .build_int_sub(len, at, "tail")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let slot_ty = self.map_slot_type(CollectionKind::BTreeMap, key_ty, value_ty)?;
        let stride = self.size_of_type(slot_ty.into())?;
        let bytes = self
            .builder
            .build_int_mul(tail, stride, "tail.bytes")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let next = self
            .builder
            .build_int_add(at, i64_ty.const_int(1, false), "at1")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let dst = self.map_slot_ptr(CollectionKind::BTreeMap, header, key_ty, value_ty, next)?;
        let src = self.map_slot_ptr(CollectionKind::BTreeMap, header, key_ty, value_ty, at)?;
        let memmove = self.get_or_declare_memmove();
        self.builder
            .build_call(memmove, &[dst.into(), src.into(), bytes.into()], "")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let key_ptr =
            self.map_slot_field_ptr(CollectionKind::BTreeMap, header, key_ty, value_ty, at, true)?;
        self.builder
            .build_store(key_ptr, key)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let value_ptr = self.map_slot_field_ptr(
            CollectionKind::BTreeMap,
            header,
            key_ty,
            value_ty,
            at,
            false,
        )?;
        self.builder
            .build_store(value_ptr, value)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.bump_len(header)?;
        self.builder
            .build_unconditional_branch(exit_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(exit_bb);
        self.builder
            .build_return(None)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }
}
