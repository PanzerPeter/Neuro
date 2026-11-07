// Integration tests for compiler lint warnings emitted via the CLI.

use crate::compile_harness::neurc_path;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn run_check(source: &str) -> (i32, String, String) {
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("lint_test.nr");
    fs::write(&path, source).expect("write source");

    let output = Command::new(neurc_path())
        .arg("check")
        .arg(&path)
        .output()
        .expect("run neurc");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn while_true_check_succeeds_and_emits_warning() {
    let source = r#"
func main() -> i32 {
    mut i: i32 = 0
    while true {
        if i == 3 { break }
        i = i + 1
    }
    return i
}
"#;
    let (code, _stdout, stderr) = run_check(source);
    assert_eq!(code, 0, "check should succeed; stderr: {}", stderr);
    // The printed name is the one `@allow` accepts, so a reader can copy it.
    assert!(
        stderr.contains("warning[prefer_loop_over_while_true]"),
        "expected warning in stderr, got: {}",
        stderr
    );
}

#[test]
fn allow_attribute_suppresses_warning() {
    let source = r#"
@allow(prefer_loop_over_while_true)
func main() -> i32 {
    mut i: i32 = 0
    while true {
        if i == 3 { break }
        i = i + 1
    }
    return i
}
"#;
    let (code, _stdout, stderr) = run_check(source);
    assert_eq!(code, 0, "check should succeed; stderr: {}", stderr);
    assert!(
        !stderr.contains("prefer_loop_over_while_true"),
        "warning should be suppressed by @allow, but stderr was: {}",
        stderr
    );
}

#[test]
fn while_val_raises_no_while_true_warning() {
    // `while val` is built on a `while` whose condition is always true; the warning is
    // for a `while true` the author wrote, and here there is none.
    let source = r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(3)
    mut total = 0
    while val Some(x) = v.pop() { total += x }
    total
}
"#;
    let (code, _stdout, stderr) = run_check(source);
    assert_eq!(code, 0, "check should succeed; stderr: {}", stderr);
    assert!(
        !stderr.contains("prefer_loop_over_while_true"),
        "`while val` should not be linted as `while true`, but stderr was: {}",
        stderr
    );
}
