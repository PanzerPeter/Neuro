//! LLVM Backend for NEURO Compiler
//! 
//! This module implements code generation from NEURO AST to LLVM IR.
//! It follows the Vertical Slice Architecture (VSA) principles with focused
//! responsibilities for each component.

pub mod codegen;
// pub mod debug_info; // Temporarily disabled
pub mod function_builder;
// pub mod intrinsics; // Temporarily disabled
pub mod module_builder;
// pub mod optimization_passes; // Temporarily disabled - requires llvm_sys
// pub mod type_mapping; // Temporarily disabled
pub mod binary_generation;

use shared_types::{Program, ast::Function, Span};
use thiserror::Error;

/// Main LLVM backend for NEURO compilation (text-based IR generator)
pub struct LLVMBackend {
    module_name: String,
    ir_lines: Vec<String>,
}

/// Errors that can occur during LLVM code generation
#[derive(Error, Debug)]
pub enum LLVMError {
    #[error("Type conversion error: {message} at {span:?}")]
    TypeConversion { message: String, span: Span },
    
    #[error("Function compilation error: {message} for function '{function_name}' at {span:?}")]
    FunctionCompilation { message: String, function_name: String, span: Span },
    
    #[error("Module generation error: {message}")]
    ModuleGeneration { message: String },
    
    #[error("LLVM initialization error: {message}")]
    Initialization { message: String },
    
    #[error("Code generation error: {message} at {span:?}")]
    CodeGeneration { message: String, span: Span },
}

/// Result of LLVM compilation
#[derive(Debug, Clone)]
pub struct CompilationResult {
    pub module_name: String,
    pub ir_code: String,
    pub optimized: bool,
    pub debug_info: bool,
}

impl LLVMBackend {
    /// Create a new LLVM backend instance
    pub fn new(module_name: &str) -> Result<Self, LLVMError> {
        Ok(LLVMBackend {
            module_name: module_name.to_string(),
            ir_lines: Vec::new(),
        })
    }
    
    /// Compile a NEURO program to LLVM IR
    pub fn compile_program(&mut self, program: &Program) -> Result<CompilationResult, LLVMError> {
        tracing::info!("Starting LLVM compilation for program");
        
        // Set up module metadata
        self.setup_module_metadata()?;
        
        // Compile all functions in the program
        for item in &program.items {
            if let shared_types::ast::Item::Function(function) = item {
                self.compile_function(function)?;
            }
        }
        
        // Generate the IR code
        let ir_code = self.get_ir_string()?;
        
        tracing::info!("LLVM compilation completed successfully");
        
        Ok(CompilationResult {
            module_name: self.module_name.clone(),
            ir_code,
            optimized: false,
            debug_info: false,
        })
    }
    
    /// Set up module-level metadata and target information
    fn setup_module_metadata(&mut self) -> Result<(), LLVMError> {
        self.ir_lines.push(format!("; ModuleID = '{}'", self.module_name));
        self.ir_lines.push(format!("source_filename = \"{}\"", self.module_name));
        self.ir_lines.push("".to_string());
        
        // Add target triple
        self.ir_lines.push("target triple = \"x86_64-pc-windows-msvc\"".to_string());
        self.ir_lines.push("".to_string());
        Ok(())
    }
    
    /// Compile a single function
    fn compile_function(&mut self, function: &Function) -> Result<(), LLVMError> {
        use crate::function_builder::TextBasedFunctionBuilder;
        
        let mut builder = TextBasedFunctionBuilder::new();
        let function_ir = builder.build_function(function)?;
        
        // Add function IR to module
        for line in function_ir.lines() {
            self.ir_lines.push(line.to_string());
        }
        self.ir_lines.push("".to_string());
        
        Ok(())
    }
    
    /// Get the LLVM IR as a string
    fn get_ir_string(&self) -> Result<String, LLVMError> {
        Ok(self.ir_lines.join("\n"))
    }
}

/// Main entry point for compiling a NEURO program to LLVM IR
pub fn compile_to_llvm(program: &Program, module_name: &str) -> Result<CompilationResult, LLVMError> {
    let mut backend = LLVMBackend::new(module_name)?;
    backend.compile_program(program)
}

/// Compile a NEURO program directly to native executable
pub fn compile_to_executable<P: AsRef<std::path::Path>>(
    program: &Program, 
    module_name: &str,
    output_path: P,
) -> Result<std::path::PathBuf, LLVMError> {
    use binary_generation::{BinaryGenerator, BinaryOptions, OptimizationLevel};
    
    // First compile to LLVM IR
    let compilation_result = compile_to_llvm(program, module_name)?;
    
    // Set up binary generation
    let output_dir = output_path.as_ref().parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let generator = BinaryGenerator::new(output_dir)?;
    
    let options = BinaryOptions {
        output_path: output_path.as_ref().to_path_buf(),
        optimization_level: OptimizationLevel::Default,
        debug_info: false,
        target_triple: Some("x86_64-pc-windows-msvc".to_string()),
    };
    
    // Generate executable
    generator.generate_executable(&compilation_result, &options)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_backend_initialization() {
        let backend = LLVMBackend::new("test_module");
        assert!(backend.is_ok(), "LLVM backend should initialize successfully");
    }
    
    #[test]
    fn test_empty_program_compilation() {
        let empty_program = Program {
            items: vec![],
            span: Span::new(0, 0),
        };
        
        let result = compile_to_llvm(&empty_program, "empty_test");
        assert!(result.is_ok(), "Empty program should compile successfully");
        
        let compilation_result = result.unwrap();
        assert_eq!(compilation_result.module_name, "empty_test");
        assert!(!compilation_result.ir_code.is_empty());
    }
}