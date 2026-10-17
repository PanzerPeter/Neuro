// Copy trait + @derive(Copy, Clone) tests (Phase 1.7)
// Verifies Copy structs are exempt from move-by-default, non-Copy structs move,
// `@derive(Copy)` on a non-Copy field is rejected, and struct `.clone()` works
// end-to-end.
use crate::compile_harness::CompileTest;

use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Path to the `neurc` binary Cargo built for this test run.
///
/// Cargo sets `CARGO_BIN_EXE_neurc` for integration tests in the `neurc`
/// package; it is absolute and already carries the platform executable
/// suffix. Do not derive it from `current_exe()`. That assumes the legacy
/// `target/<profile>/deps/` layout and breaks under Cargo's build-dir layout.
fn neurc_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
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
        stderr.contains("cannot be `Copy`"),
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
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "calling .clone() on a struct without @derive(Clone) must fail"
    );
    assert!(
        stderr.contains("struct 'Point' has no method 'clone'"),
        "{stderr}"
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
    // usable: exercising the no-move path end-to-end without struct-typed
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

/// `impl Copy` by hand and `@derive(Copy)` make the same struct: a closure that reads one
/// copies it, so reassigning the original afterwards does not change what the closure sees.
#[test]
fn regression_hand_written_copy_is_captured_by_copy() {
    let source = r#"
struct P { v: i32 }
impl Clone for P { func clone(&self) -> P { P { v: self.v } } }
impl Copy for P {}
func main() -> i32 {
    mut p = P { v: 1 }
    val f = || p.v
    p = P { v: 10 }
    f() + p.v
}
"#;
    let exit = CompileTest::new()
        .compile_and_run("hand_copy_capture.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 11);
}

/// A generic struct's `Copy`, derived or written as `impl<T: Copy> Copy`, holds for an
/// instance only when its fields copy: `Cell<string>` moves, `Cell<i32>` copies, and a
/// bounded inherent impl's methods exist for a builtin argument.
#[test]
fn regression_generic_copy_is_decided_per_instance() {
    let written = r#"
struct Cell<T> { v: T }
impl<T: Clone> Clone for Cell<T> { func clone(&self) -> Cell<T> { Cell { v: self.v.clone() } } }
impl<T: Copy> Copy for Cell<T> {}
impl<T: Copy> Cell<T> {
    func twice(self) -> T {
        val a = self
        val b = self
        b.v
    }
}
func main() -> i32 {
    mut p = Cell { v: 1 }
    val f = || p.v
    p = Cell { v: 10 }
    val q = p
    f() + q.v + p.twice()
}
"#;
    let exit = CompileTest::new()
        .compile_and_run("generic_impl_copy.nr", written)
        .expect("compile/run failed");
    assert_eq!(exit, 21);

    for header in [
        "@derive(Copy, Clone)\nstruct Cell<T> { v: T }",
        "struct Cell<T> { v: T }\nimpl<T: Clone> Clone for Cell<T> { func clone(&self) -> Cell<T> { Cell { v: self.v.clone() } } }\nimpl<T: Copy> Copy for Cell<T> {}",
    ] {
        let source = format!(
            "{header}\nfunc main() -> i32 {{\n    val a = Cell {{ v: \"a\" + \"b\" }}\n    val b = a\n    a.v.len() as i32\n}}\n"
        );
        let (success, stderr) = check_source(&source);
        assert!(!success, "`Cell<string>` must move:\n{source}");
        assert!(stderr.contains("use of moved value 'a'"), "{stderr}");
    }
}

#[test]
fn generic_impl_copy_takes_copy_bounds_only() {
    let source = r#"
trait Show { func show(&self) -> i32 }
struct Cell<T> { v: T }
impl<T: Clone> Clone for Cell<T> { func clone(&self) -> Cell<T> { Cell { v: self.v.clone() } } }
impl<T: Show> Copy for Cell<T> {}
func main() -> i32 { 0 }
"#;
    let (success, stderr) = check_source(source);
    assert!(!success);
    assert!(
        stderr.contains("may bound its parameters by `Copy` only"),
        "{stderr}"
    );
}
