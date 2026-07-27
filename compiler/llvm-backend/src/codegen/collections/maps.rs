// `HashMap<K, V>` and `BTreeMap<K, V>`.
//
// `HashMap` is open-addressed with linear probing over `{ i8 state, K key, V value }`
// slots and a power-of-two capacity, so the bucket index is a mask rather than a
// division. Removal leaves a tombstone, which keeps a probe run intact; both live and
// tombstoned slots count towards `used`, and the load factor is measured against that,
// so a table churned by insert/remove still rehashes and reclaims its tombstones.
//
// `BTreeMap` keeps `{ K key, V value }` slots sorted by key: lookup is a binary search
// and insertion memmoves the tail. That gives the ordered iteration and total-order
// contract the type promises, with a shape small enough to be verifiable; a real
// multi-way tree only changes the insert/erase constant, not the surface.

use inkwell::types::StructType;
use inkwell::values::{BasicValueEnum, FunctionValue, IntValue, PointerValue};
use inkwell::IntPredicate;
use neuro_hir::HirExpr;

use super::{collection_arg, initial_capacity, FIELD_CAP, FIELD_LEN, FIELD_USED};
use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::{CollectionKind, Type};

/// Hash-map slot states. `EMPTY` must be zero so a zeroed buffer is a valid empty table.
const STATE_EMPTY: u64 = 0;
const STATE_FULL: u64 = 1;
const STATE_TOMBSTONE: u64 = 2;

/// Field indices inside a hash-map slot `{ i8 state, K key, V value }`.
const SLOT_STATE: u32 = 0;
const SLOT_KEY_HASHED: u32 = 1;
const SLOT_VALUE_HASHED: u32 = 2;

/// Field indices inside an ordered-map slot `{ K key, V value }`.
const SLOT_KEY_ORDERED: u32 = 0;
const SLOT_VALUE_ORDERED: u32 = 1;

/// Grow when occupied slots would exceed 3/4 of capacity. Linear probing degrades
/// sharply past that point, and the quarter kept free also bounds the probe run.
const LOAD_NUMERATOR: u64 = 4;
const LOAD_DENOMINATOR: u64 = 3;

/// The sentinel a lookup returns when the key is absent.
const NOT_FOUND: i64 = -1;

impl<'ctx> CodegenContext<'ctx> {
    /// Dispatch a `HashMap` / `BTreeMap` method.
    pub(super) fn codegen_map_method(
        &mut self,
        kind: CollectionKind,
        method: &str,
        header: PointerValue<'ctx>,
        params: &[Type],
        result_ty: &Type,
        args: &[HirExpr],
    ) -> CodegenResult<Option<BasicValueEnum<'ctx>>> {
        let key_ty = collection_arg(params, 0)?;
        let value_ty = collection_arg(params, 1)?;

        match method {
            "insert" => {
                let key = self.lower_map_argument(args, 0, &key_ty)?;
                let value = self.lower_map_argument(args, 1, &value_ty)?;
                let insert = self.build_map_insert_helper(kind, &key_ty, &value_ty)?;
                self.builder
                    .build_call(insert, &[header.into(), key.into(), value.into()], "")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                for index in 0..args.len() {
                    if let Some(arg) = args.get(index) {
                        self.mark_moved_for_drop(arg);
                    }
                }
                Ok(None)
            }
            "get" => {
                let slot = self.emit_map_find(kind, header, &key_ty, &value_ty, args)?;
                let present = self.slot_found(slot)?;
                let value =
                    self.load_slot_value(kind, header, &key_ty, &value_ty, slot, present)?;
                Ok(Some(self.build_option_value(
                    result_ty, present, value, &value_ty,
                )?))
            }
            "contains_key" => {
                let slot = self.emit_map_find(kind, header, &key_ty, &value_ty, args)?;
                Ok(Some(self.slot_found(slot)?.into()))
            }
            "remove" => {
                let slot = self.emit_map_find(kind, header, &key_ty, &value_ty, args)?;
                Ok(Some(
                    self.emit_map_remove(kind, header, &key_ty, &value_ty, slot)?
                        .into(),
                ))
            }
            "keys" => Ok(Some(self.emit_map_keys(kind, header, &key_ty, &value_ty)?)),
            _ => Err(CodegenError::InternalError(format!(
                "unknown map method '{}' reached codegen",
                method
            ))),
        }
    }

    /// The LLVM slot record for a map instantiation.
    pub(super) fn map_slot_type(
        &self,
        kind: CollectionKind,
        key_ty: &Type,
        value_ty: &Type,
    ) -> CodegenResult<StructType<'ctx>> {
        let key_llvm = self.collection_value_type(key_ty)?;
        let value_llvm = self.collection_value_type(value_ty)?;
        Ok(match kind {
            CollectionKind::HashMap => self.context.struct_type(
                &[self.context.i8_type().into(), key_llvm, value_llvm],
                false,
            ),
            _ => self.context.struct_type(&[key_llvm, value_llvm], false),
        })
    }

    /// Evaluate a call argument at the collection's key/value type.
    fn lower_map_argument(
        &mut self,
        args: &[HirExpr],
        index: usize,
        target_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let expr = args.get(index).ok_or_else(|| {
            CodegenError::InternalError(format!("map method is missing argument {}", index))
        })?;
        let value = self.codegen_expr(expr)?;
        let llvm_ty = self.collection_value_type(target_ty)?;
        self.coerce_if_needed(value, llvm_ty, target_ty)
    }

    /// Call the instantiation's lookup helper for the key in `args`, yielding the slot
    /// index or [`NOT_FOUND`].
    fn emit_map_find(
        &mut self,
        kind: CollectionKind,
        header: PointerValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
        args: &[HirExpr],
    ) -> CodegenResult<IntValue<'ctx>> {
        let key = self.lower_map_argument(args, 0, key_ty)?;
        let find = self.build_map_find_helper(kind, key_ty, value_ty)?;
        Ok(self
            .builder
            .build_call(find, &[header.into(), key.into()], "map.find")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("map lookup returned void".into()))?
            .into_int_value())
    }

    /// Whether a lookup result names a live slot.
    fn slot_found(&mut self, slot: IntValue<'ctx>) -> CodegenResult<IntValue<'ctx>> {
        self.builder
            .build_int_compare(
                IntPredicate::SGE,
                slot,
                self.context.i64_type().const_zero(),
                "map.hit",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// Load the value out of slot `slot`, predicated on the lookup having hit — the
    /// buffer may be null, so the load itself must be guarded.
    fn load_slot_value(
        &mut self,
        kind: CollectionKind,
        header: PointerValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
        slot: IntValue<'ctx>,
        present: IntValue<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;
        let value_llvm = self.collection_value_type(value_ty)?;
        let out = self
            .builder
            .build_alloca(value_llvm, "map.read")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(out, value_llvm.const_zero())
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let read_bb = self.context.append_basic_block(parent_fn, "map.read.do");
        let done_bb = self.context.append_basic_block(parent_fn, "map.read.done");
        self.builder
            .build_conditional_branch(present, read_bb, done_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(read_bb);
        let value_ptr = self.map_slot_field_ptr(kind, header, key_ty, value_ty, slot, false)?;
        let value = self
            .builder
            .build_load(value_llvm, value_ptr, "map.val")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(out, value)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unconditional_branch(done_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(done_bb);
        self.builder
            .build_load(value_llvm, out, "map.read.val")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// `m.remove(k)` — `true` when the key was present. The hashed map leaves a
    /// tombstone (its `used` count is unchanged, so the slot is still probed through);
    /// the ordered map closes the gap so the slots stay sorted and dense.
    fn emit_map_remove(
        &mut self,
        kind: CollectionKind,
        header: PointerValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
        slot: IntValue<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;
        let present = self.slot_found(slot)?;
        let do_bb = self.context.append_basic_block(parent_fn, "map.rm.do");
        let done_bb = self.context.append_basic_block(parent_fn, "map.rm.done");
        self.builder
            .build_conditional_branch(present, do_bb, done_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(do_bb);
        match kind {
            CollectionKind::HashMap => {
                let state_ptr = self.hashed_slot_state_ptr(header, key_ty, value_ty, slot)?;
                self.builder
                    .build_store(
                        state_ptr,
                        self.context.i8_type().const_int(STATE_TOMBSTONE, false),
                    )
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            }
            _ => self.shift_ordered_slots_down(header, key_ty, value_ty, slot)?,
        }
        let len = self.load_header_field(header, FIELD_LEN, "map.len")?;
        let shrunk = self
            .builder
            .build_int_sub(len, self.context.i64_type().const_int(1, false), "map.len1")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.store_header_field(header, FIELD_LEN, shrunk)?;
        self.builder
            .build_unconditional_branch(done_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(done_bb);
        Ok(present)
    }

    /// `m.keys()` — a fresh `Vec<K>` of the live keys, in slot order. For the ordered
    /// map that order is ascending, which is what makes its ordering observable.
    fn emit_map_keys(
        &mut self,
        kind: CollectionKind,
        header: PointerValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let helper = self.build_map_keys_helper(kind, key_ty, value_ty)?;
        let key_llvm = self.collection_value_type(key_ty)?;
        let stride = self.size_of_type(key_llvm)?;
        let len = self.load_header_field(header, FIELD_LEN, "map.len")?;
        let bytes = self
            .builder
            .build_int_mul(len, stride, "keys.bytes")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let malloc = self.get_or_declare_malloc();
        let buffer = self
            .builder
            .build_call(malloc, &[bytes.into()], "keys.buf")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("malloc returned void".into()))?
            .into_pointer_value();
        self.builder
            .build_call(helper, &[header.into(), buffer.into()], "")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.vec_header_value(buffer, len)
    }

    /// Address of a slot's key (`key`) or value field.
    fn map_slot_field_ptr(
        &mut self,
        kind: CollectionKind,
        header: PointerValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
        slot: IntValue<'ctx>,
        key: bool,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let slot_ptr = self.map_slot_ptr(kind, header, key_ty, value_ty, slot)?;
        let slot_ty = self.map_slot_type(kind, key_ty, value_ty)?;
        let field = match (kind, key) {
            (CollectionKind::HashMap, true) => SLOT_KEY_HASHED,
            (CollectionKind::HashMap, false) => SLOT_VALUE_HASHED,
            (_, true) => SLOT_KEY_ORDERED,
            (_, false) => SLOT_VALUE_ORDERED,
        };
        self.builder
            .build_struct_gep(slot_ty, slot_ptr, field, "map.slot.field")
            .map_err(|_| CodegenError::InternalError("map slot GEP failed".into()))
    }

    /// Address of the whole slot record at `slot`.
    fn map_slot_ptr(
        &mut self,
        kind: CollectionKind,
        header: PointerValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
        slot: IntValue<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let buffer = self.load_header_buffer(header)?;
        let slot_ty = self.map_slot_type(kind, key_ty, value_ty)?;
        unsafe {
            self.builder
                .build_in_bounds_gep(slot_ty, buffer, &[slot], "map.slot")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))
        }
    }

    /// Address of a hashed slot's state byte.
    fn hashed_slot_state_ptr(
        &mut self,
        header: PointerValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
        slot: IntValue<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let slot_ptr =
            self.map_slot_ptr(CollectionKind::HashMap, header, key_ty, value_ty, slot)?;
        let slot_ty = self.map_slot_type(CollectionKind::HashMap, key_ty, value_ty)?;
        self.builder
            .build_struct_gep(slot_ty, slot_ptr, SLOT_STATE, "map.state.ptr")
            .map_err(|_| CodegenError::InternalError("map state GEP failed".into()))
    }

    /// Close the gap left by an ordered-map removal by shifting the tail one slot down.
    fn shift_ordered_slots_down(
        &mut self,
        header: PointerValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
        slot: IntValue<'ctx>,
    ) -> CodegenResult<()> {
        let i64_ty = self.context.i64_type();
        let len = self.load_header_field(header, FIELD_LEN, "map.len")?;
        let next = self
            .builder
            .build_int_add(slot, i64_ty.const_int(1, false), "rm.next")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let tail = self
            .builder
            .build_int_sub(len, next, "rm.tail")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let slot_ty = self.map_slot_type(CollectionKind::BTreeMap, key_ty, value_ty)?;
        let stride = self.size_of_type(slot_ty.into())?;
        let bytes = self
            .builder
            .build_int_mul(tail, stride, "rm.bytes")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let dst = self.map_slot_ptr(CollectionKind::BTreeMap, header, key_ty, value_ty, slot)?;
        let src = self.map_slot_ptr(CollectionKind::BTreeMap, header, key_ty, value_ty, next)?;
        let memmove = self.get_or_declare_memmove();
        self.builder
            .build_call(memmove, &[dst.into(), src.into(), bytes.into()], "")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// The instantiation-specific helper name, e.g. `__neuro_hmap_find_string_i32`.
    fn map_helper_name(
        &self,
        kind: CollectionKind,
        op: &str,
        key_ty: &Type,
        value_ty: &Type,
    ) -> String {
        format!(
            "__neuro_{}_{}_{}_{}",
            kind.tag(),
            op,
            key_ty.mangle(),
            value_ty.mangle()
        )
    }
}

/// A probe cursor: the stack slot holding the current bucket index, and the capacity it
/// wraps at (always a power of two, so wrapping is a mask).
struct ProbeCursor<'ctx> {
    cursor: PointerValue<'ctx>,
    capacity: IntValue<'ctx>,
}

impl<'ctx> CodegenContext<'ctx> {
    /// Emit the lookup helper: `find(header, key) -> i64`.
    ///
    /// The hashed form walks the probe run from the key's bucket, stopping at the first
    /// `EMPTY` slot (a tombstone continues the run). The ordered form binary-searches the
    /// sorted slots. Both return the slot index, or [`NOT_FOUND`].
    fn build_map_find_helper(
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
    fn emit_hashed_find_body(
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
        let cursor = self
            .builder
            .build_alloca(i64_ty, "cursor")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
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
    fn emit_ordered_find_body(
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
        let result_slot = self
            .builder
            .build_alloca(i64_ty, "result")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
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
    fn emit_lower_bound(
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
        let low = self
            .builder
            .build_alloca(i64_ty, "lo")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let high = self
            .builder
            .build_alloca(i64_ty, "hi")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
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

    /// Emit the insertion helper: `insert(header, key, value)`. An existing key is
    /// overwritten in place; a new key grows the table first if the load factor demands.
    fn build_map_insert_helper(
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
    fn emit_hashed_insert_body(
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
    fn emit_ordered_insert_body(
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

    /// Emit `keys(header, out_buffer)`: copy every live key into `out_buffer`, in slot
    /// order.
    fn build_map_keys_helper(
        &mut self,
        kind: CollectionKind,
        key_ty: &Type,
        value_ty: &Type,
    ) -> CodegenResult<FunctionValue<'ctx>> {
        let name = self.map_helper_name(kind, "keys", key_ty, value_ty);
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = self
            .context
            .void_type()
            .fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let key_ty = key_ty.clone();
        let value_ty = value_ty.clone();

        self.get_or_build_helper(&name, fn_type, move |ctx, func| {
            ctx.emit_map_keys_body(func, kind, &key_ty, &value_ty)
        })
    }

    /// Body of the `keys` helper.
    fn emit_map_keys_body(
        &mut self,
        func: FunctionValue<'ctx>,
        kind: CollectionKind,
        key_ty: &Type,
        value_ty: &Type,
    ) -> CodegenResult<()> {
        let i64_ty = self.context.i64_type();
        let entry = self.context.append_basic_block(func, "entry");
        let cond_bb = self.context.append_basic_block(func, "cond");
        let body_bb = self.context.append_basic_block(func, "body");
        let take_bb = self.context.append_basic_block(func, "take");
        let step_bb = self.context.append_basic_block(func, "step");
        let exit_bb = self.context.append_basic_block(func, "exit");
        self.builder.position_at_end(entry);

        let header = func
            .get_nth_param(0)
            .ok_or_else(|| CodegenError::InternalError("map keys arity".into()))?
            .into_pointer_value();
        let out = func
            .get_nth_param(1)
            .ok_or_else(|| CodegenError::InternalError("map keys arity".into()))?
            .into_pointer_value();

        // A hashed table must scan every slot (live ones are scattered); the ordered
        // one only has to walk its dense prefix.
        let limit = match kind {
            CollectionKind::HashMap => self.load_header_field(header, FIELD_CAP, "cap")?,
            _ => self.load_header_field(header, FIELD_LEN, "len")?,
        };
        let slot_slot = self
            .builder
            .build_alloca(i64_ty, "slot")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let out_slot = self
            .builder
            .build_alloca(i64_ty, "out.i")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        for target in [slot_slot, out_slot] {
            self.builder
                .build_store(target, i64_ty.const_zero())
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        }
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(cond_bb);
        let slot = self
            .builder
            .build_load(i64_ty, slot_slot, "slot.val")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let more = self
            .builder
            .build_int_compare(IntPredicate::ULT, slot, limit, "more")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(more, body_bb, exit_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(body_bb);
        let live = match kind {
            CollectionKind::HashMap => {
                let state = self.load_slot_state(header, key_ty, value_ty, slot)?;
                self.builder
                    .build_int_compare(
                        IntPredicate::EQ,
                        state,
                        self.context.i8_type().const_int(STATE_FULL, false),
                        "live",
                    )
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            }
            _ => self.context.bool_type().const_int(1, false),
        };
        self.builder
            .build_conditional_branch(live, take_bb, step_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(take_bb);
        let key = self.load_slot_key(kind, header, key_ty, value_ty, slot)?;
        let out_index = self
            .builder
            .build_load(i64_ty, out_slot, "out.val")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let key_llvm = self.collection_value_type(key_ty)?;
        let dst = unsafe {
            self.builder
                .build_in_bounds_gep(key_llvm, out, &[out_index], "out.slot")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
        };
        self.builder
            .build_store(dst, key)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let out_next = self
            .builder
            .build_int_add(out_index, i64_ty.const_int(1, false), "out.next")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(out_slot, out_next)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unconditional_branch(step_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(step_bb);
        let next = self
            .builder
            .build_int_add(slot, i64_ty.const_int(1, false), "slot.next")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(slot_slot, next)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(exit_bb);
        self.builder
            .build_return(None)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }
}

/// The `(header, key)` parameters shared by every generated map helper.
fn helper_params<'ctx>(
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
    fn bucket_of(
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
    fn advance_probe(&mut self, state: &ProbeCursor<'ctx>) -> CodegenResult<()> {
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
    fn load_slot_state(
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
    fn load_slot_key(
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
    fn write_hashed_slot(
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
    fn bump_len(&mut self, header: PointerValue<'ctx>) -> CodegenResult<()> {
        let len = self.load_header_field(header, FIELD_LEN, "len")?;
        let next = self
            .builder
            .build_int_add(len, self.context.i64_type().const_int(1, false), "len.next")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.store_header_field(header, FIELD_LEN, next)
    }

    /// Rehash into a table of twice the capacity when the next insertion would push the
    /// occupied slots past the load factor. Rehashing also drops every tombstone, which
    /// is what keeps a churned table's probe runs from growing without bound.
    fn emit_hashed_grow_if_needed(
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
    fn emit_hashed_rehash(
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
        let old_header = self
            .builder
            .build_alloca(self.collection_header_type(), "table.old")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
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

        let cursor = self
            .builder
            .build_alloca(i64_ty, "rehash.i")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
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
    fn probe_for_empty(
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
        let cursor = self
            .builder
            .build_alloca(i64_ty, "probe.i")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
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
    fn emit_ordered_grow_if_needed(
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
    fn build_ordered_reserve_helper(&mut self) -> CodegenResult<FunctionValue<'ctx>> {
        self.build_reserve_helper()
    }
}
