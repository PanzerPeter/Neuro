// Deterministic destruction (`Drop`): scope-exit destructor insertion.
//
// A binding of a `Drop` type runs its `{struct}__drop(&mut self)` destructor when
// its lexical scope ends on a *normal* exit: fall-through, `return`, `break`, or
// `continue` (a panic aborts without running destructors). Each owned
// binding carries an `i1` drop flag, set `false` when the value is moved out, so a
// moved value is not dropped twice. Every helper here is inert when the
// program declares no `Drop` types: the scope stack stays empty and nothing is
// emitted.

use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use neuro_hir::{HirExpr, HirExprKind, HirType};

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

use super::context::{CodegenContext, DropEntry, DropTarget, HeldDrop};

mod displace;
mod emit;
mod functions;
mod moves;
pub(crate) use moves::reads_through_collection_slot;

pub(crate) use functions::{ENV_HEADER_FIELDS, is_pure_place};
mod owned_strings;
pub(crate) use owned_strings::{builds_its_own_buffer, is_fresh_string};

/// The name an unbound temporary's drop entry is registered under. `__` is rejected in
/// every declared name, so no move site can reach the entry.
pub(crate) const UNBOUND_TEMPORARY: &str = "__unbound_temporary";

/// Whether `expr` builds a value no binding owns. Anything else may be a read of a value
/// some binding still owns. A field, tuple element or array element read off a fresh value
/// leaves with its reader, the rest of the value being destroyed around it
/// (`drop_temporary_except`), so it is fresh too.
pub(crate) fn is_fresh_value(expr: &HirExpr) -> bool {
    match &expr.kind {
        HirExprKind::Call { .. }
        | HirExprKind::StructLiteral { .. }
        | HirExprKind::EnumConstruct { .. }
        | HirExprKind::TupleLiteral { .. }
        | HirExprKind::ArrayLiteral { .. }
        | HirExprKind::Closure { .. }
        // A block's tail is moved out to its reader.
        | HirExprKind::Block { .. }
        | HirExprKind::Unsafe { .. } => true,
        HirExprKind::FieldAccess { object, .. } | HirExprKind::TupleIndex { object, .. } => {
            is_fresh_value(object)
        }
        // An element of a fresh array, not of a `Vec` or a slice, which only lend theirs.
        HirExprKind::Index { object, .. } => {
            matches!(object.ty, HirType::Array { .. }) && is_fresh_value(object)
        }
        _ => false,
    }
}

/// The position of a fresh temporary that its reader took (`make().f`, `(a(), b()).0`,
/// `[a(), b()][i]`), so the release of the rest leaves it alone.
pub(crate) enum TakenPosition<'a, 'ctx> {
    /// A field or tuple element, by name.
    Named(&'a str),
    /// An array element at a run-time index.
    Element(IntValue<'ctx>),
}

impl<'ctx> CodegenContext<'ctx> {
    /// Open a new lexical scope. Paired with [`pop_drop_scope`].
    ///
    /// The name scope rides along with the drop scope because they delimit the same
    /// thing: every push here is a `{ }` a binding can be declared in, and a binding
    /// whose owner is released on the way out must stop being resolvable on the way
    /// out too.
    pub(crate) fn push_drop_scope(&mut self) {
        self.drop_scopes.push(Vec::new());
        self.name_scopes.push(Vec::new());
    }

    /// Close the innermost scope, restoring the names its bindings shadowed. Drops for
    /// a scope are emitted explicitly (see [`emit_top_scope_drops`] /
    /// [`emit_drops_through`]) before the scope is popped, so the destructors have
    /// already run against the bindings being unbound here.
    pub(crate) fn pop_drop_scope(&mut self) {
        let _ = self.drop_scopes.pop();
        if let Some(shadowed) = self.name_scopes.pop() {
            self.restore_bindings(shadowed);
        }
    }

    /// Resolve how a binding of `binding_ty` is destroyed at scope exit, or `None`
    /// when it owns nothing that needs releasing.
    ///
    /// A newtype needs no arm: `Type::from_hir` erases it to its inner type, so a
    /// newtype wrapping an owner resolves to whatever the inner type resolves to.
    pub(crate) fn drop_target_of(&self, binding_ty: &Type) -> Option<DropTarget> {
        match binding_ty {
            // A collection always owns a heap buffer, independently of whether the
            // program declares any user `Drop` type.
            Type::Collection { .. } => Some(DropTarget::Collection(binding_ty.clone())),
            // So does a tensor: every construction allocates its buffer, and the type
            // says so, and there is no borrowed value of tensor type to confuse it with.
            Type::Tensor { .. } => Some(DropTarget::TensorBuffer),
            // A function value holds a count of its environment.
            Type::Function { .. } => Some(DropTarget::FunctionValue),
            // A `string` owns its buffer when its owner bit says so, which the release
            // tests, so every `string` position is an owner like any other.
            Type::String => Some(DropTarget::HeapString),
            Type::Struct(name) if self.drop_types.contains(name) => {
                Some(DropTarget::UserDrop(name.clone()))
            }
            Type::Enum(name) if self.enum_holds_owner(name) => {
                Some(DropTarget::EnumPayload(name.clone()))
            }
            // Owns nothing itself, but something inside it does: the work is in the
            // held entries the registration plans alongside this target.
            other if self.holds_owner(other) => Some(DropTarget::Aggregate),
            _ => None,
        }
    }

    /// Whether a value of `ty` owns anything that must be released, directly or through
    /// a position inside it.
    pub(crate) fn holds_owner(&self, ty: &Type) -> bool {
        match ty {
            Type::Collection { .. }
            | Type::Tensor { .. }
            | Type::Function { .. }
            | Type::String => true,
            Type::Struct(name) => {
                self.drop_types.contains(name)
                    || self
                        .struct_defs
                        .get(name)
                        .is_some_and(|fields| fields.iter().any(|(_, f)| self.holds_owner(f)))
            }
            Type::Enum(name) => self.enum_holds_owner(name),
            Type::Array { element, size } => *size > 0 && self.holds_owner(element),
            Type::Tuple(elements) => elements.iter().any(|e| self.holds_owner(e)),
            _ => false,
        }
    }

    /// Whether any variant of the named enum carries a payload field that owns something.
    fn enum_holds_owner(&self, name: &str) -> bool {
        self.type_mapper
            .enum_payload_types(name)
            .is_some_and(|variants| {
                variants
                    .iter()
                    .flatten()
                    .any(|field| self.holds_owner(field))
            })
    }

    /// Destroy `value`, the result of `expr`, when it is a fresh value no binding owns
    /// and it owns something: a statement nothing reads, or a temporary only read from
    /// (`make().id`). No scope exit reaches such a value, so without this its destructor
    /// never ran (BUG-047).
    ///
    /// Only what [`is_fresh_value`] accepts is fresh; anything else may be a read of a value
    /// some binding still owns. Inside a `pool` it
    /// does nothing: the arena, not a scope, owns what the block allocates.
    pub(crate) fn drop_unbound_temporary(
        &mut self,
        expr: &HirExpr,
        value: BasicValueEnum<'ctx>,
    ) -> CodegenResult<()> {
        self.drop_temporary_except(expr, value, None)
    }

    /// [`Self::drop_unbound_temporary`], leaving out the position `moved` when one owning
    /// position has been moved out of the temporary (`make().f`): the reader owns that one,
    /// and every other position is still the temporary's to release. A struct with a
    /// destructor of its own is left whole, since its `drop` reads every field.
    pub(crate) fn drop_temporary_except(
        &mut self,
        expr: &HirExpr,
        value: BasicValueEnum<'ctx>,
        moved: Option<TakenPosition<'_, 'ctx>>,
    ) -> CodegenResult<()> {
        let ty = Type::from_hir(&expr.ty);
        if !is_fresh_value(expr) || self.pool_depth > 0 || self.current_block_terminated() {
            return Ok(());
        }
        if !self.temporary_drops_its_owners(&ty) {
            return Ok(());
        }
        // A `string` field leaves its buffer alive until the reader releases it, so the
        // destructor may still read it.
        let moves_an_owner = matches!(moved, Some(TakenPosition::Named(field)) if
            self.held_positions(&ty).iter().any(|(name, field_ty)| {
                name == field
                    && !matches!(field_ty, Type::String)
                    && self.drop_target_of(field_ty).is_some()
            })
        );
        if moves_an_owner && matches!(self.drop_target_of(&ty), Some(DropTarget::UserDrop(_))) {
            return Ok(());
        }
        let slot = self.entry_alloca(value.get_type(), "unbound.tmp")?;
        self.builder.build_store(slot, value)?;
        let depth = self.drop_scopes.len();
        self.push_drop_scope();
        self.register_owned_binding(UNBOUND_TEMPORARY, slot, &ty)?;
        if let Some(taken) = moved {
            // The scope pushed above holds the temporary's entry alone; it has no binding
            // for `live_drop_entry` to resolve.
            let positions: Vec<(String, PointerValue<'ctx>)> = self
                .drop_scopes
                .last()
                .and_then(|scope| scope.last())
                .map(|entry| {
                    entry
                        .held
                        .iter()
                        .filter_map(|held| Some((held.path.first()?.clone(), held.flag_ptr)))
                        .collect()
                })
                .unwrap_or_default();
            let zero = self.context.bool_type().const_zero();
            for (position, flag_ptr) in positions {
                match &taken {
                    TakenPosition::Named(field) if position == *field => {
                        self.builder.build_store(flag_ptr, zero)?;
                    }
                    TakenPosition::Element(index) => {
                        let Ok(element) = position.parse::<u64>() else {
                            continue;
                        };
                        let at = index.get_type().const_int(element, false);
                        let kept = self.builder.build_int_compare(
                            inkwell::IntPredicate::NE,
                            *index,
                            at,
                            "tmp.kept",
                        )?;
                        self.builder.build_store(flag_ptr, kept)?;
                    }
                    TakenPosition::Named(_) => {}
                }
            }
        }
        self.emit_drops_through(depth)?;
        self.pop_drop_scope();
        Ok(())
    }

    /// Keep the fresh value of `expr` in `slot` until its statement ends, for a borrow of
    /// it the statement may still read. Its flag starts clear in the entry block, so a
    /// jump past the point that built it finds nothing to destroy. Inside a `pool` the
    /// arena owns it.
    pub(crate) fn hold_unbound_temporary(
        &mut self,
        expr: &HirExpr,
        slot: PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        let ty = Type::from_hir(&expr.ty);
        if !is_fresh_value(expr) || self.pool_depth > 0 {
            return Ok(());
        }
        if !self.temporary_drops_its_owners(&ty) {
            return Ok(());
        }
        self.hold_to_statement_end(slot, DropTarget::Captured(ty))
    }

    /// Register `target` at `slot` for release when the current statement ends, or at any
    /// exit that leaves it first.
    pub(crate) fn hold_to_statement_end(
        &mut self,
        slot: PointerValue<'ctx>,
        target: DropTarget,
    ) -> CodegenResult<()> {
        let bool_ty = self.context.bool_type();
        let flag_ptr = self.entry_alloca(bool_ty, "stmt.tmp.flag")?;
        let restore = self.builder.get_insert_block();
        if let Some(next) = flag_ptr
            .as_instruction()
            .and_then(|alloca| alloca.get_next_instruction())
        {
            self.builder.position_before(&next);
        }
        self.builder.build_store(flag_ptr, bool_ty.const_zero())?;
        if let Some(block) = restore {
            self.builder.position_at_end(block);
        }
        self.builder
            .build_store(flag_ptr, bool_ty.const_int(1, false))?;
        if let Some(scope) = self.drop_scopes.last_mut() {
            scope.push(DropEntry {
                name: UNBOUND_TEMPORARY.to_string(),
                storage_ptr: slot,
                flag_ptr,
                target,
                held: Vec::new(),
            });
        }
        self.statement_temporaries.push(flag_ptr);
        Ok(())
    }

    /// Release what the statement opened at `mark` still holds. An entry an inner scope
    /// took with it was released at that scope's end.
    pub(crate) fn release_statement_temporaries(&mut self, mark: usize) -> CodegenResult<()> {
        for flag_ptr in self.statement_temporaries.split_off(mark).into_iter().rev() {
            let Some(scope) = self.drop_scopes.last_mut() else {
                continue;
            };
            let Some(index) = scope.iter().position(|e| e.flag_ptr == flag_ptr) else {
                continue;
            };
            let entry = scope.remove(index);
            self.emit_one_drop(entry.storage_ptr, entry.flag_ptr, &entry.target)?;
        }
        Ok(())
    }

    /// Whether an unbound fresh temporary of `ty` has its typed owners destroyed by
    /// [`Self::drop_unbound_temporary`]: every owner but a bare tensor, whose statement
    /// and receiver consumers release it through its handle.
    pub(crate) fn temporary_drops_its_owners(&self, ty: &Type) -> bool {
        !matches!(ty, Type::Tensor { .. }) && self.holds_owner(ty)
    }

    /// Destroy the value of type `ty` at `place_ptr` that a store through a borrow is
    /// about to displace.
    ///
    /// Nothing tracks a borrowed place, so there is no flag to consult, and none is
    /// needed: nothing can be moved out of a borrow, so the value behind one is always
    /// live. What the release needs instead is a type that proves what it owns. A tensor
    /// needs no exception for arena memory: its `deleter` frees through `__neuro_release`,
    /// which leaves a buffer inside a reserved chunk to the mark restore. A value holding
    /// a `PoolAware` value is left alone, because the `pool`'s sweep may release it too,
    /// and a callee cannot see from where its caller stands whether it was registered.
    /// Inside a `pool` the arena owns what the block allocates, as for a temporary.
    pub(crate) fn drop_displaced_through_borrow(
        &mut self,
        place_ptr: PointerValue<'ctx>,
        ty: &Type,
    ) -> CodegenResult<()> {
        if self.pool_depth > 0 || !self.holds_owner(ty) || self.may_hold_pool_aware(ty) {
            return Ok(());
        }
        let depth = self.drop_scopes.len();
        self.push_drop_scope();
        self.register_owned_binding(UNBOUND_TEMPORARY, place_ptr, ty)?;
        self.emit_drops_through(depth)?;
        self.pop_drop_scope();
        Ok(())
    }

    /// Whether a value of `ty` holds a `PoolAware` value anywhere inside it.
    fn may_hold_pool_aware(&self, ty: &Type) -> bool {
        match ty {
            Type::Struct(name) => {
                self.pool_aware_types.contains(name)
                    || self.struct_defs.get(name).is_some_and(|fields| {
                        fields.iter().any(|(_, f)| self.may_hold_pool_aware(f))
                    })
            }
            Type::Enum(name) => self
                .type_mapper
                .enum_payload_types(name)
                .is_some_and(|variants| {
                    variants
                        .iter()
                        .flatten()
                        .any(|f| self.may_hold_pool_aware(f))
                }),
            Type::Array { element, .. } => self.may_hold_pool_aware(element),
            Type::Tuple(elements) => elements.iter().any(|e| self.may_hold_pool_aware(e)),
            _ => false,
        }
    }

    /// Plan the drops for every owner held inside a binding of `ty`, flattening the
    /// field and element paths under `storage_ptr` into one entry each.
    ///
    /// The address of each position is GEP'd here, at the binding site, rather than at
    /// the drop sites: the offsets are constant, and the binding site dominates every
    /// scope exit, reassignment and move that can later reach the value.
    ///
    /// The recursion terminates because a position's type is strictly smaller than its
    /// holder's; a type containing itself by value has no finite layout and is rejected
    /// long before codegen.
    fn plan_held_drops(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
        ty: &Type,
        path: &mut Vec<String>,
        out: &mut Vec<HeldDrop<'ctx>>,
    ) -> CodegenResult<()> {
        self.plan_held_drops_flagged(storage_ptr, ty, path, out, &mut |this| this.arm_drop_flag())
    }

    /// [`Self::plan_held_drops`] with each position's flag taken from `flag`, called once
    /// per owning position in plan order: a closure environment hands out its own fields.
    pub(crate) fn plan_held_drops_flagged(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
        ty: &Type,
        path: &mut Vec<String>,
        out: &mut Vec<HeldDrop<'ctx>>,
        flag: &mut dyn FnMut(&mut Self) -> CodegenResult<PointerValue<'ctx>>,
    ) -> CodegenResult<()> {
        let positions = self.held_positions(ty);
        if positions.is_empty() {
            return Ok(());
        }

        let holder_llvm = self.get_any_llvm_type(ty)?;
        for (index, (segment, position_ty)) in positions.into_iter().enumerate() {
            if !self.holds_owner(&position_ty) {
                continue;
            }
            let position_ptr = self.aggregate_position_ptr(
                holder_llvm,
                storage_ptr,
                index as u32,
                &format!("held.{}.ptr", segment),
            )?;
            path.push(segment);
            if let Some(target) = self.drop_target_of(&position_ty)
                && !matches!(target, DropTarget::Aggregate)
            {
                let flag_ptr = flag(self)?;
                out.push(HeldDrop {
                    path: path.clone(),
                    storage_ptr: position_ptr,
                    flag_ptr,
                    target,
                });
            }
            self.plan_held_drops_flagged(position_ptr, &position_ty, path, out, flag)?;
            let _ = path.pop();
        }
        Ok(())
    }

    /// How many owning positions [`Self::plan_held_drops`] plans for a value of `ty`: the
    /// flags a holder of it carries besides its own.
    pub(crate) fn held_owner_count(&self, ty: &Type) -> usize {
        self.held_positions(ty)
            .into_iter()
            .filter(|(_, position_ty)| self.holds_owner(position_ty))
            .map(|(_, position_ty)| {
                let own = self
                    .drop_target_of(&position_ty)
                    .is_some_and(|target| !matches!(target, DropTarget::Aggregate));
                usize::from(own) + self.held_owner_count(&position_ty)
            })
            .sum()
    }

    /// Register a binding that owns something, or holds something that does, for
    /// destruction at scope exit. A binding of `ty` that owns nothing registers nothing.
    pub(crate) fn register_owned_binding(
        &mut self,
        name: &str,
        storage_ptr: PointerValue<'ctx>,
        ty: &Type,
    ) -> CodegenResult<()> {
        let Some(target) = self.drop_target_of(ty) else {
            return Ok(());
        };
        let mut held = Vec::new();
        self.plan_held_drops(storage_ptr, ty, &mut Vec::new(), &mut held)?;
        let flag_ptr = self.arm_drop_flag()?;
        if let Some(scope) = self.drop_scopes.last_mut() {
            scope.push(DropEntry {
                name: name.to_string(),
                storage_ptr,
                flag_ptr,
                target,
                held,
            });
        }
        Ok(())
    }

    /// A fresh `i1` drop-flag slot in the entry block, initialized to `true`.
    fn arm_drop_flag(&mut self) -> CodegenResult<PointerValue<'ctx>> {
        let bool_ty = self.context.bool_type();
        let flag_ptr = self.entry_alloca(bool_ty, "drop.flag")?;
        self.builder
            .build_store(flag_ptr, bool_ty.const_int(1, false))?;
        Ok(flag_ptr)
    }

    /// Record an owned `Drop`-typed binding for destruction at scope exit.
    ///
    /// Allocates the binding's `i1` drop flag (initialized `true`) and pushes a
    /// [`DropEntry`] onto the innermost scope, handing the flag back. The caller must
    /// have verified the binding's type needs one via [`drop_target`].
    pub(crate) fn register_local_drop(
        &mut self,
        name: &str,
        storage_ptr: PointerValue<'ctx>,
        target: DropTarget,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let flag_ptr = self.arm_drop_flag()?;
        if let Some(scope) = self.drop_scopes.last_mut() {
            scope.push(DropEntry {
                name: name.to_string(),
                storage_ptr,
                flag_ptr,
                target,
                held: Vec::new(),
            });
        }
        Ok(flag_ptr)
    }

    /// The drop entry of the binding `name` resolves to at this point, if it owns one.
    ///
    /// Matched on the binding's storage as well as its name: a binding that owns
    /// nothing (a borrow, a scalar) registers no entry, so a lookup by name alone would
    /// fall through it to an outer owner it shadows, and a store or a move through the
    /// inner name would then release or disown the outer value.
    fn live_drop_entry(&self, name: &str) -> Option<&DropEntry<'ctx>> {
        let storage = self.variables.get(name)?;
        self.drop_scopes
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .find(|entry| entry.name == name)
            .filter(|entry| entry.storage_ptr == *storage)
            .or_else(|| {
                self.captured_drop_entries
                    .get(name)
                    .filter(|entry| entry.storage_ptr == *storage)
            })
    }

    /// How a `move` closure's environment releases a capture of type `ty`, or `None`
    /// when the value owns nothing: as a binding of `ty` would, its owning positions each
    /// under a flag of the environment's (`closures.rs`).
    pub(crate) fn captured_drop_target(&self, ty: &Type) -> Option<DropTarget> {
        self.drop_target_of(ty)
    }

    /// The flag guarding the position `path` of the binding `name`, or the binding's own
    /// flag when it plans no such position.
    pub(crate) fn owns_position_now(
        &mut self,
        name: &str,
        path: &[String],
    ) -> CodegenResult<IntValue<'ctx>> {
        let held = self
            .live_drop_entry(name)
            .and_then(|entry| entry.held.iter().find(|held| held.path == path))
            .map(|held| held.flag_ptr);
        let Some(flag_ptr) = held else {
            return self.owns_binding_now(name);
        };
        let bool_ty = self.context.bool_type();
        Ok(self
            .builder
            .build_load(bool_ty, flag_ptr, "capture.part.owns")?
            .into_int_value())
    }

    /// Whether a closure environment taking the binding `name` of `ty` by value owns what
    /// it took. A value that shares on copy was given a count of its own by the move. A
    /// `PoolAware` value stays the arena's, whose sweep releases it.
    pub(crate) fn capture_ownership(
        &mut self,
        name: &str,
        ty: &Type,
    ) -> CodegenResult<IntValue<'ctx>> {
        let bool_ty = self.context.bool_type();
        if self.shares_on_copy(ty) {
            return Ok(bool_ty.const_int(1, false));
        }
        if self
            .live_drop_entry(name)
            .is_some_and(|entry| matches!(entry.target, DropTarget::PoolRegistered))
        {
            return Ok(bool_ty.const_zero());
        }
        self.owns_binding_now(name)
    }

    /// Whether the binding `name` owns its value right now, read from its drop flag. A
    /// binding with no entry owns nothing this frame could release.
    pub(crate) fn owns_binding_now(&mut self, name: &str) -> CodegenResult<IntValue<'ctx>> {
        let bool_ty = self.context.bool_type();
        let Some(flag_ptr) = self.live_drop_entry(name).map(|entry| entry.flag_ptr) else {
            return Ok(bool_ty.const_zero());
        };
        Ok(self
            .builder
            .build_load(bool_ty, flag_ptr, "capture.owns")?
            .into_int_value())
    }

    /// Release a value a closure environment owned, whole: plan its owners now, under
    /// fresh flags, and run them at once.
    pub(super) fn emit_captured_drop(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
        ty: &Type,
    ) -> CodegenResult<()> {
        let depth = self.drop_scopes.len();
        self.push_drop_scope();
        self.register_owned_binding(UNBOUND_TEMPORARY, storage_ptr, ty)?;
        self.emit_drops_through(depth)?;
        self.pop_drop_scope();
        Ok(())
    }

    /// Address of an aggregate's `index`-th position, for a struct, tuple, or array
    /// holder alike.
    ///
    /// `build_struct_gep` refuses an array pointee, so an array indexes through a
    /// two-step GEP instead. Every index reaching here is a constant read off the
    /// holder's own layout.
    pub(crate) fn aggregate_position_ptr(
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

    /// The positions a value of `ty` holds, in declaration order, as `(accessor
    /// segment, type)`. The position of a pair in the list is its LLVM aggregate index,
    /// so it names both the path segment and the GEP.
    ///
    /// Empty for every type with no statically indexable inside, an enum included: which
    /// of an enum's payload slots is live depends on its tag, so it is walked by
    /// [`emit_enum_payload_drop`] under a switch instead of by a static path.
    fn held_positions(&self, ty: &Type) -> Vec<(String, Type)> {
        match ty {
            Type::Struct(name) => self.struct_defs.get(name).cloned().unwrap_or_default(),
            Type::Tuple(elements) => elements
                .iter()
                .enumerate()
                .map(|(i, e)| (i.to_string(), e.clone()))
                .collect(),
            Type::Array { element, size } => (0..*size)
                .map(|i| (i.to_string(), (**element).clone()))
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// The value a newtype construction wraps. The wrapper is transparent (the newtype is
/// its inner value), so `Name(p)` moves `p` exactly as a bare `p` in the same position
/// does, flags and all.
fn peel_newtype(mut expr: &HirExpr) -> &HirExpr {
    while let HirExprKind::NewtypeConstruct { value, .. } = &expr.kind {
        expr = value;
    }
    expr
}

#[cfg(test)]
mod tests {
    use super::CodegenContext;
    use neuro_hir::{HirExpr, HirExprKind, HirType};
    use shared_types::{Literal, Span};

    fn expr(kind: HirExprKind) -> HirExpr {
        HirExpr::new(kind, HirType::I32, Span::new(0, 0))
    }

    fn variable(name: &str) -> HirExpr {
        expr(HirExprKind::Variable(name.to_string()))
    }

    fn integer(value: i128) -> HirExpr {
        expr(HirExprKind::Literal(Literal::Integer(value, None)))
    }

    #[test]
    fn a_bare_binding_is_the_whole_place() {
        let place = variable("h");
        assert_eq!(
            CodegenContext::moved_place(&place),
            Some(("h", Some(Vec::new())))
        );
    }

    #[test]
    fn field_tuple_and_constant_index_segments_build_a_path() {
        let place = expr(HirExprKind::TupleIndex {
            object: Box::new(expr(HirExprKind::Index {
                object: Box::new(expr(HirExprKind::FieldAccess {
                    object: Box::new(variable("h")),
                    field: "inner".to_string(),
                })),
                index: Box::new(integer(2)),
            })),
            index: 1,
        });
        assert_eq!(
            CodegenContext::moved_place(&place),
            Some((
                "h",
                Some(vec!["inner".to_string(), "2".to_string(), "1".to_string()])
            ))
        );
    }

    /// `a[i]` names no particular element, so a move through it takes the whole
    /// binding. A `None` path is how that is spelled, and it survives further segments.
    #[test]
    fn a_runtime_index_collapses_to_the_whole_binding() {
        let element = expr(HirExprKind::Index {
            object: Box::new(variable("a")),
            index: Box::new(variable("i")),
        });
        assert_eq!(CodegenContext::moved_place(&element), Some(("a", None)));

        let field_of_element = expr(HirExprKind::FieldAccess {
            object: Box::new(element),
            field: "w".to_string(),
        });
        assert_eq!(
            CodegenContext::moved_place(&field_of_element),
            Some(("a", None))
        );
    }

    #[test]
    fn an_expression_that_is_not_a_place_names_nothing() {
        let call = expr(HirExprKind::FieldAccess {
            object: Box::new(integer(3)),
            field: "a".to_string(),
        });
        assert_eq!(CodegenContext::moved_place(&call), None);
    }
}
