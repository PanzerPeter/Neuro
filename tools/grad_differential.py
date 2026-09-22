#!/usr/bin/env python3
"""Check a gradient against central finite differences of the compiled Neuro function.

This is the oracle every automatic-differentiation item is measured against. The Enzyme
spike that preceded Neuro's own AD engine produced silently ZERO derivatives rather than
crashing, so "the transform ran" proves nothing: a gradient is only believed once a second,
independent computation of the same number agrees with it. Finite differences are that
second computation, and they need no AD machinery to exist, which is why this harness landed
before the transform rather than after it.

Two case sets share one compiled module.

The scalar cases are the calibration: `neurc` compiles each into a shared library, the
harness differentiates the compiled function numerically, and the result must match a
derivative written out by hand on this side. That fixes the step size and the tolerance and
proves the comparison has teeth, against compiled code rather than a model of it.

The tensor cases are the pass condition of the reverse-mode transform. Each is a `@grad`
function, so the module also holds its generated `__f__rev`, which returns the loss and a
`GradsOf_f` bundle of owned gradient tensors. The harness calls `__f__rev`, reads the
gradient straight out of the DLPack handle it returns, and requires it to agree with central
finite differences of the compiled primal `f` at the same point. The analytic gradient stays
as a third opinion, and the bundle's dtype and shape must be the parameter's own.

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
import struct
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

# The same balance for a function evaluated in `f32`, which every `@grad` loss is: it returns
# a rank-0 `Tensor<f32, []>`, so the primal carries about seven digits rather than sixteen.
# The step moves to eps32^(1/3), leaving four to five correct digits (the cases below land
# within 1e-4 of the analytic partials), and the tolerance widens to match with a margin of
# about ten. A shift of `SELF_TEST_PERTURBATION` still lands far outside it for every case,
# whose partials are O(1) to O(10).
F32_EPSILON = 2.0**-23
F32_STEP_SCALE = F32_EPSILON ** (1.0 / 3.0)
F32_RELATIVE_TOLERANCE = 1e-3
F32_ABSOLUTE_TOLERANCE = 1e-3

# The produced gradient against the analytic one: both are exact up to the `f32` rounding
# of a few operations, so this is far tighter than any finite difference could support.
REVERSE_RELATIVE_TOLERANCE = 1e-5
REVERSE_ABSOLUTE_TOLERANCE = 1e-5

# `__f__rev` returns a two-pointer aggregate, and the tensor cases read it through ctypes as
# the equivalent C struct. On the System V and AArch64 C ABIs that struct comes back in two
# registers, as LLVM returns the aggregate. The Windows x64 C ABI returns it through a hidden
# pointer instead, so ctypes would read garbage there and the tensor cases are skipped.
AGGREGATE_RETURN_MATCHES_C = sys.platform != "win32"


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


class TensorCase:
    """One `@grad` function of a single `f32` tensor, a point, and its true gradient.

    `source` declares the function; its first parameter is the differentiated tensor, of
    extents `shape`, and any further ones are the `f32` `constants`, which have no
    gradient. `point` lists the tensor's elements in row-major order and `gradient`
    recomputes the partials from `(*point, *constants)` by hand, in the same order.

    One differentiated parameter per case, because the bundle then holds one pointer and
    `__f__rev` returns two: the size the C ABIs return in registers (see
    `AGGREGATE_RETURN_MATCHES_C`).
    """

    def __init__(self, name, source, shape, point, gradient, constants=()):
        self.name = name
        self.source = source
        self.shape = shape
        self.point = point
        self.gradient = gradient
        self.constants = constants


def weighted_matmul_gradient(*a):
    # loss = sum((A @ B) * K) with A the [2, 3] parameter, so dA = K @ B^T.
    b = [[1.0, -1.0], [0.5, 2.0], [-2.0, 1.5]]
    k = [[1.0, 2.0], [-1.0, 0.5]]
    return [
        sum(k[i][j] * b[l][j] for j in range(2)) for i in range(2) for l in range(3)
    ]


def right_matmul_gradient(*w):
    # loss = sum((A @ W)^2) with W the [3, 2] parameter, so dW = A^T @ (2 A @ W).
    a = [[1.0, 2.0, -1.0], [0.5, -1.5, 2.0]]
    wm = [[w[2 * r], w[2 * r + 1]] for r in range(3)]
    p = [[sum(a[i][l] * wm[l][j] for l in range(3)) for j in range(2)] for i in range(2)]
    return [sum(a[i][l] * 2.0 * p[i][j] for i in range(2)) for l in range(3) for j in range(2)]


TENSOR_CASES = [
    TensorCase(
        "weighted_squares",
        """
@grad
func weighted_squares(w: &mut Tensor<f32, [3]>) -> Tensor<f32, []> {
    val squares = w * w
    val weights: Tensor<f32, [3]> = [3.0, -2.0, 0.5]
    val weighted = squares * weights
    return Tensor::scalar(weighted.sum())
}
""",
        (3,),
        (1.25, -0.5, 2.0),
        lambda a, b, c: (6.0 * a, -4.0 * b, 1.0 * c),
    ),
    TensorCase(
        "element_product",
        """
@grad
func element_product(w: &mut Tensor<f32, [2]>, scale: f32) -> Tensor<f32, []> {
    val scaled_product = w[0] * w[1] * scale
    return Tensor::scalar(scaled_product - w[0])
}
""",
        (2,),
        (1.5, -0.75),
        # The constant is an argument like any other and has no partial of its own.
        lambda x, y, s: (y * s - 1.0, x * s),
        constants=(2.5,),
    ),
    TensorCase(
        "element_quotient",
        """
@grad
func element_quotient(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val numerator = (w[0] * w[0]) + w[1]
    val denominator = (w[1] * w[1]) + 1.0
    return Tensor::scalar(numerator / denominator)
}
""",
        (2,),
        (1.5, 2.0),
        # The scalar `rational` case above, now through the transform's quotient rule.
        lambda x, y: (
            2.0 * x / (1.0 + y * y),
            (1.0 + y * y - 2.0 * y * (x * x + y)) / (1.0 + y * y) ** 2,
        ),
    ),
    TensorCase(
        "left_matmul",
        """
@grad
func left_matmul(a: &mut Tensor<f32, [2, 3]>) -> Tensor<f32, []> {
    val b: Tensor<f32, [3, 2]> = [[1.0, -1.0], [0.5, 2.0], [-2.0, 1.5]]
    val k: Tensor<f32, [2, 2]> = [[1.0, 2.0], [-1.0, 0.5]]
    val contracted = a @ b
    val weighted = contracted * k
    return Tensor::scalar(weighted.sum())
}
""",
        (2, 3),
        (0.5, -1.0, 2.0, 1.5, 0.25, -0.75),
        weighted_matmul_gradient,
    ),
    TensorCase(
        "right_matmul",
        """
@grad
func right_matmul(w: &mut Tensor<f32, [3, 2]>) -> Tensor<f32, []> {
    val a: Tensor<f32, [2, 3]> = [[1.0, 2.0, -1.0], [0.5, -1.5, 2.0]]
    val contracted = a @ w
    val squares = &contracted * &contracted
    return Tensor::scalar(squares.sum())
}
""",
        (3, 2),
        (0.5, -0.25, 1.0, 0.75, -0.5, 0.125),
        right_matmul_gradient,
    ),
    TensorCase(
        "broadcast_row",
        """
@grad
func broadcast_row(w: &mut Tensor<f32, [3]>) -> Tensor<f32, []> {
    val m: Tensor<f32, [2, 3]> = [[1.0, -2.0, 0.5], [0.25, 1.5, -1.0]]
    val shifted = m + w
    val squares = &shifted * &shifted
    return Tensor::scalar(squares.sum())
}
""",
        (3,),
        (0.5, -0.25, 1.0),
        # The row is stretched over both rows of `m`, so its adjoint sums back over them.
        lambda a, b, c: tuple(
            2.0 * ((m0 + w) + (m1 + w))
            for w, m0, m1 in ((a, 1.0, 0.25), (b, -2.0, 1.5), (c, 0.5, -1.0))
        ),
    ),
    TensorCase(
        "broadcast_column",
        """
@grad
func broadcast_column(w: &mut Tensor<f32, [2, 1]>) -> Tensor<f32, []> {
    val m: Tensor<f32, [2, 3]> = [[1.0, -2.0, 0.5], [0.25, 1.5, -1.0]]
    val scaled = w * m
    val squares = &scaled * &scaled
    return Tensor::scalar(squares.sum())
}
""",
        (2, 1),
        (0.75, -1.25),
        # An extent-1 axis stretched to 3: summed back, then restored to [2, 1].
        lambda p, q: (
            2.0 * p * (1.0 + 4.0 + 0.25),
            2.0 * q * (0.0625 + 2.25 + 1.0),
        ),
    ),
    TensorCase(
        "row_means",
        """
@grad
func row_means(w: &mut Tensor<f32, [2, 3]>) -> Tensor<f32, []> {
    val means = w.mean(axis: 1)
    val squares = &means * &means
    return Tensor::scalar(squares.sum())
}
""",
        (2, 3),
        (1.0, 2.0, -0.5, 0.25, -1.5, 3.0),
        lambda *w: tuple(
            2.0 * (sum(w[3 * (k // 3) : 3 * (k // 3) + 3]) / 3.0) / 3.0 for k in range(6)
        ),
    ),
    TensorCase(
        "negation_and_reuse",
        """
@grad
func negation_and_reuse(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val negated = 0.0 - w
    val squared = negated * w
    val total = squared + w
    val inverse = 1.0 / w
    val offset = -total.sum()
    return Tensor::scalar(w.mean() + inverse.sum() - offset)
}
""",
        (2,),
        (1.25, -0.8),
        # `w` is read five times, once through a scalar negation; its adjoint is the sum of
        # all five contributions.
        lambda a, b: tuple(-2.0 * x + 1.0 + 0.5 - 1.0 / (x * x) for x in (a, b)),
    ),
    TensorCase(
        "rank_zero_cube",
        """
@grad
func rank_zero_cube(w: &mut Tensor<f32, []>) -> Tensor<f32, []> {
    val square = w * w
    return square * w
}
""",
        (),
        (1.3,),
        lambda x: (3.0 * x * x,),
    ),
    TensorCase(
        "unread_element",
        """
@grad
func unread_element(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    return Tensor::scalar((w[0] * w[0]) + 1.0)
}
""",
        (2,),
        (3.0, 11.0),
        # The tensor analogue of `unused_parameter`: an element nothing reads.
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


def compare(
    produced,
    expected,
    produced_by="finite differences",
    expected_by="the derivative rules",
    relative=RELATIVE_TOLERANCE,
    absolute=ABSOLUTE_TOLERANCE,
):
    """Assert two gradients agree componentwise, or raise the report."""
    if len(produced) != len(expected):
        raise Failure(f"arity: {len(produced)} partials against {len(expected)}")
    for axis, (got, want) in enumerate(zip(produced, expected)):
        if not math.isfinite(got):
            raise Failure(f"partial {axis}: {produced_by} produced {got!r}")
        tolerance = relative * max(abs(got), abs(want)) + absolute
        if abs(got - want) > tolerance:
            raise Failure(
                f"partial {axis}: {produced_by} give {got!r}, {expected_by} give {want!r}"
            )


def build_library(neurc, work_dir):
    """Compile every case into one shared library and return its path."""
    source_path = work_dir / "grad_cases.nr"
    # Two cases share one function on purpose (the branch pair), so identical sources must
    # not be emitted twice: the module would declare the same function name twice and fail
    # to compile. Keyed by source text, in declaration order.
    sources = dict.fromkeys(case.source for case in CASES)
    sources.update(dict.fromkeys(case.source for case in TENSOR_CASES))
    shapes = dict.fromkeys(case.shape for case in TENSOR_CASES)
    sources.update(dict.fromkeys(constructor_source(shape) for shape in shapes))
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


def constructor_name(shape):
    return "make_tensor_" + ("x".join(str(extent) for extent in shape) or "scalar")


def constructor_source(shape):
    """A Neuro function building an `f32` tensor of `shape` from one `f64` per element.

    The input tensors are built by compiled code, not assembled on this side, so the handle
    `__f__rev` reads is exactly the one a Neuro caller would pass and the harness never has
    to know the control block's layout. Arguments are `f64` because that is the one float
    width ctypes passes unambiguously; the narrowing happens in Neuro.
    """
    count = math.prod(shape)
    params = ", ".join(f"a{k}: f64" for k in range(count))
    elements = [f"a{k} as f32" for k in range(count)]

    def nest(extents, flat):
        if not extents:
            return flat[0]
        step = len(flat) // extents[0]
        rows = [nest(extents[1:], flat[i * step : (i + 1) * step]) for i in range(extents[0])]
        return "[" + ", ".join(rows) + "]"

    body = f"Tensor::from({nest(shape, elements)})" if shape else f"Tensor::scalar({elements[0]})"
    dims = ", ".join(str(extent) for extent in shape)
    return (
        f"\nfunc {constructor_name(shape)}({params}) -> Tensor<f32, [{dims}]> {{\n"
        f"    return {body}\n}}\n"
    )


class DLDevice(ctypes.Structure):
    _fields_ = [("device_type", ctypes.c_int32), ("device_id", ctypes.c_int32)]


class DLDataType(ctypes.Structure):
    _fields_ = [("code", ctypes.c_uint8), ("bits", ctypes.c_uint8), ("lanes", ctypes.c_uint16)]


class DLTensor(ctypes.Structure):
    _fields_ = [
        ("data", ctypes.c_void_p),
        ("device", DLDevice),
        ("ndim", ctypes.c_int32),
        ("dtype", DLDataType),
        ("shape", ctypes.POINTER(ctypes.c_int64)),
        ("strides", ctypes.POINTER(ctypes.c_int64)),
        ("byte_offset", ctypes.c_uint64),
    ]


class DLPackVersion(ctypes.Structure):
    _fields_ = [("major", ctypes.c_uint32), ("minor", ctypes.c_uint32)]


DELETER = ctypes.CFUNCTYPE(None, ctypes.c_void_p)


class DLManagedTensorVersioned(ctypes.Structure):
    _fields_ = [
        ("version", DLPackVersion),
        ("manager_ctx", ctypes.c_void_p),
        ("deleter", DELETER),
        ("flags", ctypes.c_uint64),
        ("dl_tensor", DLTensor),
    ]


class GradientBundle(ctypes.Structure):
    """`GradsOf_f` for a function of one tensor: one owned handle."""

    _fields_ = [("gradient", ctypes.c_void_p)]


class ReverseResult(ctypes.Structure):
    """`(Tensor<f32, []>, GradsOf_f)`, the value `__f__rev` returns."""

    _fields_ = [("loss", ctypes.c_void_p), ("grads", GradientBundle)]


# `kDLFloat`, the type code every tensor here must carry.
DLPACK_FLOAT = 2
F32_BITS = 32


def to_f32(value):
    return struct.unpack("f", struct.pack("f", value))[0]


def read_tensor(handle, what):
    """The elements and extents of an `f32` tensor handle, checked against DLPack."""
    if not handle:
        raise Failure(f"{what} is a null handle")
    tensor = DLManagedTensorVersioned.from_address(handle).dl_tensor
    if tensor.dtype.code != DLPACK_FLOAT or tensor.dtype.bits != F32_BITS:
        raise Failure(f"{what} has dtype code {tensor.dtype.code}, {tensor.dtype.bits} bits")
    extents = tuple(tensor.shape[axis] for axis in range(tensor.ndim))
    elements = (ctypes.c_float * math.prod(extents)).from_address(tensor.data)
    return list(elements), extents


def release(handle):
    """Free a tensor through its own deleter, the one release path DLPack allows."""
    DLManagedTensorVersioned.from_address(handle).deleter(handle)


class TensorEntry:
    """The compiled primal, derivative and input constructor of one tensor case."""

    def __init__(self, library, case):
        self.case = case
        self.make = getattr(library, constructor_name(case.shape))
        self.make.restype = ctypes.c_void_p
        self.make.argtypes = [ctypes.c_double] * math.prod(case.shape)
        arguments = [ctypes.POINTER(ctypes.c_void_p)] + [ctypes.c_float] * len(case.constants)
        self.primal = getattr(library, case.name)
        self.primal.restype = ctypes.c_void_p
        self.primal.argtypes = arguments
        self.reverse = getattr(library, f"__{case.name}__rev")
        self.reverse.restype = ReverseResult
        self.reverse.argtypes = arguments

    def loss(self, point):
        """The compiled primal at `point`, as the one element of its rank-0 result."""
        tensor = ctypes.c_void_p(self.make(*point))
        result = self.primal(ctypes.byref(tensor), *self.case.constants)
        try:
            (value,), _ = read_tensor(result, f"{self.case.name}'s loss")
        finally:
            release(result)
            release(tensor.value)
        return value

    def derivative(self, point):
        """`__f__rev` at `point`: the loss it reports and the gradient in its bundle."""
        tensor = ctypes.c_void_p(self.make(*point))
        result = self.reverse(ctypes.byref(tensor), *self.case.constants)
        try:
            loss, loss_extents = read_tensor(result.loss, "the reverse pass's loss")
            gradient, extents = read_tensor(result.grads.gradient, "the bundle's gradient")
        finally:
            release(result.loss)
            release(result.grads.gradient)
            release(tensor.value)
        if loss_extents != ():
            raise Failure(f"the reverse pass returned a loss of extents {loss_extents}")
        if extents != self.case.shape:
            raise Failure(
                f"the bundle's gradient has extents {extents}, the parameter {self.case.shape}"
            )
        return loss[0], gradient


def fd_gradient_f32(loss, point):
    """Central differences of an `f32` function, stepping each element in `f32`.

    The same method as `fd_gradient`, at the `f32` step. The perturbed coordinates are
    rounded to `f32` before the span is taken, because that rounded value is what the tensor
    actually holds.
    """
    partials = []
    for axis, coordinate in enumerate(point):
        step = F32_STEP_SCALE * max(abs(coordinate), 1.0)
        forward = list(point)
        backward = list(point)
        forward[axis] = to_f32(coordinate + step)
        backward[axis] = to_f32(coordinate - step)
        span = forward[axis] - backward[axis]
        partials.append((loss(forward) - loss(backward)) / span)
    return partials


def run_tensor_case(entry, corrupt):
    case = entry.case
    point = [to_f32(value) for value in case.point]
    expected = list(case.gradient(*point, *case.constants))
    if not detectable(expected):
        raise Failure(
            f"every partial at {case.point} is zero, so the case cannot tell a correct "
            "gradient from one that returns zeros"
        )
    primal = entry.loss(point)
    reported, produced = entry.derivative(point)
    compare(
        [reported],
        [primal],
        "the reverse pass's loss",
        "the primal",
        REVERSE_RELATIVE_TOLERANCE,
        REVERSE_ABSOLUTE_TOLERANCE,
    )
    if corrupt:
        produced = [partial + SELF_TEST_PERTURBATION for partial in produced]
    finite = fd_gradient_f32(entry.loss, point)
    compare(
        finite,
        expected,
        relative=F32_RELATIVE_TOLERANCE,
        absolute=F32_ABSOLUTE_TOLERANCE,
    )
    compare(
        produced,
        finite,
        "`__f__rev`",
        "finite differences",
        F32_RELATIVE_TOLERANCE,
        F32_ABSOLUTE_TOLERANCE,
    )
    compare(
        produced,
        expected,
        "`__f__rev`",
        "the derivative rules",
        REVERSE_RELATIVE_TOLERANCE,
        REVERSE_ABSOLUTE_TOLERANCE,
    )


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
        for case in tensor_cases():
            try:
                run_tensor_case(TensorEntry(library, case), corrupt)
            except Failure as failure:
                failures.append(f"{case.name}: {failure}")
    return failures


def tensor_cases():
    """The tensor cases this platform can drive; see `AGGREGATE_RETURN_MATCHES_C`."""
    if AGGREGATE_RETURN_MATCHES_C:
        return TENSOR_CASES
    return []


def case_count():
    return len(CASES) + len(tensor_cases())


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

    if not AGGREGATE_RETURN_MATCHES_C:
        print(
            "skipping the tensor cases: the Windows x64 C ABI returns `__f__rev`'s "
            "aggregate through memory, not in the registers LLVM uses",
            file=sys.stderr,
        )

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
        missed = case_count() - len(failures)
        if missed:
            print(
                f"self-test: {missed} shifted case(s) were accepted; the comparison does "
                "not actually compare",
                file=sys.stderr,
            )
            return 1
        print(f"self-test: every one of {case_count()} shifted cases rejected")
        return 0

    for failure in failures:
        print(failure, file=sys.stderr)
    if failures:
        print(
            f"{len(failures)} of {case_count()} cases disagree with the derivative rules",
            file=sys.stderr,
        )
        return 1

    print(f"{case_count()} gradients agree with finite differences of the compiled code")
    return 0


if __name__ == "__main__":
    sys.exit(main())
