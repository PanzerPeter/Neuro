// What `grad_differential` and `numpy_differential` share: both drive a Python harness from
// `cargo test`, and a missing prerequisite skips rather than fails.

use std::path::PathBuf;
use std::process::{Command, Output};

/// Exit status the harness uses for "prerequisite missing", the autotools convention.
const EXIT_SKIPPED: i32 = 77;

/// Path to the `neurc` binary Cargo built for this test run.
///
/// Cargo sets `CARGO_BIN_EXE_neurc` for integration tests in the `neurc` package; it is
/// absolute and already carries the platform executable suffix.
fn neurc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
}

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <root>/compiler/neurc
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Run the `harness` script with `extra_args`, or report why it could not be run.
///
/// `None` means the interpreter itself is absent, which is the one condition the harness
/// cannot report on its own behalf.
pub fn run_harness(harness: &str, extra_args: &[&str]) -> Option<Output> {
    let root = workspace_root();
    let harness = root.join(harness);
    for interpreter in ["python3", "python"] {
        let result = Command::new(interpreter)
            .arg(&harness)
            .arg("--neurc")
            .arg(neurc_path())
            .args(extra_args)
            .current_dir(&root)
            .output();
        if let Ok(output) = result {
            return Some(output);
        }
    }
    None
}

/// Assert the harness succeeded, treating a missing prerequisite as a pass.
pub fn expect_success(output: Option<Output>, subject: &str) {
    let Some(output) = output else {
        eprintln!("skipping {subject}: no python interpreter on PATH");
        return;
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.code() == Some(EXIT_SKIPPED) {
        eprintln!("skipping {subject}: {}", stderr.trim());
        return;
    }
    assert!(
        output.status.success(),
        "{subject} failed:\nstdout: {stdout}\nstderr: {stderr}"
    );
}
