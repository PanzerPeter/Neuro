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
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use inkwell::IntPredicate;
use neuro_hir::HirExpr;

use super::{collection_arg, FIELD_LEN};
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

mod growth;
mod insertion;
mod iteration;
mod lookup;
mod probing;

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
