//! Import resolution logic

use crate::{ModuleError, ModuleId};
use shared_types::Import;
use std::path::{Path, PathBuf};
use std::collections::HashMap;

/// Resolves import paths to module IDs
#[derive(Debug)]
pub struct ImportResolver {
    /// Cache of resolved import paths
    resolved_cache: HashMap<String, ModuleId>,
    /// Search paths for modules  
    search_paths: Vec<PathBuf>,
}

impl ImportResolver {
    /// Create a new import resolver
    pub fn new() -> Self {
        Self {
            resolved_cache: HashMap::new(),
            search_paths: vec![
                PathBuf::from("."), // Current directory
                PathBuf::from("src"), // Common source directory
                PathBuf::from("lib"), // Common library directory
            ],
        }
    }

    /// Add a search path for modules
    pub fn add_search_path<P: AsRef<Path>>(&mut self, path: P) {
        self.search_paths.push(path.as_ref().to_path_buf());
    }

    /// Resolve a file path to a module ID (for initial loading)
    pub fn resolve_path<P: AsRef<Path>>(&mut self, path: P) -> Result<ModuleId, ModuleError> {
        let path = path.as_ref();
        
        if !path.exists() {
            return Err(ModuleError::FileNotFound {
                path: path.to_path_buf(),
            });
        }

        // For now, just create a unique ID based on the canonical path
        let canonical_path = path.canonicalize()
            .map_err(|e| ModuleError::FileReadError {
                path: path.to_path_buf(),
                error: e.to_string(),
            })?;
        
        let id = self.path_to_module_id(&canonical_path);
        Ok(id)
    }

    /// Resolve an import statement to a module ID
    pub fn resolve_import(
        &mut self,
        import: &Import,
        current_module_path: &PathBuf,
    ) -> Result<ModuleId, ModuleError> {
        // Check cache first
        if let Some(&cached_id) = self.resolved_cache.get(&import.path) {
            return Ok(cached_id);
        }

        let resolved_path = self.resolve_import_path(&import.path, current_module_path)?;
        
        if !resolved_path.exists() {
            return Err(ModuleError::ImportResolutionFailed {
                path: import.path.clone(),
            });
        }

        let id = self.path_to_module_id(&resolved_path);
        self.resolved_cache.insert(import.path.clone(), id);
        
        Ok(id)
    }

    /// Resolve an import path string to a filesystem path
    fn resolve_import_path(
        &self,
        import_path: &str,
        current_module_path: &PathBuf,
    ) -> Result<PathBuf, ModuleError> {
        // Handle relative imports (starting with ./ or ../)
        if import_path.starts_with("./") || import_path.starts_with("../") {
            return self.resolve_relative_import(import_path, current_module_path);
        }

        // Handle absolute imports by searching in search paths
        self.resolve_absolute_import(import_path)
    }

    /// Resolve a relative import path
    fn resolve_relative_import(
        &self,
        import_path: &str,
        current_module_path: &PathBuf,
    ) -> Result<PathBuf, ModuleError> {
        let current_dir = current_module_path
            .parent()
            .ok_or_else(|| ModuleError::InvalidModulePath {
                path: current_module_path.to_string_lossy().to_string(),
            })?;

        let mut resolved_path = current_dir.join(import_path);
        
        // Try with .nr extension if not present
        if resolved_path.extension().is_none() {
            resolved_path.set_extension("nr");
        }

        Ok(resolved_path)
    }

    /// Resolve an absolute import path
    fn resolve_absolute_import(&self, import_path: &str) -> Result<PathBuf, ModuleError> {
        // Convert import path to file path (replace :: with /)
        let file_path = import_path.replace("::", "/");

        // Try each search path
        for search_path in &self.search_paths {
            let mut candidate = search_path.join(&file_path);
            
            // Try with .nr extension if not present
            if candidate.extension().is_none() {
                candidate.set_extension("nr");
            }

            if candidate.exists() {
                return Ok(candidate);
            }

            // Also try as a directory with mod.nr
            let mod_file = search_path.join(&file_path).join("mod.nr");
            if mod_file.exists() {
                return Ok(mod_file);
            }
        }

        Err(ModuleError::ImportResolutionFailed {
            path: import_path.to_string(),
        })
    }

    /// Convert a file path to a module ID
    fn path_to_module_id(&self, path: &PathBuf) -> ModuleId {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        ModuleId::new(hasher.finish())
    }

    /// Get all cached resolved imports
    pub fn get_cached_imports(&self) -> &HashMap<String, ModuleId> {
        &self.resolved_cache
    }

    /// Clear the resolution cache
    pub fn clear_cache(&mut self) {
        self.resolved_cache.clear();
    }
}

impl Default for ImportResolver {
    fn default() -> Self {
        Self::new()
    }
}