//! Integration tests for the module system

use module_system::{ModuleSystem, ModuleId};
use shared_types::{Program, Item, Import, Function, Block};
use shared_types::{Span, Type};
use std::path::PathBuf;

/// Helper function to create a minimal test program
fn create_test_program(items: Vec<Item>) -> Program {
    Program {
        items,
        span: Span::new(0, 100),
    }
}

/// Helper function to create a test function
fn create_test_function(name: &str) -> Function {
    Function {
        name: name.to_string(),
        parameters: vec![],
        return_type: Some(Type::Int),
        body: Block {
            statements: vec![],
            span: Span::new(0, 10),
        },
        span: Span::new(0, 50),
    }
}

/// Helper function to create an import
fn create_import(path: &str) -> Import {
    Import {
        path: path.to_string(),
        span: Span::new(0, path.len()),
    }
}

#[test]
fn test_module_system_creation() {
    let module_system = ModuleSystem::new();
    assert_eq!(module_system.registry.len(), 0);
}

#[test]
fn test_register_simple_module() {
    let mut module_system = ModuleSystem::new();
    
    let program = create_test_program(vec![
        Item::Function(create_test_function("main")),
    ]);
    
    let path = PathBuf::from("test_module.nr");
    let module_id = module_system.register_module(path.clone(), program);
    
    let module = module_system.get_module(module_id).unwrap();
    assert_eq!(module.path, path);
    assert_eq!(module.exports.len(), 1);
    assert_eq!(module.exports[0], "main");
}

#[test]
fn test_module_with_imports() {
    let mut module_system = ModuleSystem::new();
    
    let program = create_test_program(vec![
        Item::Import(create_import("./math")),
        Item::Function(create_test_function("calculate")),
    ]);
    
    let path = PathBuf::from("calculator.nr");
    let module_id = module_system.register_module(path, program);
    
    let module = module_system.get_module(module_id).unwrap();
    assert_eq!(module.imports.len(), 1);
    assert_eq!(module.imports[0].path, "./math");
    assert_eq!(module.exports.len(), 1);
    assert_eq!(module.exports[0], "calculate");
}

#[test]
fn test_duplicate_module_registration() {
    let mut module_system = ModuleSystem::new();
    
    let program1 = create_test_program(vec![
        Item::Function(create_test_function("func1")),
    ]);
    
    let program2 = create_test_program(vec![
        Item::Function(create_test_function("func2")),
    ]);
    
    let path = PathBuf::from("duplicate.nr");
    let id1 = module_system.register_module(path.clone(), program1);
    let id2 = module_system.register_module(path, program2);
    
    // Should return the same ID for the same path
    assert_eq!(id1, id2);
    assert_eq!(module_system.registry.len(), 1);
}

#[test]
fn test_module_exports_identifier() {
    let mut module_system = ModuleSystem::new();
    
    let program = create_test_program(vec![
        Item::Function(create_test_function("public_func")),
        Item::Function(create_test_function("another_func")),
    ]);
    
    let path = PathBuf::from("exports_test.nr");
    let module_id = module_system.register_module(path, program);
    
    let module = module_system.get_module(module_id).unwrap();
    assert!(module.exports_identifier("public_func"));
    assert!(module.exports_identifier("another_func"));
    assert!(!module.exports_identifier("non_existent"));
}

#[test]
fn test_get_nonexistent_module() {
    let module_system = ModuleSystem::new();
    let fake_id = ModuleId::new(999);
    
    assert!(module_system.get_module(fake_id).is_none());
}

#[test]
fn test_empty_program() {
    let mut module_system = ModuleSystem::new();
    
    let empty_program = create_test_program(vec![]);
    let path = PathBuf::from("empty.nr");
    let module_id = module_system.register_module(path, empty_program);
    
    let module = module_system.get_module(module_id).unwrap();
    assert_eq!(module.imports.len(), 0);
    assert_eq!(module.exports.len(), 0);
}

#[test]
fn test_multiple_modules() {
    let mut module_system = ModuleSystem::new();
    
    // Register first module
    let program1 = create_test_program(vec![
        Item::Function(create_test_function("func1")),
    ]);
    let path1 = PathBuf::from("module1.nr");
    let id1 = module_system.register_module(path1.clone(), program1);
    
    // Register second module
    let program2 = create_test_program(vec![
        Item::Import(create_import("./module1")),
        Item::Function(create_test_function("func2")),
    ]);
    let path2 = PathBuf::from("module2.nr");
    let id2 = module_system.register_module(path2.clone(), program2);
    
    // Verify both modules exist
    assert_ne!(id1, id2);
    assert_eq!(module_system.registry.len(), 2);
    
    let module1 = module_system.get_module(id1).unwrap();
    let module2 = module_system.get_module(id2).unwrap();
    
    assert_eq!(module1.path, path1);
    assert_eq!(module2.path, path2);
    assert_eq!(module1.imports.len(), 0);
    assert_eq!(module2.imports.len(), 1);
}