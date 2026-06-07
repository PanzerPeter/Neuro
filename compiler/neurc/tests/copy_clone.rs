// Copy trait + @derive(Copy, Clone) tests (Phase 1.7 §2.3)
// Verifies Copy structs are exempt from move-by-default, non-Copy structs move,
// `@derive(Copy)` on a non-Copy field is rejected, and struct `.clone()` works
// end-to-end.
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
fn non_copy_struct_use_after_move_is_rejected() {
    let source = r#"
struct Point { x: i32, y: i32 }

func main() -> i32 {
    val a = Point { x: 1, y: 2 }
    val b = a
    val r = a.x
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "moving a non-Copy struct then reading it must fail"
    );
    assert!(
        stderr.contains("use of moved value"),
        "expected move diagnostic, got: {stderr}"
    );
}

#[test]
fn derive_copy_on_string_field_is_rejected() {
    let source = r#"
@derive(Copy)
struct Holder { name: string }

func main() -> i32 { 0 }
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "deriving Copy with a non-Copy field must fail");
    assert!(
        stderr.contains("cannot derive Copy"),
        "expected Copy-derive diagnostic, got: {stderr}"
    );
}

#[test]
fn clone_on_non_clone_struct_is_rejected() {
    let source = r#"
struct Point { x: i32, y: i32 }

func main() -> i32 {
    val a = Point { x: 1, y: 2 }
    val b = a.clone()
    return 0
}
"#;
    let (success, _stderr) = check_source(source);
    assert!(
        !success,
        "calling .clone() on a struct without @derive(Clone) must fail"
    );
}

#[test]
fn copy_struct_reuse_compiles_and_runs() {
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct Point { x: i32, y: i32 }

func main() -> i32 {
    val a = Point { x: 3, y: 4 }
    val b = a
    return a.x + b.y
}
"#;
    let exit_code = test
        .compile_and_run("copy_struct.nr", source)
        .expect("Copy struct program should compile and run");
    assert_eq!(exit_code, 7);
}

#[test]
fn struct_clone_compiles_and_runs() {
    let test = CompileTest::new();
    let source = r#"
@derive(Clone)
struct Point { x: i32, y: i32 }

func main() -> i32 {
    val a = Point { x: 5, y: 6 }
    val c = a.clone()
    return a.x + c.y
}
"#;
    let exit_code = test
        .compile_and_run("struct_clone.nr", source)
        .expect("struct clone program should compile and run");
    assert_eq!(exit_code, 11);
}

#[test]
fn copy_struct_rebound_multiple_times_runs() {
    // A Copy struct can be duplicated into several bindings, each independently
    // usable — exercising the no-move path end-to-end without struct-typed
    // function parameters (a separate Phase 2 codegen item).
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct Point { x: i32, y: i32 }

func main() -> i32 {
    val a = Point { x: 4, y: 5 }
    val b = a
    val c = a
    return a.x + b.y + c.x
}
"#;
    let exit_code = test
        .compile_and_run("copy_rebind.nr", source)
        .expect("Copy struct multi-rebind should compile and run");
    assert_eq!(exit_code, 13);
}
