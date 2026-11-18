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

mod differential_harness;

use differential_harness::{expect_success, run_harness};

const HARNESS: &str = "tools/grad_differential.py";

/// Every case's gradient, taken numerically from the compiled function, agrees with the
/// derivative rules written out by hand on the harness's side, and every `@grad` case's
/// gradients, as `.backward()` parks them and `.grad()` reads them back, agree with both.
#[test]
fn finite_differences_agree_with_the_derivative_rules() {
    expect_success(
        run_harness(HARNESS, &[]),
        "the finite-difference gradient harness",
    );
}

/// The comparison actually compares, and a case that could not detect a producer returning
/// zeros is refused. Every expectation is shifted by one and every case must then be
/// rejected; a harness that silently accepted a wrong gradient would report the same success
/// as the test above and mean nothing. That failure mode is not hypothetical: the Enzyme
/// spike this harness replaces returned zeros rather than crashing.
#[test]
fn a_shifted_expectation_is_rejected_by_every_case() {
    expect_success(
        run_harness(HARNESS, &["--self-test"]),
        "the finite-difference gradient harness self-test",
    );
}
