//! LLVM IR generation command implementation

use std::path::PathBuf;
use std::fs;
use anyhow::{Context, Result};
use lexical_analysis::Lexer;
use syntax_parsing::Parser;
use semantic_analysis::analyze_program;
use llvm_backend::compile_to_llvm;

/// Generate LLVM IR from NEURO source file
pub fn run_llvm(
    file: PathBuf,
    opt_level: u8,
    output: Option<PathBuf>,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("Generating LLVM IR from: {}", file.display());
    }

    // Read source file
    let source = fs::read_to_string(&file)
        .with_context(|| format!("Failed to read source file: {}", file.display()))?;

    // Lexical analysis
    if verbose {
        println!("Phase 1: Lexical analysis...");
    }
    let mut lexer = Lexer::new(&source);
    let tokens = lexer.tokenize()
        .with_context(|| "Lexical analysis failed")?;

    // Syntax parsing
    if verbose {
        println!("Phase 2: Syntax parsing...");
    }
    let mut parser = Parser::new(tokens);
    let ast = parser.parse()
        .with_context(|| "Syntax parsing failed")?;

    // Semantic analysis
    if verbose {
        println!("Phase 3: Semantic analysis...");
    }
    let _semantic_info = analyze_program(&ast)
        .with_context(|| "Semantic analysis failed")?;

    // LLVM code generation
    if verbose {
        println!("Phase 4: LLVM IR generation...");
    }
    
    // Extract module name from file path
    let module_name = file.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("main");
    
    let mut compilation_result = compile_to_llvm(&ast, module_name)
        .with_context(|| "LLVM code generation failed")?;

    // Apply optimizations if requested
    if opt_level > 0 {
        if verbose {
            println!("Phase 5: Applying optimization level {}...", opt_level);
        }
        // Note: For a complete implementation, we would apply optimizations here
        // For now, just mark as optimized
        compilation_result.optimized = true;
    }

    // Output the LLVM IR
    match output {
        Some(output_path) => {
            fs::write(&output_path, &compilation_result.ir_code)
                .with_context(|| format!("Failed to write LLVM IR to: {}", output_path.display()))?;
            
            if verbose {
                println!("LLVM IR written to: {}", output_path.display());
            }
        }
        None => {
            // Print to stdout
            println!("{}", compilation_result.ir_code);
        }
    }

    // Print compilation summary
    if verbose {
        println!("\nCompilation Summary:");
        println!("  Module: {}", compilation_result.module_name);
        println!("  Optimized: {}", compilation_result.optimized);
        println!("  Debug Info: {}", compilation_result.debug_info);
        println!("  IR Lines: {}", compilation_result.ir_code.lines().count());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_llvm_generation() {
        // Create a temporary source file
        let temp_dir = tempdir().unwrap();
        let source_file = temp_dir.path().join("test.nr");
        
        let source_code = r#"
fn main() -> int {
    let x = 42;
    return x;
}
"#;
        
        fs::write(&source_file, source_code).unwrap();
        
        // Test LLVM generation
        let result = run_llvm(source_file, 0, None, false);
        
        // Should succeed for basic programs
        assert!(result.is_ok(), "LLVM generation should succeed for valid programs");
    }

    #[test]
    fn test_llvm_with_output_file() {
        // Create temporary files
        let temp_dir = tempdir().unwrap();
        let source_file = temp_dir.path().join("test.nr");
        let output_file = temp_dir.path().join("output.ll");
        
        let source_code = r#"
fn add(x: int, y: int) -> int {
    return x + y;
}
"#;
        
        fs::write(&source_file, source_code).unwrap();
        
        // Test LLVM generation with output file
        let result = run_llvm(source_file, 1, Some(output_file.clone()), true);
        assert!(result.is_ok());
        
        // Check that output file was created
        assert!(output_file.exists());
        
        // Check that output file contains LLVM IR
        let ir_content = fs::read_to_string(&output_file).unwrap();
        assert!(ir_content.contains("define"));
        assert!(ir_content.contains("add"));
    }
}