//! Drives the DLPack differential harness from `cargo test`.
//!
//! The comparison itself lives in `tools/dlpack_differential.py`, because the oracle is
//! NumPy and the import path is `numpy.from_dlpack`: reaching either from Rust would mean
//! reimplementing a DLPack consumer here, and a consumer this compiler wrote is not an
//! independent check of the handle this compiler emits. What these tests own is the
//! decision to run it at all.
//!
//! A missing prerequisite SKIPS rather than fails. The harness needs python3, NumPy 2.1 or
//! newer, and a C compiler to link the shared library; none of those is required to build
//! the compiler, and a contributor without them must still be able to run the suite. The
//! harness reports a missing prerequisite as exit 77 so it is distinguishable from a real
//! disagreement, which fails.

mod differential_harness;

use differential_harness::{expect_success, run_harness};

const HARNESS: &str = "tools/dlpack_differential.py";

/// Every tensor result the harness computes agrees with NumPy's own, read through the
/// handle rather than through any Neuro-side accessor.
#[test]
fn tensor_results_agree_with_numpy_through_the_dlpack_handle() {
    expect_success(run_harness(HARNESS, &[]), "the DLPack differential harness");
}

/// The comparison actually compares. Every expectation is perturbed by one and every case
/// must then be rejected; a harness that silently accepted a wrong answer would report the
/// same success as the test above and mean nothing.
#[test]
fn a_perturbed_expectation_is_rejected_by_every_case() {
    expect_success(
        run_harness(HARNESS, &["--self-test"]),
        "the DLPack differential harness self-test",
    );
}
