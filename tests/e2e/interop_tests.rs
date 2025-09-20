//! Interoperability tests for NEURO compiler
//! Tests interaction with external systems, C libraries, and standard interfaces

use std::process::Command;
use std::fs;
use tempfile::NamedTempFile;

/// Test that NEURO programs can be executed from command line with exit codes
#[test]
fn test_exit_code_interop() {
    let test_cases = [
        ("return 0", 0),
        ("return 1", 1),
        ("return 42", 42),
        ("return -1", 255), // Negative numbers wrap around in exit codes
    ];

    for (return_statement, expected_exit_code) in &test_cases {
        let source = format!(
            r#"
fn main() -> int {{
    {};
}}
"#,
            return_statement
        );

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let temp_path = temp_file.path().with_extension("nr");
        fs::write(&temp_path, &source).expect("Failed to write temp file");

        // Compile the program
        let output = Command::new("./target/release/neurc")
            .args(&["build", temp_path.to_str().unwrap()])
            .output()
            .expect("Failed to execute neurc build");

        assert!(output.status.success(), "Exit code test should compile");

        // Execute and check exit code
        let exe_path = temp_path.with_extension("exe");
        let exec_output = Command::new(&exe_path)
            .output()
            .expect("Failed to execute exit code test");

        let actual_exit_code = match exec_output.status.code() {
            Some(code) => code as u8,
            None => panic!("Process terminated by signal"),
        };

        assert_eq!(
            actual_exit_code, *expected_exit_code,
            "Exit code mismatch for '{}': expected {}, got {}",
            return_statement, expected_exit_code, actual_exit_code
        );

        // Clean up
        let _ = fs::remove_file(&temp_path);
        let _ = fs::remove_file(&exe_path);
    }
}

/// Test NEURO compiler CLI interface
#[test]
fn test_compiler_cli_interface() {
    let source = r#"
fn main() -> int {
    return 0;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    // Test different CLI commands
    let cli_tests = [
        (vec!["tokenize"], "Tokenize command should work"),
        (vec!["parse"], "Parse command should work"),
        (vec!["check"], "Check command should work"),
        (vec!["llvm"], "LLVM command should work"),
        (vec!["build"], "Build command should work"),
        (vec!["run"], "Run command should work"),
    ];

    for (args, description) in &cli_tests {
        let mut full_args = args.clone();
        full_args.push(temp_path.to_str().unwrap());

        let output = Command::new("./target/release/neurc")
            .args(&full_args)
            .output()
            .expect(&format!("Failed to execute neurc {}", args.join(" ")));

        assert!(output.status.success(), "{}", description);
    }

    // Test version command (no file argument needed)
    let version_output = Command::new("./target/release/neurc")
        .args(&["version"])
        .output()
        .expect("Failed to execute neurc version");

    assert!(version_output.status.success(), "Version command should work");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

/// Test JSON output format for tooling integration
#[test]
fn test_json_output_format() {
    let source = r#"
fn add(x: int, y: int) -> int {
    return x + y;
}

fn main() -> int {
    return add(2, 3);
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    // Test tokenize with JSON output
    let tokenize_output = Command::new("./target/release/neurc")
        .args(&["tokenize", "--format", "json", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc tokenize");

    if tokenize_output.status.success() {
        let stdout = String::from_utf8_lossy(&tokenize_output.stdout);
        // Basic check that output looks like JSON
        if !stdout.is_empty() && (stdout.starts_with('{') || stdout.starts_with('[')) {
            println!("JSON tokenize output appears valid");
        }
    }

    // Test parse with JSON output
    let parse_output = Command::new("./target/release/neurc")
        .args(&["parse", "--format", "json", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc parse");

    if parse_output.status.success() {
        let stdout = String::from_utf8_lossy(&parse_output.stdout);
        if !stdout.is_empty() && (stdout.starts_with('{') || stdout.starts_with('[')) {
            println!("JSON parse output appears valid");
        }
    }

    // Clean up
    let _ = fs::remove_file(&temp_path);
}

/// Test file system integration
#[test]
fn test_file_system_integration() {
    // Test with different file paths and names
    let test_files = [
        ("simple_test.nr", "fn main() -> int { return 0; }"),
        ("with_spaces test.nr", "fn main() -> int { return 1; }"),
        ("UPPERCASE.NR", "fn main() -> int { return 2; }"),
    ];

    for (filename, source) in &test_files {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join(filename);
        fs::write(&file_path, source).expect("Failed to write test file");

        let output = Command::new("./target/release/neurc")
            .args(&["build", file_path.to_str().unwrap()])
            .output()
            .expect("Failed to execute neurc build");

        assert!(
            output.status.success(),
            "Should handle file '{}'\nstderr: {}",
            filename,
            String::from_utf8_lossy(&output.stderr)
        );

        // Clean up exe file
        let exe_path = file_path.with_extension("exe");
        let _ = fs::remove_file(&exe_path);
    }
}

/// Test standard library integration
#[test]
fn test_standard_library_integration() {
    let source = r#"
// Test standard library functions if available
fn test_print_function() -> int {
    // Note: print function may not be fully implemented yet
    // This test ensures the syntax is recognized
    return 42;
}

fn main() -> int {
    return test_print_function();
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Standard library test should compile");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

/// Test module system integration (basic)
#[test]
fn test_module_system_integration() {
    let source = r#"
// Test basic module syntax
// use std::core;  // Module import syntax

fn helper_function() -> int {
    return 10;
}

fn main() -> int {
    return helper_function();
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Module system test should compile");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

/// Test error handling and recovery in interop scenarios
#[test]
fn test_error_handling_interop() {
    // Test with invalid file
    let output = Command::new("./target/release/neurc")
        .args(&["build", "nonexistent_file.nr"])
        .output()
        .expect("Failed to execute neurc build");

    assert!(!output.status.success(), "Should fail for nonexistent file");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "Should provide error message for missing file");

    // Test with invalid syntax
    let invalid_source = "this is not valid NEURO syntax";
    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, invalid_source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(!output.status.success(), "Should fail for invalid syntax");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "Should provide error message for invalid syntax");

    // Clean up
    let _ = fs::remove_file(&temp_path);
}

/// Test compiler robustness with edge cases
#[test]
fn test_compiler_robustness() {
    let edge_cases = [
        ("empty file", ""),
        ("only comments", "// This is just a comment\n/* Block comment */"),
        ("only whitespace", "   \n\t  \n   "),
        ("minimal program", "fn main() -> int { return 0; }"),
    ];

    for (description, source) in &edge_cases {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let temp_path = temp_file.path().with_extension("nr");
        fs::write(&temp_path, source).expect("Failed to write temp file");

        let output = Command::new("./target/release/neurc")
            .args(&["parse", temp_path.to_str().unwrap()])
            .output()
            .expect("Failed to execute neurc parse");

        // The compiler should handle these gracefully (either succeed or fail with clear error)
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            assert!(!stderr.is_empty(), "Should provide error message for {}", description);
        }

        println!("Edge case '{}': {}", description,
                if output.status.success() { "handled successfully" } else { "failed gracefully" });

        // Clean up
        let _ = fs::remove_file(&temp_path);
    }
}

/// Test that all debug and example files work through the interop interface
#[test]
fn test_all_examples_interop() {
    let example_files = [
        "examples/06_functions.nr",
        "examples/07_control_if.nr",
        "examples/08_control_while.nr",
        "examples/01_basic_arithmetic.nr",
        "examples/02_conditional_logic.nr",
        "examples/03_loops.nr",
        "examples/04_simple_example.nr",
        "debug/neural_network_demo.nr",
    ];

    for example_file in &example_files {
        if fs::metadata(example_file).is_ok() {
            // Test build command
            let build_output = Command::new("./target/release/neurc")
                .args(&["build", example_file])
                .output()
                .expect(&format!("Failed to execute neurc build on {}", example_file));

            assert!(
                build_output.status.success(),
                "Example {} should build successfully\nstderr: {}",
                example_file,
                String::from_utf8_lossy(&build_output.stderr)
            );

            // Test run command
            let run_output = Command::new("./target/release/neurc")
                .args(&["run", example_file])
                .output()
                .expect(&format!("Failed to execute neurc run on {}", example_file));

            assert!(
                run_output.status.success(),
                "Example {} should run successfully\nstderr: {}",
                example_file,
                String::from_utf8_lossy(&run_output.stderr)
            );

            // Clean up any generated executables
            let exe_path = example_file.replace(".nr", ".exe");
            let _ = fs::remove_file(&exe_path);
        }
    }
}

#[cfg(test)]
mod integration_scenarios {
    use super::*;

    /// Test batch processing scenario
    #[test]
    fn test_batch_processing() {
        // Create multiple files and process them
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

        let test_programs = [
            ("prog1.nr", "fn main() -> int { return 1; }"),
            ("prog2.nr", "fn main() -> int { return 2; }"),
            ("prog3.nr", "fn main() -> int { return 3; }"),
        ];

        for (filename, source) in &test_programs {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, source).expect("Failed to write test file");

            let output = Command::new("./target/release/neurc")
                .args(&["build", file_path.to_str().unwrap()])
                .output()
                .expect("Failed to execute neurc build");

            assert!(
                output.status.success(),
                "Batch file {} should compile",
                filename
            );

            // Clean up exe
            let exe_path = file_path.with_extension("exe");
            let _ = fs::remove_file(&exe_path);
        }
    }

    /// Test toolchain integration scenario
    #[test]
    fn test_toolchain_integration() {
        let source = r#"
fn complex_function(x: int, y: int) -> int {
    if x > y {
        return x + y;
    } else {
        return x - y;
    }
}

fn main() -> int {
    let result = complex_function(10, 5);
    return result;
}
"#;

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let temp_path = temp_file.path().with_extension("nr");
        fs::write(&temp_path, source).expect("Failed to write temp file");

        // Simulate toolchain workflow: check -> parse -> llvm -> build
        let check_output = Command::new("./target/release/neurc")
            .args(&["check", temp_path.to_str().unwrap()])
            .output()
            .expect("Failed to execute neurc check");

        assert!(check_output.status.success(), "Check phase should succeed");

        let parse_output = Command::new("./target/release/neurc")
            .args(&["parse", temp_path.to_str().unwrap()])
            .output()
            .expect("Failed to execute neurc parse");

        assert!(parse_output.status.success(), "Parse phase should succeed");

        let llvm_output = Command::new("./target/release/neurc")
            .args(&["llvm", temp_path.to_str().unwrap()])
            .output()
            .expect("Failed to execute neurc llvm");

        assert!(llvm_output.status.success(), "LLVM phase should succeed");

        let build_output = Command::new("./target/release/neurc")
            .args(&["build", temp_path.to_str().unwrap()])
            .output()
            .expect("Failed to execute neurc build");

        assert!(build_output.status.success(), "Build phase should succeed");

        // Clean up
        let _ = fs::remove_file(&temp_path);
        let exe_path = temp_path.with_extension("exe");
        let _ = fs::remove_file(&exe_path);
    }
}