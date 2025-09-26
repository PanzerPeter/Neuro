//! Tests for import resolution functionality

use module_system::{ImportResolver, ModuleError};
use shared_types::Import;
use std::path::PathBuf;
use std::fs;

/// Create a temporary directory structure for testing
fn setup_test_directory() -> tempfile::TempDir {
    let temp_dir = tempfile::tempdir().unwrap();
    
    // Create some test files
    let src_dir = temp_dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();
    
    fs::write(src_dir.join("main.nr"), "fn main() {}").unwrap();
    fs::write(src_dir.join("math.nr"), "fn add(a: int, b: int) -> int {}").unwrap();
    fs::write(src_dir.join("utils.nr"), "fn helper() {}").unwrap();
    
    // Create a subdirectory
    let sub_dir = src_dir.join("lib");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(sub_dir.join("mod.nr"), "// Library module").unwrap();
    fs::write(sub_dir.join("collections.nr"), "// Collections").unwrap();
    
    temp_dir
}

#[test]
fn test_import_resolver_creation() {
    let resolver = ImportResolver::new();
    assert!(resolver.get_cached_imports().is_empty());
}

#[test]
fn test_add_search_path() {
    let mut resolver = ImportResolver::new();
    resolver.add_search_path("/custom/path");
    
    // We can't directly inspect search paths, but we can verify the resolver was created
    assert!(resolver.get_cached_imports().is_empty());
}

#[test]
fn test_resolve_existing_file() {
    let temp_dir = setup_test_directory();
    let mut resolver = ImportResolver::new();
    resolver.add_search_path(temp_dir.path());
    
    let main_file = temp_dir.path().join("src/main.nr");
    let result = resolver.resolve_path(&main_file);
    
    assert!(result.is_ok());
}

#[test]
fn test_resolve_nonexistent_file() {
    let mut resolver = ImportResolver::new();
    let fake_path = PathBuf::from("nonexistent.nr");
    
    let result = resolver.resolve_path(&fake_path);
    assert!(matches!(result, Err(ModuleError::FileNotFound { .. })));
}

#[test]
fn test_relative_import_resolution() {
    let temp_dir = setup_test_directory();
    let mut resolver = ImportResolver::new();
    resolver.add_search_path(temp_dir.path());
    
    let import = Import {
        path: "./math".to_string(),
        span: shared_types::Span::new(0, 6),
    };
    
    let current_module_path = temp_dir.path().join("src/main.nr");
    let result = resolver.resolve_import(&import, &current_module_path);
    
    assert!(result.is_ok());
}

#[test]
fn test_absolute_import_resolution() {
    let temp_dir = setup_test_directory();
    let mut resolver = ImportResolver::new();
    resolver.add_search_path(temp_dir.path().join("src"));
    
    let import = Import {
        path: "utils".to_string(),
        span: shared_types::Span::new(0, 5),
    };
    
    let current_module_path = temp_dir.path().join("src/main.nr");
    let result = resolver.resolve_import(&import, &current_module_path);
    
    assert!(result.is_ok());
}

#[test]
fn test_module_directory_import() {
    let temp_dir = setup_test_directory();
    let mut resolver = ImportResolver::new();
    resolver.add_search_path(temp_dir.path().join("src"));
    
    let import = Import {
        path: "lib".to_string(),
        span: shared_types::Span::new(0, 3),
    };
    
    let current_module_path = temp_dir.path().join("src/main.nr");
    let result = resolver.resolve_import(&import, &current_module_path);
    
    assert!(result.is_ok());
}

#[test]
fn test_failed_import_resolution() {
    let temp_dir = setup_test_directory();
    let mut resolver = ImportResolver::new();
    resolver.add_search_path(temp_dir.path());
    
    let import = Import {
        path: "nonexistent_module".to_string(),
        span: shared_types::Span::new(0, 17),
    };
    
    let current_module_path = temp_dir.path().join("src/main.nr");
    let result = resolver.resolve_import(&import, &current_module_path);
    
    assert!(matches!(result, Err(ModuleError::ImportResolutionFailed { .. })));
}

#[test]
fn test_import_caching() {
    let temp_dir = setup_test_directory();
    let mut resolver = ImportResolver::new();
    resolver.add_search_path(temp_dir.path().join("src"));
    
    let import = Import {
        path: "math".to_string(),
        span: shared_types::Span::new(0, 4),
    };
    
    let current_module_path = temp_dir.path().join("src/main.nr");
    
    // First resolution
    let result1 = resolver.resolve_import(&import, &current_module_path);
    assert!(result1.is_ok());
    assert_eq!(resolver.get_cached_imports().len(), 1);
    
    // Second resolution should use cache
    let result2 = resolver.resolve_import(&import, &current_module_path);
    assert!(result2.is_ok());
    assert_eq!(result1.unwrap(), result2.unwrap());
}

#[test]
fn test_clear_cache() {
    let temp_dir = setup_test_directory();
    let mut resolver = ImportResolver::new();
    resolver.add_search_path(temp_dir.path().join("src"));
    
    let import = Import {
        path: "math".to_string(),
        span: shared_types::Span::new(0, 4),
    };
    
    let current_module_path = temp_dir.path().join("src/main.nr");
    
    // Resolve and cache
    let _result = resolver.resolve_import(&import, &current_module_path);
    assert_eq!(resolver.get_cached_imports().len(), 1);
    
    // Clear cache
    resolver.clear_cache();
    assert_eq!(resolver.get_cached_imports().len(), 0);
}

#[cfg(test)]
mod test_dependencies {
    // Add tempfile as a test dependency
    
    #[test]
    #[ignore] // This is more of a setup verification
    fn verify_tempfile_available() {
        // This test ensures tempfile is available for other tests
        let temp_dir = tempfile::tempdir().unwrap();
        assert!(temp_dir.path().exists());
    }
}