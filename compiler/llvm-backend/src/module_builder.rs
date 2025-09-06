//! Module compilation support for LLVM backend
//! 
//! This module handles the compilation of NEURO modules and their dependencies.

use shared_types::{Program, ast::Import};
use crate::{LLVMError, CompilationResult};
use std::collections::HashMap;

/// Module builder for managing NEURO module compilation
pub struct ModuleBuilder {
    /// Compiled modules cache
    compiled_modules: HashMap<String, CompilationResult>,
    
    /// Module dependency graph
    dependency_graph: HashMap<String, Vec<String>>,
}

impl ModuleBuilder {
    /// Create a new module builder
    pub fn new() -> Self {
        Self {
            compiled_modules: HashMap::new(),
            dependency_graph: HashMap::new(),
        }
    }
    
    /// Compile a program with its dependencies
    pub fn compile_program_with_dependencies(
        &mut self,
        program: &Program,
        module_name: &str,
    ) -> Result<CompilationResult, LLVMError> {
        tracing::info!("Compiling program with dependencies: {}", module_name);
        
        // Analyze dependencies
        self.analyze_dependencies(program, module_name)?;
        
        // Compile dependencies first
        self.compile_dependencies(module_name)?;
        
        // Compile the main program
        let result = crate::compile_to_llvm(program, module_name)?;
        
        // Cache the result
        self.compiled_modules.insert(module_name.to_string(), result.clone());
        
        tracing::info!("Successfully compiled program with dependencies: {}", module_name);
        Ok(result)
    }
    
    /// Analyze the dependency graph for a program
    fn analyze_dependencies(
        &mut self,
        program: &Program,
        module_name: &str,
    ) -> Result<(), LLVMError> {
        let mut dependencies = Vec::new();
        
        for item in &program.items {
            if let shared_types::ast::Item::Import(import) = item {
                let dep_name = self.extract_module_name(import)?;
                dependencies.push(dep_name);
            }
        }
        
        // Check for circular dependencies
        if self.has_circular_dependency(module_name, &dependencies) {
            return Err(LLVMError::ModuleGeneration {
                message: format!("Circular dependency detected for module '{}'", module_name),
            });
        }
        
        self.dependency_graph.insert(module_name.to_string(), dependencies);
        Ok(())
    }
    
    /// Extract module name from import statement
    fn extract_module_name(&self, import: &Import) -> Result<String, LLVMError> {
        // For now, use the first component of the path
        if let Some(first_component) = import.path.split("::").next() {
            Ok(first_component.to_string())
        } else {
            Err(LLVMError::ModuleGeneration {
                message: format!("Invalid import path: {}", import.path),
            })
        }
    }
    
    /// Check for circular dependencies
    fn has_circular_dependency(&self, module_name: &str, new_deps: &[String]) -> bool {
        for dep in new_deps {
            if self.dependency_leads_to_module(dep, module_name) {
                return true;
            }
        }
        false
    }
    
    /// Check if a dependency path leads back to a module
    fn dependency_leads_to_module(&self, start: &str, target: &str) -> bool {
        let mut visited = std::collections::HashSet::new();
        self.dependency_leads_to_module_impl(start, target, &mut visited)
    }
    
    /// Implementation of circular dependency check with visited tracking
    fn dependency_leads_to_module_impl(&self, start: &str, target: &str, visited: &mut std::collections::HashSet<String>) -> bool {
        if start == target {
            return true;
        }
        
        if visited.contains(start) {
            return false; // Already visited, avoid infinite recursion
        }
        
        visited.insert(start.to_string());
        
        if let Some(deps) = self.dependency_graph.get(start) {
            for dep in deps {
                if self.dependency_leads_to_module_impl(dep, target, visited) {
                    return true;
                }
            }
        }
        
        false
    }
    
    /// Compile all dependencies for a module
    fn compile_dependencies(&mut self, module_name: &str) -> Result<(), LLVMError> {
        if let Some(deps) = self.dependency_graph.get(module_name).cloned() {
            for dep in deps {
                if !self.compiled_modules.contains_key(&dep) {
                    // For now, we assume dependencies are standard library modules
                    // In a real implementation, we would load and compile the dependency
                    self.compile_standard_library_module(&dep)?;
                }
            }
        }
        Ok(())
    }
    
    /// Compile a standard library module (placeholder implementation)
    fn compile_standard_library_module(&mut self, module_name: &str) -> Result<(), LLVMError> {
        tracing::debug!("Compiling standard library module: {}", module_name);
        
        match module_name {
            "std" => {
                // Create a basic std module with common functions
                let std_ir = self.generate_std_module_ir()?;
                let result = CompilationResult {
                    module_name: "std".to_string(),
                    ir_code: std_ir,
                    optimized: false,
                    debug_info: false,
                };
                self.compiled_modules.insert("std".to_string(), result);
            },
            _ => {
                // For unknown modules, create an empty module
                let empty_ir = self.generate_empty_module_ir(module_name)?;
                let result = CompilationResult {
                    module_name: module_name.to_string(),
                    ir_code: empty_ir,
                    optimized: false,
                    debug_info: false,
                };
                self.compiled_modules.insert(module_name.to_string(), result);
            }
        }
        
        Ok(())
    }
    
    /// Generate LLVM IR for the std module
    fn generate_std_module_ir(&self) -> Result<String, LLVMError> {
        // For now, return a minimal std module with basic math functions
        Ok(r#"; ModuleID = 'std'
source_filename = "std"

; Math functions declarations
declare float @sin(float)
declare float @cos(float)
declare float @tan(float)
declare float @sqrt(float)
declare float @log(float)
declare float @exp(float)
declare float @pow(float, float)
declare float @abs(float)

; Integer math functions
declare i32 @abs_int(i32)
"#.to_string())
    }
    
    /// Generate empty LLVM IR for an unknown module
    fn generate_empty_module_ir(&self, module_name: &str) -> Result<String, LLVMError> {
        Ok(format!(r#"; ModuleID = '{}'
source_filename = "{}"

; Empty module - no definitions
"#, module_name, module_name))
    }
    
    /// Get the compilation result for a module
    pub fn get_module_result(&self, module_name: &str) -> Option<&CompilationResult> {
        self.compiled_modules.get(module_name)
    }
    
    /// Get all compiled modules
    pub fn get_all_modules(&self) -> &HashMap<String, CompilationResult> {
        &self.compiled_modules
    }
    
    /// Link multiple modules together
    pub fn link_modules(&self, main_module: &str) -> Result<String, LLVMError> {
        let mut linked_ir = String::new();
        
        // Add all dependency modules first
        if let Some(deps) = self.dependency_graph.get(main_module) {
            for dep in deps {
                if let Some(module_result) = self.compiled_modules.get(dep) {
                    linked_ir.push_str(&format!("; === Module: {} ===\n", dep));
                    linked_ir.push_str(&module_result.ir_code);
                    linked_ir.push('\n');
                }
            }
        }
        
        // Add the main module
        if let Some(main_result) = self.compiled_modules.get(main_module) {
            linked_ir.push_str(&format!("; === Main Module: {} ===\n", main_module));
            linked_ir.push_str(&main_result.ir_code);
        }
        
        Ok(linked_ir)
    }
}

impl Default for ModuleBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_types::{Program, Import};
    
    #[test]
    fn test_module_builder_creation() {
        let builder = ModuleBuilder::new();
        assert!(builder.compiled_modules.is_empty());
        assert!(builder.dependency_graph.is_empty());
    }
    
    #[test]
    fn test_circular_dependency_detection() {
        let mut builder = ModuleBuilder::new();
        
        // Set up a circular dependency: A -> B -> A
        builder.dependency_graph.insert("A".to_string(), vec!["B".to_string()]);
        builder.dependency_graph.insert("B".to_string(), vec!["A".to_string()]);
        
        // Test that C depending on A would NOT create a circular dependency (A doesn't depend on C)
        assert!(!builder.has_circular_dependency("C", &["A".to_string()]));
        
        // Test that A depending on C would create a circular dependency if C also depended on A
        builder.dependency_graph.insert("C".to_string(), vec!["A".to_string()]);
        assert!(builder.has_circular_dependency("A", &["C".to_string()]));
    }
    
    #[test]
    fn test_empty_program_compilation() {
        let mut builder = ModuleBuilder::new();
        let empty_program = Program {
            items: vec![],
            span: shared_types::Span::new(0, 0),
        };
        
        let result = builder.compile_program_with_dependencies(&empty_program, "test");
        assert!(result.is_ok());
        
        let compilation_result = result.unwrap();
        assert_eq!(compilation_result.module_name, "test");
    }
    
    #[test]
    fn test_std_module_ir_generation() {
        let builder = ModuleBuilder::new();
        let std_ir = builder.generate_std_module_ir();
        assert!(std_ir.is_ok());
        
        let ir_code = std_ir.unwrap();
        assert!(ir_code.contains("ModuleID = 'std'"));
        assert!(ir_code.contains("declare float @sin(float)"));
    }
}