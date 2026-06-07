// Neuro Programming Language - Semantic Analysis
// Symbol table with lexical scoping support

use std::collections::HashMap;

use shared_types::Span;

use crate::types::Type;

/// Information about a symbol (variable)
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SymbolInfo {
    pub(crate) ty: Type,
    pub(crate) mutable: bool,
    /// The span at which this binding's value was moved out, or `None` while the
    /// binding still owns its value. Drives use-after-move detection (§2.2).
    pub(crate) moved_at: Option<Span>,
}

impl SymbolInfo {
    pub(crate) fn new(ty: Type, mutable: bool) -> Self {
        Self {
            ty,
            mutable,
            moved_at: None,
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

    /// Exit the current scope
    pub(crate) fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
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
