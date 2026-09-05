// Tensor ownership and move semantics: a tensor is not `Copy`, so it
// moves on assignment and on being passed; `.clone()` is the explicit deep copy and
// `.to(device)` the consuming device transfer. End to end through `neurc`.
mod common;

use common::CompileTest;

use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn neurc_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
}

/// Type-check `source`, returning whether it passed and everything the compiler printed.
fn check_source(source: &str) -> (bool, String) {
    let temp_dir = TempDir::new().expect("temp directory");
    let source_path = temp_dir.path().join("tensor_ownership.nr");
    fs::write(&source_path, source).expect("write source");

    let output = Command::new(neurc_path())
        .arg("check")
        .arg(&source_path)
        .output()
        .expect("run neurc check");

    let mut printed = String::from_utf8_lossy(&output.stdout).into_owned();
    printed.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.success(), printed)
}

fn run_program(name: &str, source: &str) -> i32 {
    CompileTest::new()
        .compile_and_run(name, source)
        .unwrap_or_else(|e| panic!("{name} should compile and run: {e}"))
}

/// `.clone()` is the opt-out of move-by-default: both the original and the copy stay
/// usable, and each can be consumed independently.
#[test]
fn a_cloned_tensor_is_independent_of_its_source() {
    let source = r#"
func take(t: Tensor<f32, [2, 2]>) -> i32 {
    return 3
}

func main() -> i32 {
    val a = Tensor::<f32, [2, 2]>::identity()
    val b = a.clone()
    return take(a) + take(b)
}
"#;
    assert_eq!(run_program("tensor_clone.nr", source), 6);
}

/// Cloning through a borrow is how a tensor someone else owns gets copied: the borrow is
/// not consumed, and the result is an owned tensor the caller may move on.
#[test]
fn a_tensor_clones_through_a_borrow() {
    let source = r#"
func take(t: Tensor<f32, [2, 2]>) -> i32 {
    return 4
}

func copy_of(t: &Tensor<f32, [2, 2]>) -> i32 {
    return take(t.clone())
}

func main() -> i32 {
    val a = Tensor::<f32, [2, 2]>::ones()
    return copy_of(&a) + copy_of(&a)
}
"#;
    assert_eq!(run_program("tensor_borrow_clone.nr", source), 8);
}

/// A host transfer is the move itself: the tensor arrives on the other side of `.to`
/// unchanged, and the program runs.
#[test]
fn a_host_device_transfer_compiles_and_runs() {
    let source = r#"
func take(t: Tensor<f32, [4]>) -> i32 {
    return 9
}

func main() -> i32 {
    val a: Tensor<f32, [4]> = [1.0, 2.0, 3.0, 4.0]
    val here = a.to(Device::CPU)
    return take(here)
}
"#;
    assert_eq!(run_program("tensor_to_cpu.nr", source), 9);
}

#[test]
fn a_device_transfer_consumes_the_tensor() {
    let source = r#"
func take(t: Tensor<f32, [2, 2]>) -> i32 {
    return 1
}

func main() -> i32 {
    val a = Tensor::<f32, [2, 2]>::identity()
    val moved = a.to(Device::CPU)
    return take(a)
}
"#;
    let (success, printed) = check_source(source);
    assert!(!success, "`.to` consumes its receiver");
    assert!(
        printed.contains("use of moved value"),
        "expected a move diagnostic, got: {printed}"
    );
}

/// The zero-cost sharing path: a borrowed tensor is not moved, so the
/// owner may keep using it after any number of borrows.
#[test]
fn a_borrowed_tensor_is_not_moved() {
    let source = r#"
func rows(t: &Tensor<f32, [2, 3]>) -> i32 {
    return 2
}

func main() -> i32 {
    val a: Tensor<f32, [2, 3]> = [
        [1.0, 2.0, 3.0],
        [4.0, 5.0, 6.0]
    ]
    return rows(&a) + rows(&a) + rows(&a)
}
"#;
    assert_eq!(run_program("tensor_borrow.nr", source), 6);
}

/// A borrow cannot be consumed, so `.to` is not offered on one.
#[test]
fn a_device_transfer_is_rejected_on_a_borrowed_tensor() {
    let source = r#"
func send(t: &Tensor<f32, [2, 2]>) -> i32 {
    val moved = t.to(Device::CPU)
    return 0
}

func main() -> i32 {
    return 0
}
"#;
    let (success, printed) = check_source(source);
    assert!(!success, "a borrow cannot be transferred");
    assert!(
        printed.contains("has no method 'to'"),
        "expected a method-not-found diagnostic, got: {printed}"
    );
}

/// There is no device but the host yet, and the device is an ordinary runtime value, so
/// the mismatch is caught where the value is known: the program aborts with a diagnostic
/// rather than pretending the buffer moved.
#[test]
fn a_transfer_to_an_absent_device_aborts_with_a_diagnostic() {
    let source = r#"
func main() -> i32 {
    val a = Tensor::<f32, [2, 2]>::identity()
    val there = a.to(Device::GPU(0))
    return 0
}
"#;
    let test = CompileTest::new();
    let path = test.write_source("tensor_to_gpu.nr", source);
    let binary = test.compile(&path).expect("a device transfer compiles");
    let output = Command::new(&binary).output().expect("run the program");
    assert!(!output.status.success(), "a missing device is not a no-op");
    let printed = String::from_utf8_lossy(&output.stderr);
    assert!(
        printed.contains("non-host device"),
        "expected the device diagnostic, got: {printed}"
    );
}
