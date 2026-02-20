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

fn write_source(temp_dir: &TempDir, filename: &str, source: &str) -> PathBuf {
    let path = temp_dir.path().join(filename);
    fs::write(&path, source).expect("Failed to write source file");
    path
}

#[test]
fn check_command_success_writes_stdout() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source = r#"
func main() -> i32 {
    return 0
}
"#;

    let source_path = write_source(&temp_dir, "check_success.nr", source);

    let output = Command::new(neurc_path())
        .arg("check")
        .arg(&source_path)
        .output()
        .expect("Failed to execute neurc check");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "Expected success, stderr: {stderr}");
    assert!(
        stdout.contains("Type checking passed"),
        "Expected type-check success message in stdout, got: {stdout}"
    );
    assert!(
        stderr.trim().is_empty(),
        "Expected empty stderr on success, got: {stderr}"
    );
}

#[test]
fn check_command_error_is_nonzero_and_stderr() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source = r#"
func main() -> i32 {
    val x: i32 = true
    return x
}
"#;

    let source_path = write_source(&temp_dir, "check_failure.nr", source);

    let output = Command::new(neurc_path())
        .arg("check")
        .arg(&source_path)
        .output()
        .expect("Failed to execute neurc check");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "Expected non-zero exit on type errors"
    );
    assert!(
        stderr.contains("Type errors found"),
        "Expected type error header in stderr, got: {stderr}"
    );
    assert!(
        stderr.contains("type error(s) found"),
        "Expected summary error in stderr, got: {stderr}"
    );
    assert!(
        stdout.trim().is_empty(),
        "Expected empty stdout on check failure, got: {stdout}"
    );
}

#[test]
fn compile_command_error_is_nonzero_and_stderr() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source = r#"
func main() -> i32 {
    val x: i32 = true
    return x
}
"#;

    let source_path = write_source(&temp_dir, "compile_failure.nr", source);
    let output_path = source_path.with_extension(if cfg!(target_os = "windows") {
        "exe"
    } else {
        ""
    });

    let output = Command::new(neurc_path())
        .arg("compile")
        .arg(&source_path)
        .arg("-o")
        .arg(&output_path)
        .output()
        .expect("Failed to execute neurc compile");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "Expected non-zero exit on compile type errors"
    );
    assert!(
        stderr.contains("Compilation failed") || stderr.contains("Type errors found"),
        "Expected compilation/type failure message in stderr, got: {stderr}"
    );
    assert!(
        stdout.trim().is_empty(),
        "Expected empty stdout on compile failure, got: {stdout}"
    );
}