// NEURO Programming Language - Semantic Analysis
// Symbol table with lexical scoping support

use std::collections::HashMap;

use crate::types::Type;

/// Symbol table with lexical scoping support
#[derive(Debug)]
pub(crate) struct SymbolTable {
    /// Stack of scopes (innermost scope is last)
    scopes: Vec<HashMap<String, Type>>,
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
    pub(crate) fn define(&mut self, name: String, ty: Type) -> Result<(), String> {
        if let Some(current_scope) = self.scopes.last_mut() {
            if current_scope.contains_key(&name) {
                return Err(name);
            }
            current_scope.insert(name, ty);
            Ok(())
        } else {
            Err(name)
        }
    }

    /// Look up a variable in all scopes (innermost to outermost)
    pub(crate) fn lookup(&self, name: &str) -> Option<&Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_table_scoping() {
        let mut table = SymbolTable::new();

        // Define in global scope
        assert!(table.define("x".to_string(), Type::I32).is_ok());
        assert_eq!(table.lookup("x"), Some(&Type::I32));

        // Define in nested scope
        table.push_scope();
        assert!(table.define("y".to_string(), Type::Bool).is_ok());
        assert_eq!(table.lookup("y"), Some(&Type::Bool));
        assert_eq!(table.lookup("x"), Some(&Type::I32)); // Can still see outer scope

        // Shadow variable
        assert!(table.define("x".to_string(), Type::F64).is_ok());
        assert_eq!(table.lookup("x"), Some(&Type::F64)); // Sees inner definition

        // Pop scope
        table.pop_scope();
        assert_eq!(table.lookup("x"), Some(&Type::I32)); // Back to outer definition
        assert_eq!(table.lookup("y"), None); // Inner variable gone
    }

    #[test]
    fn symbol_table_duplicate_definition() {
        let mut table = SymbolTable::new();
        assert!(table.define("x".to_string(), Type::I32).is_ok());
        assert!(table.define("x".to_string(), Type::Bool).is_err());
    }
}
