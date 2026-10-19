//! The ownership flags of the `string` positions inside a holder: armed by the store
//! that allocated into them, handed over when the holder moves.

use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use neuro_hir::{HirExpr, HirExprKind};

use crate::codegen::context::{BranchStringSlots, CodegenContext, DropTarget};
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

use super::peel_newtype;

impl<'ctx> CodegenContext<'ctx> {
    /// Arm the `string` positions of `name` that `value` stored a freshly allocated
    /// buffer into, so the holder's destruction releases them.
    ///
    /// Reads the holder's literal shape rather than its type: `P { name: a + b }` owns
    /// the concatenation and `P { name: "lit" }` owns nothing, and only the expression
    /// tells the two apart. A call arms the positions its callee always allocated
    /// (`StringOwnership::held_strings_returned`). A holder built any other way, read
    /// out of another value say, arms nothing here, because the buffer behind its
    /// positions may be one something else still owns; a branch's arms hand theirs over
    /// through [`codegen_branch_tail`].
    pub(crate) fn arm_stored_string_positions(
        &mut self,
        name: &str,
        prefix: &[String],
        value: &HirExpr,
    ) -> CodegenResult<()> {
        // A holder forwarder's positions are owned exactly when the arguments it stored
        // there were, which the call read before moving them.
        let forwarded = match &peel_newtype(value).kind {
            HirExprKind::Call { callee, .. } => self
                .summary_key(callee)
                .and_then(|key| match self.forwarded_holder_owners.take() {
                    Some((called, owners)) if called == key => Some(owners),
                    _ => None,
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        for (path, owner) in forwarded {
            let path: Vec<String> = prefix.iter().chain(&path).cloned().collect();
            let flag_ptr = self.live_drop_entry(name).and_then(|entry| {
                entry
                    .held
                    .iter()
                    .find(|held| held.path == path && matches!(held.target, DropTarget::HeapString))
                    .map(|held| held.flag_ptr)
            });
            if let Some(flag_ptr) = flag_ptr {
                self.builder.build_store(flag_ptr, owner)?;
            }
        }

        let mut armed: Vec<Vec<String>> = Vec::new();
        self.collect_armed_positions(value, &mut prefix.to_vec(), &mut armed);
        if armed.is_empty() {
            return Ok(());
        }

        let flags: Vec<PointerValue<'ctx>> = self
            .live_drop_entry(name)
            .map(|entry| {
                entry
                    .held
                    .iter()
                    .filter(|held| matches!(held.target, DropTarget::HeapString))
                    .filter(|held| armed.iter().any(|path| path == &held.path))
                    .map(|held| held.flag_ptr)
                    .collect()
            })
            .unwrap_or_default();

        let armed_flag = self.context.bool_type().const_int(1, false);
        for flag_ptr in flags {
            self.builder.build_store(flag_ptr, armed_flag)?;
        }
        Ok(())
    }

    /// The current flags of the `string` positions the holder `expr` names, keyed by
    /// path: which of them own their buffer at this point of the run. Empty unless `expr`
    /// is a bare binding, the one holder whose positions a move hands over whole.
    pub(crate) fn load_held_string_flags(
        &mut self,
        expr: &HirExpr,
    ) -> CodegenResult<Vec<(Vec<String>, IntValue<'ctx>)>> {
        let HirExprKind::Variable(name) = &peel_newtype(expr).kind else {
            return Ok(Vec::new());
        };
        let positions: Vec<(Vec<String>, PointerValue<'ctx>)> = self
            .live_drop_entry(name)
            .map(|entry| {
                entry
                    .held
                    .iter()
                    .filter(|held| matches!(held.target, DropTarget::HeapString))
                    .map(|held| (held.path.clone(), held.flag_ptr))
                    .collect()
            })
            .unwrap_or_default();
        let bool_ty = self.context.bool_type();
        let mut flags = Vec::with_capacity(positions.len());
        for (path, flag_ptr) in positions {
            let owns = self
                .builder
                .build_load(bool_ty, flag_ptr, "held.str.owns")?
                .into_int_value();
            flags.push((path, owns));
        }
        Ok(flags)
    }

    /// Which `string` positions of a value of type `ty` that `value` owns, by path: `true`
    /// for one it provably allocated, the runtime flag for one a moved binding hands over,
    /// and nothing for the rest. Read before the store's move clears those flags.
    pub(crate) fn stored_string_owners(
        &mut self,
        value: &HirExpr,
        ty: &Type,
    ) -> CodegenResult<Vec<(Vec<String>, IntValue<'ctx>)>> {
        if matches!(ty, Type::String) {
            return Ok(vec![(Vec::new(), self.argument_string_owner(value)?)]);
        }
        let mut proven = Vec::new();
        self.collect_armed_positions(value, &mut Vec::new(), &mut proven);
        let armed = self.context.bool_type().const_int(1, false);
        let mut owners: Vec<(Vec<String>, IntValue<'ctx>)> =
            proven.into_iter().map(|path| (path, armed)).collect();
        owners.extend(self.load_held_string_flags(value)?);
        Ok(owners)
    }

    /// Make every `string` position of the `ty` value just stored at `place_ptr` through a
    /// borrow own its bytes: a position `owners` does not say is owned gets a heap copy.
    ///
    /// Whoever lent the place releases it by a flag this store cannot see, armed or not.
    /// A `.rodata` literal left under an armed flag is handed to `free` at the owner's
    /// scope exit, which aborts; a heap copy is at worst a leak under a clear one. The
    /// copy comes from `malloc` even inside a `pool`: the place outlives the block.
    pub(crate) fn own_strings_stored_through_borrow(
        &mut self,
        place_ptr: PointerValue<'ctx>,
        ty: &Type,
        owners: &[(Vec<String>, IntValue<'ctx>)],
    ) -> CodegenResult<()> {
        let mut positions = Vec::new();
        self.string_position_ptrs(place_ptr, ty, &mut Vec::new(), &mut positions)?;
        let parent_fn = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("a store emitted outside a function".to_string())
        })?;
        let string_llvm = self.get_any_llvm_type(&Type::String)?;
        for (path, position_ptr) in positions {
            let owned = owners
                .iter()
                .find(|(owned_path, _)| *owned_path == path)
                .map(|(_, owns)| *owns);
            if owned.is_some_and(|owns| owns.get_zero_extended_constant() == Some(1)) {
                continue;
            }
            let copy_bb = self
                .context
                .append_basic_block(parent_fn, "borrowed.str.copy");
            let done_bb = self
                .context
                .append_basic_block(parent_fn, "borrowed.str.done");
            match owned {
                Some(owns) => self
                    .builder
                    .build_conditional_branch(owns, done_bb, copy_bb)?,
                None => self.builder.build_unconditional_branch(copy_bb)?,
            };
            self.builder.position_at_end(copy_bb);
            let bytes = self
                .builder
                .build_load(string_llvm, position_ptr, "borrowed.str")?;
            let depth = std::mem::replace(&mut self.pool_depth, 0);
            let copy = self.copy_string_bytes(bytes);
            self.pool_depth = depth;
            self.builder.build_store(position_ptr, copy?)?;
            self.builder.build_unconditional_branch(done_bb)?;
            self.builder.position_at_end(done_bb);
        }
        Ok(())
    }

    /// The address of every `string` position of a `ty` value at `base_ptr`, the value
    /// itself included when it is one, keyed by the paths [`plan_held_drops`] uses.
    fn string_position_ptrs(
        &mut self,
        base_ptr: PointerValue<'ctx>,
        ty: &Type,
        path: &mut Vec<String>,
        out: &mut Vec<(Vec<String>, PointerValue<'ctx>)>,
    ) -> CodegenResult<()> {
        if matches!(ty, Type::String) {
            out.push((path.clone(), base_ptr));
            return Ok(());
        }
        let positions = self.held_positions(ty);
        if positions.is_empty() {
            return Ok(());
        }
        let holder_llvm = self.get_any_llvm_type(ty)?;
        for (index, (segment, position_ty)) in positions.into_iter().enumerate() {
            if !matches!(position_ty, Type::String) && !self.holds_string_position(&position_ty) {
                continue;
            }
            let position_ptr =
                self.aggregate_position_ptr(holder_llvm, base_ptr, index as u32, "borrowed.pos")?;
            path.push(segment);
            self.string_position_ptrs(position_ptr, &position_ty, path, out)?;
            let _ = path.pop();
        }
        Ok(())
    }

    /// Store flags [`load_held_string_flags`] read off a moved holder into the same
    /// positions of `name`, the holder that took the value.
    pub(crate) fn store_held_string_flags(
        &mut self,
        name: &str,
        flags: &[(Vec<String>, IntValue<'ctx>)],
    ) -> CodegenResult<()> {
        let targets: Vec<(PointerValue<'ctx>, IntValue<'ctx>)> = self
            .live_drop_entry(name)
            .map(|entry| {
                flags
                    .iter()
                    .filter_map(|(path, owns)| {
                        entry
                            .held
                            .iter()
                            .find(|held| {
                                matches!(held.target, DropTarget::HeapString) && &held.path == path
                            })
                            .map(|held| (held.flag_ptr, *owns))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (flag_ptr, owns) in targets {
            self.builder.build_store(flag_ptr, owns)?;
        }
        Ok(())
    }

    /// Evaluate `value` into position `segment` of the aggregate literal being built, and
    /// move it in: the place it names stops owning what it held.
    ///
    /// While a holder is being built ([`CodegenContext::literal_string_moves`]), a `string`
    /// binding moved in hands its runtime flag to the position, read before the move clears
    /// it. A nested literal extends the path; anything else is evaluated with the collection
    /// suspended, because a literal inside it (a call's argument, a branch) builds a value
    /// that is not this holder's.
    pub(crate) fn codegen_literal_position(
        &mut self,
        segment: String,
        value: &HirExpr,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let nested = matches!(
            value.kind,
            HirExprKind::StructLiteral { .. }
                | HirExprKind::TupleLiteral { .. }
                | HirExprKind::ArrayLiteral { .. }
        );
        let suspended = if nested {
            if let Some(moves) = &mut self.literal_string_moves {
                moves.path.push(segment.clone());
            }
            None
        } else {
            self.literal_string_moves.take()
        };
        let val = self.codegen_expr(value);
        if nested {
            if let Some(moves) = &mut self.literal_string_moves {
                let _ = moves.path.pop();
            }
        } else {
            self.literal_string_moves = suspended;
        }
        let val = val?;
        if self.literal_string_moves.is_some()
            && matches!(Type::from_hir(&value.ty), Type::String)
            && let Some(owns) = self.load_owned_string_flag(value)?
            && let Some(moves) = &mut self.literal_string_moves
        {
            let mut path = moves.path.clone();
            path.push(segment);
            moves.flags.push((path, owns));
        }
        self.mark_moved_for_drop(value)?;
        Ok(val)
    }

    /// One cleared slot per `string` position of `ty`, for the arms of the branch `value`
    /// to store their positions' ownership into (`None` unless `value` is a branch
    /// building a holder of `string` positions). A literal or a value of unproven origin
    /// in an arm leaves its slot clear, so it is never released.
    pub(crate) fn open_branch_string_slots(
        &mut self,
        value: &HirExpr,
        ty: &Type,
    ) -> CodegenResult<Option<BranchStringSlots<'ctx>>> {
        if !Self::is_branch(value) {
            return Ok(None);
        }
        let paths = self.string_position_paths(ty);
        if paths.is_empty() {
            return Ok(None);
        }
        let bool_ty = self.context.bool_type();
        let mut slots = Vec::with_capacity(paths.len());
        for path in paths {
            let slot = self.entry_alloca(bool_ty, "branch.str.owns")?;
            self.builder.build_store(slot, bool_ty.const_zero())?;
            slots.push((path, slot));
        }
        Ok(Some(slots))
    }

    /// The ownership the branch's arms left in `slots`, keyed by path, for
    /// [`store_held_string_flags`] to hand to the holder that took the value.
    pub(crate) fn load_branch_string_slots(
        &mut self,
        slots: &[(Vec<String>, PointerValue<'ctx>)],
    ) -> CodegenResult<Vec<(Vec<String>, IntValue<'ctx>)>> {
        let bool_ty = self.context.bool_type();
        let mut flags = Vec::with_capacity(slots.len());
        for (path, slot) in slots {
            let owns = self
                .builder
                .build_load(bool_ty, *slot, "branch.str.owned")?
                .into_int_value();
            flags.push((path.clone(), owns));
        }
        Ok(flags)
    }

    /// Evaluate `expr`, the tail an arm of a holder-building branch yields, and store
    /// into `slots` the ownership of each `string` position it hands over: armed for a
    /// provable allocation, the moved binding's flag for a binding (whole, or moved into
    /// a literal position), clear for the rest. A tail that is itself a branch passes the
    /// slots on to its own arms; a tail that diverges stores nothing. Read before the
    /// caller's move of `expr` clears the flags.
    pub(crate) fn codegen_branch_tail(
        &mut self,
        expr: &HirExpr,
        slots: Option<&[(Vec<String>, PointerValue<'ctx>)]>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let Some(slots) = slots else {
            return self.codegen_expr(expr);
        };
        if Self::is_branch(expr) {
            self.branch_string_slots = Some(slots.to_vec());
            let val = self.codegen_expr(expr);
            self.branch_string_slots = None;
            return val;
        }
        let collecting = Self::is_aggregate_literal(expr).then(Default::default);
        let outer = std::mem::replace(&mut self.literal_string_moves, collecting);
        let val = self.codegen_expr(expr);
        let mut owners = std::mem::replace(&mut self.literal_string_moves, outer)
            .map(|moves| moves.flags)
            .unwrap_or_default();
        let val = val?;
        if self.current_block_terminated() {
            return Ok(val);
        }
        let mut proven = Vec::new();
        self.collect_armed_positions(expr, &mut Vec::new(), &mut proven);
        let armed = self.context.bool_type().const_int(1, false);
        owners.extend(proven.into_iter().map(|path| (path, armed)));
        owners.extend(self.load_held_string_flags(expr)?);
        for (path, slot) in slots {
            if let Some((_, owns)) = owners.iter().find(|(owned, _)| owned == path) {
                self.builder.build_store(*slot, *owns)?;
            }
        }
        Ok(val)
    }

    /// Whether `value` is a branch whose arms' tails [`codegen_branch_tail`] follows.
    ///
    /// Every kind listed here must `.take()` [`CodegenContext::branch_string_slots`] as
    /// the first step of its codegen function, before it evaluates anything, or a
    /// condition, scrutinee or earlier statement would store into the holder's slots.
    fn is_branch(value: &HirExpr) -> bool {
        matches!(
            value.kind,
            HirExprKind::If { .. }
                | HirExprKind::Match { .. }
                | HirExprKind::Block { .. }
                | HirExprKind::Unsafe { .. }
        )
    }

    /// Whether `value` is an aggregate literal, the one holder shape whose positions'
    /// moved-in ownership [`codegen_literal_position`] can collect.
    pub(crate) fn is_aggregate_literal(value: &HirExpr) -> bool {
        matches!(
            value.kind,
            HirExprKind::StructLiteral { .. }
                | HirExprKind::TupleLiteral { .. }
                | HirExprKind::ArrayLiteral { .. }
        )
    }

    /// The paths under `value` that a provable allocation was stored into, walking the
    /// aggregate literals in step with the path segments [`plan_held_drops`] assigned.
    ///
    /// A newtype contributes no segment: it is erased to its inner type at every other
    /// point in the drop pass, so its positions are its inner type's.
    pub(super) fn collect_armed_positions(
        &self,
        value: &HirExpr,
        path: &mut Vec<String>,
        out: &mut Vec<Vec<String>>,
    ) {
        if matches!(Type::from_hir(&value.ty), Type::String) {
            if self.produces_owned_string(value) {
                out.push(path.clone());
            }
            return;
        }
        match &value.kind {
            HirExprKind::StructLiteral { fields, .. } => {
                for field in fields {
                    path.push(field.name.clone());
                    self.collect_armed_positions(&field.value, path, out);
                    let _ = path.pop();
                }
            }
            HirExprKind::ArrayLiteral { elements } | HirExprKind::TupleLiteral { elements } => {
                for (index, element) in elements.iter().enumerate() {
                    path.push(index.to_string());
                    self.collect_armed_positions(element, path, out);
                    let _ = path.pop();
                }
            }
            HirExprKind::NewtypeConstruct { value: inner, .. } => {
                self.collect_armed_positions(inner, path, out)
            }
            // A call hands over the positions its callee stored an allocation into on
            // every return path, which the ownership summary read off its body.
            HirExprKind::Call { callee, .. } => {
                let Some(key) = self.summary_key(callee) else {
                    return;
                };
                for position in self.string_ownership.held_strings_returned(&key) {
                    out.push(path.iter().chain(position).cloned().collect());
                }
            }
            _ => {}
        }
    }

    /// Address of an aggregate's `index`-th position, for a struct, tuple, or array
    /// holder alike.
    ///
    /// `build_struct_gep` refuses an array pointee, so an array indexes through a
    /// two-step GEP instead. Every index reaching here is a constant read off the
    /// holder's own layout.
    pub(super) fn aggregate_position_ptr(
        &self,
        holder_llvm: BasicTypeEnum<'ctx>,
        base_ptr: PointerValue<'ctx>,
        index: u32,
        name: &str,
    ) -> CodegenResult<PointerValue<'ctx>> {
        if !holder_llvm.is_array_type() {
            return self
                .builder
                .build_struct_gep(holder_llvm, base_ptr, index, name)
                .map_err(CodegenError::from);
        }
        let i64_ty = self.context.i64_type();
        // SAFETY: `index` is a position of `holder_llvm`'s own layout, so it is within
        // the array's length by construction and the GEP stays inside the object.
        unsafe {
            self.builder
                .build_in_bounds_gep(
                    holder_llvm,
                    base_ptr,
                    &[i64_ty.const_zero(), i64_ty.const_int(index as u64, false)],
                    name,
                )
                .map_err(CodegenError::from)
        }
    }
}
