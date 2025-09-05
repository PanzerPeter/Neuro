//! Module registry for tracking loaded modules

use shared_types::{Program, Import, Item};
use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Unique identifier for a module
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModuleId(pub u64);

impl ModuleId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for ModuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Module({})", self.0)
    }
}

/// A loaded module with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Module {
    pub id: ModuleId,
    pub path: PathBuf,
    pub program: Program,
    pub imports: Vec<Import>,
    pub exports: Vec<String>, // For now, simple string identifiers
}

impl Module {
    /// Create a new module from a program
    pub fn new(id: ModuleId, path: PathBuf, program: Program) -> Self {
        let imports = Self::extract_imports(&program);
        let exports = Self::extract_exports(&program);
        
        Self {
            id,
            path,
            program,
            imports,
            exports,
        }
    }

    /// Extract import statements from the program
    fn extract_imports(program: &Program) -> Vec<Import> {
        program.items
            .iter()
            .filter_map(|item| match item {
                Item::Import(import) => Some(import.clone()),
                _ => None,
            })
            .collect()
    }

    /// Extract exported identifiers from the program
    fn extract_exports(program: &Program) -> Vec<String> {
        // For now, we'll consider all top-level functions and structs as exported
        // In the future, this should respect explicit export declarations
        program.items
            .iter()
            .filter_map(|item| match item {
                Item::Function(func) => Some(func.name.clone()),
                Item::Struct(struct_def) => Some(struct_def.name.clone()),
                Item::Import(_) => None,
            })
            .collect()
    }

    /// Check if this module exports a given identifier
    pub fn exports_identifier(&self, name: &str) -> bool {
        self.exports.contains(&name.to_string())
    }
}

/// Registry for tracking loaded modules
#[derive(Debug, Default)]
pub struct ModuleRegistry {
    modules: HashMap<ModuleId, Module>,
    path_to_id: HashMap<PathBuf, ModuleId>,
    next_id: u64,
}

impl ModuleRegistry {
    /// Create a new module registry
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
            path_to_id: HashMap::new(),
            next_id: 1,
        }
    }

    /// Register a new module
    pub fn register(&mut self, path: PathBuf, program: Program) -> ModuleId {
        // Check if module already exists
        if let Some(&existing_id) = self.path_to_id.get(&path) {
            return existing_id;
        }

        let id = ModuleId(self.next_id);
        self.next_id += 1;

        let module = Module::new(id, path.clone(), program);
        
        self.modules.insert(id, module);
        self.path_to_id.insert(path, id);
        
        id
    }

    /// Get a module by ID
    pub fn get(&self, id: ModuleId) -> Option<&Module> {
        self.modules.get(&id)
    }

    /// Get a module by path
    pub fn get_by_path(&self, path: &PathBuf) -> Option<&Module> {
        self.path_to_id
            .get(path)
            .and_then(|&id| self.modules.get(&id))
    }

    /// Get all loaded modules
    pub fn all_modules(&self) -> impl Iterator<Item = &Module> {
        self.modules.values()
    }

    /// Check if a module is registered
    pub fn contains_path(&self, path: &PathBuf) -> bool {
        self.path_to_id.contains_key(path)
    }

    /// Get the number of registered modules
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }
}