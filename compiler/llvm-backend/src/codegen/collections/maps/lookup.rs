// Generated `find(header, key) -> i64` helper: hashed probe run and ordered binary search.
//
// One of the map-codegen modules under `maps`; each adds methods to the same
// `impl CodegenContext` block.

use inkwell::values::{BasicValueEnum, FunctionValue, IntValue, PointerValue};
use inkwell::IntPredicate;

use super::probing::{helper_params, ProbeCursor};
use super::{NOT_FOUND, STATE_EMPTY, STATE_FULL};
use crate::codegen::collections::{FIELD_CAP, FIELD_LEN};
use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::{CollectionKind, Type};

impl<'ctx> CodegenContext<'ctx> {
    /// Emit the lookup helper: `find(header, key) -> i64`.
    ///
    /// The hashed form walks the probe run from the key's bucket, stopping at the first
    /// `EMPTY` slot (a tombstone continues the run). The ordered form binary-searches the
    /// sorted slots. Both return the slot index, or [`NOT_FOUND`].
    pub(super) fn build_map_find_helper(
        &mut self,
        kind: CollectionKind,
        key_ty: &Type,
        value_ty: &Type,
    ) -> CodegenResult<FunctionValue<'ctx>> {
        let name = self.map_helper_name(kind, "find", key_ty, value_ty);
        let key_llvm = self.collection_value_type(key_ty)?;
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = self
            .context
            .i64_type()
            .fn_type(&[ptr_ty.into(), key_llvm.into()], false);
        let key_ty = key_ty.clone();
        let value_ty = value_ty.clone();

        self.get_or_build_helper(&name, fn_type, move |ctx, func| match kind {
            CollectionKind::HashMap => ctx.emit_hashed_find_body(func, &key_ty, &value_ty),
            _ => ctx.emit_ordered_find_body(func, &key_ty, &value_ty),
        })
    }

    /// Body of the hashed lookup: linear probing from `hash & (cap - 1)`.
    pub(super) fn emit_hashed_find_body(
        &mut self,
        func: FunctionValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
    ) -> CodegenResult<()> {
        let i64_ty = self.context.i64_type();
        let entry = self.context.append_basic_block(func, "entry");
        let empty_bb = self.context.append_basic_block(func, "empty.table");
        let probe_bb = self.context.append_basic_block(func, "probe");
        let occupied_bb = self.context.append_basic_block(func, "occupied");
        let compare_bb = self.context.append_basic_block(func, "compare");
        let advance_bb = self.context.append_basic_block(func, "advance");
        let hit_bb = self.context.append_basic_block(func, "hit");
        let miss_bb = self.context.append_basic_block(func, "miss");
        self.builder.position_at_end(entry);

        let (header, key) = helper_params(func)?;
        let capacity = self.load_header_field(header, FIELD_CAP, "cap")?;
        let has_slots = self
            .builder
            .build_int_compare(IntPredicate::UGT, capacity, i64_ty.const_zero(), "any")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let cursor = self.entry_alloca(i64_ty, "cursor")?;
        let start = self.bucket_of(key_ty, key, capacity)?;
        self.builder
            .build_store(cursor, start)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(has_slots, probe_bb, empty_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(empty_bb);
        self.builder
            .build_unconditional_branch(miss_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        // Stop at the first EMPTY slot: the key would have been placed there.
        self.builder.position_at_end(probe_bb);
        let slot = self
            .builder
            .build_load(i64_ty, cursor, "slot")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let state = self.load_slot_state(header, key_ty, value_ty, slot)?;
        let is_empty = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                state,
                self.context.i8_type().const_int(STATE_EMPTY, false),
                "is.empty",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(is_empty, miss_bb, occupied_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        // A tombstone is skipped; only a FULL slot's key is compared.
        self.builder.position_at_end(occupied_bb);
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
            .build_conditional_branch(is_full, compare_bb, advance_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(compare_bb);
        let stored = self.load_slot_key(CollectionKind::HashMap, header, key_ty, value_ty, slot)?;
        let same = self.emit_key_eq(key_ty, stored, key)?;
        self.builder
            .build_conditional_branch(same, hit_bb, advance_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(advance_bb);
        self.advance_probe(&ProbeCursor { cursor, capacity })?;
        self.builder
            .build_unconditional_branch(probe_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(hit_bb);
        let found = self
            .builder
            .build_load(i64_ty, cursor, "found")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_return(Some(&found))
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(miss_bb);
        let missing = i64_ty.const_int(NOT_FOUND as u64, true);
        self.builder
            .build_return(Some(&missing))
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Body of the ordered lookup: binary search, returning the matching index or
    /// [`NOT_FOUND`].
    pub(super) fn emit_ordered_find_body(
        &mut self,
        func: FunctionValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
    ) -> CodegenResult<()> {
        let i64_ty = self.context.i64_type();
        let entry = self.context.append_basic_block(func, "entry");
        let exit_bb = self.context.append_basic_block(func, "exit");
        self.builder.position_at_end(entry);

        let (header, key) = helper_params(func)?;
        let bound = self.emit_lower_bound(header, key_ty, value_ty, key)?;
        let len = self.load_header_field(header, FIELD_LEN, "len")?;

        // `lower_bound` is the first slot not ordered before the key; it names a match
        // exactly when it is in range and its key is not ordered after the key either.
        let in_range = self
            .builder
            .build_int_compare(IntPredicate::ULT, bound, len, "in.range")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let match_bb = self.context.append_basic_block(func, "check.match");
        let result_slot = self.entry_alloca(i64_ty, "result")?;
        self.builder
            .build_store(result_slot, i64_ty.const_int(NOT_FOUND as u64, true))
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(in_range, match_bb, exit_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(match_bb);
        let stored =
            self.load_slot_key(CollectionKind::BTreeMap, header, key_ty, value_ty, bound)?;
        let greater = self.emit_key_lt(key_ty, key, stored)?;
        let equal = self
            .builder
            .build_not(greater, "is.equal")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let chosen = self
            .builder
            .build_select(
                equal,
                bound,
                i64_ty.const_int(NOT_FOUND as u64, true),
                "chosen",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(result_slot, chosen)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unconditional_branch(exit_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(exit_bb);
        let result = self
            .builder
            .build_load(i64_ty, result_slot, "result.val")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_return(Some(&result))
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Binary search for the first slot whose key is not ordered before `key`.
    pub(super) fn emit_lower_bound(
        &mut self,
        header: PointerValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
        key: BasicValueEnum<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;
        let i64_ty = self.context.i64_type();
        let low = self.entry_alloca(i64_ty, "lo")?;
        let high = self.entry_alloca(i64_ty, "hi")?;
        let len = self.load_header_field(header, FIELD_LEN, "len")?;
        self.builder
            .build_store(low, i64_ty.const_zero())
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(high, len)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let cond_bb = self.context.append_basic_block(parent_fn, "bs.cond");
        let body_bb = self.context.append_basic_block(parent_fn, "bs.body");
        let go_right = self.context.append_basic_block(parent_fn, "bs.right");
        let go_left = self.context.append_basic_block(parent_fn, "bs.left");
        let exit_bb = self.context.append_basic_block(parent_fn, "bs.exit");
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(cond_bb);
        let lo = self
            .builder
            .build_load(i64_ty, low, "lo.val")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let hi = self
            .builder
            .build_load(i64_ty, high, "hi.val")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let more = self
            .builder
            .build_int_compare(IntPredicate::ULT, lo, hi, "bs.more")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(more, body_bb, exit_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(body_bb);
        let span = self
            .builder
            .build_int_sub(hi, lo, "bs.span")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let half = self
            .builder
            .build_right_shift(span, i64_ty.const_int(1, false), false, "bs.half")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let mid = self
            .builder
            .build_int_add(lo, half, "bs.mid")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let stored = self.load_slot_key(CollectionKind::BTreeMap, header, key_ty, value_ty, mid)?;
        let before = self.emit_key_lt(key_ty, stored, key)?;
        self.builder
            .build_conditional_branch(before, go_right, go_left)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(go_right);
        let after_mid = self
            .builder
            .build_int_add(mid, i64_ty.const_int(1, false), "bs.mid1")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(low, after_mid)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(go_left);
        self.builder
            .build_store(high, mid)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(exit_bb);
        Ok(self
            .builder
            .build_load(i64_ty, low, "bound")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value())
    }
}
