//! NEURO Module System
//! 
//! This slice handles module resolution, import/export analysis, and package loading.
//! It follows VSA principles by being self-contained and focused on module management.

pub mod import_resolver;
pub mod module_registry;
pub mod dependency_graph;
pub mod error;

pub use import_resolver::*;
pub use module_registry::*;
pub use dependency_graph::*;
pub use error::*;

use shared_types::{Program, Import};
use std::path::{Path, PathBuf};
use std::collections::HashMap;

/// Main module system interface
pub struct ModuleSystem {
    pub registry: ModuleRegistry,
    resolver: ImportResolver,
}

impl ModuleSystem {
    /// Create a new module system
    pub fn new() -> Self {
        Self {
            registry: ModuleRegistry::new(),
            resolver: ImportResolver::new(),
        }
    }

    /// Load a module from a file path
    pub fn load_module<P: AsRef<Path>>(&mut self, path: P) -> Result<ModuleId, ModuleError> {
        self.resolver.resolve_path(path)
    }

    /// Register a parsed program as a module
    pub fn register_module(&mut self, path: PathBuf, program: Program) -> ModuleId {
        self.registry.register(path, program)
    }

    /// Get a module by ID
    pub fn get_module(&self, id: ModuleId) -> Option<&Module> {
        self.registry.get(id)
    }

    /// Resolve all imports in a module
    pub fn resolve_imports(&mut self, module_id: ModuleId) -> Result<Vec<ModuleId>, ModuleError> {
        let module = self.registry.get(module_id)
            .ok_or(ModuleError::ModuleNotFound(format!("{:?}", module_id)))?;
        
        let mut resolved_modules = Vec::new();
        
        for import in &module.imports {
            let resolved_id = self.resolver.resolve_import(import, &module.path)?;
            resolved_modules.push(resolved_id);
        }
        
        Ok(resolved_modules)
    }
}

impl Default for ModuleSystem {
    fn default() -> Self {
        Self::new()
    }
}