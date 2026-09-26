// End-to-end tests for the panic runtime.
//
// `panic(msg)` / `assert(cond)` / `unreachable()` print a diagnostic with source location
// to stderr and abort the process via `abort()` (SIGABRT): no stack unwinding. These tests
// compile each program and assert both the runtime termination behavior and the emitted
// diagnostic text.
use std::path::PathBuf;
use std::process::{Command, Output};

/// Path to the `neurc` binary Cargo built for this test run.
///
/// Cargo sets `CARGO_BIN_EXE_neurc` for integration tests in the `neurc`
/// package; it is absolute and already carries the platform executable
/// suffix. Do not derive it from `current_exe()`. That assumes the legacy
/// `target/<profile>/deps/` layout and breaks under Cargo's build-dir layout.
fn neurc_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
}

/// Compile `source` at `-O0`, returning the executable path.
fn compile_source(source: &str, tag: &str) -> PathBuf {
    let dir = std::env::temp_dir();
    let src = dir.join(format!("neuro_panic_{tag}.nr"));
    let exe = dir.join(format!("neuro_panic_{tag}"));
    std::fs::write(&src, source).expect("write source");

    let output = Command::new(neurc_path())
        .arg("compile")
        .arg(&src)
        .arg("-O")
        .arg("0")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("run neurc");

    assert!(
        output.status.success(),
        "compile failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    exe
}

fn run(exe: &PathBuf) -> Output {
    Command::new(exe).output().expect("run executable")
}

/// True when the process was aborted rather than exiting normally. On Unix `abort()` is
/// delivered as SIGABRT, so there is no exit code (`code()` is `None`). On Windows the
/// abort surfaces as a non-zero/negative exit code.
fn aborted(output: &Output) -> bool {
    match output.status.code() {
        None => true,
        Some(code) => code != 0,
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

const PANIC_PROG: &str = r#"
func main() -> i32 {
    panic("boom")
    return 0
}
"#;

const ASSERT_FALSE_PROG: &str = r#"
func main() -> i32 {
    assert(false)
    return 0
}
"#;

const ASSERT_TRUE_PROG: &str = r#"
func main() -> i32 {
    assert(true)
    return 0
}
"#;

const UNREACHABLE_PROG: &str = r#"
func main() -> i32 {
    unreachable()
    return 0
}
"#;

/// A `panic` in tail (implicit-return) position of a non-void function must still
/// compile and verify (the block ends in `unreachable`, not a `ret`).
const PANIC_TAIL_PROG: &str = r#"
func bail() -> i32 {
    panic("bail")
}

func main() -> i32 {
    val x = bail()
    return x
}
"#;

#[test]
fn panic_aborts_and_prints_message() {
    let exe = compile_source(PANIC_PROG, "msg");
    let output = run(&exe);
    assert!(
        aborted(&output),
        "expected panic to abort, exited with {:?}",
        output.status.code()
    );
    let err = stderr(&output);
    assert!(err.contains("panic: boom"), "stderr was: {err}");
    assert!(
        err.contains(" at "),
        "expected source location, stderr: {err}"
    );
}

/// The location column counts characters: the `é` before `panic` is two bytes but one
/// column, so the call sits at column 13 of line 3, not 14.
const PANIC_AFTER_MULTIBYTE_PROG: &str = r#"
func main() -> i32 {
    /* é */ panic("boom")
    return 0
}
"#;

#[test]
fn panic_location_column_counts_characters() {
    let exe = compile_source(PANIC_AFTER_MULTIBYTE_PROG, "multibyte_col");
    let err = stderr(&run(&exe));
    assert!(err.contains(".nr:3:13"), "stderr was: {err}");
}

#[test]
fn assert_false_aborts() {
    let exe = compile_source(ASSERT_FALSE_PROG, "false");
    let output = run(&exe);
    assert!(
        aborted(&output),
        "expected assert(false) to abort, exited with {:?}",
        output.status.code()
    );
    assert!(
        stderr(&output).contains("assertion failed"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn assert_true_continues() {
    let exe = compile_source(ASSERT_TRUE_PROG, "true");
    let output = run(&exe);
    assert_eq!(
        output.status.code(),
        Some(0),
        "expected assert(true) to exit 0, stderr: {}",
        stderr(&output)
    );
}

#[test]
fn unreachable_aborts_with_diagnostic() {
    let exe = compile_source(UNREACHABLE_PROG, "unreach");
    let output = run(&exe);
    assert!(aborted(&output), "expected unreachable() to abort");
    assert!(
        stderr(&output).contains("entered unreachable code"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn panic_in_tail_position_compiles_and_aborts() {
    let exe = compile_source(PANIC_TAIL_PROG, "tail");
    let output = run(&exe);
    assert!(aborted(&output), "expected tail panic to abort");
    assert!(
        stderr(&output).contains("panic: bail"),
        "stderr: {}",
        stderr(&output)
    );
}

/// With both streams on one pipe, output printed before a panic comes before the panic's
/// diagnostic. The buffer used to be drained in front of the `abort`, after the diagnostic
/// had already been written, so a log capturing `2>&1` showed the panic first.
#[test]
fn test_bug_067_buffered_output_precedes_the_panic_on_a_shared_stream() {
    let exe = compile_source(
        r#"
func main() -> i32 {
    println("before")
    panic("boom")
    0
}
"#,
        "shared_stream",
    );
    let log = std::env::temp_dir().join("neuro_panic_shared_stream.log");
    let file = std::fs::File::create(&log).expect("create log");
    let status = Command::new(&exe)
        .stdout(file.try_clone().expect("clone log handle"))
        .stderr(file)
        .status()
        .expect("run executable");
    assert!(!status.success(), "the panic must abort");
    let merged = std::fs::read_to_string(&log).expect("read log");
    let before = merged.find("before").expect("stdout text in the log");
    let panic = merged
        .find("panic: boom")
        .expect("panic diagnostic in the log");
    assert!(before < panic, "stdout must come first, got:\n{merged}");
}
