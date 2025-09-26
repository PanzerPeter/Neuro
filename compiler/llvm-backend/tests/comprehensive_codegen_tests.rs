//! Comprehensive code generation tests for the LLVM backend
//! These tests verify that various program structures generate valid LLVM IR

use llvm_backend::compile_to_llvm;
use lexical_analysis::Lexer;
use syntax_parsing::Parser;

/// Helper function to compile source code to LLVM IR
fn compile_source_to_ir(source: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize()?;

    let mut parser = Parser::new(tokens);
    let program = parser.parse()?;

    let result = compile_to_llvm(&program, "test_module")?;
    Ok(result.ir_code)
}

/// Helper function to assert that IR contains expected patterns
fn assert_ir_contains(ir: &str, patterns: &[&str], description: &str) {
    for pattern in patterns {
        assert!(
            ir.contains(pattern),
            "{}: IR should contain '{}'\nGenerated IR:\n{}",
            description,
            pattern,
            ir
        );
    }
}

/// Helper function to assert that IR is valid (basic checks)
fn assert_ir_valid(ir: &str, description: &str) {
    // Basic LLVM IR validation
    assert!(ir.contains("ModuleID"), "{}: Should have module ID", description);
    assert!(ir.contains("target triple"), "{}: Should have target triple", description);
    assert!(!ir.trim().is_empty(), "{}: IR should not be empty", description);
}

#[test]
fn test_simple_function_generation() {
    let source = r#"
fn main() -> int {
    return 42;
}
"#;

    let ir = compile_source_to_ir(source).expect("Should compile successfully");
    assert_ir_valid(&ir, "Simple function");

    let expected_patterns = vec![
        "define",           // Function definition
        "main",            // Function name
        "ret i32",         // Return statement
        "42",              // Return value
    ];

    assert_ir_contains(&ir, &expected_patterns, "Simple function");
}

#[test]
fn test_function_with_parameters() {
    let source = r#"
fn add(x: int, y: int) -> int {
    return x + y;
}
"#;

    let ir = compile_source_to_ir(source).expect("Should compile successfully");
    assert_ir_valid(&ir, "Function with parameters");

    let expected_patterns = vec![
        "define",           // Function definition
        "add",             // Function name
        "i32 %",           // Parameters
        "add i32",         // Addition operation
        "ret i32",         // Return statement
    ];

    assert_ir_contains(&ir, &expected_patterns, "Function with parameters");
}

#[test]
fn test_arithmetic_operations() {
    let source = r#"
fn arithmetic() -> int {
    return 10 + 5 * 2 - 3 / 1;
}
"#;

    let ir = compile_source_to_ir(source).expect("Should compile successfully");
    assert_ir_valid(&ir, "Arithmetic operations");

    let expected_patterns = vec![
        "add i32",         // Addition
        "mul i32",         // Multiplication
        "sub i32",         // Subtraction
        "sdiv i32",        // Division
    ];

    assert_ir_contains(&ir, &expected_patterns, "Arithmetic operations");
}

#[test]
fn test_comparison_operations() {
    let source = r#"
fn compare(x: int, y: int) -> bool {
    return x > y;
}
"#;

    let ir = compile_source_to_ir(source).expect("Should compile successfully");
    assert_ir_valid(&ir, "Comparison operations");

    let expected_patterns = vec![
        "icmp",            // Integer comparison
        "sgt",             // Signed greater than
        "ret i1",          // Boolean return
    ];

    assert_ir_contains(&ir, &expected_patterns, "Comparison operations");
}

#[test]
fn test_if_statement_generation() {
    let source = r#"
fn conditional(x: int) -> int {
    if x > 0 {
        return 1;
    } else {
        return 0;
    }
}
"#;

    let ir = compile_source_to_ir(source).expect("Should compile successfully");
    assert_ir_valid(&ir, "If statement");

    let expected_patterns = vec![
        "icmp",            // Condition evaluation
        "br i1",           // Conditional branch
        "label",           // Basic blocks
    ];

    assert_ir_contains(&ir, &expected_patterns, "If statement");
}

#[test]
fn test_while_loop_generation() {
    let source = r#"
fn loop_test(n: int) -> int {
    let i = 0;
    while i < n {
        i = i + 1;
    }
    return i;
}
"#;

    let ir = compile_source_to_ir(source).expect("Should compile successfully");
    assert_ir_valid(&ir, "While loop");

    let expected_patterns = vec![
        "alloca",          // Local variable allocation
        "br label",        // Unconditional branch to loop
        "icmp",            // Loop condition
        "br i1",           // Conditional branch
        "add i32",         // Loop increment
    ];

    assert_ir_contains(&ir, &expected_patterns, "While loop");
}

#[test]
fn test_function_calls() {
    let source = r#"
fn double(x: int) -> int {
    return x * 2;
}

fn main() -> int {
    return double(21);
}
"#;

    let ir = compile_source_to_ir(source).expect("Should compile successfully");
    assert_ir_valid(&ir, "Function calls");

    let expected_patterns = vec![
        "define",          // Function definitions
        "call i32",        // Function call
        "double",          // Function name
    ];

    assert_ir_contains(&ir, &expected_patterns, "Function calls");
}

#[test]
fn test_recursive_function() {
    let source = r#"
fn factorial(n: int) -> int {
    if n <= 1 {
        return 1;
    } else {
        return n * factorial(n - 1);
    }
}
"#;

    let ir = compile_source_to_ir(source).expect("Should compile successfully");
    assert_ir_valid(&ir, "Recursive function");

    let expected_patterns = vec![
        "define",          // Function definition
        "factorial",       // Function name
        "call i32",        // Recursive call
        "icmp",            // Base case condition
        "br i1",           // Conditional branch
        "mul i32",         // Multiplication
        "sub i32",         // Subtraction for n-1
    ];

    assert_ir_contains(&ir, &expected_patterns, "Recursive function");
}

#[test]
fn test_multiple_functions() {
    let source = r#"
fn add(x: int, y: int) -> int {
    return x + y;
}

fn multiply(x: int, y: int) -> int {
    return x * y;
}

fn compute(a: int, b: int, c: int) -> int {
    return add(multiply(a, b), c);
}
"#;

    let ir = compile_source_to_ir(source).expect("Should compile successfully");
    assert_ir_valid(&ir, "Multiple functions");

    let expected_patterns = vec![
        "define i32 @add",
        "define i32 @multiply",
        "define i32 @compute",
        "call i32 @multiply",
        "call i32 @add",
    ];

    assert_ir_contains(&ir, &expected_patterns, "Multiple functions");
}

#[test]
fn test_boolean_operations() {
    let source = r#"
fn boolean_logic(a: bool, b: bool) -> bool {
    return a && b || !a;
}
"#;

    let ir = compile_source_to_ir(source).expect("Should compile successfully");
    assert_ir_valid(&ir, "Boolean operations");

    // The exact IR patterns depend on how boolean operations are implemented
    // This test ensures the code compiles and generates some form of boolean logic
    assert!(ir.contains("define"), "Should have function definition");
}

#[test]
fn test_variable_assignment() {
    let source = r#"
fn variables() -> int {
    let x = 10;
    let y = 20;
    x = x + y;
    return x;
}
"#;

    let ir = compile_source_to_ir(source).expect("Should compile successfully");
    assert_ir_valid(&ir, "Variable assignment");

    let expected_patterns = vec![
        "alloca",          // Variable allocation
        "store",           // Variable assignment
        "load",            // Variable access
        "add i32",         // Addition
    ];

    assert_ir_contains(&ir, &expected_patterns, "Variable assignment");
}

#[test]
fn test_nested_expressions() {
    let source = r#"
fn nested() -> int {
    return ((1 + 2) * (3 - 4)) / (5 + 6);
}
"#;

    let ir = compile_source_to_ir(source).expect("Should compile successfully");
    assert_ir_valid(&ir, "Nested expressions");

    let expected_patterns = vec![
        "add i32",         // Addition operations
        "sub i32",         // Subtraction
        "mul i32",         // Multiplication
        "sdiv i32",        // Division
    ];

    assert_ir_contains(&ir, &expected_patterns, "Nested expressions");
}

#[test]
fn test_void_function() {
    let source = r#"
fn void_func() {
    let x = 42;
}

fn main() -> int {
    void_func();
    return 0;
}
"#;

    let ir = compile_source_to_ir(source).expect("Should compile successfully");
    assert_ir_valid(&ir, "Void function");

    let expected_patterns = vec![
        "define void @void_func",
        "call void @void_func",
        "ret void",
        "ret i32",
    ];

    assert_ir_contains(&ir, &expected_patterns, "Void function");
}

#[test]
fn test_complex_control_flow() {
    let source = r#"
fn complex_flow(n: int) -> int {
    let result = 0;
    let i = 0;

    while i < n {
        if i % 2 == 0 {
            result = result + i;
        } else {
            result = result - i;
        }
        i = i + 1;
    }

    return result;
}
"#;

    let ir = compile_source_to_ir(source).expect("Should compile successfully");
    assert_ir_valid(&ir, "Complex control flow");

    let expected_patterns = vec![
        "alloca",          // Variable allocations
        "br label",        // Loop structure
        "icmp",            // Comparisons
        "br i1",           // Conditional branches
        "srem i32",        // Modulo operation
        "add i32",         // Addition
        "sub i32",         // Subtraction
    ];

    assert_ir_contains(&ir, &expected_patterns, "Complex control flow");
}

#[test]
fn test_ir_module_structure() {
    let source = r#"
fn test() -> int {
    return 1;
}
"#;

    let ir = compile_source_to_ir(source).expect("Should compile successfully");

    // Test overall module structure
    let expected_structure = vec![
        "ModuleID",                    // Module identifier
        "target triple",               // Target information
        "declare i32 @printf",         // Built-in function declarations
        "define",                      // Function definition
    ];

    assert_ir_contains(&ir, &expected_structure, "Module structure");

    // Ensure IR is well-formed (basic syntax check)
    assert!(ir.starts_with(';'), "IR should start with comment");
    assert!(ir.contains('{') && ir.contains('}'), "IR should have braces");
}