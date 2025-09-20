//! End-to-end tests for simple NEURO programs
//! Tests complete programs that demonstrate language features working together

use std::process::Command;
use std::fs;
use tempfile::NamedTempFile;

/// Test a simple hello world equivalent program
#[test]
fn test_simple_hello_world() {
    let source = r#"
fn main() -> int {
    return 42;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    // Test compilation
    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Simple hello world should compile\nstderr: {}",
            String::from_utf8_lossy(&output.stderr));

    // Test execution
    let exe_path = temp_path.with_extension("exe");
    assert!(exe_path.exists(), "Executable should be created");

    let exec_output = Command::new(&exe_path)
        .output()
        .expect("Failed to execute generated binary");

    assert!(exec_output.status.success(), "Hello world should execute successfully");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let _ = fs::remove_file(&exe_path);
}

/// Test basic arithmetic operations
#[test]
fn test_arithmetic_program() {
    let source = r#"
fn add(x: int, y: int) -> int {
    return x + y;
}

fn subtract(x: int, y: int) -> int {
    return x - y;
}

fn multiply(x: int, y: int) -> int {
    return x * y;
}

fn divide(x: int, y: int) -> int {
    if y != 0 {
        return x / y;
    }
    return 0;
}

fn main() -> int {
    let a = 20;
    let b = 5;

    let sum = add(a, b);        // 25
    let diff = subtract(a, b);  // 15
    let product = multiply(a, b); // 100
    let quotient = divide(a, b);  // 4

    return sum + diff + product + quotient; // 144
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Arithmetic program should compile");

    let exe_path = temp_path.with_extension("exe");
    let exec_output = Command::new(&exe_path)
        .output()
        .expect("Failed to execute arithmetic program");

    assert!(exec_output.status.success(), "Arithmetic program should execute successfully");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let _ = fs::remove_file(&exe_path);
}

/// Test control flow with conditionals and loops
#[test]
fn test_control_flow_program() {
    let source = r#"
fn factorial(n: int) -> int {
    if n <= 1 {
        return 1;
    }
    return n * factorial(n - 1);
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

fn sum_range(start: int, end: int) -> int {
    let mut sum = 0;
    let mut i = start;
    while i <= end {
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}

fn main() -> int {
    let fact5 = factorial(5);    // 120
    let fib7 = fibonacci(7);     // 13
    let sum_1_to_10 = sum_range(1, 10); // 55

    return fact5 + fib7 + sum_1_to_10; // 188
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Control flow program should compile");

    let exe_path = temp_path.with_extension("exe");
    let exec_output = Command::new(&exe_path)
        .output()
        .expect("Failed to execute control flow program");

    assert!(exec_output.status.success(), "Control flow program should execute successfully");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let _ = fs::remove_file(&exe_path);
}

/// Test boolean logic and comparisons
#[test]
fn test_boolean_logic_program() {
    let source = r#"
fn is_even(n: int) -> bool {
    return n % 2 == 0;
}

fn is_positive(n: int) -> bool {
    return n > 0;
}

fn logical_operations(a: bool, b: bool) -> bool {
    let and_result = a && b;
    let or_result = a || b;
    let not_a = !a;

    return and_result || or_result || not_a;
}

fn compare_numbers(x: int, y: int) -> int {
    if x > y {
        return 1;
    } else if x < y {
        return -1;
    } else {
        return 0;
    }
}

fn main() -> int {
    let num = 8;
    let even = is_even(num);
    let positive = is_positive(num);

    let logic_result = logical_operations(even, positive);
    let comparison = compare_numbers(10, 5);

    if logic_result && comparison > 0 {
        return num;
    }
    return 0;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Boolean logic program should compile");

    let exe_path = temp_path.with_extension("exe");
    let exec_output = Command::new(&exe_path)
        .output()
        .expect("Failed to execute boolean logic program");

    assert!(exec_output.status.success(), "Boolean logic program should execute successfully");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let _ = fs::remove_file(&exe_path);
}

/// Test complex nested function calls
#[test]
fn test_nested_function_calls() {
    let source = r#"
fn helper1(x: int) -> int {
    return x * 2;
}

fn helper2(x: int) -> int {
    return x + 10;
}

fn helper3(x: int) -> int {
    return x - 5;
}

fn complex_calculation(input: int) -> int {
    let step1 = helper1(input);        // input * 2
    let step2 = helper2(step1);        // (input * 2) + 10
    let step3 = helper3(step2);        // ((input * 2) + 10) - 5
    return step3;
}

fn recursive_sum(n: int) -> int {
    if n <= 0 {
        return 0;
    }
    return n + recursive_sum(n - 1);
}

fn main() -> int {
    let input = 5;
    let complex_result = complex_calculation(input); // ((5 * 2) + 10) - 5 = 15
    let recursive_result = recursive_sum(4);         // 4 + 3 + 2 + 1 = 10

    return complex_result + recursive_result; // 25
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Nested function calls should compile");

    let exe_path = temp_path.with_extension("exe");
    let exec_output = Command::new(&exe_path)
        .output()
        .expect("Failed to execute nested function program");

    assert!(exec_output.status.success(), "Nested function program should execute successfully");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let _ = fs::remove_file(&exe_path);
}

/// Test variable scoping and mutability
#[test]
fn test_variable_scoping() {
    let source = r#"
fn test_local_variables() -> int {
    let x = 10;
    let y = 20;

    if x < y {
        let z = x + y;
        return z;
    } else {
        let z = x - y;
        return z;
    }
}

fn test_mutable_variables() -> int {
    let mut counter = 0;
    let mut i = 0;

    while i < 5 {
        counter = counter + i;
        i = i + 1;
    }

    return counter;
}

fn main() -> int {
    let local_result = test_local_variables();    // 30
    let mutable_result = test_mutable_variables(); // 0+1+2+3+4 = 10

    return local_result + mutable_result; // 40
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Variable scoping program should compile");

    let exe_path = temp_path.with_extension("exe");
    let exec_output = Command::new(&exe_path)
        .output()
        .expect("Failed to execute variable scoping program");

    assert!(exec_output.status.success(), "Variable scoping program should execute successfully");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let _ = fs::remove_file(&exe_path);
}

/// Test edge cases and boundary conditions
#[test]
fn test_edge_cases() {
    let source = r#"
fn test_zero_division_protection(a: int, b: int) -> int {
    if b == 0 {
        return 0;
    }
    return a / b;
}

fn test_negative_numbers(x: int) -> int {
    if x < 0 {
        return -x; // Make positive
    }
    return x;
}

fn test_large_numbers() -> int {
    let large1 = 1000;
    let large2 = 2000;
    return large1 + large2;
}

fn main() -> int {
    let div_result = test_zero_division_protection(10, 0); // Protected division
    let abs_result = test_negative_numbers(-15);           // Absolute value
    let large_result = test_large_numbers();               // Large number handling

    return div_result + abs_result + large_result; // 0 + 15 + 3000 = 3015
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Edge cases program should compile");

    let exe_path = temp_path.with_extension("exe");
    let exec_output = Command::new(&exe_path)
        .output()
        .expect("Failed to execute edge cases program");

    assert!(exec_output.status.success(), "Edge cases program should execute successfully");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let _ = fs::remove_file(&exe_path);
}

/// Test all examples from the examples/ directory work correctly
#[test]
fn test_example_programs_compile() {
    let example_files = [
        "examples/06_functions.nr",
        "examples/07_control_if.nr",
        "examples/08_control_while.nr",
        "examples/01_basic_arithmetic.nr",
        "examples/02_conditional_logic.nr",
        "examples/03_loops.nr",
        "examples/04_simple_example.nr",
    ];

    for example_file in &example_files {
        let output = Command::new("./target/release/neurc")
            .args(&["build", example_file])
            .output()
            .expect(&format!("Failed to execute neurc build on {}", example_file));

        assert!(output.status.success(),
                "Example {} should compile\nstderr: {}",
                example_file,
                String::from_utf8_lossy(&output.stderr));

        // Clean up the generated executable
        let exe_path = example_file.replace(".nr", ".exe");
        let _ = fs::remove_file(&exe_path);
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    /// Test that compilation time is reasonable for simple programs
    #[test]
    fn test_compilation_performance() {
        let source = r#"
fn recursive_factorial(n: int) -> int {
    if n <= 1 {
        return 1;
    }
    return n * recursive_factorial(n - 1);
}

fn iterative_factorial(n: int) -> int {
    let mut result = 1;
    let mut i = 1;
    while i <= n {
        result = result * i;
        i = i + 1;
    }
    return result;
}

fn main() -> int {
    let recursive_result = recursive_factorial(10);
    let iterative_result = iterative_factorial(10);
    return recursive_result + iterative_result;
}
"#;

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let temp_path = temp_file.path().with_extension("nr");
        fs::write(&temp_path, source).expect("Failed to write temp file");

        let start_time = Instant::now();

        let output = Command::new("./target/release/neurc")
            .args(&["build", temp_path.to_str().unwrap()])
            .output()
            .expect("Failed to execute neurc build");

        let compilation_time = start_time.elapsed();

        assert!(output.status.success(), "Performance test program should compile");

        // Compilation should complete in reasonable time (less than 30 seconds for simple program)
        assert!(compilation_time.as_secs() < 30,
                "Compilation took too long: {:?}", compilation_time);

        // Clean up
        let _ = fs::remove_file(&temp_path);
        let exe_path = temp_path.with_extension("exe");
        let _ = fs::remove_file(&exe_path);
    }
}