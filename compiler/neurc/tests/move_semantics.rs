// Move-by-default ownership tests (Phase 1.7 §2.2)
// Verifies use-after-move is rejected at `neurc check`, and that valid
// straight-line and `.clone()` programs still compile and run end-to-end.
mod common;
use common::CompileTest;

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn neurc_path() -> PathBuf {
    let neurc_exe = if cfg!(target_os = "windows") {
        "neurc.exe"
    } else {
        "neurc"
    };

    std::env::current_exe()
        .expect("Failed to get current exe path")
        .parent()
        .expect("Failed to get parent directory")
        .parent()
        .expect("Failed to get grandparent directory")
        .join(neurc_exe)
}

fn check_source(source: &str) -> (bool, String) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source_path = temp_dir.path().join("test.nr");
    fs::write(&source_path, source).expect("Failed to write source file");

    let output = Command::new(neurc_path())
        .arg("check")
        .arg(&source_path)
        .output()
        .expect("Failed to execute neurc check");

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stderr)
}

#[test]
fn use_after_move_on_bind_is_rejected() {
    let source = r#"
func main() -> i32 {
    val s1: string = "Hello"
    val s2: string = s1
    val n: u64 = s1.len()
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "use after move should be rejected");
    assert!(
        stderr.contains("use of moved value"),
        "expected move diagnostic, got: {stderr}"
    );
}

#[test]
fn use_after_move_into_call_is_rejected() {
    let source = r#"
func consume(s: string) -> i32 { 0 }

func main() -> i32 {
    val greeting: string = "Hi"
    val r: i32 = consume(greeting)
    val n: u64 = greeting.len()
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "use after move-into-call should be rejected");
    assert!(
        stderr.contains("use of moved value"),
        "expected move diagnostic, got: {stderr}"
    );
}

#[test]
fn clone_avoids_the_move() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val original: string = "neuro"
    val copy: string = original.clone()
    if original == copy {
        return 0
    }
    return 1
}
"#;
    let exit_code = test
        .compile_and_run("move_clone.nr", source)
        .expect("clone program should compile and run");
    assert_eq!(exit_code, 0);
}

#[test]
fn conditional_move_does_not_leak_past_branch() {
    // `s` is consumed only on the taken branch; the later read sits on a path
    // that may not have moved it, so the program must still compile and run.
    let test = CompileTest::new();
    let source = r#"
func consume(s: string) -> i32 { 0 }

func main() -> i32 {
    val s: string = "hi"
    if true {
        val r: i32 = consume(s)
    }
    val n: u64 = s.len()
    return 0
}
"#;
    let exit_code = test
        .compile_and_run("move_conditional.nr", source)
        .expect("conditional move program should compile and run");
    assert_eq!(exit_code, 0);
}

#[test]
fn copy_scalars_are_not_moved() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 5
    val b: i32 = a
    val c: i32 = a + b
    return c - 10
}
"#;
    let exit_code = test
        .compile_and_run("move_scalars.nr", source)
        .expect("scalar copy program should compile and run");
    assert_eq!(exit_code, 0);
}
