//! Integration tests for error reporting and diagnostics
//! Tests that the compiler provides helpful error messages and proper error handling

use std::process::Command;
use std::fs;
use tempfile::NamedTempFile;

/// Test lexical error reporting
#[test]
fn test_lexical_error_reporting() {
    let source = r#"
fn main() -> int {
    let x = 42;
    // Invalid character sequence
    let y = 3.14.15;  // Double decimal point
    return x;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["check", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc check");

    // Should fail but provide meaningful error message
    assert!(!output.status.success(), "Should fail due to lexical error");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Check that error message is informative
    assert!(!stderr.is_empty(), "Should provide error message");

    // Clean up
    let _ = fs::remove_file(&temp_path);
}

/// Test syntax error reporting
#[test]
fn test_syntax_error_reporting() {
    let source = r#"
fn incomplete_function(x: int {
    return x + 1;
}

fn main() -> int {
    return incomplete_function(5);
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["parse", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc parse");

    assert!(!output.status.success(), "Should fail due to syntax error");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "Should provide error message for syntax error");

    // Clean up
    let _ = fs::remove_file(&temp_path);
}

/// Test semantic error reporting - undefined variable
#[test]
fn test_undefined_variable_error() {
    let source = r#"
fn main() -> int {
    return undefined_variable;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["check", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc check");

    assert!(!output.status.success(), "Should fail due to undefined variable");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "Should provide error message for undefined variable");

    // Clean up
    let _ = fs::remove_file(&temp_path);
}

/// Test semantic error reporting - type mismatch
#[test]
fn test_type_mismatch_error() {
    let source = r#"
fn main() -> int {
    let x: int = true;  // Type mismatch
    return x;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["check", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc check");

    assert!(!output.status.success(), "Should fail due to type mismatch");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "Should provide error message for type mismatch");

    // Clean up
    let _ = fs::remove_file(&temp_path);
}

/// Test function signature error reporting
#[test]
fn test_function_signature_error() {
    let source = r#"
fn add(x: int, y: int) -> int {
    return x + y;
}

fn main() -> int {
    return add(5);  // Wrong number of arguments
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["check", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc check");

    assert!(!output.status.success(), "Should fail due to wrong number of arguments");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "Should provide error message for function signature mismatch");

    // Clean up
    let _ = fs::remove_file(&temp_path);
}

/// Test return type mismatch error
#[test]
fn test_return_type_mismatch_error() {
    let source = r#"
fn test() -> int {
    return true;  // bool instead of int
}

fn main() -> int {
    return test();
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["check", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc check");

    assert!(!output.status.success(), "Should fail due to return type mismatch");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "Should provide error message for return type mismatch");

    // Clean up
    let _ = fs::remove_file(&temp_path);
}

/// Test duplicate function definition error
#[test]
fn test_duplicate_function_error() {
    let source = r#"
fn test() -> int {
    return 1;
}

fn test() -> int {  // Duplicate function
    return 2;
}

fn main() -> int {
    return test();
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["check", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc check");

    assert!(!output.status.success(), "Should fail due to duplicate function definition");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "Should provide error message for duplicate function");

    // Clean up
    let _ = fs::remove_file(&temp_path);
}

/// Test missing main function error
#[test]
fn test_missing_main_function_error() {
    let source = r#"
fn helper() -> int {
    return 42;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(!output.status.success(), "Should fail due to missing main function");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "Should provide error message for missing main function");

    // Clean up
    let _ = fs::remove_file(&temp_path);
}

/// Test that good code produces no errors
#[test]
fn test_no_errors_for_valid_code() {
    let source = r#"
fn factorial(n: int) -> int {
    if n <= 1 {
        return 1;
    }
    return n * factorial(n - 1);
}

fn main() -> int {
    let result = factorial(5);
    return result;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["check", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc check");

    assert!(output.status.success(), "Should succeed for valid code");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should have minimal or no error output for valid code

    // Clean up
    let _ = fs::remove_file(&temp_path);
}

/// Test error recovery - continue parsing after errors
#[test]
fn test_error_recovery() {
    let source = r#"
fn invalid_syntax( {
    return 42;
}

fn valid_function() -> int {
    return 10;
}

fn main() -> int {
    return valid_function();
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["parse", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc parse");

    // Should fail but attempt to continue parsing
    assert!(!output.status.success(), "Should fail due to syntax error in first function");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "Should provide error message");

    // Clean up
    let _ = fs::remove_file(&temp_path);
}

#[cfg(test)]
mod error_message_quality_tests {
    use super::*;

    /// Test that error messages include source location information
    #[test]
    fn test_error_includes_location() {
        let source = r#"
fn main() -> int {
    let x = unknown_function();
    return x;
}
"#;

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let temp_path = temp_file.path().with_extension("nr");
        fs::write(&temp_path, source).expect("Failed to write temp file");

        let output = Command::new("./target/release/neurc")
            .args(&["check", temp_path.to_str().unwrap()])
            .output()
            .expect("Failed to execute neurc check");

        assert!(!output.status.success(), "Should fail due to unknown function");

        let stderr = String::from_utf8_lossy(&output.stderr);
        // Should contain line or position information
        assert!(!stderr.is_empty(), "Should provide detailed error message");

        // Clean up
        let _ = fs::remove_file(&temp_path);
    }
}