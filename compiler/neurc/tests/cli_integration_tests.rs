//! Integration tests for the neurc CLI

use std::process::Command;
use std::fs;
use tempfile::{tempdir, NamedTempFile};

fn get_neurc_path() -> std::path::PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_dir = std::path::Path::new(manifest_dir).parent().unwrap().parent().unwrap();
    
    if cfg!(debug_assertions) {
        workspace_dir.join("target").join("debug").join("neurc.exe")
    } else {
        workspace_dir.join("target").join("release").join("neurc.exe")
    }
}

#[test]
fn test_version_command() {
    let output = Command::new(&get_neurc_path())
        .arg("version")
        .output()
        .expect("Failed to execute neurc");
    
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("NEURO Programming Language"));
    assert!(stdout.contains("Version: 0.1.0"));
}

#[test]
fn test_help_command() {
    let output = Command::new(&get_neurc_path())
        .arg("--help")
        .output()
        .expect("Failed to execute neurc");
    
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("compiler for the NEURO programming language"));
    assert!(stdout.contains("Commands:"));
}

#[test]
fn test_eval_simple_expression() {
    let output = Command::new(&get_neurc_path())
        .arg("eval")
        .arg("2 + 3 * 4")
        .output()
        .expect("Failed to execute neurc");
    
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("14"));
}

#[test]
fn test_eval_boolean_expression() {
    let output = Command::new(&get_neurc_path())
        .arg("eval")
        .arg("42 == 42")
        .output()
        .expect("Failed to execute neurc");
    
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("true"));
}

#[test]
fn test_eval_string_concatenation() {
    let output = Command::new(&get_neurc_path())
        .arg("eval")
        .arg(r#""Hello" + " World""#)
        .output()
        .expect("Failed to execute neurc");
    
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Hello World"));
}

#[test]
fn test_tokenize_command() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(&temp_file, "let x = 42;").unwrap();
    
    let output = Command::new(&get_neurc_path())
        .arg("tokenize")
        .arg(temp_file.path())
        .output()
        .expect("Failed to execute neurc");
    
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Keyword(Let)"));
    assert!(stdout.contains("Identifier(\"x\")"));
    assert!(stdout.contains("Assign"));
    assert!(stdout.contains("Integer(\"42\")"));
}

#[test]
fn test_parse_command() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(&temp_file, "fn main() { let x = 42; }").unwrap();
    
    let output = Command::new(&get_neurc_path())
        .arg("parse")
        .arg(temp_file.path())
        .output()
        .expect("Failed to execute neurc");
    
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Function 'main'"));
}

#[test]
fn test_check_valid_file() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(&temp_file, "fn main() -> int { return 42; }").unwrap();
    
    let output = Command::new(&get_neurc_path())
        .arg("check")
        .arg(temp_file.path())
        .output()
        .expect("Failed to execute neurc");
    
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("OK"));
    assert!(stdout.contains("SUCCESS"));
}

#[test]
fn test_check_invalid_file() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(&temp_file, "fn invalid syntax {").unwrap();
    
    let output = Command::new(&get_neurc_path())
        .arg("check")
        .arg(temp_file.path())
        .output()
        .expect("Failed to execute neurc");
    
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("ERROR"));
}

#[test]
fn test_analyze_command() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(&temp_file, "fn add(x: int, y: int) -> int { return x + y; }").unwrap();
    
    let output = Command::new(&get_neurc_path())
        .arg("analyze")
        .arg(temp_file.path())
        .output()
        .expect("Failed to execute neurc");
    
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Semantic Analysis Results"));
    assert!(stdout.contains("Symbols"));
}

#[test]
fn test_compile_command() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(&temp_file, "fn main() { let x = 42; }").unwrap();
    
    let output = Command::new(&get_neurc_path())
        .arg("compile")
        .arg(temp_file.path())
        .output()
        .expect("Failed to execute neurc");
    
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("SUCCESS"));
}

#[test]
fn test_nonexistent_file() {
    let output = Command::new(&get_neurc_path())
        .arg("check")
        .arg("nonexistent_file.nr")
        .output()
        .expect("Failed to execute neurc");
    
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Failed to read"));
}

#[test]
fn test_verbose_flag() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(&temp_file, "fn main() -> int { return 42; }").unwrap();
    
    let output = Command::new(&get_neurc_path())
        .arg("--verbose")
        .arg("check")
        .arg(temp_file.path())
        .output()
        .expect("Failed to execute neurc");
    
    assert!(output.status.success());
    // Verbose mode should produce more output
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let total_output = stdout + &stderr;
    
    // Should contain more detailed information in verbose mode
    assert!(total_output.len() > 50); // Basic heuristic for more output
}

#[test]
fn test_eval_invalid_expression() {
    let output = Command::new(&get_neurc_path())
        .arg("eval")
        .arg("2 +")
        .output()
        .expect("Failed to execute neurc");
    
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("ERROR") || stderr.contains("error"));
}

#[test]
fn test_multiple_files_check() {
    let temp_dir = tempdir().unwrap();
    
    let file1 = temp_dir.path().join("file1.nr");
    let file2 = temp_dir.path().join("file2.nr");
    
    fs::write(&file1, "fn test1() -> int { return 1; }").unwrap();
    fs::write(&file2, "fn test2() -> int { return 2; }").unwrap();
    
    let output = Command::new(&get_neurc_path())
        .arg("check")
        .arg(&file1)
        .arg(&file2)
        .output()
        .expect("Failed to execute neurc");
    
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("2 files passed"));
}

#[test]
fn test_empty_file() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(&temp_file, "").unwrap();
    
    let output = Command::new(&get_neurc_path())
        .arg("check")
        .arg(temp_file.path())
        .output()
        .expect("Failed to execute neurc");
    
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("OK"));
}

#[test] 
fn test_optimization_levels() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(&temp_file, "fn main() { let x = 42; }").unwrap();
    
    for opt_level in 0..=3 {
        let output = Command::new(&get_neurc_path())
            .arg("compile")
            .arg("--opt-level")
            .arg(opt_level.to_string())
            .arg(temp_file.path())
            .output()
            .expect("Failed to execute neurc");
        
        assert!(output.status.success(), "Optimization level {} failed", opt_level);
    }
}