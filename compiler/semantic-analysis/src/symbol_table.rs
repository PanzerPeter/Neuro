// Symbol table with lexical scoping support

use std::collections::{BTreeMap, HashMap};

use shared_types::Span;

use crate::types::Type;

/// A borrow held by a reference binding: the place it points at and whether the
/// borrow is exclusive (`&mut`). Lets the borrow be released when the holding
/// binding leaves scope.
#[derive(Debug, Clone, PartialEq)]
struct BorrowProvenance {
    place: String,
    exclusive: bool,
}

/// What a binding has given away: the whole value, or individual sub-places of it.
///
/// A sub-place is keyed by its path below the binding — `"label"`, `"0"`, `"0.name"` —
/// so that `val (a, b) = pair` moves two different elements rather than the same root
/// twice. Only a statically nameable step joins a path; a runtime array index cannot be
/// one, so a move through it takes the whole binding instead.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct MoveState {
    /// The span at which the whole value was moved out, or `None` while the binding
    /// still owns it.
    pub(crate) whole: Option<Span>,
    parts: BTreeMap<String, Span>,
}

impl MoveState {
    /// The span of the move that makes reading the sub-place `path` invalid: the whole
    /// binding, an ancestor of `path`, or a descendant (which leaves `path` itself
    /// partially moved). `path` is empty for the binding read as a whole.
    fn conflict(&self, path: &str) -> Option<Span> {
        if let Some(span) = self.whole {
            return Some(span);
        }
        self.parts
            .iter()
            .find(|(moved, _)| is_path_related(moved, path))
            .map(|(_, span)| *span)
    }

    /// The span of any outstanding move, whole or partial: reading the binding as a
    /// whole conflicts with every one of them.
    pub(crate) fn conflict_with_whole(&self) -> Option<Span> {
        self.conflict("")
    }

    fn clear(&mut self) {
        self.whole = None;
        self.parts.clear();
    }

    /// The span of a move this state has and `before` did not, if any. A loop body
    /// uses it to report a move its next iteration would perform again.
    fn introduced_since(&self, before: &MoveState) -> Option<Span> {
        if let (None, Some(span)) = (before.whole, self.whole) {
            return Some(span);
        }
        self.parts
            .iter()
            .find(|(path, _)| !before.parts.contains_key(*path))
            .map(|(_, span)| *span)
    }
}

/// Whether reading `path` touches storage that a move of `moved` already gave away:
/// either path contains the other. The empty path is the whole binding, and contains
/// every sub-place.
fn is_path_related(moved: &str, path: &str) -> bool {
    let contains = |outer: &str, inner: &str| {
        outer.is_empty() || inner == outer || inner.starts_with(&format!("{}.", outer))
    };
    contains(moved, path) || contains(path, moved)
}

/// Information about a symbol (variable)
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SymbolInfo {
    pub(crate) ty: Type,
    pub(crate) mutable: bool,
    /// What this binding has given away. Drives use-after-move detection.
    pub(crate) moves: MoveState,
    /// Borrows taken against this binding's place that outlive a single statement:
    /// each one held by a reference binding (`val r = &x`) until it leaves scope.
    shared_persistent: u32,
    exclusive_persistent: u32,
    /// Borrows against this place that live only for the current statement (call
    /// arguments, conditions, return values). Cleared at the end of every
    /// statement so a transient borrow never leaks past the statement taking it.
    shared_transient: u32,
    exclusive_transient: u32,
    /// Set when this binding is itself a reference that borrows another place
    /// (`val r = &x`); drives release of the borrow when this binding dies.
    borrows: Option<BorrowProvenance>,
}

impl SymbolInfo {
    pub(crate) fn new(ty: Type, mutable: bool) -> Self {
        Self {
            ty,
            mutable,
            moves: MoveState::default(),
            shared_persistent: 0,
            exclusive_persistent: 0,
            shared_transient: 0,
            exclusive_transient: 0,
            borrows: None,
        }
    }
}

/// Symbol table with lexical scoping support
#[derive(Debug)]
pub(crate) struct SymbolTable {
    /// Stack of scopes (innermost scope is last)
    scopes: Vec<HashMap<String, SymbolInfo>>,
}

impl SymbolTable {
    pub(crate) fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    /// Enter a new scope (e.g., function body, block)
    pub(crate) fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Exit the current scope, releasing every borrow held by a binding that
    /// dies with it. A reference binding (`val r = &x`) holds a borrow against an
    /// outer place; once `r` is gone the borrow is over, so the outer place's
    /// persistent borrow count is decremented. Borrows targeting a
    /// place that lived in the same dying scope need no release: the place is
    /// gone too, so a target absent from the surviving scopes is simply skipped.
    pub(crate) fn pop_scope(&mut self) {
        if self.scopes.len() <= 1 {
            return;
        }
        let Some(dying) = self.scopes.pop() else {
            return;
        };
        for info in dying.values() {
            if let Some(prov) = &info.borrows {
                self.release_persistent(prov);
            }
        }
    }

    /// Define a variable in the current scope
    pub(crate) fn define(&mut self, name: String, ty: Type, mutable: bool) -> Result<(), String> {
        if let Some(current_scope) = self.scopes.last_mut() {
            if current_scope.contains_key(&name) {
                return Err(name);
            }
            current_scope.insert(name, SymbolInfo::new(ty, mutable));
            Ok(())
        } else {
            Err(name)
        }
    }

    /// How many scopes are currently open. Paired with [`SymbolTable::defining_depth`]
    /// to tell a binding declared inside a region from one that outlives it, which is
    /// what a `pool` block needs to know about an assignment's target.
    pub(crate) fn depth(&self) -> usize {
        self.scopes.len()
    }

    /// Index of the innermost scope that defines `name`, or `None` when the name is
    /// not a live binding.
    pub(crate) fn defining_depth(&self, name: &str) -> Option<usize> {
        self.scopes
            .iter()
            .rposition(|scope| scope.contains_key(name))
    }

    /// Look up a variable in all scopes (innermost to outermost)
    pub(crate) fn lookup(&self, name: &str) -> Option<&SymbolInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }
        None
    }

    fn lookup_mut(&mut self, name: &str) -> Option<&mut SymbolInfo> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(info) = scope.get_mut(name) {
                return Some(info);
            }
        }
        None
    }

    /// Total borrows currently active against `place` (persistent plus
    /// transient) as `(shared, exclusive)`. `None` when the name is not a live
    /// binding (e.g. a constant or an undefined name). Drives the
    /// coexistence checks at each borrow site.
    pub(crate) fn borrow_counts(&self, place: &str) -> Option<(u32, u32)> {
        let info = self.lookup(place)?;
        Some((
            info.shared_persistent + info.shared_transient,
            info.exclusive_persistent + info.exclusive_transient,
        ))
    }

    /// Borrows against `place` held by a live reference binding, as
    /// `(shared, exclusive)`. Unlike [`borrow_counts`] this excludes the transient
    /// borrows taken earlier in the current statement, which end with the call or
    /// expression that took them rather than freezing the place for the statement.
    ///
    /// [`borrow_counts`]: SymbolTable::borrow_counts
    pub(crate) fn persistent_borrow_counts(&self, place: &str) -> Option<(u32, u32)> {
        let info = self.lookup(place)?;
        Some((info.shared_persistent, info.exclusive_persistent))
    }

    /// Register a borrow of `place` that lives only for the current statement
    /// (a call argument, a condition, a returned reference). Cleared by
    /// [`clear_transient_borrows`]. No-op when `place` is not a live binding.
    ///
    /// [`clear_transient_borrows`]: SymbolTable::clear_transient_borrows
    pub(crate) fn add_transient_borrow(&mut self, place: &str, exclusive: bool) {
        if let Some(info) = self.lookup_mut(place) {
            if exclusive {
                info.exclusive_transient = info.exclusive_transient.saturating_add(1);
            } else {
                info.shared_transient = info.shared_transient.saturating_add(1);
            }
        }
    }

    /// Promote the transient borrow of `place` taken while checking an
    /// initializer into a persistent borrow held by `holder` (`val holder = &place`).
    /// The borrow is released when `holder` leaves scope (see [`pop_scope`]).
    ///
    /// [`pop_scope`]: SymbolTable::pop_scope
    pub(crate) fn attach_borrow(&mut self, holder: &str, place: &str, exclusive: bool) {
        if let Some(info) = self.lookup_mut(place) {
            if exclusive {
                info.exclusive_transient = info.exclusive_transient.saturating_sub(1);
                info.exclusive_persistent = info.exclusive_persistent.saturating_add(1);
            } else {
                info.shared_transient = info.shared_transient.saturating_sub(1);
                info.shared_persistent = info.shared_persistent.saturating_add(1);
            }
        }
        if let Some(info) = self.lookup_mut(holder) {
            info.borrows = Some(BorrowProvenance {
                place: place.to_string(),
                exclusive,
            });
        }
    }

    /// The place `holder` borrows, if `holder` is a reference binding created by a
    /// direct `&place` initializer (`val holder = &place`). Lets the
    /// returned-reference check trace a returned local reference back to the place
    /// it points into and reject it when that place is itself function-local.
    pub(crate) fn borrow_provenance(&self, holder: &str) -> Option<String> {
        self.lookup(holder)
            .and_then(|info| info.borrows.as_ref().map(|prov| prov.place.clone()))
    }

    /// Release the persistent borrow held by `holder`, if any: used before a
    /// `mut` reference binding is reassigned, so its previous borrowee is freed
    /// before the new borrow is checked. No-op when `holder` holds no borrow.
    pub(crate) fn release_borrow_of(&mut self, holder: &str) {
        let prov = self.lookup_mut(holder).and_then(|info| info.borrows.take());
        if let Some(prov) = prov {
            self.release_persistent(&prov);
        }
    }

    fn release_persistent(&mut self, prov: &BorrowProvenance) {
        if let Some(info) = self.lookup_mut(&prov.place) {
            if prov.exclusive {
                info.exclusive_persistent = info.exclusive_persistent.saturating_sub(1);
            } else {
                info.shared_persistent = info.shared_persistent.saturating_sub(1);
            }
        }
    }

    /// Drop every transient borrow. Called at the end of each statement: a borrow
    /// passed to a call or used in a condition lives only for that statement, so
    /// it must not block a later borrow of the same place. Persistent
    /// borrows (held by live reference bindings) are untouched.
    pub(crate) fn clear_transient_borrows(&mut self) {
        for scope in &mut self.scopes {
            for info in scope.values_mut() {
                info.shared_transient = 0;
                info.exclusive_transient = 0;
            }
        }
    }

    /// Mark the binding named `name` as moved-out at `span` (innermost match).
    /// No-op when the name is not bound (e.g. a constant, which is a value, not
    /// a moveable owner).
    pub(crate) fn mark_moved(&mut self, name: &str, span: Span) {
        if let Some(info) = self.lookup_mut(name) {
            info.moves.whole = Some(span);
        }
    }

    /// Mark the sub-place `path` of `name` as moved-out at `span`, leaving the
    /// binding's other sub-places owned. An empty `path` is the whole binding.
    pub(crate) fn mark_place_moved(&mut self, name: &str, path: &str, span: Span) {
        if path.is_empty() {
            self.mark_moved(name, span);
            return;
        }
        if let Some(info) = self.lookup_mut(name) {
            info.moves.parts.insert(path.to_string(), span);
        }
    }

    /// Clear the moved state of `name`: the binding owns a fresh value again
    /// (e.g. after reassigning a `mut`).
    pub(crate) fn clear_moved(&mut self, name: &str) {
        if let Some(info) = self.lookup_mut(name) {
            info.moves.clear();
        }
    }

    /// The span of the move that makes reading sub-place `path` of `name` invalid,
    /// or `None` when that storage is still owned. An empty `path` reads the whole
    /// binding, which any outstanding move conflicts with.
    pub(crate) fn place_moved_at(&self, name: &str, path: &str) -> Option<Span> {
        self.lookup(name)?.moves.conflict(path)
    }

    /// Capture the moved-state of every currently-visible binding, in a stable
    /// order. Paired with [`restore_moves`] to bound a conditional region (an
    /// `if`/`while`/`for` body) so that a move inside it does not leak out onto
    /// paths that never executed it. The scope stack must be identical at the
    /// matching `restore_moves` call so the flat order lines up.
    ///
    /// [`restore_moves`]: SymbolTable::restore_moves
    pub(crate) fn snapshot_moves(&self) -> Vec<MoveState> {
        let mut snapshot = Vec::new();
        for scope in &self.scopes {
            for info in scope.values() {
                snapshot.push(info.moves.clone());
            }
        }
        snapshot
    }

    /// Every binding that was intact at [`snapshot_moves`] and is moved-out now,
    /// with the span of the move. Walks the scope stack in the same flat order as
    /// the snapshot, so it must be called with the same scope stack.
    ///
    /// A loop body uses this to tell a move it performed on a binding declared
    /// outside the loop — which the next iteration would perform again — from one
    /// the body re-established before the iteration ended.
    ///
    /// [`snapshot_moves`]: SymbolTable::snapshot_moves
    pub(crate) fn moves_since(&self, snapshot: &[MoveState]) -> Vec<(String, Span)> {
        let mut introduced = Vec::new();
        let mut idx = 0;
        for scope in &self.scopes {
            for (name, info) in scope {
                if let Some(before) = snapshot.get(idx) {
                    if let Some(span) = info.moves.introduced_since(before) {
                        introduced.push((name.clone(), span));
                    }
                }
                idx += 1;
            }
        }
        introduced
    }

    /// Restore moved-state captured by [`snapshot_moves`]. Entries beyond the
    /// snapshot length (bindings introduced after the snapshot) are left as-is.
    ///
    /// [`snapshot_moves`]: SymbolTable::snapshot_moves
    pub(crate) fn restore_moves(&mut self, snapshot: &[MoveState]) {
        let mut idx = 0;
        for scope in &mut self.scopes {
            for info in scope.values_mut() {
                if let Some(state) = snapshot.get(idx) {
                    info.moves = state.clone();
                }
                idx += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_table_scoping() {
        let mut table = SymbolTable::new();

        // Define in global scope
        assert!(table.define("x".to_string(), Type::I32, false).is_ok());
        assert_eq!(table.lookup("x"), Some(&SymbolInfo::new(Type::I32, false)));

        // Define in nested scope
        table.push_scope();
        assert!(table.define("y".to_string(), Type::Bool, true).is_ok());
        assert_eq!(table.lookup("y"), Some(&SymbolInfo::new(Type::Bool, true)));
        assert_eq!(table.lookup("x"), Some(&SymbolInfo::new(Type::I32, false))); // Can still see outer scope

        // Shadow variable
        assert!(table.define("x".to_string(), Type::F64, true).is_ok());
        assert_eq!(table.lookup("x"), Some(&SymbolInfo::new(Type::F64, true))); // Sees inner definition

        // Pop scope
        table.pop_scope();
        assert_eq!(table.lookup("x"), Some(&SymbolInfo::new(Type::I32, false))); // Back to outer definition
        assert_eq!(table.lookup("y"), None); // Inner variable gone
    }

    #[test]
    fn symbol_table_duplicate_definition() {
        let mut table = SymbolTable::new();
        assert!(table.define("x".to_string(), Type::I32, false).is_ok());
        assert!(table.define("x".to_string(), Type::Bool, true).is_err());
    }

    #[test]
    fn symbol_table_mutability_tracking() {
        let mut table = SymbolTable::new();

        // Immutable variable
        assert!(table.define("x".to_string(), Type::I32, false).is_ok());
        let x_info = table.lookup("x").unwrap();
        assert!(!x_info.mutable);
        assert_eq!(x_info.ty, Type::I32);

        // Mutable variable
        assert!(table.define("y".to_string(), Type::F64, true).is_ok());
        let y_info = table.lookup("y").unwrap();
        assert!(y_info.mutable);
        assert_eq!(y_info.ty, Type::F64);
    }
}
