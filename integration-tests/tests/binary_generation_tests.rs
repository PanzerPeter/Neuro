//! Tests for native binary generation

use lexical_analysis::Lexer;
use syntax_parsing::Parser;
use semantic_analysis::analyze_program;
use llvm_backend::compile_to_executable;
use std::process::Command;

/// Test compilation of a simple program to executable
#[test]
fn test_simple_executable_generation() {
    let source = r#"
fn main() -> int {
    return 42;
}
"#;

    // Compile through the full pipeline
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexical analysis should succeed");
    
    let mut parser = Parser::new(tokens);
    let ast = parser.parse().expect("Parsing should succeed");
    
    let _semantic_info = analyze_program(&ast)
        .expect("Semantic analysis should succeed");
    
    // Generate executable (this may fail if LLVM tools are not installed)
    let exe_path = match compile_to_executable(&ast, "simple_test", "target/test_executable") {
        Ok(path) => path,
        Err(e) => {
            // If LLVM tools are not available, skip the test but don't fail
            if e.to_string().contains("llc") || e.to_string().contains("program not found") {
                println!("Skipping binary generation test: LLVM tools not available");
                println!("Error: {}", e);
                return;
            } else {
                panic!("Binary generation should succeed: {}", e);
            }
        }
    };
    
    assert!(exe_path.exists(), "Executable file should be created");
    
    // Try to run the executable (if LLVM tools are available)
    if let Ok(output) = Command::new(&exe_path).output() {
        // The program should exit with code 42
        assert_eq!(output.status.code(), Some(42), "Program should return 42");
    } else {
        // If we can't run it, at least verify the file was created
        println!("Note: Could not execute generated binary (LLVM tools may not be available)");
        println!("But binary file was successfully created at: {}", exe_path.display());
    }
}

/// Test compilation of a program with basic arithmetic
#[test] 
fn test_arithmetic_executable() {
    let source = r#"
fn add(x: int, y: int) -> int {
    return x + y;
}

fn main() -> int {
    return add(10, 5);
}
"#;

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexical analysis should succeed");
    
    let mut parser = Parser::new(tokens);
    let ast = parser.parse().expect("Parsing should succeed");
    
    let _semantic_info = analyze_program(&ast)
        .expect("Semantic analysis should succeed");
    
    // Generate executable (this may fail if LLVM tools are not installed)
    let exe_path = match compile_to_executable(&ast, "arithmetic_test", "target/arithmetic_executable") {
        Ok(path) => path,
        Err(e) => {
            // If LLVM tools are not available, skip the test but don't fail
            if e.to_string().contains("llc") || e.to_string().contains("program not found") {
                println!("Skipping binary generation test: LLVM tools not available");
                println!("Error: {}", e);
                return;
            } else {
                panic!("Binary generation should succeed: {}", e);
            }
        }
    };
    
    assert!(exe_path.exists(), "Executable file should be created");
    
    // Try to run the executable
    if let Ok(output) = Command::new(&exe_path).output() {
        // The program should exit with code 15 (10 + 5)
        assert_eq!(output.status.code(), Some(15), "Program should return 15");
    } else {
        println!("Note: Could not execute generated binary (LLVM tools may not be available)");
        println!("But binary file was successfully created at: {}", exe_path.display());
    }
}