//! Integration tests for the full NEURO compilation pipeline

use lexical_analysis::Lexer;
use syntax_parsing::Parser;
use semantic_analysis::analyze_program;
use llvm_backend::compile_to_llvm;

/// Test the complete compilation pipeline for a simple function
#[test]
fn test_simple_function_compilation() {
    let source = r#"
fn test() -> int {
    return 42;
}
"#;

    // Step 1: Lexical Analysis
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexical analysis should succeed");
    
    // Step 2: Syntax Parsing
    let mut parser = Parser::new(tokens);
    let ast = parser.parse().expect("Parsing should succeed");
    
    assert_eq!(ast.items.len(), 1, "Should have exactly one function");
    
    // Step 3: Semantic Analysis
    let _semantic_info = analyze_program(&ast)
        .expect("Semantic analysis should succeed");
    
    // Step 4: LLVM Code Generation
    let compilation_result = compile_to_llvm(&ast, "test_module")
        .expect("LLVM compilation should succeed");
    
    assert!(!compilation_result.ir_code.is_empty(), "Should generate LLVM IR");
    assert!(compilation_result.ir_code.contains("define"), "Should contain function definition");
    assert!(compilation_result.ir_code.contains("test"), "Should contain function name");
}

/// Test compilation of function with parameters
#[test]
fn test_function_with_parameters() {
    let source = r#"
fn add(x: int, y: int) -> int {
    return x + y;
}
"#;

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexical analysis should succeed");
    
    let mut parser = Parser::new(tokens);
    let ast = parser.parse().expect("Parsing should succeed");
    
    let _semantic_info = analyze_program(&ast)
        .expect("Semantic analysis should succeed");
    
    let compilation_result = compile_to_llvm(&ast, "add_module")
        .expect("LLVM compilation should succeed");
    
    assert!(compilation_result.ir_code.contains("define"));
    assert!(compilation_result.ir_code.contains("add"));
}

/// Test compilation of function with control flow
#[test]
fn test_control_flow_compilation() {
    let source = r#"
fn test_if(n: int) -> int {
    if n > 0 {
        return 1;
    }
    return 0;
}
"#;

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexical analysis should succeed");
    
    let mut parser = Parser::new(tokens);
    let ast = parser.parse().expect("Parsing should succeed");
    
    let _semantic_info = analyze_program(&ast)
        .expect("Semantic analysis should succeed");
    
    let compilation_result = compile_to_llvm(&ast, "control_flow_module")
        .expect("LLVM compilation should succeed");
    
    assert!(compilation_result.ir_code.contains("define"));
    assert!(compilation_result.ir_code.contains("test_if"));
}

/// Test compilation with logical expressions
#[test]
fn test_logical_expressions_compilation() {
    let source = r#"
fn test_logic(x: bool, y: bool) -> bool {
    return x && y || !x;
}
"#;

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexical analysis should succeed");
    
    let mut parser = Parser::new(tokens);
    let ast = parser.parse().expect("Parsing should succeed");
    
    let _semantic_info = analyze_program(&ast)
        .expect("Semantic analysis should succeed");
    
    let compilation_result = compile_to_llvm(&ast, "logic_module")
        .expect("LLVM compilation should succeed");
    
    assert!(compilation_result.ir_code.contains("define"));
    assert!(compilation_result.ir_code.contains("test_logic"));
}

/// Test compilation of the complex program that originally failed
#[test]
fn test_complex_program_compilation() {
    let source = r#"
fn is_prime(n: int) -> bool {
    if n <= 1 {
        return false;
    }
    if n <= 3 {
        return true;
    }
    if n % 2 == 0 || n % 3 == 0 {
        return false;
    }
    
    let mut i = 5;
    while i * i <= n {
        if n % i == 0 || n % (i + 2) == 0 {
            return false;
        }
        i = i + 6;
    }
    return true;
}

fn fibonacci(n: int) -> int {
    if n <= 0 {
        return 0;
    }
    if n == 1 {
        return 1;
    }
    return fibonacci(n - 1) + fibonacci(n - 2);
}

fn logical_test(x: int, y: int) -> bool {
    let is_positive = x > 0 && y > 0;
    let has_zero = x == 0 || y == 0;
    let is_negative = !is_positive && !has_zero;
    
    return is_positive || has_zero || is_negative;
}

fn main() -> int {
    let num = 17;
    let prime_result = is_prime(num);
    
    let fib_result = fibonacci(8);
    
    let logic_result = logical_test(-5, 10);
    
    if prime_result && fib_result > 20 {
        return 1;
    }
    
    if !logic_result {
        return -1;
    }
    
    return 0;
}
"#;

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexical analysis should succeed");
    
    let mut parser = Parser::new(tokens);
    let ast = parser.parse().expect("Parsing should succeed");
    
    assert_eq!(ast.items.len(), 4, "Should have exactly 4 functions");
    
    let _semantic_info = analyze_program(&ast)
        .expect("Semantic analysis should succeed");
    
    let compilation_result = compile_to_llvm(&ast, "complex_module")
        .expect("LLVM compilation should succeed");
    
    assert!(compilation_result.ir_code.contains("define"));
    assert!(compilation_result.ir_code.contains("is_prime"));
    assert!(compilation_result.ir_code.contains("fibonacci"));
    assert!(compilation_result.ir_code.contains("logical_test"));
    assert!(compilation_result.ir_code.contains("main"));
}

/// Test compilation error handling
#[test]
fn test_compilation_error_handling() {
    // Test with invalid syntax
    let invalid_source = "fn incomplete(";
    
    let mut lexer = Lexer::new(invalid_source);
    let tokens = lexer.tokenize().expect("Tokenization should still work");
    
    let mut parser = Parser::new(tokens);
    let result = parser.parse();
    
    assert!(result.is_err(), "Parsing should fail for invalid syntax");
}

/// Test compilation with different data types
#[test]
fn test_different_data_types() {
    let source = r#"
fn test_types() -> bool {
    let int_val = 42;
    let float_val = 3.14;
    let bool_val = true;
    let string_val = "hello";
    
    return bool_val;
}
"#;

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexical analysis should succeed");
    
    let mut parser = Parser::new(tokens);
    let ast = parser.parse().expect("Parsing should succeed");
    
    let _semantic_info = analyze_program(&ast)
        .expect("Semantic analysis should succeed");
    
    let compilation_result = compile_to_llvm(&ast, "types_module")
        .expect("LLVM compilation should succeed");
    
    assert!(compilation_result.ir_code.contains("define"));
    assert!(compilation_result.ir_code.contains("test_types"));
}