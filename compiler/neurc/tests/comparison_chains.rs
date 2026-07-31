use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Path to the `neurc` binary Cargo built for this test run.
///
/// Cargo sets `CARGO_BIN_EXE_neurc` for integration tests in the `neurc`
/// package; it is absolute and already carries the platform executable
/// suffix. Do not derive it from `current_exe()` — that assumes the legacy
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
fn chained_less_than_rejected() {
    let source = r#"
func main() -> i32 {
    val a: i32 = 1
    val b: i32 = 2
    val c: i32 = 3
    val x: bool = a < b < c
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "Chained comparison should be rejected");
    assert!(
        stderr.contains("cannot be chained"),
        "Expected 'cannot be chained' in error, got: {stderr}"
    );
}

#[test]
fn chained_greater_equal_rejected() {
    let source = r#"
func main() -> i32 {
    val a: i32 = 3
    val b: i32 = 2
    val c: i32 = 1
    val x: bool = a >= b > c
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "Chained comparison should be rejected");
    assert!(
        stderr.contains("cannot be chained"),
        "Expected 'cannot be chained' in error, got: {stderr}"
    );
}

#[test]
fn chained_equality_rejected() {
    let source = r#"
func main() -> i32 {
    val a: i32 = 1
    val b: i32 = 1
    val c: i32 = 1
    val x: bool = a == b == c
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "Chained equality should be rejected");
    assert!(
        stderr.contains("cannot be chained"),
        "Expected 'cannot be chained' in error, got: {stderr}"
    );
}

#[test]
fn single_comparison_accepted() {
    let source = r#"
func main() -> i32 {
    val a: i32 = 1
    val b: i32 = 2
    val x: bool = a < b
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(success, "Single comparison should pass, got: {stderr}");
}

#[test]
fn logical_and_comparisons_accepted() {
    let source = r#"
func main() -> i32 {
    val a: i32 = 1
    val b: i32 = 2
    val c: i32 = 3
    val x: bool = a < b && b < c
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        success,
        "Comparisons joined with && should pass, got: {stderr}"
    );
}

#[test]
fn chained_not_equal_rejected() {
    let source = r#"
func main() -> i32 {
    val a: i32 = 1
    val b: i32 = 2
    val c: i32 = 3
    val x: bool = a != b != c
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "Chained != should be rejected");
    assert!(
        stderr.contains("cannot be chained"),
        "Expected 'cannot be chained' in error, got: {stderr}"
    );
}
