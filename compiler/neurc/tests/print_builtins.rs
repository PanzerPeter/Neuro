// End-to-end tests for the standard-output builtins.
//
// `print(text)` / `println(text)` write their argument to stdout (fd 1) and return unit;
// `println` appends a newline. These tests compile each program and assert on the bytes
// the process actually wrote, its exit code, and — since the panic runtime owns stderr —
// that nothing leaked onto the error stream.
use std::path::PathBuf;
use std::process::{Command, Output};

/// Path to the `neurc` binary Cargo built for this test run.
///
/// Cargo sets `CARGO_BIN_EXE_neurc` for integration tests in the `neurc`
/// package; it is absolute and already carries the platform executable
/// suffix. Do not derive it from `current_exe()` — that assumes the legacy
/// `target/<profile>/deps/` layout and breaks under Cargo's build-dir layout.
fn neurc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
}

/// Compile `source` at `-O0`, returning the executable path.
fn compile_source(source: &str, tag: &str) -> PathBuf {
    let dir = std::env::temp_dir();
    let src = dir.join(format!("neuro_print_{tag}.nr"));
    let exe = dir.join(format!("neuro_print_{tag}"));
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

/// Compile and run `source`, returning the process output. `Command::output` gives the
/// child a pipe rather than a terminal, so this is also the short-write path.
fn run_program(source: &str, tag: &str) -> Output {
    let exe = compile_source(source, tag);
    Command::new(&exe).output().expect("run executable")
}

/// Type-check `source`, returning the compiler's combined diagnostics on rejection.
fn check_error(source: &str, tag: &str) -> String {
    let dir = std::env::temp_dir();
    let src = dir.join(format!("neuro_print_{tag}.nr"));
    std::fs::write(&src, source).expect("write source");

    let output = Command::new(neurc_path())
        .arg("check")
        .arg(&src)
        .output()
        .expect("run neurc");

    assert!(!output.status.success(), "expected `check` to reject {tag}");
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The bytes the program wrote to stdout, with line endings normalized to `\n`.
///
/// On Windows fd 1 is a CRT *text-mode* descriptor, so the one `\n` byte the builtin
/// writes reaches the pipe as `\r\n` — the same translation a C `printf` gets, and the
/// convention a native tool is expected to follow. These tests assert *which text* was
/// written, not the platform's line-ending policy, so the translation is undone here.
fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
}

#[test]
fn println_writes_its_argument_and_a_newline_to_stdout() {
    let output = run_program(
        r#"
func main() -> i32 {
    println("hello")
    println("world")
    return 0
}
"#,
        "println_lines",
    );

    assert_eq!(stdout(&output), "hello\nworld\n");
    assert!(output.stderr.is_empty(), "stderr: {:?}", output.stderr);
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn print_writes_no_newline() {
    let output = run_program(
        r#"
func main() -> i32 {
    print("a")
    print("b")
    println("c")
    return 0
}
"#,
        "print_no_newline",
    );

    assert_eq!(stdout(&output), "abc\n");
}

#[test]
fn an_empty_string_prints_only_the_newline() {
    let output = run_program(
        r#"
func main() -> i32 {
    print("")
    println("")
    return 0
}
"#,
        "print_empty",
    );

    assert_eq!(stdout(&output), "\n");
}

#[test]
fn interpolation_renders_before_the_call() {
    // The holes are formatted by the interpolation path, so `println` still sees one
    // ordinary `string` argument.
    let output = run_program(
        r#"
func main() -> i32 {
    val count: i32 = 3
    val ratio: f64 = 2.5
    val name: string = "neuro"
    println("{name}: {count} items at {ratio:.2} each")
    return 0
}
"#,
        "print_interp",
    );

    assert_eq!(stdout(&output), "neuro: 3 items at 2.50 each\n");
}

#[test]
fn a_string_slice_is_printable() {
    // `.slice(range)` yields `&string`, which is the same fat pointer.
    let output = run_program(
        r#"
func main() -> i32 {
    val text: string = "abcdef"
    println(text.slice(1..4))
    return 0
}
"#,
        "print_slice",
    );

    assert_eq!(stdout(&output), "bcd\n");
}

#[test]
fn printing_does_not_consume_its_argument() {
    // The text is read, not moved, so the binding stays usable afterwards.
    let output = run_program(
        r#"
func main() -> i32 {
    val text: string = "kept"
    println(text)
    println(text)
    return text.len() as i32
}
"#,
        "print_no_move",
    );

    assert_eq!(stdout(&output), "kept\nkept\n");
    assert_eq!(output.status.code(), Some(4));
}

#[test]
fn a_large_string_is_written_in_full_through_a_pipe() {
    // `write` may consume less than it is offered — a pipe with a full buffer does
    // exactly that — so the whole buffer must survive the retry loop.
    let output = run_program(
        r#"
func main() -> i32 {
    mut buffer: string = "0123456789abcdef0123456789abcdef"
    for i in 0..12 {
        buffer = buffer + buffer
    }
    print(buffer)
    return 0
}
"#,
        "print_large",
    );

    assert_eq!(output.stdout.len(), 32 * 4096);
    assert!(output.stdout.ends_with(b"0123456789abcdef"));
}

#[test]
fn printing_inside_a_loop_repeats_the_output() {
    let output = run_program(
        r#"
func main() -> i32 {
    for i in 0..3 {
        println("tick {i}")
    }
    return 0
}
"#,
        "print_loop",
    );

    assert_eq!(stdout(&output), "tick 0\ntick 1\ntick 2\n");
}

#[test]
fn a_user_function_shadows_the_builtin() {
    let output = run_program(
        r#"
func println(n: i32) -> i32 {
    return n * 2
}

func main() -> i32 {
    return println(11)
}
"#,
        "print_shadowed",
    );

    assert!(output.stdout.is_empty(), "stdout: {:?}", output.stdout);
    assert_eq!(output.status.code(), Some(22));
}

#[test]
fn a_non_string_argument_is_rejected() {
    let error = check_error(
        r#"
func main() -> i32 {
    println(42)
    return 0
}
"#,
        "print_wrong_type",
    );

    assert!(
        error.contains("expected string"),
        "unexpected diagnostic: {error}"
    );
}

#[test]
fn the_wrong_argument_count_is_rejected() {
    for (source, tag) in [
        ("println()", "print_zero_args"),
        ("println(\"a\", \"b\")", "print_two_args"),
    ] {
        let error = check_error(
            &format!("func main() -> i32 {{\n    {source}\n    return 0\n}}\n"),
            tag,
        );
        assert!(
            error.contains("incorrect number of arguments"),
            "unexpected diagnostic for {tag}: {error}"
        );
    }
}
