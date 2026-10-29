//! Move sites: resolving the place a move names and clearing the drop flags of what
//! left it.

use inkwell::values::{IntValue, PointerValue};
use neuro_hir::{HirExpr, HirExprKind, HirPlace};
use shared_types::Literal;

use crate::codegen::context::{CodegenContext, DropTarget};
use crate::errors::CodegenResult;
use crate::types::Type;

use super::peel_newtype;

impl<'ctx> CodegenContext<'ctx> {
    /// Whether `expr` names a binding the enclosing `pool` already registered, making
    /// this initializer a transfer of a live registration rather than a construction.
    pub(crate) fn moves_a_pool_registration(&self, expr: &HirExpr) -> bool {
        let HirExprKind::Variable(name) = &expr.kind else {
            return false;
        };
        self.live_drop_entry(name)
            .is_some_and(|entry| matches!(entry.target, DropTarget::PoolRegistered))
    }

    /// The binding a store's place is rooted in, or `None` where the place reaches
    /// storage no binding here names — a `*p` write, whose referent belongs to whoever
    /// handed the reference over.
    pub(crate) fn place_root(place: &HirPlace) -> Option<&str> {
        match place {
            HirPlace::Var { name, .. } => Some(name),
            HirPlace::Field { object, .. }
            | HirPlace::TupleIndex { object, .. }
            | HirPlace::Index { object, .. }
            | HirPlace::TensorIndex { object, .. } => {
                Self::moved_place(object).map(|(root, _)| root)
            }
            HirPlace::Deref { .. } => None,
        }
    }

    /// Resolve the place `expr` names as a binding plus the field and element path
    /// taken through it, or `None` when it is not rooted at a plain binding.
    ///
    /// A non-constant index yields `None` rather than a path: `a[i]` names no
    /// particular element, so the language makes a move through it a move of the whole
    /// binding, which is what a `None` path spells at the one caller below.
    pub(super) fn moved_place(expr: &HirExpr) -> Option<(&str, Option<Vec<String>>)> {
        match &expr.kind {
            HirExprKind::Variable(name) => Some((name, Some(Vec::new()))),
            HirExprKind::FieldAccess { object, field } => Self::extend_place(object, field),
            HirExprKind::TupleIndex { object, index } => {
                Self::extend_place(object, &index.to_string())
            }
            HirExprKind::Index { object, index } => match &index.kind {
                HirExprKind::Literal(Literal::Integer(value, _)) if *value >= 0 => {
                    Self::extend_place(object, &value.to_string())
                }
                _ => {
                    let (root, _) = Self::moved_place(object)?;
                    Some((root, None))
                }
            },
            _ => None,
        }
    }

    /// Append one accessor segment to the place `object` names, keeping the collapse to
    /// the whole binding that an unevaluable index already forced.
    pub(super) fn extend_place<'e>(
        object: &'e HirExpr,
        segment: &str,
    ) -> Option<(&'e str, Option<Vec<String>>)> {
        let (root, path) = Self::moved_place(object)?;
        Some((
            root,
            path.map(|mut path| {
                path.push(segment.to_string());
                path
            }),
        ))
    }

    /// Clear the drop flag of the place `expr` names, if it is a tracked owner being
    /// moved out of. Mirrors the move sites the type checker validates.
    ///
    /// A path into a holder disowns that position and everything beneath it, leaving the
    /// holder's siblings armed — that is what makes destructuring a sequence of ordinary
    /// moves. Naming the binding itself, or naming a place through an index the compiler
    /// cannot evaluate, disowns the whole binding.
    ///
    /// A value that shares on copy (function values and holders of nothing else) is
    /// copied, not moved: the language lets the source be read again, so the copy takes
    /// counts of its own and the source keeps its flags.
    pub(crate) fn mark_moved_for_drop(&mut self, expr: &HirExpr) -> CodegenResult<()> {
        let expr = peel_newtype(expr);
        let ty = Type::from_hir(&expr.ty);
        if self.shares_on_copy(&ty) {
            return self.retain_copied(expr, &ty);
        }
        if self.drop_scopes.is_empty() {
            return Ok(());
        }
        let Some((name, path)) = Self::moved_place(expr) else {
            return Ok(());
        };
        // The whole-binding collapse stands in for "some element left"; a read that moves
        // nothing out must not disown the holder, or none of its owners is ever released.
        if path.is_none() && self.read_moves_nothing(expr) {
            return Ok(());
        }

        let mut flags: Vec<PointerValue<'ctx>> = Vec::new();
        let entry = self.live_drop_entry(name);
        let Some(entry) = entry else {
            return Ok(());
        };
        let prefix = path.unwrap_or_default();
        // A captured value has no held entries to disown one part at a time, so moving
        // any part of it that owns something disowns the whole: what is left behind leaks
        // rather than being released along with the part already gone. Reading a part
        // that owns nothing (`n.id`) takes nothing.
        // A `string` position the environment flags on its own is the exception: clearing
        // that flag is all the move takes.
        let takes_an_owner = self.drop_target_of(&ty).is_some()
            && !(matches!(ty, Type::String) && entry.held.iter().any(|held| held.path == prefix));
        if prefix.is_empty() || (matches!(entry.target, DropTarget::Captured(_)) && takes_an_owner)
        {
            flags.push(entry.flag_ptr);
        }
        flags.extend(
            entry
                .held
                .iter()
                .filter(|held| held.path.starts_with(&prefix))
                .map(|held| held.flag_ptr),
        );

        let zero = self.context.bool_type().const_zero();
        for flag_ptr in flags {
            self.builder.build_store(flag_ptr, zero)?;
        }
        Ok(())
    }

    /// Whether reading `expr` leaves its holder owning everything it owned: a `Copy`
    /// value is copied out, and a collection slot, or a part of one, is never moved out
    /// of (the checker refuses it), so a read of one is a read in place.
    pub(super) fn read_moves_nothing(&self, expr: &HirExpr) -> bool {
        if reads_through_collection_slot(expr) {
            return true;
        }
        let ty = Type::from_hir(&expr.ty);
        !matches!(ty, Type::String) && self.drop_target_of(&ty).is_none()
    }

    /// Whether `expr` reads a value out of a position another binding already owns, so
    /// that the holder's own drop is what releases it.
    ///
    /// The distinction matters where an owned value is copied into a temporary to be
    /// read: the copy aliases the holder's buffer, so registering it as an owner in its
    /// own right would release that buffer twice.
    pub(crate) fn reads_a_held_place(&self, expr: &HirExpr) -> bool {
        let Some((name, path)) = Self::moved_place(expr) else {
            return false;
        };
        // A bare binding is not a held place: its own entry is what releases it.
        if path.as_ref().is_some_and(|segments| segments.is_empty()) {
            return false;
        }
        let prefix = path.unwrap_or_default();
        self.live_drop_entry(name)
            .is_some_and(|entry| entry.held.iter().any(|held| held.path.starts_with(&prefix)))
    }

    /// Disown every owner a binding holds, without touching the binding's own flag, and
    /// hand back each cleared flag with the value it held.
    ///
    /// A `match` that binds a payload by value is the one move site with no place
    /// expression to name: which position left depends on the tag. Clearing all of them
    /// is the conservative answer for an arm that binds — a part of the variant it did
    /// not bind leaks rather than being released twice. An arm that binds nothing takes
    /// nothing, and stores the returned values back.
    pub(crate) fn mark_held_moved_for_drop(
        &mut self,
        name: &str,
    ) -> CodegenResult<Vec<(PointerValue<'ctx>, IntValue<'ctx>)>> {
        let flags: Vec<PointerValue<'ctx>> = self
            .live_drop_entry(name)
            .map(|entry| {
                let mut flags: Vec<PointerValue<'ctx>> =
                    entry.held.iter().map(|held| held.flag_ptr).collect();
                if matches!(entry.target, DropTarget::EnumPayload(_)) {
                    flags.push(entry.flag_ptr);
                }
                flags
            })
            .unwrap_or_default();

        let bool_ty = self.context.bool_type();
        let mut saved = Vec::with_capacity(flags.len());
        for flag_ptr in flags {
            let owned = self
                .builder
                .build_load(bool_ty, flag_ptr, "match.owned")?
                .into_int_value();
            saved.push((flag_ptr, owned));
        }
        for (flag_ptr, _) in &saved {
            self.builder.build_store(*flag_ptr, bool_ty.const_zero())?;
        }
        Ok(saved)
    }
}

/// Whether `expr` names a collection slot or a field, tuple or array element inside one.
fn reads_through_collection_slot(expr: &HirExpr) -> bool {
    match &expr.kind {
        HirExprKind::Index { object, .. } => {
            matches!(
                Type::from_hir(&object.ty).referent(),
                Type::Collection { .. }
            ) || reads_through_collection_slot(object)
        }
        HirExprKind::FieldAccess { object, .. } | HirExprKind::TupleIndex { object, .. } => {
            reads_through_collection_slot(object)
        }
        _ => false,
    }
}
