// Integration tests for compiler lint warnings emitted via the CLI.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn neurc_path() -> std::path::PathBuf {
    let exe_name = if cfg!(target_os = "windows") {
        "neurc.exe"
    } else {
        "neurc"
    };
    std::env::current_exe()
        .expect("current exe")
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace target dir")
        .join(exe_name)
}

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
    assert!(
        stderr.contains("prefer-loop-over-while-true"),
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
        !stderr.contains("prefer-loop-over-while-true"),
        "warning should be suppressed by @allow, but stderr was: {}",
        stderr
    );
}
