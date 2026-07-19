// Drop trait + deterministic destruction tests (Phase 1.7).
//
// Each Drop type below holds a `&mut i32` "sink" and increments it in `drop`, so
// the number of destructor calls is observable through the program's exit code
// once the dropping scope has closed. This exercises scope-exit insertion, LIFO
// order, move elision (a moved value is not double-dropped), and the Copy/Drop
// conflict rule end-to-end.
mod common;
use common::CompileTest;

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

const PROBE: &str = r#"
struct Probe { sink: &mut i32 }

impl Drop for Probe {
    func drop(&mut self) { *self.sink = *self.sink + 1 }
}
"#;

#[test]
fn destructor_runs_once_at_scope_exit() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
func main() -> i32 {{
    mut count: i32 = 0
    {{
        val p = Probe {{ sink: &mut count }}
    }}
    return count
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_once.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(exit_code, 1, "the destructor must run exactly once");
}

#[test]
fn two_owned_values_drop_twice() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
func main() -> i32 {{
    mut count: i32 = 0
    {{
        val a = Probe {{ sink: &mut count }}
        val b = Probe {{ sink: &mut count }}
    }}
    return count
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_twice.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(exit_code, 2, "both owned values must be dropped");
}

#[test]
fn moved_value_is_not_double_dropped() {
    // `val q = p` moves `p`; only the new owner `q` is dropped. Without the
    // drop flag this would run the destructor twice.
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
func main() -> i32 {{
    mut count: i32 = 0
    {{
        val p = Probe {{ sink: &mut count }}
        val q = p
    }}
    return count
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_move.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(exit_code, 1, "a moved value must be dropped exactly once");
}

#[test]
fn loop_body_value_drops_each_iteration() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
func main() -> i32 {{
    mut count: i32 = 0
    for i in 0..5 {{
        val p = Probe {{ sink: &mut count }}
    }}
    return count
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_loop.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(
        exit_code, 5,
        "a loop-body value is dropped on each iteration"
    );
}

#[test]
fn copy_and_drop_conflict_is_rejected() {
    let source = r#"
@derive(Copy)
struct Bad { x: i32 }

impl Drop for Bad {
    func drop(&mut self) { }
}

func main() -> i32 { 0 }
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "a Copy type implementing Drop must be rejected");
    assert!(
        stderr.contains("cannot be Copy"),
        "expected the Copy/Drop conflict diagnostic, got: {stderr}"
    );
}
