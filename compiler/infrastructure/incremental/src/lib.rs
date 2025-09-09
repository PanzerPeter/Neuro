//! Incremental Compilation Support for NEURO
//!
//! This module provides caching and dependency tracking for efficient
//! incremental compilation, avoiding re-compilation of unchanged modules.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use anyhow::{Context, Result};

/// Compilation cache for incremental builds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationCache {
    /// File hash -> compilation result cache
    pub file_cache: HashMap<String, CachedCompilation>,
    /// File path -> file hash mapping
    pub file_hashes: HashMap<PathBuf, String>,
    /// Dependency graph: file -> set of dependencies
    pub dependencies: HashMap<PathBuf, HashSet<PathBuf>>,
    /// Cache format version for compatibility
    pub version: String,
}

/// Cached compilation result for a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedCompilation {
    /// Source file path
    pub source_path: PathBuf,
    /// File content hash
    pub content_hash: String,
    /// Compilation timestamp
    pub timestamp: u64,
    /// Generated LLVM IR (if applicable)
    pub llvm_ir: Option<String>,
    /// Compilation success status
    pub success: bool,
    /// Error messages (if any)
    pub errors: Vec<String>,
    /// Dependencies that were resolved during compilation
    pub dependencies: HashSet<PathBuf>,
}

/// Incremental compilation manager
pub struct IncrementalCompiler {
    /// Compilation cache
    cache: CompilationCache,
    /// Cache file path
    cache_path: PathBuf,
    /// Whether cache was loaded successfully
    cache_loaded: bool,
}

impl CompilationCache {
    /// Create a new empty compilation cache
    pub fn new() -> Self {
        Self {
            file_cache: HashMap::new(),
            file_hashes: HashMap::new(),
            dependencies: HashMap::new(),
            version: "1.0.0".to_string(),
        }
    }

    /// Load cache from file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read cache file: {}", path.as_ref().display()))?;
        
        let cache: CompilationCache = serde_json::from_str(&content)
            .with_context(|| "Failed to deserialize compilation cache")?;
        
        // Verify cache version compatibility
        if cache.version != "1.0.0" {
            return Err(anyhow::anyhow!(
                "Incompatible cache version: {} (expected 1.0.0)", 
                cache.version
            ));
        }
        
        Ok(cache)
    }

    /// Save cache to file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = serde_json::to_string_pretty(self)
            .with_context(|| "Failed to serialize compilation cache")?;
        
        // Ensure parent directory exists
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create cache directory: {}", parent.display()))?;
        }
        
        fs::write(path.as_ref(), content)
            .with_context(|| format!("Failed to write cache file: {}", path.as_ref().display()))?;
        
        Ok(())
    }

    /// Check if a file needs recompilation
    pub fn needs_recompilation<P: AsRef<Path>>(&self, file_path: P) -> Result<bool> {
        let path = file_path.as_ref();
        
        // Check if file exists in cache
        let current_hash = Self::compute_file_hash(path)?;
        
        if let Some(cached_hash) = self.file_hashes.get(path) {
            if cached_hash == &current_hash {
                // Check if any dependencies have changed
                if let Some(deps) = self.dependencies.get(path) {
                    for dep_path in deps {
                        if self.needs_recompilation(dep_path)? {
                            return Ok(true); // Dependency changed, need recompilation
                        }
                    }
                }
                return Ok(false); // File and dependencies unchanged
            }
        }
        
        Ok(true) // File not in cache or hash changed
    }

    /// Update cache entry for a file
    pub fn update_entry(
        &mut self,
        file_path: PathBuf,
        compilation: CachedCompilation,
    ) {
        self.file_hashes.insert(file_path.clone(), compilation.content_hash.clone());
        self.file_cache.insert(compilation.content_hash.clone(), compilation.clone());
        
        // Update dependency graph
        if !compilation.dependencies.is_empty() {
            self.dependencies.insert(file_path, compilation.dependencies);
        }
    }

    /// Get cached compilation result
    pub fn get_cached_result<P: AsRef<Path>>(&self, file_path: P) -> Option<&CachedCompilation> {
        let path = file_path.as_ref();
        let hash = self.file_hashes.get(path)?;
        self.file_cache.get(hash)
    }

    /// Compute SHA-256 hash of file content
    pub fn compute_file_hash<P: AsRef<Path>>(file_path: P) -> Result<String> {
        let content = fs::read(file_path.as_ref())
            .with_context(|| format!("Failed to read file: {}", file_path.as_ref().display()))?;
        
        let mut hasher = Sha256::new();
        hasher.update(&content);
        Ok(format!("{:x}", hasher.finalize()))
    }

    /// Clear cache entries for files that no longer exist
    pub fn cleanup_stale_entries(&mut self) -> Result<()> {
        let mut stale_paths = Vec::new();
        
        for path in self.file_hashes.keys() {
            if !path.exists() {
                stale_paths.push(path.clone());
            }
        }
        
        for path in stale_paths {
            if let Some(hash) = self.file_hashes.remove(&path) {
                self.file_cache.remove(&hash);
            }
            self.dependencies.remove(&path);
        }
        
        Ok(())
    }

    /// Get compilation statistics
    pub fn get_stats(&self) -> CompilationStats {
        let total_files = self.file_hashes.len();
        let successful_compilations = self.file_cache.values()
            .filter(|c| c.success)
            .count();
        
        CompilationStats {
            total_files,
            successful_compilations,
            failed_compilations: total_files - successful_compilations,
            cache_size_mb: 0.0, // Could implement actual size calculation
        }
    }
}

/// Compilation statistics for reporting
#[derive(Debug, Clone)]
pub struct CompilationStats {
    pub total_files: usize,
    pub successful_compilations: usize,
    pub failed_compilations: usize,
    pub cache_size_mb: f64,
}

impl IncrementalCompiler {
    /// Create new incremental compiler with cache at given path
    pub fn new<P: AsRef<Path>>(cache_path: P) -> Self {
        let cache_path = cache_path.as_ref().to_path_buf();
        
        // Try to load existing cache
        let (cache, cache_loaded) = match CompilationCache::load_from_file(&cache_path) {
            Ok(cache) => (cache, true),
            Err(_) => (CompilationCache::new(), false),
        };
        
        Self {
            cache,
            cache_path,
            cache_loaded,
        }
    }

    /// Check if incremental compilation is available
    pub fn is_cache_loaded(&self) -> bool {
        self.cache_loaded
    }

    /// Check if file needs recompilation
    pub fn needs_recompilation<P: AsRef<Path>>(&self, file_path: P) -> Result<bool> {
        self.cache.needs_recompilation(file_path)
    }

    /// Get cached compilation result
    pub fn get_cached_result<P: AsRef<Path>>(&self, file_path: P) -> Option<&CachedCompilation> {
        self.cache.get_cached_result(file_path)
    }

    /// Record successful compilation
    pub fn record_compilation(
        &mut self,
        file_path: PathBuf,
        llvm_ir: Option<String>,
        dependencies: HashSet<PathBuf>,
    ) -> Result<()> {
        let content_hash = CompilationCache::compute_file_hash(&file_path)?;
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let cached_compilation = CachedCompilation {
            source_path: file_path.clone(),
            content_hash,
            timestamp,
            llvm_ir,
            success: true,
            errors: vec![],
            dependencies,
        };

        self.cache.update_entry(file_path, cached_compilation);
        Ok(())
    }

    /// Record failed compilation
    pub fn record_failure(
        &mut self,
        file_path: PathBuf,
        errors: Vec<String>,
    ) -> Result<()> {
        let content_hash = CompilationCache::compute_file_hash(&file_path)?;
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let cached_compilation = CachedCompilation {
            source_path: file_path.clone(),
            content_hash,
            timestamp,
            llvm_ir: None,
            success: false,
            errors,
            dependencies: HashSet::new(),
        };

        self.cache.update_entry(file_path, cached_compilation);
        Ok(())
    }

    /// Save cache to disk
    pub fn save_cache(&self) -> Result<()> {
        self.cache.save_to_file(&self.cache_path)
    }

    /// Clean up stale cache entries
    pub fn cleanup(&mut self) -> Result<()> {
        self.cache.cleanup_stale_entries()
    }

    /// Get compilation statistics
    pub fn get_stats(&self) -> CompilationStats {
        self.cache.get_stats()
    }

    /// Clear entire cache
    pub fn clear_cache(&mut self) -> Result<()> {
        self.cache = CompilationCache::new();
        if self.cache_path.exists() {
            fs::remove_file(&self.cache_path)
                .with_context(|| format!("Failed to remove cache file: {}", self.cache_path.display()))?;
        }
        Ok(())
    }
}

impl Default for CompilationCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_cache_creation() {
        let cache = CompilationCache::new();
        assert_eq!(cache.version, "1.0.0");
        assert!(cache.file_cache.is_empty());
        assert!(cache.file_hashes.is_empty());
        assert!(cache.dependencies.is_empty());
    }

    #[test]
    fn test_incremental_compiler_creation() {
        let temp_dir = tempdir().unwrap();
        let cache_path = temp_dir.path().join("cache.json");
        
        let compiler = IncrementalCompiler::new(&cache_path);
        assert!(!compiler.is_cache_loaded()); // No cache file exists yet
    }

    #[test]
    fn test_file_hash_computation() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.nr");
        
        fs::write(&test_file, "fn main() -> int { return 42; }").unwrap();
        
        let hash1 = CompilationCache::compute_file_hash(&test_file).unwrap();
        let hash2 = CompilationCache::compute_file_hash(&test_file).unwrap();
        
        assert_eq!(hash1, hash2); // Same content should have same hash
        assert_eq!(hash1.len(), 64); // SHA-256 hash length
    }

    #[test]
    fn test_needs_recompilation() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.nr");
        
        fs::write(&test_file, "fn main() -> int { return 42; }").unwrap();
        
        let mut cache = CompilationCache::new();
        
        // First check - should need compilation (not in cache)
        assert!(cache.needs_recompilation(&test_file).unwrap());
        
        // Add to cache
        let hash = CompilationCache::compute_file_hash(&test_file).unwrap();
        let compilation = CachedCompilation {
            source_path: test_file.clone(),
            content_hash: hash.clone(),
            timestamp: 0,
            llvm_ir: None,
            success: true,
            errors: vec![],
            dependencies: HashSet::new(),
        };
        
        cache.update_entry(test_file.clone(), compilation);
        
        // Second check - should not need compilation (in cache, unchanged)
        assert!(!cache.needs_recompilation(&test_file).unwrap());
        
        // Modify file
        fs::write(&test_file, "fn main() -> int { return 43; }").unwrap();
        
        // Third check - should need compilation (file changed)
        assert!(cache.needs_recompilation(&test_file).unwrap());
    }

    #[test]
    fn test_cache_save_and_load() {
        let temp_dir = tempdir().unwrap();
        let cache_path = temp_dir.path().join("cache.json");
        let test_file = temp_dir.path().join("test.nr");
        
        fs::write(&test_file, "fn main() -> int { return 42; }").unwrap();
        
        // Create and populate cache
        let mut cache = CompilationCache::new();
        let hash = CompilationCache::compute_file_hash(&test_file).unwrap();
        let compilation = CachedCompilation {
            source_path: test_file.clone(),
            content_hash: hash,
            timestamp: 12345,
            llvm_ir: Some("define i32 @main() { ret i32 42 }".to_string()),
            success: true,
            errors: vec![],
            dependencies: HashSet::new(),
        };
        
        cache.update_entry(test_file.clone(), compilation);
        
        // Save cache
        cache.save_to_file(&cache_path).unwrap();
        assert!(cache_path.exists());
        
        // Load cache
        let loaded_cache = CompilationCache::load_from_file(&cache_path).unwrap();
        
        // Verify loaded cache
        assert_eq!(loaded_cache.version, "1.0.0");
        assert_eq!(loaded_cache.file_hashes.len(), 1);
        assert_eq!(loaded_cache.file_cache.len(), 1);
        
        let cached_result = loaded_cache.get_cached_result(&test_file).unwrap();
        assert_eq!(cached_result.timestamp, 12345);
        assert!(cached_result.success);
        assert_eq!(cached_result.llvm_ir.as_ref().unwrap(), "define i32 @main() { ret i32 42 }");
    }

    #[test]
    fn test_compilation_stats() {
        let mut cache = CompilationCache::new();
        let temp_dir = tempdir().unwrap();
        
        // Add successful compilation
        let file1 = temp_dir.path().join("success.nr");
        fs::write(&file1, "fn main() -> int { return 42; }").unwrap();
        let hash1 = CompilationCache::compute_file_hash(&file1).unwrap();
        let compilation1 = CachedCompilation {
            source_path: file1.clone(),
            content_hash: hash1,
            timestamp: 0,
            llvm_ir: Some("llvm ir".to_string()),
            success: true,
            errors: vec![],
            dependencies: HashSet::new(),
        };
        cache.update_entry(file1, compilation1);
        
        // Add failed compilation
        let file2 = temp_dir.path().join("failure.nr");
        fs::write(&file2, "invalid syntax").unwrap();
        let hash2 = CompilationCache::compute_file_hash(&file2).unwrap();
        let compilation2 = CachedCompilation {
            source_path: file2.clone(),
            content_hash: hash2,
            timestamp: 0,
            llvm_ir: None,
            success: false,
            errors: vec!["Syntax error".to_string()],
            dependencies: HashSet::new(),
        };
        cache.update_entry(file2, compilation2);
        
        let stats = cache.get_stats();
        assert_eq!(stats.total_files, 2);
        assert_eq!(stats.successful_compilations, 1);
        assert_eq!(stats.failed_compilations, 1);
    }
}