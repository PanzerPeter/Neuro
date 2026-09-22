#!/usr/bin/env python3
"""Check a gradient against central finite differences of the compiled Neuro function.

This is the oracle every automatic-differentiation item is measured against. The Enzyme
spike that preceded Neuro's own AD engine produced silently ZERO derivatives rather than
crashing, so "the transform ran" proves nothing: a gradient is only believed once a second,
independent computation of the same number agrees with it. Finite differences are that
second computation, and they need no AD machinery to exist, which is why this harness lands
before the transform rather than after it.

What it checks today: `neurc` compiles each case into a shared library, the harness
differentiates the compiled function numerically, and the result must match a derivative
written out by hand on this side. That is the calibration step — it fixes the step size, the
tolerance and the case set, and proves the comparison has teeth — and it is a real check of
the primal the AD transform will read, because every perturbed point is evaluated by
compiled code rather than by a model of it.

When the reverse-mode transform lands it emits a pure `__f__rev` sibling per `@grad`
function. Driving that is one more call site on top of `fd_gradient` and `compare` below:
the produced gradient replaces `expected` and the finite-difference result becomes the
reference, with the case's analytic gradient staying as a third opinion. Nothing in the
comparison core changes, which is the point of landing it first.

Pipeline: write one Neuro module holding every case's function, `neurc compile --emit obj`,
link it into a shared library with the platform C compiler, `ctypes`-load it, and call each
function at the perturbed points. NumPy is deliberately NOT a dependency — the oracle here
is arithmetic this file performs, not a library's derivative.

Run it directly, or through `cargo test -p neurc --test grad_differential`:

    python tools/grad_differential.py --neurc target/debug/neurc
    python tools/grad_differential.py --self-test
"""

import argparse
import ctypes
import math
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

# The exit status a missing prerequisite reports, distinct from a failed comparison so the
# Rust driver can skip rather than fail. 77 is the GNU autotools convention for "skipped".
EXIT_SKIPPED = 77

# Central differences trade truncation error, O(h^2), against cancellation in the
# subtraction, O(eps/h). The sum is minimised near eps^(1/3), which for IEEE double leaves
# roughly ten correct digits — enough that a tolerance loose enough to absorb the numerical
# error is still far tighter than any wrong derivative would land.
STEP_SCALE = sys.float_info.epsilon ** (1.0 / 3.0)

# Compared as `|a - b| <= RELATIVE * max(|a|, |b|) + ABSOLUTE`. The relative term carries
# the check for gradients of ordinary magnitude; the absolute term exists because a true
# partial of exactly zero has no scale to be relative to, and cancellation still leaves
# noise on the order of the step there.
RELATIVE_TOLERANCE = 1e-6
ABSOLUTE_TOLERANCE = 1e-8

# How far a `--self-test` run moves each expectation. Every case's partials are O(1) to
# O(100) here, so a shift of one is comfortably outside the tolerance above and inside the
# range where a comparison that merely checked magnitudes could not get lucky.
SELF_TEST_PERTURBATION = 1.0


def find_c_compiler():
    for candidate in ("cc", "clang", "gcc"):
        if shutil.which(candidate):
            return candidate
    return None


C_COMPILER = find_c_compiler()


class Case:
    """One Neuro function, a point to differentiate it at, and its true gradient.

    `source` declares the function; `gradient` recomputes the partials from the derivative
    rules written out again on this side. The duplication is the point: a reference that
    asked the compiler for the derivative would test the compiler against itself, which is
    the failure mode this whole harness exists to rule out.

    `point` must avoid every kink in `source`. A central difference straddles the point it
    is taken at, so evaluating one where a branch switches sides measures the average of two
    different functions and is wrong about both. A case exercising a branch therefore picks
    a point strictly inside one arm.
    """

    def __init__(self, name, source, point, gradient):
        self.name = name
        self.source = source
        self.point = point
        self.gradient = gradient


CASES = [
    Case(
        "affine",
        """
func affine(x: f64, y: f64) -> f64 {
    return (3.0 * x) + (-2.0 * y) + 7.0
}
""",
        (1.25, -0.5),
        # A constant gradient: the one case whose finite difference is exact, so a failure
        # here is the harness's own arithmetic rather than a tolerance that is too tight.
        lambda x, y: (3.0, -2.0),
    ),
    Case(
        "product",
        """
func product(x: f64, y: f64) -> f64 {
    return x * y
}
""",
        (2.5, -4.0),
        lambda x, y: (y, x),
    ),
    Case(
        "cubic_polynomial",
        """
func cubic_polynomial(x: f64, y: f64) -> f64 {
    val cube = x * x * x
    val cross = 2.0 * x * y
    return cube + cross - (y * y)
}
""",
        (1.5, 0.75),
        lambda x, y: (3.0 * x * x + 2.0 * y, 2.0 * x - 2.0 * y),
    ),
    Case(
        "rational",
        """
func rational(x: f64, y: f64) -> f64 {
    val numerator = (x * x) + y
    val denominator = 1.0 + (y * y)
    return numerator / denominator
}
""",
        (1.5, 2.0),
        # d/dy of (x^2 + y) / (1 + y^2) by the quotient rule.
        lambda x, y: (
            2.0 * x / (1.0 + y * y),
            (1.0 + y * y - 2.0 * y * (x * x + y)) / (1.0 + y * y) ** 2,
        ),
    ),
    Case(
        "nested_calls",
        """
func scale(value: f64) -> f64 {
    return 4.0 * value
}

func shift(value: f64) -> f64 {
    return value - 1.5
}

func nested_calls(x: f64) -> f64 {
    val inner = shift(x)
    return scale(inner * inner)
}
""",
        (0.875,),
        # 4 * (x - 1.5)^2, differentiated through two calls the transform will have to
        # follow rather than treat as opaque.
        lambda x: (8.0 * (x - 1.5),),
    ),
    Case(
        "taken_branch",
        """
func taken_branch(x: f64, y: f64) -> f64 {
    if x > y { return 3.0 * x * x }
    return (x * y) - 2.0
}
""",
        (2.0, 0.5),
        # Strictly inside the `x > y` arm, so the derivative is that arm's alone.
        lambda x, y: (6.0 * x, 0.0),
    ),
    Case(
        "untaken_branch",
        """
func untaken_branch(x: f64, y: f64) -> f64 {
    if x > y { return 3.0 * x * x }
    return (x * y) - 2.0
}
""",
        (0.5, 2.0),
        # The same function inside the other arm. Both points are needed: a transform that
        # differentiated only the first arm would pass `taken_branch` and fail here.
        lambda x, y: (y, x),
    ),
    Case(
        "accumulating_loop",
        """
func accumulating_loop(x: f64, y: f64) -> f64 {
    mut total: f64 = 0.0
    mut step: i32 = 0
    while step < 4 {
        total = total + (x * y)
        total = total * 1.5
        step = step + 1
    }
    return total
}
""",
        (1.25, 3.0),
        # The loop is linear in the product, so the accumulated coefficient is
        # sum over k in 1..=4 of 1.5^k; the partials are that times y and x.
        lambda x, y: (
            sum(1.5**k for k in range(1, 5)) * y,
            sum(1.5**k for k in range(1, 5)) * x,
        ),
    ),
    Case(
        "power_loop",
        """
func power_loop(x: f64) -> f64 {
    mut acc: f64 = 1.0
    mut step: i32 = 0
    while step < 5 {
        acc = acc * x
        step = step + 1
    }
    return acc
}
""",
        (1.4,),
        # x^5, built by a loop rather than by repeated multiplication in one expression, so
        # the derivative crosses a carried dependency instead of a straight-line chain.
        lambda x: (5.0 * x**4,),
    ),
    Case(
        "unused_parameter",
        """
func unused_parameter(x: f64, y: f64) -> f64 {
    return (x * x) + 1.0
}
""",
        (3.0, 11.0),
        # A parameter the body never reads has a partial of exactly zero. This is the one
        # place a zero is the right answer, and it is why the all-zero guard below rejects
        # only gradients that are entirely zero rather than any zero component.
        lambda x, y: (2.0 * x, 0.0),
    ),
]


class Failure(Exception):
    """A case whose gradient disagreed with the oracle, carrying the report to print."""


def fd_gradient(entry, point):
    """Central-difference gradient of `entry` at `point`, one partial per coordinate.

    `entry` is called through `ctypes`, so every evaluation runs the compiled Neuro
    function. The step is scaled by the coordinate's own magnitude because a fixed step is
    either below the representable gap at large arguments or needlessly lossy at small ones;
    the `max` against one keeps a coordinate at zero from producing a step of zero.
    """
    partials = []
    for axis, coordinate in enumerate(point):
        step = STEP_SCALE * max(abs(coordinate), 1.0)
        forward = list(point)
        backward = list(point)
        forward[axis] = coordinate + step
        backward[axis] = coordinate - step
        # Re-reading the steps off the perturbed values rather than using `2 * step`
        # accounts for the rounding that storing them introduced, which is the difference
        # between eight and ten correct digits at this step size.
        span = forward[axis] - backward[axis]
        if span == 0.0:
            raise Failure(
                f"step {step!r} at coordinate {axis} is below the representable gap "
                f"near {coordinate!r}; no difference can be taken"
            )
        partials.append((entry(*forward) - entry(*backward)) / span)
    return partials


def compare(produced, expected):
    """Assert two gradients agree componentwise, or raise the report."""
    if len(produced) != len(expected):
        raise Failure(f"arity: {len(produced)} partials against {len(expected)}")
    for axis, (got, want) in enumerate(zip(produced, expected)):
        if not math.isfinite(got):
            raise Failure(f"partial {axis}: finite differences produced {got!r}")
        tolerance = RELATIVE_TOLERANCE * max(abs(got), abs(want)) + ABSOLUTE_TOLERANCE
        if abs(got - want) > tolerance:
            raise Failure(
                f"partial {axis}: finite differences give {got!r}, "
                f"the derivative rules give {want!r}"
            )


def build_library(neurc, work_dir):
    """Compile every case into one shared library and return its path."""
    source_path = work_dir / "grad_cases.nr"
    # Two cases share one function on purpose (the branch pair), so identical sources must
    # not be emitted twice: the module would declare the same function name twice and fail
    # to compile. Keyed by source text, in declaration order.
    sources = dict.fromkeys(case.source for case in CASES)
    source_path.write_text("".join(sources), encoding="utf-8")

    object_path = work_dir / "grad_cases.o"
    compile_result = subprocess.run(
        [str(neurc), "compile", "--emit", "obj", "-o", str(object_path), str(source_path)],
        capture_output=True,
        text=True,
    )
    if compile_result.returncode != 0:
        raise Failure(
            "neurc could not compile the case module:\n"
            f"{compile_result.stdout}{compile_result.stderr}"
        )

    library_path = work_dir / "libgrad_cases.so"
    link_result = subprocess.run(
        [C_COMPILER, "-shared", "-o", str(library_path), str(object_path), "-lm"],
        capture_output=True,
        text=True,
    )
    if link_result.returncode != 0:
        raise Failure(
            f"{C_COMPILER} could not link the case object into a shared library:\n"
            f"{link_result.stdout}{link_result.stderr}"
        )
    return library_path


def bind_entry(library, case):
    """Look up one case's compiled function and describe its C signature."""
    entry = getattr(library, case.name)
    entry.restype = ctypes.c_double
    entry.argtypes = [ctypes.c_double] * len(case.point)
    return entry


def detectable(expected):
    """Whether a gradient could distinguish a correct producer from one returning zeros.

    The Enzyme spike failed by handing back zeros rather than by crashing, so a case whose
    true gradient is entirely zero would pass under that failure and is worthless as a pass
    condition. A single zero component is fine and `unused_parameter` relies on it.
    """
    return any(partial != 0.0 for partial in expected)


def run(neurc, corrupt):
    """Compile, differentiate and compare every case. Returns the list of failure reports."""
    failures = []
    # Windows keeps a loaded module's file locked and `ctypes` has no portable unload, so
    # the directory may refuse to go. Losing a temp directory is not a failed comparison.
    with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as work_dir:
        library = ctypes.CDLL(str(build_library(neurc, Path(work_dir))))
        for case in CASES:
            expected = list(case.gradient(*case.point))
            if not detectable(expected):
                failures.append(
                    f"{case.name}: every partial at {case.point} is zero, so the case "
                    "cannot tell a correct gradient from one that returns zeros"
                )
                continue
            if corrupt:
                expected = [partial + SELF_TEST_PERTURBATION for partial in expected]
            try:
                produced = fd_gradient(bind_entry(library, case), case.point)
                compare(produced, expected)
            except Failure as failure:
                failures.append(f"{case.name}: {failure}")
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--neurc",
        default="target/debug/neurc",
        help="path to the neurc binary to test (default: target/debug/neurc)",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="shift every expectation and require the comparison to reject all of them",
    )
    args = parser.parse_args()

    if C_COMPILER is None:
        print("no C compiler on PATH; cannot link a shared library", file=sys.stderr)
        return EXIT_SKIPPED

    neurc = Path(args.neurc)
    if not neurc.is_file():
        print(f"neurc not found at {neurc}", file=sys.stderr)
        return EXIT_SKIPPED

    try:
        failures = run(neurc, corrupt=args.self_test)
    except Failure as failure:
        print(failure, file=sys.stderr)
        return 1

    if args.self_test:
        # The zero guard decides whether a case is worth running at all, so it is checked
        # here rather than by adding a case that exists only to be refused.
        if detectable([0.0, 0.0]) or not detectable([0.0, 1.0]):
            print(
                "self-test: the zero guard does not distinguish an entirely zero gradient "
                "from one with a single zero partial",
                file=sys.stderr,
            )
            return 1
        missed = len(CASES) - len(failures)
        if missed:
            print(
                f"self-test: {missed} shifted case(s) were accepted; the comparison does "
                "not actually compare",
                file=sys.stderr,
            )
            return 1
        print(f"self-test: every one of {len(CASES)} shifted cases rejected")
        return 0

    for failure in failures:
        print(failure, file=sys.stderr)
    if failures:
        print(
            f"{len(failures)} of {len(CASES)} cases disagree with the derivative rules",
            file=sys.stderr,
        )
        return 1

    print(f"{len(CASES)} finite-difference gradients agree with the derivative rules")
    return 0


if __name__ == "__main__":
    sys.exit(main())
