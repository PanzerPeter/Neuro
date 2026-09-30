//! Destroying the value a store displaces: a reassigned binding, a held position, an
//! array element, and re-arming the flags of what replaced it.

use inkwell::IntPredicate;
use inkwell::types::ArrayType;
use inkwell::values::PointerValue;
use neuro_hir::{HirExpr, HirExprKind, HirType};

use crate::codegen::context::{CodegenContext, DropEntry, DropTarget};
use crate::errors::{CodegenError, CodegenResult};

/// The receiver's name in a method body.
const SELF_BINDING: &str = "self";

impl<'ctx> CodegenContext<'ctx> {
    /// Release the value a reassigned binding is about to lose, and hand back its drop
    /// flag and target so the caller can re-arm them for the incoming value.
    ///
    /// The caller must have evaluated the new value BEFORE calling this: a reassignment
    /// is allowed to read the binding it overwrites (`s = s + "!"` concatenates out of
    /// the very buffer this then frees), so releasing first would hand the concatenation
    /// freed memory.
    ///
    /// Two bindings answer `None` and keep their prior value. One the drop pass never
    /// tracked owns nothing to release. A pool-registered one is released by the arena's
    /// LIFO sweep at the block's closing brace and by nothing else, so a per-assignment
    /// release would free a pointer the arena still holds.
    pub(crate) fn drop_reassigned_value(
        &mut self,
        name: &str,
    ) -> CodegenResult<Option<(PointerValue<'ctx>, DropTarget)>> {
        let entry = self.live_drop_entry(name).map(Self::snapshot_entry);

        let Some(pending) = entry else {
            return Ok(None);
        };
        let (_, flag_ptr, target) = pending[0].clone();
        if matches!(target, DropTarget::PoolRegistered) {
            return Ok(None);
        }

        for (storage_ptr, flag_ptr, target) in pending {
            self.emit_one_drop(storage_ptr, flag_ptr, &target)?;
        }
        Ok(Some((flag_ptr, target)))
    }

    /// Release what `name`'s held position at `path` is about to lose to a field
    /// assignment, then re-arm that position for the value replacing it.
    ///
    /// Ordered like [`drop_reassigned_value`] and for the same reason: the caller has
    /// already evaluated the new value, so the field may be read on the way to
    /// replacing itself. The storage the position addresses does not move, so re-arming
    /// is a flag store and needs no second plan.
    ///
    /// A `string` position is released here but left disarmed: `p.name = "lit"`
    /// displaces an owner and takes on none, so what re-arms it is the incoming value's
    /// own shape, through [`arm_stored_string_positions`].
    pub(crate) fn drop_displaced_held_value(
        &mut self,
        name: &str,
        path: &[String],
    ) -> CodegenResult<()> {
        let pending: Vec<(PointerValue<'ctx>, PointerValue<'ctx>, DropTarget)> = self
            .live_drop_entry(name)
            .map(|entry| {
                entry
                    .held
                    .iter()
                    .filter(|held| held.path.starts_with(path))
                    .map(|held| (held.storage_ptr, held.flag_ptr, held.target.clone()))
                    .collect()
            })
            .unwrap_or_default();

        let armed = self.context.bool_type().const_int(1, false);
        for (storage_ptr, flag_ptr, target) in pending {
            self.emit_one_drop(storage_ptr, flag_ptr, &target)?;
            if matches!(target, DropTarget::HeapString) {
                continue;
            }
            self.builder.build_store(flag_ptr, armed)?;
        }
        Ok(())
    }

    /// Release what the position `object.segment` loses to a store, then arm it for
    /// `value`. A segment is a field name or a literal array position, at any depth.
    ///
    /// Only a position inside a binding this frame owns has flags to consult; a place
    /// reached through a borrow belongs to whoever lent it and is left alone here.
    pub(crate) fn displace_held_position(
        &mut self,
        object: &HirExpr,
        segment: &str,
        value: &HirExpr,
    ) -> CodegenResult<()> {
        let Some((root, Some(path))) = Self::extend_place(object, segment) else {
            return Ok(());
        };
        let root = root.to_string();
        self.drop_displaced_held_value(&root, &path)?;
        // A `string` position the store hands a fresh buffer takes ownership of it here:
        // the release above left every such position disarmed, because the type says
        // nothing about what the incoming value owns.
        self.arm_stored_string_positions(&root, &path, value)
    }

    /// Whether the place `expr` names is reached through a reference: a `&mut` binding
    /// or receiver, a `*r`, or a field or element of one at any depth. Such a place
    /// belongs to whoever lent it, so no drop entry of this frame tracks it, and what a
    /// store there displaces is released by
    /// [`drop_displaced_through_borrow`](CodegenContext::drop_displaced_through_borrow).
    pub(crate) fn reached_through_borrow(&self, expr: &HirExpr) -> bool {
        if matches!(expr.ty, HirType::Reference { .. }) {
            return true;
        }
        match &expr.kind {
            // A `&mut self` receiver is typed as the struct it points at.
            HirExprKind::Variable(name) => name == SELF_BINDING && self.borrowed_self,
            HirExprKind::Deref { .. } => true,
            HirExprKind::FieldAccess { object, .. }
            | HirExprKind::TupleIndex { object, .. }
            | HirExprKind::Index { object, .. }
            | HirExprKind::NewtypeAccess { object } => self.reached_through_borrow(object),
            _ => false,
        }
    }

    /// [`displace_held_position`] for an array element whose position is known only at
    /// run time: the address being written is compared against each element the owner
    /// tracks, and the one it matches is released and re-armed.
    pub(crate) fn displace_array_element(
        &mut self,
        object: &HirExpr,
        array_llvm: ArrayType<'ctx>,
        array_ptr: PointerValue<'ctx>,
        element_ptr: PointerValue<'ctx>,
        value: &HirExpr,
    ) -> CodegenResult<()> {
        let Some((root, Some(prefix))) = Self::moved_place(object) else {
            return Ok(());
        };
        let root = root.to_string();
        let Some(entry) = self.live_drop_entry(&root) else {
            return Ok(());
        };
        let tracked: Vec<Vec<String>> = (0..array_llvm.len())
            .map(|k| {
                let mut path = prefix.clone();
                path.push(k.to_string());
                path
            })
            .filter(|path| entry.held.iter().any(|held| held.path.starts_with(path)))
            .collect();
        if tracked.is_empty() {
            return Ok(());
        }

        let parent_fn = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("element store emitted outside a function".to_string())
        })?;
        let i64t = self.context.i64_type();
        let written = self
            .builder
            .build_ptr_to_int(element_ptr, i64t, "elem.addr")?;
        // ponytail: one compare per tracked element; switch on the index instead if
        // arrays of owners grow large enough for the chain to matter.
        for path in tracked {
            let position: u64 = path
                .last()
                .and_then(|segment| segment.parse().ok())
                .ok_or_else(|| {
                    CodegenError::InternalError("array position is not a number".to_string())
                })?;
            // SAFETY: `position` is below the array's length, so the GEP stays inside it.
            let position_ptr = unsafe {
                self.builder.build_in_bounds_gep(
                    array_llvm,
                    array_ptr,
                    &[i64t.const_zero(), i64t.const_int(position, false)],
                    "held.elem.ptr",
                )?
            };
            let position_addr =
                self.builder
                    .build_ptr_to_int(position_ptr, i64t, "held.elem.addr")?;
            let hit = self.builder.build_int_compare(
                IntPredicate::EQ,
                written,
                position_addr,
                "held.elem.hit",
            )?;
            let release_bb = self.context.append_basic_block(parent_fn, "displace.elem");
            let next_bb = self.context.append_basic_block(parent_fn, "displace.next");
            self.builder
                .build_conditional_branch(hit, release_bb, next_bb)?;
            self.builder.position_at_end(release_bb);
            self.drop_displaced_held_value(&root, &path)?;
            self.arm_stored_string_positions(&root, &path, value)?;
            self.builder.build_unconditional_branch(next_bb)?;
            self.builder.position_at_end(next_bb);
        }
        Ok(())
    }

    /// The drop work one entry stands for, innermost-holder first: the binding's own
    /// release, then each owner it holds in declaration order.
    pub(super) fn snapshot_entry(
        entry: &DropEntry<'ctx>,
    ) -> Vec<(PointerValue<'ctx>, PointerValue<'ctx>, DropTarget)> {
        let mut pending = vec![(entry.storage_ptr, entry.flag_ptr, entry.target.clone())];
        pending.extend(
            entry
                .held
                .iter()
                .map(|held| (held.storage_ptr, held.flag_ptr, held.target.clone())),
        );
        pending
    }

    /// Arm `flag_ptr` for the value a reassignment has just stored, so scope exit
    /// releases the incoming value rather than the one already released.
    ///
    /// Ownership is re-derived from the assigned expression, not assumed: a `string`
    /// binding owns a buffer only when the expression that produced it allocated one, so
    /// reassigning a heap string from a `.rodata` literal must leave the flag clear.
    /// Every other target's type proves the ownership on its own.
    pub(crate) fn rearm_drop_flag(
        &mut self,
        flag_ptr: PointerValue<'ctx>,
        target: &DropTarget,
        value: &HirExpr,
    ) -> CodegenResult<()> {
        let owns_new_value = match target {
            DropTarget::HeapString => self.produces_owned_string(value),
            _ => true,
        };
        let bool_ty = self.context.bool_type();
        self.builder
            .build_store(flag_ptr, bool_ty.const_int(owns_new_value as u64, false))?;
        Ok(())
    }

    /// Re-arm every held position of a reassigned binding, so scope exit releases what
    /// the incoming holder holds rather than what the released one held.
    ///
    /// Unconditional for a position whose own type proves it owns something, which no
    /// assignment can change. A `string` position is skipped and left disarmed: its
    /// ownership comes from the expression stored into it, so
    /// [`arm_stored_string_positions`] arms it from the incoming value instead.
    pub(crate) fn rearm_held_drop_flags(&mut self, name: &str) -> CodegenResult<()> {
        let flags: Vec<PointerValue<'ctx>> = self
            .live_drop_entry(name)
            .map(|entry| {
                entry
                    .held
                    .iter()
                    .filter(|held| !matches!(held.target, DropTarget::HeapString))
                    .map(|held| held.flag_ptr)
                    .collect()
            })
            .unwrap_or_default();

        let armed = self.context.bool_type().const_int(1, false);
        for flag_ptr in flags {
            self.builder.build_store(flag_ptr, armed)?;
        }
        Ok(())
    }
}
