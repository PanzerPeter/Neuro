//! Scope management for semantic analysis
//!
//! This module provides scoping and symbol table functionality

use shared_types::{Type, Span};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Symbol information stored in scope
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Symbol {
    Variable {
        name: String,
        var_type: Type,
        mutable: bool,
        span: Span,
    },
    Function {
        name: String,
        params: Vec<Type>,
        return_type: Type,
        span: Span,
    },
}

impl Symbol {
    pub fn name(&self) -> &str {
        match self {
            Symbol::Variable { name, .. } => name,
            Symbol::Function { name, .. } => name,
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Symbol::Variable { span, .. } => *span,
            Symbol::Function { span, .. } => *span,
        }
    }
}

/// Scoped symbol table with nested scope support
#[derive(Debug, Clone)]
pub struct Scope {
    scopes: Vec<HashMap<String, Symbol>>,
}

impl Scope {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()], // Global scope
        }
    }

    /// Push a new scope
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pop the current scope
    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Insert a symbol into the current scope
    pub fn insert(&mut self, symbol: Symbol) -> Option<Symbol> {
        let name = symbol.name().to_string();
        self.scopes
            .last_mut()
            .unwrap()
            .insert(name, symbol)
    }

    /// Look up a symbol, searching from innermost to outermost scope
    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        for scope in self.scopes.iter().rev() {
            if let Some(symbol) = scope.get(name) {
                return Some(symbol);
            }
        }
        None
    }

    /// Check if a symbol exists in the current scope (not parent scopes)
    pub fn exists_in_current_scope(&self, name: &str) -> bool {
        self.scopes
            .last()
            .map(|scope| scope.contains_key(name))
            .unwrap_or(false)
    }

    /// Get all symbols from all scopes
    pub fn all_symbols(&self) -> HashMap<String, Symbol> {
        let mut all = HashMap::new();
        for scope in &self.scopes {
            for (name, symbol) in scope {
                all.insert(name.clone(), symbol.clone());
            }
        }
        all
    }
}

impl Default for Scope {
    fn default() -> Self {
        Self::new()
    }
}