// End-to-end tests for integer overflow semantics.
//
// Debug builds (`-O0`) panic on `+`/`-`/`*` overflow; release builds (`-O1..-O3`)
// wrap (two's complement). These tests compile the same overflowing program at
// both optimization levels and assert the runtime behavior differs accordingly.
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

/// Path to the `neurc` binary Cargo built for this test run.
///
/// Cargo sets `CARGO_BIN_EXE_neurc` for integration tests in the `neurc`
/// package; it is absolute and already carries the platform executable
/// suffix. Do not derive it from `current_exe()` — that assumes the legacy
/// `target/<profile>/deps/` layout and breaks under Cargo's build-dir layout.
fn neurc_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
}

/// Compile `source` at optimization level `opt`, returning the executable path.
fn compile_source(source: &str, tag: &str, opt: &str) -> PathBuf {
    let dir = std::env::temp_dir();
    let src = dir.join(format!("neuro_overflow_{tag}.nr"));
    let exe = dir.join(format!("neuro_overflow_{tag}"));
    std::fs::write(&src, source).expect("write source");

    let output = Command::new(neurc_path())
        .arg("compile")
        .arg(&src)
        .arg("-O")
        .arg(opt)
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

fn run(exe: &PathBuf) -> ExitStatus {
    Command::new(exe).output().expect("run executable").status
}

/// What the program wrote to stderr before it stopped.
fn stderr_of(exe: &PathBuf) -> String {
    let output = Command::new(exe).output().expect("run executable");
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// True if the process was aborted by the panic runtime rather than exiting normally.
///
/// On Unix the abort is delivered as a signal (SIGABRT), so there is no exit code
/// (`code()` is `None`). Windows has no signals: `abort` surfaces as an NTSTATUS
/// exception code in the `0xC000_0000+` range, which `ExitStatus::code` returns as
/// a negative `i32`. A normal/wrapped exit always yields a small non-negative code.
fn trapped(status: ExitStatus) -> bool {
    match status.code() {
        None => true,
        Some(code) => code < 0,
    }
}

/// The low byte of the program's return value, as observed through the process
/// exit code. Unix truncates exit codes to 8 bits already; Windows preserves the
/// full 32-bit value, so we mask to the low byte for a platform-independent check.
fn exit_low_byte(status: ExitStatus) -> Option<u8> {
    status.code().map(|code| code as u8)
}

/// `200u8 + 100u8` overflows u8. Debug build must abort (terminated by signal),
/// not return the wrapped value.
const UNSIGNED_OVERFLOW: &str = r#"
func main() -> i32 {
    mut x: u8 = 200u8
    val y: u8 = 100u8
    val z: u8 = x + y
    return z as i32
}
"#;

/// `i32::MAX * 2` overflows a signed integer. Debug build must abort.
const SIGNED_OVERFLOW: &str = r#"
func main() -> i32 {
    mut x: i32 = 2147483647
    val y: i32 = 2
    val z: i32 = x * y
    return z
}
"#;

#[test]
fn unsigned_overflow_aborts_in_debug() {
    let exe = compile_source(UNSIGNED_OVERFLOW, "u_dbg", "0");
    let status = run(&exe);
    assert!(
        trapped(status),
        "expected debug build to abort, but it exited with {:?}",
        status.code()
    );
}

#[test]
fn unsigned_overflow_wraps_in_release() {
    let exe = compile_source(UNSIGNED_OVERFLOW, "u_rel", "2");
    let status = run(&exe);
    // 300 mod 256 = 44.
    assert_eq!(exit_low_byte(status), Some(44));
}

#[test]
fn signed_overflow_aborts_in_debug() {
    let exe = compile_source(SIGNED_OVERFLOW, "s_dbg", "0");
    let status = run(&exe);
    assert!(
        trapped(status),
        "expected debug build to abort, but it exited with {:?}",
        status.code()
    );
}

#[test]
fn signed_overflow_wraps_in_release() {
    let exe = compile_source(SIGNED_OVERFLOW, "s_rel", "2");
    let status = run(&exe);
    // 2147483647 * 2 wraps to -2; the low byte of the exit code is 254.
    assert_eq!(exit_low_byte(status), Some(254));
}

#[test]
fn an_overflow_says_what_failed_and_where() {
    // An overflow is a panic like any other, so it prints a located diagnostic. It used
    // to execute a bare `llvm.trap`, which reaches the programmer only as
    // `SIGILL` / `Illegal instruction` and names neither the failure nor its line.
    let exe = compile_source(SIGNED_OVERFLOW, "s_msg", "0");
    let stderr = stderr_of(&exe);
    assert!(
        stderr.contains("panic: integer overflow at"),
        "an overflow must name itself and its source location, got:\n{stderr}"
    );
}
