// Move-by-default ownership tests (Phase 1.7)
// Verifies use-after-move is rejected at `neurc check`, and that valid
// straight-line and `.clone()` programs still compile and run end-to-end.
mod common;
use common::CompileTest;

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

// A move out of a binding declared outside a loop repeats on the next iteration.
// Before these landed, the checker restored the body's move state wholesale, so the
// program compiled and freed the same buffer once per iteration.

#[test]
fn regression_move_in_while_body_is_rejected_not_double_freed() {
    let source = r#"
func eat(t: Tensor<i32, [3]>) -> i32 { return t[0] }

func main() -> i32 {
    val b: Tensor<i32, [3]> = [1, 2, 3]
    mut s = 0
    mut i = 0
    while i < 2 {
        s = s + eat(b)
        i = i + 1
    }
    return s
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "a move repeated by a `while` body should be rejected"
    );
    assert!(
        stderr.contains("is moved out inside a loop body"),
        "expected the loop-move diagnostic, got: {stderr}"
    );
}

#[test]
fn regression_move_in_for_body_is_rejected() {
    let source = r#"
func eat(t: Tensor<i32, [3]>) -> i32 { return t[0] }

func main() -> i32 {
    val b: Tensor<i32, [3]> = [1, 2, 3]
    mut s = 0
    for i in 0..2 {
        s = s + eat(b)
    }
    return s
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "a move repeated by a `for` body should be rejected"
    );
    assert!(
        stderr.contains("is moved out inside a loop body"),
        "expected the loop-move diagnostic, got: {stderr}"
    );
}

#[test]
fn regression_move_of_drop_value_in_loop_body_is_rejected() {
    // The `Drop` case is the silent one: no allocator is involved, so the old
    // behaviour ran the destructor once per iteration without aborting.
    let source = r#"
struct Res { id: i32 }

impl Drop for Res {
    func drop(&mut self) {
        println("drop {self.id}")
    }
}

func eat(r: Res) -> i32 { return r.id }

func main() -> i32 {
    val r = Res { id: 7 }
    mut s = 0
    mut i = 0
    while i < 2 {
        s = s + eat(r)
        i = i + 1
    }
    return s
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "a repeated move of a Drop value should be rejected"
    );
    assert!(
        stderr.contains("is moved out inside a loop body"),
        "expected the loop-move diagnostic, got: {stderr}"
    );
}

#[test]
fn move_in_loop_body_is_allowed_when_the_binding_is_given_a_fresh_value() {
    let test = CompileTest::new();
    let source = r#"
func eat(t: Tensor<i32, [3]>) -> i32 { return t[0] }

func main() -> i32 {
    mut b: Tensor<i32, [3]> = [1, 2, 3]
    mut s = 0
    mut i = 0
    while i < 3 {
        s = s + eat(b)
        b = [1, 2, 3]
        i = i + 1
    }
    return s - 3
}
"#;
    let exit_code = test
        .compile_and_run("move_loop_reassigned.nr", source)
        .expect("a body that re-establishes the binding should compile and run");
    assert_eq!(exit_code, 0);
}

#[test]
fn move_in_loop_body_is_allowed_when_the_body_always_breaks() {
    let test = CompileTest::new();
    let source = r#"
func eat(t: Tensor<i32, [3]>) -> i32 { return t[0] }

func main() -> i32 {
    val b: Tensor<i32, [3]> = [7, 2, 3]
    mut s = 0
    mut i = 0
    while i < 3 {
        s = s + eat(b)
        break
    }
    return s - 7
}
"#;
    let exit_code = test
        .compile_and_run("move_loop_break.nr", source)
        .expect("a body that always leaves the loop moves once and should compile");
    assert_eq!(exit_code, 0);
}

#[test]
fn move_of_a_binding_declared_inside_the_loop_body_is_allowed() {
    let test = CompileTest::new();
    let source = r#"
func eat(t: Tensor<i32, [3]>) -> i32 { return t[0] }

func main() -> i32 {
    mut s = 0
    mut i = 0
    while i < 3 {
        val local: Tensor<i32, [3]> = [2, 0, 0]
        s = s + eat(local)
        i = i + 1
    }
    return s - 6
}
"#;
    let exit_code = test
        .compile_and_run("move_loop_local.nr", source)
        .expect("a binding fresh each iteration should compile and run");
    assert_eq!(exit_code, 0);
}
