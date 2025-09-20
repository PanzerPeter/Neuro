//! Integration tests for the NEURO compilation pipeline
//! Tests the complete flow from source code to executable generation

use std::process::Command;
use std::fs;
use std::path::Path;
use tempfile::NamedTempFile;

/// Test the complete compilation pipeline using neurc build command
#[test]
fn test_complete_pipeline_simple_function() {
    let source = r#"
fn add(x: int, y: int) -> int {
    return x + y;
}

fn main() -> int {
    return add(5, 3);
}
"#;

    // Create temporary file
    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    // Test neurc build command
    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "neurc build should succeed\nstderr: {}",
            String::from_utf8_lossy(&output.stderr));

    // Check that executable was created
    let exe_path = temp_path.with_extension("exe");
    assert!(exe_path.exists(), "Executable should be created");

    // Test executable execution
    let exec_output = Command::new(&exe_path)
        .output()
        .expect("Failed to execute generated binary");

    assert!(exec_output.status.success(), "Generated executable should run successfully");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let _ = fs::remove_file(&exe_path);
}

/// Test compilation pipeline with control flow
#[test]
fn test_pipeline_with_control_flow() {
    let source = r#"
fn factorial(n: int) -> int {
    if n <= 1 {
        return 1;
    }
    return n * factorial(n - 1);
}

fn main() -> int {
    return factorial(5);
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

    assert!(output.status.success(), "neurc build should succeed for recursive function");

    let exe_path = temp_path.with_extension("exe");
    assert!(exe_path.exists(), "Executable should be created");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let _ = fs::remove_file(&exe_path);
}

/// Test compilation pipeline with loops
#[test]
fn test_pipeline_with_loops() {
    let source = r#"
fn sum_to(n: int) -> int {
    let mut i = 0;
    let mut sum = 0;
    while i < n {
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}

fn main() -> int {
    return sum_to(10);
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "neurc build should succeed for loops");

    let exe_path = temp_path.with_extension("exe");
    assert!(exe_path.exists(), "Executable should be created");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let _ = fs::remove_file(&exe_path);
}

/// Test compilation with complex expressions and logical operations
#[test]
fn test_pipeline_complex_expressions() {
    let source = r#"
fn complex_logic(x: int, y: int, z: bool) -> bool {
    let result = (x > y && z) || (x + y > 10);
    return result && !z;
}

fn main() -> int {
    let test1 = complex_logic(5, 3, true);
    let test2 = complex_logic(2, 8, false);

    if test1 || test2 {
        return 1;
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

    assert!(output.status.success(), "neurc build should succeed for complex expressions");

    let exe_path = temp_path.with_extension("exe");
    assert!(exe_path.exists(), "Executable should be created");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let _ = fs::remove_file(&exe_path);
}

/// Test that neurc run command works
#[test]
fn test_neurc_run_command() {
    let source = r#"
fn main() -> int {
    return 42;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    // Test neurc run command
    let output = Command::new("./target/release/neurc")
        .args(&["run", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc run");

    assert!(output.status.success(), "neurc run should succeed");

    // Clean up
    let _ = fs::remove_file(&temp_path);
}

/// Test compilation with multiple functions
#[test]
fn test_pipeline_multiple_functions() {
    let source = r#"
fn helper1(x: int) -> int {
    return x * 2;
}

fn helper2(x: int) -> int {
    return x + 10;
}

fn combine(a: int, b: int) -> int {
    return helper1(a) + helper2(b);
}

fn main() -> int {
    return combine(5, 3);
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "neurc build should succeed for multiple functions");

    let exe_path = temp_path.with_extension("exe");
    assert!(exe_path.exists(), "Executable should be created");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let _ = fs::remove_file(&exe_path);
}

#[cfg(test)]
mod pipeline_error_tests {
    use super::*;

    /// Test that compilation fails gracefully with syntax errors
    #[test]
    fn test_pipeline_syntax_error() {
        let source = r#"
fn incomplete_function(
"#;

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let temp_path = temp_file.path().with_extension("nr");
        fs::write(&temp_path, source).expect("Failed to write temp file");

        let output = Command::new("./target/release/neurc")
            .args(&["build", temp_path.to_str().unwrap()])
            .output()
            .expect("Failed to execute neurc build");

        assert!(!output.status.success(), "neurc build should fail for syntax errors");

        // Clean up
        let _ = fs::remove_file(&temp_path);
    }

    /// Test that compilation fails gracefully with semantic errors
    #[test]
    fn test_pipeline_semantic_error() {
        let source = r#"
fn main() -> int {
    return undefined_variable;
}
"#;

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let temp_path = temp_file.path().with_extension("nr");
        fs::write(&temp_path, source).expect("Failed to write temp file");

        let output = Command::new("./target/release/neurc")
            .args(&["build", temp_path.to_str().unwrap()])
            .output()
            .expect("Failed to execute neurc build");

        assert!(!output.status.success(), "neurc build should fail for undefined variables");

        // Clean up
        let _ = fs::remove_file(&temp_path);
    }
}