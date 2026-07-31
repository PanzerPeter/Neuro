// Capacity growth: the hashed load-factor check and rehash, and the ordered reserve.
//
// One of the map-codegen modules under `maps`; each adds methods to the same
// `impl CodegenContext` block.

use inkwell::values::{FunctionValue, IntValue, PointerValue};
use inkwell::IntPredicate;

use super::probing::ProbeCursor;
use super::{LOAD_DENOMINATOR, LOAD_NUMERATOR, STATE_FULL};
use crate::codegen::collections::{initial_capacity, FIELD_CAP, FIELD_LEN, FIELD_USED};
use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::{CollectionKind, Type};

impl<'ctx> CodegenContext<'ctx> {
    /// Rehash into a table of twice the capacity when the next insertion would push the
    /// occupied slots past the load factor. Rehashing also drops every tombstone, which
    /// is what keeps a churned table's probe runs from growing without bound.
    pub(super) fn emit_hashed_grow_if_needed(
        &mut self,
        header: PointerValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
    ) -> CodegenResult<()> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;
        let i64_ty = self.context.i64_type();
        let used = self.load_header_field(header, FIELD_USED, "used")?;
        let capacity = self.load_header_field(header, FIELD_CAP, "cap")?;
        let next_used = self
            .builder
            .build_int_add(used, i64_ty.const_int(1, false), "used.next")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let scaled_used = self
            .builder
            .build_int_mul(
                next_used,
                i64_ty.const_int(LOAD_NUMERATOR, false),
                "used.scaled",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let scaled_cap = self
            .builder
            .build_int_mul(
                capacity,
                i64_ty.const_int(LOAD_DENOMINATOR, false),
                "cap.scaled",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let needs_growth = self
            .builder
            .build_int_compare(IntPredicate::UGE, scaled_used, scaled_cap, "grow?")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let grow_bb = self.context.append_basic_block(parent_fn, "hmap.grow");
        let done_bb = self.context.append_basic_block(parent_fn, "hmap.grown");
        self.builder
            .build_conditional_branch(needs_growth, grow_bb, done_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(grow_bb);
        self.emit_hashed_rehash(header, key_ty, value_ty, capacity)?;
        self.builder
            .build_unconditional_branch(done_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder.position_at_end(done_bb);
        Ok(())
    }

    /// Allocate a doubled, zeroed table and reinsert every live entry into it.
    pub(super) fn emit_hashed_rehash(
        &mut self,
        header: PointerValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
        capacity: IntValue<'ctx>,
    ) -> CodegenResult<()> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;
        let i64_ty = self.context.i64_type();
        let slot_ty = self.map_slot_type(CollectionKind::HashMap, key_ty, value_ty)?;
        let stride = self.size_of_type(slot_ty.into())?;

        let doubled = self
            .builder
            .build_int_mul(capacity, i64_ty.const_int(2, false), "cap.double")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let min_cap = i64_ty.const_int(initial_capacity(), false);
        let too_small = self
            .builder
            .build_int_compare(IntPredicate::ULT, doubled, min_cap, "cap.small")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let new_cap = self
            .builder
            .build_select(too_small, min_cap, doubled, "cap.new")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let bytes = self
            .builder
            .build_int_mul(new_cap, stride, "cap.bytes")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let malloc = self.get_or_declare_malloc();
        let fresh = self
            .builder
            .build_call(malloc, &[bytes.into()], "table.new")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("malloc returned void".into()))?
            .into_pointer_value();
        let memset = self.get_or_declare_memset();
        self.builder
            .build_call(
                memset,
                &[
                    fresh.into(),
                    self.context.i32_type().const_zero().into(),
                    bytes.into(),
                ],
                "",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        // The old table is read through a private header so the live one can be swapped
        // to the fresh buffer before reinsertion.
        let old_header = self.entry_alloca(self.collection_header_type(), "table.old")?;
        let old_buffer = self.load_header_buffer(header)?;
        self.store_header_buffer(old_header, old_buffer)?;
        self.store_header_field(old_header, FIELD_CAP, capacity)?;

        self.store_header_buffer(header, fresh)?;
        self.store_header_field(header, FIELD_CAP, new_cap)?;
        let zero = i64_ty.const_zero();
        self.store_header_field(header, FIELD_LEN, zero)?;
        self.store_header_field(header, FIELD_USED, zero)?;

        let cond_bb = self.context.append_basic_block(parent_fn, "rehash.cond");
        let body_bb = self.context.append_basic_block(parent_fn, "rehash.body");
        let move_bb = self.context.append_basic_block(parent_fn, "rehash.move");
        let step_bb = self.context.append_basic_block(parent_fn, "rehash.step");
        let exit_bb = self.context.append_basic_block(parent_fn, "rehash.done");

        let cursor = self.entry_alloca(i64_ty, "rehash.i")?;
        self.builder
            .build_store(cursor, zero)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(cond_bb);
        let index = self
            .builder
            .build_load(i64_ty, cursor, "rehash.iv")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let more = self
            .builder
            .build_int_compare(IntPredicate::ULT, index, capacity, "rehash.more")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(more, body_bb, exit_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(body_bb);
        let state = self.load_slot_state(old_header, key_ty, value_ty, index)?;
        let live = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                state,
                self.context.i8_type().const_int(STATE_FULL, false),
                "rehash.live",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(live, move_bb, step_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(move_bb);
        let key =
            self.load_slot_key(CollectionKind::HashMap, old_header, key_ty, value_ty, index)?;
        let value_ptr = self.map_slot_field_ptr(
            CollectionKind::HashMap,
            old_header,
            key_ty,
            value_ty,
            index,
            false,
        )?;
        let value_llvm = self.collection_value_type(value_ty)?;
        let value = self
            .builder
            .build_load(value_llvm, value_ptr, "rehash.val")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let target = self.bucket_of(key_ty, key, new_cap)?;
        let placed = self.probe_for_empty(header, key_ty, value_ty, target, new_cap)?;
        self.write_hashed_slot(header, key_ty, value_ty, placed, key, value)?;
        self.bump_len(header)?;
        let used = self.load_header_field(header, FIELD_USED, "used")?;
        let used_next = self
            .builder
            .build_int_add(used, i64_ty.const_int(1, false), "used.next")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.store_header_field(header, FIELD_USED, used_next)?;
        self.builder
            .build_unconditional_branch(step_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(step_bb);
        let next = self
            .builder
            .build_int_add(index, i64_ty.const_int(1, false), "rehash.next")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(cursor, next)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(exit_bb);
        let free = self.get_or_declare_free();
        self.builder
            .build_call(free, &[old_buffer.into()], "")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Walk forward from `start` to the first non-`FULL` slot. Used during a rehash,
    /// where the destination table is freshly zeroed and every key is distinct, so the
    /// walk always terminates.
    pub(super) fn probe_for_empty(
        &mut self,
        header: PointerValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
        start: IntValue<'ctx>,
        capacity: IntValue<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;
        let i64_ty = self.context.i64_type();
        let cursor = self.entry_alloca(i64_ty, "probe.i")?;
        self.builder
            .build_store(cursor, start)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let cond_bb = self.context.append_basic_block(parent_fn, "probe.cond");
        let step_bb = self.context.append_basic_block(parent_fn, "probe.step");
        let exit_bb = self.context.append_basic_block(parent_fn, "probe.exit");
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(cond_bb);
        let slot = self
            .builder
            .build_load(i64_ty, cursor, "probe.slot")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let state = self.load_slot_state(header, key_ty, value_ty, slot)?;
        let taken = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                state,
                self.context.i8_type().const_int(STATE_FULL, false),
                "probe.taken",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(taken, step_bb, exit_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(step_bb);
        self.advance_probe(&ProbeCursor { cursor, capacity })?;
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(exit_bb);
        Ok(self
            .builder
            .build_load(i64_ty, cursor, "probe.found")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value())
    }

    /// Grow the ordered map's slot array when the next insertion would overflow it.
    pub(super) fn emit_ordered_grow_if_needed(
        &mut self,
        header: PointerValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
    ) -> CodegenResult<()> {
        let slot_ty = self.map_slot_type(CollectionKind::BTreeMap, key_ty, value_ty)?;
        let stride = self.size_of_type(slot_ty.into())?;
        let reserve = self.build_ordered_reserve_helper()?;
        self.builder
            .build_call(reserve, &[header.into(), stride.into()], "")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// The ordered map's growth is the same doubling `realloc` a `Vec` uses, since its
    /// slots are a dense array too — only the stride differs, and that is a parameter.
    pub(super) fn build_ordered_reserve_helper(&mut self) -> CodegenResult<FunctionValue<'ctx>> {
        self.build_reserve_helper()
    }
}
