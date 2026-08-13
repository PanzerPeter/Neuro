// End-to-end tests for error-path outlining.
//
// Every panic-family failure path is emitted into a module-private cold function and
// called from the failure site, so the diagnostic machinery never sits inline in the hot
// path. The transformation must be invisible from the outside: these tests drive a
// failure through an outlined thunk in programs that combine several language features,
// and assert the diagnostic text and the abort are exactly what they were when the
// machinery was inline. The `-O2` cases matter most — the thunks are `noinline`, so the
// optimizer must not fold them back in and must not lose the message.

use std::path::PathBuf;
use std::process::{Command, Output};

/// Path to the `neurc` binary Cargo built for this test run.
fn neurc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
}

/// Compile `source` at the given optimization level, returning the executable path.
fn compile_source(source: &str, tag: &str, optimization: &str) -> PathBuf {
    let dir = std::env::temp_dir();
    let src = dir.join(format!("neuro_outline_{tag}.nr"));
    let exe = dir.join(format!("neuro_outline_{tag}"));
    std::fs::write(&src, source).expect("write source");

    let output = Command::new(neurc_path())
        .arg("compile")
        .arg(&src)
        .arg("-O")
        .arg(optimization)
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
/// delivered as SIGABRT, so there is no exit code; on Windows it surfaces as non-zero.
fn aborted(output: &Output) -> bool {
    match output.status.code() {
        None => true,
        Some(code) => code != 0,
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A `Vec` index guard reached through a struct method, a `match`, and a `for` loop.
/// `Vec` bounds are checked in every build, so this exercises the outlined thunk at `-O2`.
const VEC_GUARD_PROG: &str = r#"
struct Reading {
    slot: i32
}

impl Reading {
    func target(&self) -> i32 {
        self.slot
    }
}

enum Mode {
    Scan,
    Probe(i32)
}

func pick(mode: Mode) -> i32 {
    match mode {
        Mode::Scan => 0,
        Mode::Probe(n) => n
    }
}

func main() -> i32 {
    mut samples: Vec<i32> = Vec::new()
    for i in 0..3 {
        samples.push(i * 2)
    }

    val probe = Reading { slot: pick(Mode::Probe(7)) }
    return samples[probe.target()]
}
"#;

/// The same guard, but the index is in range: the program must run to completion and
/// return normally, proving the outlining did not disturb the success path.
const VEC_IN_BOUNDS_PROG: &str = r#"
struct Reading {
    slot: i32
}

impl Reading {
    func target(&self) -> i32 {
        self.slot
    }
}

func main() -> i32 {
    mut samples: Vec<i32> = Vec::new()
    for i in 0..3 {
        samples.push(i * 2)
    }

    val probe = Reading { slot: 2 }
    return samples[probe.target()]
}
"#;

/// A runtime `string` message travels to the thunk as a `(ptr, len)` pair rather than
/// being baked in, so a message built at runtime must still print in full.
const RUNTIME_MESSAGE_PROG: &str = r#"
func main() -> i32 {
    val label: string = "sensor"
    panic(label + " out of range")
}
"#;

/// Two instances of one generic function share a single outlined thunk; whichever
/// instance trips the assertion must still print the diagnostic.
const SHARED_THUNK_PROG: &str = r#"
func require_positive<T>(value: T, ok: bool) -> T {
    assert(ok)
    value
}

func main() -> i32 {
    val a = require_positive(1, true)
    val b = require_positive(2.5, false)
    return a
}
"#;

#[test]
fn vec_bounds_guard_aborts_through_the_thunk_at_o2() {
    let exe = compile_source(VEC_GUARD_PROG, "vec_guard", "2");
    let output = run(&exe);
    assert!(
        aborted(&output),
        "expected the bounds guard to abort, exited with {:?}",
        output.status.code()
    );
    let err = stderr(&output);
    assert!(
        err.contains("panic: Vec index out of bounds"),
        "stderr was: {err}"
    );
    assert!(
        err.contains(" at "),
        "expected a source location, stderr: {err}"
    );
}

#[test]
fn the_success_path_is_unchanged_by_outlining() {
    for level in ["0", "2"] {
        let exe = compile_source(VEC_IN_BOUNDS_PROG, &format!("in_bounds_{level}"), level);
        let output = run(&exe);
        assert_eq!(
            output.status.code(),
            Some(4),
            "expected samples[2] == 4 at -O{level}, stderr: {}",
            stderr(&output)
        );
    }
}

#[test]
fn a_runtime_message_survives_the_thunk_call() {
    for level in ["0", "2"] {
        let exe = compile_source(RUNTIME_MESSAGE_PROG, &format!("runtime_msg_{level}"), level);
        let output = run(&exe);
        assert!(
            aborted(&output),
            "expected panic to abort at -O{level}, exited with {:?}",
            output.status.code()
        );
        let err = stderr(&output);
        assert!(
            err.contains("panic: sensor out of range"),
            "stderr at -O{level} was: {err}"
        );
    }
}

#[test]
fn a_shared_thunk_still_reports_its_diagnostic() {
    let exe = compile_source(SHARED_THUNK_PROG, "shared_thunk", "0");
    let output = run(&exe);
    assert!(
        aborted(&output),
        "expected the assertion to abort, exited with {:?}",
        output.status.code()
    );
    let err = stderr(&output);
    assert!(err.contains("assertion failed"), "stderr was: {err}");
}
