use std::fs;
use std::path::PathBuf;
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

    assert!(
        output.status.success(),
        "Expected success, stderr: {stderr}"
    );
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

/// Regression: a program with no `main` is a compiler error, not a linker error.
///
/// The pipeline used to run to completion and hand a `main`-less object file to the
/// system linker, so the user saw `undefined reference to 'main'` naming the C runtime's
/// `Scrt1.o` rather than their own program.
#[test]
fn compile_reports_a_missing_main_itself() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source_path = temp_dir.path().join("no_main.nr");
    fs::write(&source_path, "func helper() -> i32 { 1 }\n").expect("Failed to write source");

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

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "Expected non-zero exit when `main` is missing"
    );
    assert!(
        stderr.contains("no `main` function found"),
        "Expected a missing-entry-point diagnostic, got: {stderr}"
    );
    assert!(
        !stderr.contains("undefined reference"),
        "The linker was reached despite the missing `main`: {stderr}"
    );
}

/// `run` is only useful if the shell sees the program's own status, not the driver's.
#[test]
fn run_forwards_program_stdout_and_exit_code() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source = r#"
func main() -> i32 {
    println("ran")
    return 3
}
"#;
    let source_path = write_source(&temp_dir, "run_exit.nr", source);

    let output = Command::new(neurc_path())
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("Failed to execute neurc run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(3),
        "Expected the program's own exit code, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("ran"),
        "Expected the program's stdout, got: {stdout}"
    );
    assert!(
        !stdout.contains("Successfully compiled"),
        "`run` must not print the compile banner into the program's output: {stdout}"
    );
}

/// The executable goes to a temporary directory, so `run` leaves the source tree clean.
/// Without this, running an example twice would litter `examples/` with binaries.
#[test]
fn run_writes_no_executable_beside_the_source() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source_path = write_source(&temp_dir, "run_clean.nr", "func main() -> i32 { 0 }\n");

    let output = Command::new(neurc_path())
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("Failed to execute neurc run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let leftovers: Vec<_> = fs::read_dir(temp_dir.path())
        .expect("Failed to read temp directory")
        .filter_map(|entry| entry.ok().map(|e| e.file_name()))
        .filter(|name| name != "run_clean.nr")
        .collect();
    assert!(
        leftovers.is_empty(),
        "`run` left artifacts beside the source: {leftovers:?}"
    );
}

/// A non-`.nr` input is rejected by `run` for the same reason `compile` rejects it.
#[test]
fn run_rejects_a_non_nr_extension() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source_path = write_source(&temp_dir, "wrong.txt", "func main() -> i32 { 0 }\n");

    let output = Command::new(neurc_path())
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("Failed to execute neurc run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "Expected a non-zero exit");
    assert!(
        stderr.contains(".nr extension"),
        "Expected an extension diagnostic, got: {stderr}"
    );
}

/// A diagnostic names a source location a reader can act on: the file, the line, the
/// column, the offending line of source, and a caret under the span. Byte offsets are
/// an internal representation and must not reach the user.
#[test]
fn check_command_error_renders_source_location() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source = "func main() -> i32 {\n    val x: i32 = \"hello\"\n    return x\n}\n";

    let source_path = write_source(&temp_dir, "located.nr", source);

    let output = Command::new(neurc_path())
        .arg("check")
        .arg(&source_path)
        .output()
        .expect("Failed to execute neurc check");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("Span {"),
        "A raw Span must never reach the user, got: {stderr}"
    );
    assert!(
        stderr.contains("located.nr:2:5"),
        "Expected a file:line:column location, got: {stderr}"
    );
    assert!(
        stderr.contains("val x: i32 = \"hello\""),
        "Expected the offending source line, got: {stderr}"
    );
    assert!(
        stderr.contains('^'),
        "Expected a caret under the span, got: {stderr}"
    );
}

/// `--emit obj` stops before the linker and writes the object file itself. The magic bytes
/// are asserted rather than the file's mere existence: an object file that is really a
/// linked executable would satisfy a size check and fail at the first `-shared` link.
#[test]
fn emit_obj_writes_a_relocatable_object() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source_path = write_source(
        &temp_dir,
        "library.nr",
        "func main() -> i32 {\n    return 0\n}\n",
    );
    let object_path = temp_dir.path().join("library.o");

    let output = Command::new(neurc_path())
        .arg("compile")
        .arg("--emit")
        .arg("obj")
        .arg("-o")
        .arg(&object_path)
        .arg(&source_path)
        .output()
        .expect("Failed to execute neurc compile");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Expected --emit obj to succeed, stderr: {stderr}"
    );

    let bytes = fs::read(&object_path).expect("--emit obj must write the output path");
    assert!(
        bytes.len() > 4,
        "Expected an object file with content, got {} bytes",
        bytes.len()
    );
    // ELF, Mach-O (64-bit, either endianness) and COFF, the three this backend targets.
    let magic = &bytes[..4];
    assert!(
        magic == b"\x7fELF"
            || magic == b"\xcf\xfa\xed\xfe"
            || magic == b"\xfe\xed\xfa\xcf"
            || bytes[..2] == [0x64, 0x86]
            || bytes[..2] == [0x4c, 0x01],
        "Expected an object file's magic bytes, got {magic:02x?}"
    );
}

/// An object file may be a library, and a library has no entry point, so `--emit obj`
/// carries none of the `main` requirement an executable does. Without this the harness
/// that links a shared library of tensor-returning functions could not compile one.
#[test]
fn emit_obj_does_not_require_a_main_function() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source_path = write_source(
        &temp_dir,
        "no_main.nr",
        "func twice(value: i32) -> i32 {\n    return value * 2\n}\n",
    );
    let object_path = temp_dir.path().join("no_main.o");

    let output = Command::new(neurc_path())
        .arg("compile")
        .arg("--emit")
        .arg("obj")
        .arg("-o")
        .arg(&object_path)
        .arg(&source_path)
        .output()
        .expect("Failed to execute neurc compile");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Expected a `main`-less module to emit an object, stderr: {stderr}"
    );
    assert!(
        object_path.is_file(),
        "Expected the object file at {}",
        object_path.display()
    );
}
