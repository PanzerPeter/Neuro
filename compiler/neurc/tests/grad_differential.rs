//! Drives the finite-difference gradient harness from `cargo test`.
//!
//! The comparison itself lives in `tools/grad_differential.py`, because it has to call the
//! compiled Neuro functions at perturbed points and `ctypes` is the established path for
//! that in this repo. What these tests own is the decision to run it at all.
//!
//! A missing prerequisite SKIPS rather than fails. The harness needs python3 and a C
//! compiler to link the shared library; neither is required to build the compiler, and a
//! contributor without them must still be able to run the suite. The harness reports a
//! missing prerequisite as exit 77 so it is distinguishable from a real disagreement, which
//! fails.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Exit status the harness uses for "prerequisite missing", the autotools convention.
const EXIT_SKIPPED: i32 = 77;

const HARNESS: &str = "tools/grad_differential.py";

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

/// Run the harness with `extra_args`, or report why it could not be run.
///
/// `None` means the interpreter itself is absent, which is the one condition the harness
/// cannot report on its own behalf.
fn run_harness(extra_args: &[&str]) -> Option<Output> {
    let root = workspace_root();
    let harness: &Path = &root.join(HARNESS);
    for interpreter in ["python3", "python"] {
        let result = Command::new(interpreter)
            .arg(harness)
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
fn expect_success(output: Option<Output>, subject: &str) {
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

/// Every case's gradient, taken numerically from the compiled function, agrees with the
/// derivative rules written out by hand on the harness's side, and every `@grad` case's
/// generated `__f__rev` agrees with both.
#[test]
fn finite_differences_agree_with_the_derivative_rules() {
    expect_success(run_harness(&[]), "the finite-difference gradient harness");
}

/// The comparison actually compares, and a case that could not detect a producer returning
/// zeros is refused. Every expectation is shifted by one and every case must then be
/// rejected; a harness that silently accepted a wrong gradient would report the same success
/// as the test above and mean nothing. That failure mode is not hypothetical: the Enzyme
/// spike this harness replaces returned zeros rather than crashing.
#[test]
fn a_shifted_expectation_is_rejected_by_every_case() {
    expect_success(
        run_harness(&["--self-test"]),
        "the finite-difference gradient harness self-test",
    );
}
