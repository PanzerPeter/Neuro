//! High-level code generation utilities
//! 
//! This module provides high-level code generation functions and utilities
//! for generating LLVM IR from NEURO AST.

use shared_types::{Program, ast::Statement, Span};
use crate::{LLVMError, CompilationResult};

/// Code generation options
#[derive(Debug, Clone)]
pub struct CodeGenOptions {
    pub optimize: bool,
    pub debug_info: bool,
    pub target_triple: Option<String>,
}

impl Default for CodeGenOptions {
    fn default() -> Self {
        Self {
            optimize: false,
            debug_info: false,
            target_triple: None,
        }
    }
}

/// High-level code generator
pub struct CodeGenerator {
    options: CodeGenOptions,
}

impl CodeGenerator {
    /// Create a new code generator with options
    pub fn new(options: CodeGenOptions) -> Self {
        Self { options }
    }
    
    /// Generate LLVM IR for a program
    pub fn generate(&self, program: &Program, module_name: &str) -> Result<CompilationResult, LLVMError> {
        tracing::info!("Generating code for program: {}", module_name);
        
        // Use the main compile_to_llvm function
        let mut result = crate::compile_to_llvm(program, module_name)?;
        
        // Apply options
        result.optimized = self.options.optimize;
        result.debug_info = self.options.debug_info;
        
        tracing::info!("Code generation completed for: {}", module_name);
        Ok(result)
    }
    
    /// Generate code for a single statement (utility function)
    pub fn generate_statement_ir(&self, statement: &Statement) -> Result<String, LLVMError> {
        // This would be used for interactive evaluation
        // For now, return a placeholder
        match statement {
            Statement::Expression(_) => {
                Ok("; Expression statement IR would go here".to_string())
            },
            Statement::Let(_) => {
                Ok("; Let statement IR would go here".to_string())
            },
            Statement::Return(_) => {
                Ok("; Return statement IR would go here".to_string())
            },
            _ => {
                Err(LLVMError::CodeGeneration {
                    message: "Statement type not supported for individual IR generation".to_string(),
                    span: Span::new(0, 0),
                })
            }
        }
    }
    
    /// Generate a wrapper main function for a program
    pub fn generate_main_wrapper(&self, program: &Program) -> Result<String, LLVMError> {
        let mut wrapper = String::new();
        
        wrapper.push_str("; Main wrapper function\n");
        wrapper.push_str("define i32 @main() {\n");
        wrapper.push_str("entry:\n");
        
        // Look for a main function in the program
        let has_main = program.items.iter().any(|item| {
            if let shared_types::ast::Item::Function(f) = item {
                f.name == "main"
            } else {
                false
            }
        });
        
        if has_main {
            wrapper.push_str("  %result = call i32 @main()\n");
            wrapper.push_str("  ret i32 %result\n");
        } else {
            wrapper.push_str("  ret i32 0\n");
        }
        
        wrapper.push_str("}\n");
        
        Ok(wrapper)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_types::{Program, ast::Function, Type, Span};
    
    #[test]
    fn test_codegen_creation() {
        let options = CodeGenOptions::default();
        let codegen = CodeGenerator::new(options);
        assert!(!codegen.options.optimize);
        assert!(!codegen.options.debug_info);
    }
    
    #[test]
    fn test_main_wrapper_generation() {
        let codegen = CodeGenerator::new(CodeGenOptions::default());
        
        // Test program without main function
        let empty_program = Program {
            items: vec![],
            span: Span::new(0, 0),
        };
        
        let wrapper = codegen.generate_main_wrapper(&empty_program);
        assert!(wrapper.is_ok());
        
        let wrapper_code = wrapper.unwrap();
        assert!(wrapper_code.contains("define i32 @main()"));
        assert!(wrapper_code.contains("ret i32 0"));
    }
    
    #[test]
    fn test_main_wrapper_with_main_function() {
        let codegen = CodeGenerator::new(CodeGenOptions::default());
        
        // Test program with main function
        let main_function = Function {
            name: "main".to_string(),
            parameters: vec![],
            return_type: Some(Type::Int),
            body: shared_types::ast::Block {
                statements: vec![],
                span: Span::new(5, 10),
            },
            span: Span::new(0, 10),
        };
        
        let program = Program {
            items: vec![shared_types::ast::Item::Function(main_function)],
            span: Span::new(0, 20),
        };
        
        let wrapper = codegen.generate_main_wrapper(&program);
        assert!(wrapper.is_ok());
        
        let wrapper_code = wrapper.unwrap();
        assert!(wrapper_code.contains("call i32 @main()"));
    }
}