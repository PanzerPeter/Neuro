//! Tests for dependency graph functionality

use module_system::{DependencyGraph, ModuleId, ModuleError};

#[test]
fn test_empty_dependency_graph() {
    let graph = DependencyGraph::new();
    assert!(graph.is_empty());
    assert_eq!(graph.len(), 0);
}

#[test]
fn test_add_single_dependency() {
    let mut graph = DependencyGraph::new();
    let module_a = ModuleId::new(1);
    let module_b = ModuleId::new(2);
    
    graph.add_dependency(module_a, module_b);
    
    assert!(!graph.is_empty());
    assert_eq!(graph.len(), 2);
    
    let deps = graph.get_dependencies(module_a);
    assert!(deps.contains(&module_b));
    assert_eq!(deps.len(), 1);
}

#[test]
fn test_multiple_dependencies() {
    let mut graph = DependencyGraph::new();
    let module_a = ModuleId::new(1);
    let module_b = ModuleId::new(2);
    let module_c = ModuleId::new(3);
    
    graph.add_dependency(module_a, module_b);
    graph.add_dependency(module_a, module_c);
    
    let deps = graph.get_dependencies(module_a);
    assert!(deps.contains(&module_b));
    assert!(deps.contains(&module_c));
    assert_eq!(deps.len(), 2);
}

#[test]
fn test_no_dependencies() {
    let graph = DependencyGraph::new();
    let module_a = ModuleId::new(1);
    
    let deps = graph.get_dependencies(module_a);
    assert!(deps.is_empty());
}

#[test]
fn test_simple_cycle_detection() {
    let mut graph = DependencyGraph::new();
    let module_a = ModuleId::new(1);
    let module_b = ModuleId::new(2);
    
    // Create a simple cycle: A -> B -> A
    graph.add_dependency(module_a, module_b);
    graph.add_dependency(module_b, module_a);
    
    let result = graph.detect_cycles();
    assert!(matches!(result, Err(ModuleError::CircularDependency { .. })));
}

#[test]
fn test_no_cycle_detection() {
    let mut graph = DependencyGraph::new();
    let module_a = ModuleId::new(1);
    let module_b = ModuleId::new(2);
    let module_c = ModuleId::new(3);
    
    // Create a chain: A -> B -> C (no cycle)
    graph.add_dependency(module_a, module_b);
    graph.add_dependency(module_b, module_c);
    
    let result = graph.detect_cycles();
    assert!(result.is_ok());
}

#[test]
fn test_complex_cycle_detection() {
    let mut graph = DependencyGraph::new();
    let module_a = ModuleId::new(1);
    let module_b = ModuleId::new(2);
    let module_c = ModuleId::new(3);
    let module_d = ModuleId::new(4);
    
    // Create dependencies: A -> B -> C -> D -> B (cycle in B -> C -> D -> B)
    graph.add_dependency(module_a, module_b);
    graph.add_dependency(module_b, module_c);
    graph.add_dependency(module_c, module_d);
    graph.add_dependency(module_d, module_b);
    
    let result = graph.detect_cycles();
    assert!(matches!(result, Err(ModuleError::CircularDependency { .. })));
}

#[test]
fn test_self_dependency() {
    let mut graph = DependencyGraph::new();
    let module_a = ModuleId::new(1);
    
    // Module depends on itself
    graph.add_dependency(module_a, module_a);
    
    let result = graph.detect_cycles();
    assert!(matches!(result, Err(ModuleError::CircularDependency { .. })));
}

#[test]
fn test_topological_sort_simple() {
    let mut graph = DependencyGraph::new();
    let module_a = ModuleId::new(1);
    let module_b = ModuleId::new(2);
    let module_c = ModuleId::new(3);
    
    // A -> B -> C
    graph.add_dependency(module_a, module_b);
    graph.add_dependency(module_b, module_c);
    
    let result = graph.topological_sort();
    assert!(result.is_ok());
    
    let sorted = result.unwrap();
    
    // Since A -> B -> C, C should come first (no dependencies), then B, then A
    let pos_a = sorted.iter().position(|&x| x == module_a).unwrap();
    let pos_b = sorted.iter().position(|&x| x == module_b).unwrap();
    let pos_c = sorted.iter().position(|&x| x == module_c).unwrap();
    
    assert!(pos_c < pos_b);
    assert!(pos_b < pos_a);
}

#[test]
fn test_topological_sort_with_cycle() {
    let mut graph = DependencyGraph::new();
    let module_a = ModuleId::new(1);
    let module_b = ModuleId::new(2);
    
    // A -> B -> A (cycle)
    graph.add_dependency(module_a, module_b);
    graph.add_dependency(module_b, module_a);
    
    let result = graph.topological_sort();
    assert!(matches!(result, Err(ModuleError::CircularDependency { .. })));
}

#[test]
fn test_topological_sort_independent_modules() {
    let mut graph = DependencyGraph::new();
    let module_a = ModuleId::new(1);
    let module_b = ModuleId::new(2);
    
    // Two independent modules (no dependencies between them)
    // Just add them to the graph without dependencies
    graph.add_dependency(module_a, module_a); // This will create a cycle, let's avoid it
    
    // Actually, let's create a proper test with independent modules
    let mut graph = DependencyGraph::new();
    let module_c = ModuleId::new(3);
    let module_d = ModuleId::new(4);
    
    // C -> A, D -> B (independent chains)
    graph.add_dependency(module_c, module_a);
    graph.add_dependency(module_d, module_b);
    
    let result = graph.topological_sort();
    assert!(result.is_ok());
    
    let sorted = result.unwrap();
    assert_eq!(sorted.len(), 4);
}

#[test]
fn test_empty_graph_topological_sort() {
    let graph = DependencyGraph::new();
    let result = graph.topological_sort();
    
    assert!(result.is_ok());
    let sorted = result.unwrap();
    assert!(sorted.is_empty());
}