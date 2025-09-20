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
pub mod jit_executor;

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

        // First pass: register all function signatures
        let mut builder = crate::function_builder::TextBasedFunctionBuilder::new();
        for item in &program.items {
            if let shared_types::ast::Item::Function(function) = item {
                builder.register_function_signature(function);
            }
        }

        // Second pass: compile all functions with known signatures
        for item in &program.items {
            if let shared_types::ast::Item::Function(function) = item {
                self.compile_function_with_signatures(function, &builder)?;
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

        // Add built-in function declarations
        self.add_builtin_function_declarations();

        Ok(())
    }

    /// Add declarations for built-in functions
    fn add_builtin_function_declarations(&mut self) {
        // Declare external C runtime functions
        self.ir_lines.push("; External C runtime function declarations".to_string());
        self.ir_lines.push("declare i32 @printf(ptr, ...)".to_string());
        self.ir_lines.push("declare i32 @puts(ptr)".to_string());
        self.ir_lines.push("".to_string());

        // Add global format string for integer printing
        self.ir_lines.push("; Global format string for printing integers".to_string());
        self.ir_lines.push("@.str.int = private unnamed_addr constant [4 x i8] c\"%d\\0A\\00\", align 1".to_string());
        self.ir_lines.push("".to_string());

        // Implement the print function using printf
        self.ir_lines.push("; Built-in print function implementation".to_string());
        self.ir_lines.push("define i32 @print(i32 %value) {".to_string());
        self.ir_lines.push("entry:".to_string());
        self.ir_lines.push("  %0 = call i32 (ptr, ...) @printf(ptr @.str.int, i32 %value)".to_string());
        self.ir_lines.push("  ret i32 %0".to_string());
        self.ir_lines.push("}".to_string());
        self.ir_lines.push("".to_string());
    }
    
    /// Compile a single function
    fn compile_function(&mut self, function: &Function) -> Result<(), LLVMError> {
        use crate::function_builder::TextBasedFunctionBuilder;

        tracing::debug!("Compiling function: {}", function.name);

        let mut builder = TextBasedFunctionBuilder::new();
        let function_ir = builder.build_function(function)
            .map_err(|e| {
                LLVMError::FunctionCompilation {
                    message: format!("Failed to compile function: {}", e),
                    function_name: function.name.clone(),
                    span: function.span,
                }
            })?;

        // Add function IR to module
        for line in function_ir.lines() {
            self.ir_lines.push(line.to_string());
        }
        self.ir_lines.push("".to_string());

        tracing::debug!("Successfully compiled function: {}", function.name);
        Ok(())
    }

    /// Compile a single function with pre-registered signatures
    fn compile_function_with_signatures(&mut self, function: &Function, signatures: &crate::function_builder::TextBasedFunctionBuilder) -> Result<(), LLVMError> {
        tracing::debug!("Compiling function with signatures: {}", function.name);

        let mut builder = signatures.clone();
        let function_ir = builder.build_function(function)
            .map_err(|e| {
                LLVMError::FunctionCompilation {
                    message: format!("Failed to compile function: {}", e),
                    function_name: function.name.clone(),
                    span: function.span,
                }
            })?;

        // Add function IR to module
        for line in function_ir.lines() {
            self.ir_lines.push(line.to_string());
        }
        self.ir_lines.push("".to_string());

        tracing::debug!("Successfully compiled function: {}", function.name);
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

    // Try LLVM compilation first
    match try_llvm_compilation(program, module_name, &output_path) {
        Ok(path) => Ok(path),
        Err(llvm_error) => {
            tracing::warn!("LLVM compilation failed: {}. Falling back to JIT execution.", llvm_error);
            // Fall back to JIT execution for now
            Err(LLVMError::ModuleGeneration {
                message: format!("LLVM tools not available. {}", llvm_error),
            })
        }
    }
}

/// Try to compile using LLVM tools
fn try_llvm_compilation<P: AsRef<std::path::Path>>(
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

/// Execute a NEURO program using JIT compilation
pub fn execute_program_jit(program: &Program) -> Result<jit_executor::JitResult, LLVMError> {
    jit_executor::execute_program(program)
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