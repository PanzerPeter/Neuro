// Deterministic destruction (`Drop`): scope-exit destructor insertion.
//
// A binding of a `Drop` type runs its `{struct}__drop(&mut self)` destructor when
// its lexical scope ends on a *normal* exit: fall-through, `return`, `break`, or
// `continue` (a panic aborts without running destructors). Each owned
// binding carries an `i1` drop flag, set `false` when the value is moved out, so a
// moved value is not dropped twice. Every helper here is inert when the
// program declares no `Drop` types: the scope stack stays empty and nothing is
// emitted.

use inkwell::values::{BasicValueEnum, PointerValue};
use neuro_hir::{HirExpr, HirExprKind, HirType};

use crate::errors::CodegenResult;
use crate::types::Type;

use super::context::{CodegenContext, DropEntry, DropTarget, HeldDrop};

mod displace;
mod emit;
mod moves;
mod owned_strings;
mod string_positions;

/// The name an unbound temporary's drop entry is registered under. `__` is rejected in
/// every declared name, so no move site can reach the entry.
const UNBOUND_TEMPORARY: &str = "__unbound_temporary";

/// The drop flag of a `string` position inside a holder. It is the one flag that
/// starts `false`, so its name is what tells an armed position from a planned one.
pub(crate) const STRING_POSITION_FLAG: &str = "str.owned.flag";

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
    /// when it owns nothing that needs releasing. The HIR carries the binding's
    /// resolved type, so this reads it directly.
    pub(crate) fn drop_target(&self, binding_ty: &HirType) -> Option<DropTarget> {
        self.drop_target_of(&Type::from_hir(binding_ty))
    }

    /// The same resolution over an already-lowered type, for the call sites that reach
    /// codegen with a [`Type`] rather than the HIR it came from.
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
            Type::Struct(name) if self.drop_types.contains(name) => {
                Some(DropTarget::UserDrop(name.clone()))
            }
            Type::Enum(name) if self.enum_holds_owner(name) => {
                Some(DropTarget::EnumPayload(name.clone()))
            }
            // Owns nothing itself, but something inside it does: the work is in the
            // held entries the registration plans alongside this target. A `string`
            // position counts here even though a `string` binding does not, because a
            // position has storage of its own that the store into it can arm.
            other if self.holds_owner(other) || self.holds_string_position(other) => {
                Some(DropTarget::Aggregate)
            }
            _ => None,
        }
    }

    /// Whether a value of `ty` owns anything that must be released, directly or through
    /// a position inside it.
    ///
    /// A `string` answers `false` on purpose: a string binding owns its buffer only when
    /// its initializer allocated one ([`produces_owned_string`]), which is a property of
    /// the expression and not of the type, so no position reached through a type can be
    /// proven to own one.
    pub(crate) fn holds_owner(&self, ty: &Type) -> bool {
        match ty {
            Type::Collection { .. } | Type::Tensor { .. } => true,
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

    /// Whether a value of `ty` has a `string` position inside it: a struct field, an
    /// array or tuple element, or one reached through those.
    ///
    /// Separate from [`holds_owner`] because the two answer different questions. A
    /// typed owner is destroyed wherever the type appears, an enum payload slot
    /// included, where no flag exists to guard it. A `string` position is destroyed only
    /// when the store into it provably allocated, which needs the flag, so it is planned
    /// where a flag can be planned and nowhere else.
    fn holds_string_position(&self, ty: &Type) -> bool {
        self.held_positions(ty).iter().any(|(_, position_ty)| {
            matches!(position_ty, Type::String) || self.holds_string_position(position_ty)
        })
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

    /// Whether a value of `ty` runs a user destructor somewhere inside it.
    fn holds_user_drop(&self, ty: &Type) -> bool {
        match ty {
            Type::Struct(name) => {
                self.drop_types.contains(name)
                    || self
                        .struct_defs
                        .get(name)
                        .is_some_and(|fields| fields.iter().any(|(_, t)| self.holds_user_drop(t)))
            }
            Type::Enum(name) => self
                .type_mapper
                .enum_payload_types(name)
                .is_some_and(|variants| variants.iter().flatten().any(|t| self.holds_user_drop(t))),
            Type::Tuple(elements) => elements.iter().any(|t| self.holds_user_drop(t)),
            Type::Array { element, .. } => self.holds_user_drop(element),
            _ => false,
        }
    }

    /// Destroy `value`, the result of `expr`, when it is a fresh value no binding owns
    /// and it runs a user destructor: a statement nothing reads, or a temporary only read
    /// from (`make().id`). No scope exit reaches such a value, so without this its
    /// destructor never ran (BUG-047).
    ///
    /// Only a call, a struct literal or an enum construction is fresh; anything else may
    /// be a read of a value some binding still owns. Inside a `pool` it does nothing: the
    /// arena, not a scope, owns what the block allocates.
    pub(crate) fn drop_unbound_temporary(
        &mut self,
        expr: &HirExpr,
        value: BasicValueEnum<'ctx>,
    ) -> CodegenResult<()> {
        let fresh = matches!(
            expr.kind,
            HirExprKind::Call { .. }
                | HirExprKind::StructLiteral { .. }
                | HirExprKind::EnumConstruct { .. }
        );
        let ty = Type::from_hir(&expr.ty);
        if !fresh || self.pool_depth > 0 || !self.holds_user_drop(&ty) {
            return Ok(());
        }
        if self.current_block_terminated() {
            return Ok(());
        }
        let slot = self.entry_alloca(value.get_type(), "unbound.tmp")?;
        self.builder.build_store(slot, value)?;
        let depth = self.drop_scopes.len();
        self.push_drop_scope();
        self.register_owned_binding(UNBOUND_TEMPORARY, slot, &ty)?;
        self.emit_drops_through(depth)?;
        self.pop_drop_scope();
        Ok(())
    }

    /// Destroy the value of type `ty` at `place_ptr` that a store through a borrow is
    /// about to displace.
    ///
    /// Nothing tracks a borrowed place, so there is no flag to consult, and none is
    /// needed: nothing can be moved out of a borrow, so the value behind one is always
    /// live. What the release needs instead is a type that proves what it owns. A
    /// `string` position proves nothing and stays disarmed, and a value holding a tensor
    /// or a `PoolAware` value is left alone, because either may belong to a `pool`'s
    /// arena, which a callee cannot see from where its caller stands. Inside a `pool`
    /// the arena owns what the block allocates, as for a temporary.
    pub(crate) fn drop_displaced_through_borrow(
        &mut self,
        place_ptr: PointerValue<'ctx>,
        ty: &Type,
    ) -> CodegenResult<()> {
        if self.pool_depth > 0 || !self.holds_owner(ty) || self.may_hold_arena_memory(ty) {
            return Ok(());
        }
        let depth = self.drop_scopes.len();
        self.push_drop_scope();
        self.register_owned_binding(UNBOUND_TEMPORARY, place_ptr, ty)?;
        self.emit_drops_through(depth)?;
        self.pop_drop_scope();
        Ok(())
    }

    /// Whether a value of `ty` holds a tensor or a `PoolAware` value anywhere inside it.
    fn may_hold_arena_memory(&self, ty: &Type) -> bool {
        match ty {
            Type::Tensor { .. } => true,
            Type::Struct(name) => {
                self.pool_aware_types.contains(name)
                    || self.struct_defs.get(name).is_some_and(|fields| {
                        fields.iter().any(|(_, f)| self.may_hold_arena_memory(f))
                    })
            }
            Type::Enum(name) => self
                .type_mapper
                .enum_payload_types(name)
                .is_some_and(|variants| {
                    variants
                        .iter()
                        .flatten()
                        .any(|f| self.may_hold_arena_memory(f))
                }),
            Type::Array { element, .. } => self.may_hold_arena_memory(element),
            Type::Tuple(elements) => elements.iter().any(|e| self.may_hold_arena_memory(e)),
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
        let positions = self.held_positions(ty);
        if positions.is_empty() {
            return Ok(());
        }

        let holder_llvm = self.get_any_llvm_type(ty)?;
        for (index, (segment, position_ty)) in positions.into_iter().enumerate() {
            let is_string = matches!(position_ty, Type::String);
            if !is_string
                && !self.holds_owner(&position_ty)
                && !self.holds_string_position(&position_ty)
            {
                continue;
            }
            let position_ptr = self.aggregate_position_ptr(
                holder_llvm,
                storage_ptr,
                index as u32,
                &format!("held.{}.ptr", segment),
            )?;
            path.push(segment);
            // A `string` position starts DISARMED: the type proves nothing, so the
            // position owns a buffer only once a store that provably allocated has
            // armed it. Every other target's type is the proof, so it starts armed.
            if is_string {
                let flag_ptr = self.disarmed_drop_flag()?;
                out.push(HeldDrop {
                    path: path.clone(),
                    storage_ptr: position_ptr,
                    flag_ptr,
                    target: DropTarget::HeapString,
                });
                let _ = path.pop();
                continue;
            }
            if let Some(target) = self.drop_target_of(&position_ty) {
                if !matches!(target, DropTarget::Aggregate) {
                    let flag_ptr = self.arm_drop_flag()?;
                    out.push(HeldDrop {
                        path: path.clone(),
                        storage_ptr: position_ptr,
                        flag_ptr,
                        target,
                    });
                }
            }
            self.plan_held_drops(position_ptr, &position_ty, path, out)?;
            let _ = path.pop();
        }
        Ok(())
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

    /// The same slot, initialized to `false`, for a position that owns nothing until a
    /// store proves it does. The initializing store is in the entry block so a position
    /// a conditional never writes still reads a definite `false` at scope exit.
    ///
    /// Named apart from [`arm_drop_flag`]'s slot because the two carry different claims:
    /// this one says "not yet", and a `store i1 true` against it is the whole record of
    /// which `string` positions a program actually owns.
    fn disarmed_drop_flag(&mut self) -> CodegenResult<PointerValue<'ctx>> {
        let bool_ty = self.context.bool_type();
        let flag_ptr = self.entry_alloca(bool_ty, STRING_POSITION_FLAG)?;
        self.builder.build_store(flag_ptr, bool_ty.const_zero())?;
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
