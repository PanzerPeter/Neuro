// Neuro Programming Language - Semantic Analysis
// Symbol table with lexical scoping support

use std::collections::HashMap;

use shared_types::Span;

use crate::types::Type;

/// A borrow held by a reference binding: the place it points at and whether the
/// borrow is exclusive (`&mut`). Lets the borrow be released when the holding
/// binding leaves scope (§2.4, §2.5).
#[derive(Debug, Clone, PartialEq)]
struct BorrowProvenance {
    place: String,
    exclusive: bool,
}

/// Information about a symbol (variable)
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SymbolInfo {
    pub(crate) ty: Type,
    pub(crate) mutable: bool,
    /// The span at which this binding's value was moved out, or `None` while the
    /// binding still owns its value. Drives use-after-move detection (§2.2).
    pub(crate) moved_at: Option<Span>,
    /// Borrows taken against this binding's place that outlive a single statement —
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
            moved_at: None,
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
    /// persistent borrow count is decremented (§2.4, §2.5). Borrows targeting a
    /// place that lived in the same dying scope need no release — the place is
    /// gone too — so a target absent from the surviving scopes is simply skipped.
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

    /// Total borrows currently active against `place` — persistent plus
    /// transient — as `(shared, exclusive)`. `None` when the name is not a live
    /// binding (e.g. a constant or an undefined name). Drives the §2.4/§2.5
    /// coexistence checks at each borrow site.
    pub(crate) fn borrow_counts(&self, place: &str) -> Option<(u32, u32)> {
        let info = self.lookup(place)?;
        Some((
            info.shared_persistent + info.shared_transient,
            info.exclusive_persistent + info.exclusive_transient,
        ))
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
    /// it points into and reject it when that place is itself function-local (§2.6).
    pub(crate) fn borrow_provenance(&self, holder: &str) -> Option<String> {
        self.lookup(holder)
            .and_then(|info| info.borrows.as_ref().map(|prov| prov.place.clone()))
    }

    /// Release the persistent borrow held by `holder`, if any — used before a
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
    /// it must not block a later borrow of the same place (§2.4, §2.5). Persistent
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
        for scope in self.scopes.iter_mut().rev() {
            if let Some(info) = scope.get_mut(name) {
                info.moved_at = Some(span);
                return;
            }
        }
    }

    /// Clear the moved state of `name` — the binding owns a fresh value again
    /// (e.g. after reassigning a `mut`).
    pub(crate) fn clear_moved(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(info) = scope.get_mut(name) {
                info.moved_at = None;
                return;
            }
        }
    }

    /// Capture the moved-state of every currently-visible binding, in a stable
    /// order. Paired with [`restore_moves`] to bound a conditional region (an
    /// `if`/`while`/`for` body) so that a move inside it does not leak out onto
    /// paths that never executed it. The scope stack must be identical at the
    /// matching `restore_moves` call so the flat order lines up.
    ///
    /// [`restore_moves`]: SymbolTable::restore_moves
    pub(crate) fn snapshot_moves(&self) -> Vec<Option<Span>> {
        let mut snapshot = Vec::new();
        for scope in &self.scopes {
            for info in scope.values() {
                snapshot.push(info.moved_at);
            }
        }
        snapshot
    }

    /// Restore moved-state captured by [`snapshot_moves`]. Entries beyond the
    /// snapshot length (bindings introduced after the snapshot) are left as-is.
    ///
    /// [`snapshot_moves`]: SymbolTable::snapshot_moves
    pub(crate) fn restore_moves(&mut self, snapshot: &[Option<Span>]) {
        let mut idx = 0;
        for scope in &mut self.scopes {
            for info in scope.values_mut() {
                if let Some(state) = snapshot.get(idx) {
                    info.moved_at = *state;
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
