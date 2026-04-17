// NEURO Programming Language - Semantic Analysis
// Main type checking engine

use std::collections::HashMap;

use ast_types::Item;

use crate::errors::TypeError;
use crate::symbol_table::SymbolTable;
use crate::types::Type;

/// Type checker state
pub(crate) struct TypeChecker {
    /// Symbol table for variables
    symbols: SymbolTable,
    /// Function signatures (global scope) — includes mangled method names
    functions: HashMap<String, Type>,
    /// Struct definitions: name → ordered list of (field_name, field_type)
    struct_defs: HashMap<String, Vec<(String, Type)>>,
    /// Methods per struct: struct_name → method_name → mangled function key in `functions`
    ///
    /// The mangled key follows the convention `StructName__methodName`.
    impl_methods: HashMap<String, HashMap<String, String>>,
    /// Compile-time constant names and their declared types (module and function scope).
    pub(crate) constants: HashMap<String, Type>,
    /// Collected type errors
    errors: Vec<TypeError>,
    /// Current function's return type (for validating return statements)
    current_function_return_type: Option<Type>,
    /// Nesting depth of active loop statements.
    loop_depth: u32,
}

mod declarations;
mod expressions;
mod literals;
mod resolution;
mod statements;

#[cfg(test)]
mod tests;

impl TypeChecker {
    pub(crate) fn new() -> Self {
        Self {
            symbols: SymbolTable::new(),
            functions: HashMap::new(),
            struct_defs: HashMap::new(),
            impl_methods: HashMap::new(),
            constants: HashMap::new(),
            errors: Vec::new(),
            current_function_return_type: None,
            loop_depth: 0,
        }
    }

    /// Record an error and continue type checking
    pub(crate) fn record_error(&mut self, error: TypeError) {
        self.errors.push(error);
    }

    /// Get all collected errors
    pub(crate) fn into_errors(self) -> Vec<TypeError> {
        self.errors
    }

    /// Check if there are any errors
    pub(crate) fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Check a complete program
    pub(crate) fn check_program(&mut self, items: &[Item]) -> Result<(), ()> {
        // Pass 1: register struct definitions so type names resolve in method signatures.
        for item in items {
            if let Item::Struct(def) = item {
                let _ = self.register_struct(def);
            }
        }

        // Pass 2: register impl method signatures (uses struct_defs from pass 1).
        for item in items {
            if let Item::Impl(def) = item {
                let _ = self.register_impl(def);
            }
        }

        // Pass 3: register module-level constants so they are visible in function bodies
        // regardless of source order.
        for item in items {
            if let Item::Const(def) = item {
                let _ = self.register_const_item(def);
            }
        }

        // Pass 4: check function, method, and const bodies.
        for item in items {
            match item {
                Item::Function(func) => {
                    let _ = self.check_function(func);
                }
                Item::Impl(def) => self.check_impl(def),
                Item::Const(def) => {
                    let _ = self.check_const_item(def);
                }
                Item::Struct(_) => {}
            }
        }

        if self.has_errors() {
            Err(())
        } else {
            Ok(())
        }
    }
}
