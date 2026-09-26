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

The tensor cases are the pass condition of the reverse-mode transform and of the
`.backward()` / `.grad()` layer that runs it. Each is a `@grad` function, and the module
holds, beside it, a generated Neuro probe that builds the parameters, calls the function,
runs `.backward()` and returns one element of one parameter's `.grad()` (or the loss) as an
`f64`. The harness requires those gradients to agree with central finite differences of the
compiled primal `f` at the same point; the analytic gradient stays as a third opinion.
Every probe has a scalar signature, so no aggregate crosses the C boundary and the cases run
on every platform, with any number of differentiated parameters.

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

# The probe index that asks for the loss rather than a gradient element.
PROBE_LOSS = -1


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
    """One `@grad` function of `f32` tensors, a point, and its true gradient.

    `source` declares the function; its leading parameters are the differentiated tensors,
    the first of extents `shape` and any others of `more_shapes`, and the rest are the `f32`
    `constants`, which have no gradient. `point` lists every tensor's elements in row-major
    order, one tensor after another, and `gradient` recomputes the partials from
    `(*point, *constants)` by hand, in the same order. `callee` is the name a call spells
    when it differs from `name`, the compiled symbol: a generic instance is called by its
    template's name. `receiver`, when set, is the expression a `@grad` method is called on:
    the probe binds it and calls `callee` as its method, while `name` is a free function
    doing the same, which is what the finite differences evaluate. `extra_arguments` are
    Neuro expressions the probe's call passes after the tensors and before the constants,
    such as a closure for a function-typed parameter; `name` is then a free function passing
    the same, for the same reason. `fields` maps a tensor's index to the field path it
    occupies in `receiver` (`"layer.w"`), for a method whose `wrt:` names that path: the
    receiver expression moves the tensor `w<index>` in, the call does not pass it, and its
    gradient is read back through the receiver.

    `path`, when set, names a second function in `source` with the primal's signature that
    computes only the path the primal executes at `point`. It exists for a point AT a kink,
    where a central difference of the primal straddles two paths and measures neither: the
    language rules that the derivative there is the executed path's, so the reference
    becomes finite differences of that path, which is smooth at the point. The primal and
    the path must agree on the loss there, which is what proves the path is the executed one.
    """

    def __init__(
        self,
        name,
        source,
        shape,
        point,
        gradient,
        constants=(),
        path=None,
        more_shapes=(),
        callee=None,
        receiver=None,
        extra_arguments=(),
        fields=(),
    ):
        self.name = name
        self.source = source
        self.shapes = (shape, *more_shapes)
        self.point = point
        self.gradient = gradient
        self.constants = constants
        self.path = path
        self.callee = callee or name
        self.receiver = receiver
        self.extra_arguments = extra_arguments
        self.fields = dict(fields)

    def split(self, point):
        """`point` cut into one run of elements per differentiated tensor."""
        runs = []
        start = 0
        for shape in self.shapes:
            count = math.prod(shape)
            runs.append(point[start : start + count])
            start += count
        return runs


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


# One branching body, differentiated at a point inside each arm.
PIECEWISE = """
@grad
func taken_arm(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val x = w[0]
    val y = w[1]
    mut s = x * y
    mut v = w * 2.0
    if x > y {
        s = s + (x * x * 3.0)
        v = &v * w
    } else {
        s = s - y
    }
    return Tensor::scalar(s + v.sum())
}
"""

# An early return, an `if` expression with an `else if`, and a short-circuit `&&`.
EARLY_RETURN = """
@grad
func early_return(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val x = w[0]
    val y = w[1]
    val scale = if x > 1.0 && y > 0.0 { x * 0.0 + 2.0 } else if x > 0.0 { x } else { -x }
    if y < 0.0 { return Tensor::scalar(scale * y) }
    return Tensor::scalar(scale * x * y)
}
"""


# The receiver `method_receiver` is called on, written once for its wrapper and its probe.
WEIGHTING = (
    "Weighting { gain: 1.5, rounds: 2, offsets: [1.0, -2.0, 0.5], "
    "damping: Damping { factor: 0.5 } }"
)


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
        # A shape-generic `@grad` function is differentiated per instance; this is the
        # `[3]` one the caller below instantiates, under its mangled name.
        "shape_generic_g_c3",
        """
@grad
func shape_generic<N>(w: &mut Tensor<f32, [N]>, scale: f32) -> Tensor<f32, []> {
    val first = w[0]
    val last = w[2]
    return Tensor::scalar((first * last + first) * scale)
}

func instantiate_shape_generic(w: &mut Tensor<f32, [3]>) -> Tensor<f32, []> {
    shape_generic(w, 1.0f32)
}
""",
        (3,),
        (1.5, -0.75, 2.0),
        lambda a, b, c, s: ((c + 1.0) * s, 0.0, a * s),
        constants=(2.5,),
        callee="shape_generic",
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
    TensorCase(
        "taken_arm",
        PIECEWISE,
        (2,),
        (2.0, 0.5),
        # x > y: s = xy + 3x^2 and v = 2w * w, so the loss is xy + 5x^2 + 2y^2.
        lambda x, y: (y + 10.0 * x, x + 4.0 * y),
    ),
    TensorCase(
        "untaken_arm",
        PIECEWISE.replace("taken_arm", "untaken_arm"),
        (2,),
        (0.5, 2.0),
        # The same body in the other arm: s = xy - y and v = 2w, so xy + 2x + y. A transform
        # that swept only the first arm would pass `taken_arm` and fail here.
        lambda x, y: (y + 2.0, x + 1.0),
    ),
    TensorCase(
        "early_return",
        EARLY_RETURN,
        (2,),
        (1.5, 0.5),
        # `x > 1 && y > 0` holds, so the scale is the constant 2 and the loss 2xy.
        lambda x, y: (2.0 * y, 2.0 * x),
    ),
    TensorCase(
        "early_return_other_path",
        EARLY_RETURN.replace("early_return", "early_return_other_path"),
        (2,),
        (0.5, -1.5),
        # The `&&` fails on its first operand, the `else if` takes x, and y < 0 returns
        # early: the loss is x * y.
        lambda x, y: (y, x),
    ),
    TensorCase(
        "tensor_power_loop",
        """
@grad
func tensor_power_loop(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    mut acc = w * 1.0
    mut step = 0
    while step < 3 {
        acc = &acc * w
        step += 1
    }
    return Tensor::scalar(acc.sum())
}
""",
        (2,),
        (1.1, -0.7),
        # A tensor carried through three iterations: the loss is the sum of w^4.
        lambda a, b: (4.0 * a**3, 4.0 * b**3),
    ),
    TensorCase(
        "data_dependent_loop",
        """
@grad
func data_dependent_loop(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    mut total = w[0]
    while total < 10.0 {
        total = total * w[1] + 1.0
    }
    return Tensor::scalar(total * w[0])
}
""",
        (2,),
        (1.5, 2.0),
        # The trip count is decided by the parameter: 1.5 -> 4 -> 9 -> 19, three iterations,
        # with the last test 9 < 10 far enough from the edge for the difference to stay on
        # it. total = x y^3 + y^2 + y + 1, and the loss is total * x.
        lambda x, y: (2.0 * x * y**3 + y * y + y + 1.0, 3.0 * x * x * y * y + 2.0 * x * y + x),
    ),
    TensorCase(
        "nested_loops",
        """
@grad
func nested_loops(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    mut acc = w * 1.0
    mut outer = 0
    while outer < 2 {
        mut inner = 0
        while inner < 2 {
            if w[0] > 0.0 {
                acc = &acc * w
            } else {
                acc = &acc + w
            }
            inner += 1
        }
        acc = &acc * 0.5
        outer += 1
    }
    return Tensor::scalar(acc.sum())
}
""",
        (2,),
        (0.9, 1.2),
        # Two outer iterations of (two multiplications by w, then a halving): 0.25 w^5.
        lambda a, b: (1.25 * a**4, 1.25 * b**4),
    ),
    TensorCase(
        "loop_carried_loss",
        """
@grad
func loop_carried_loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    mut total: Tensor<f32, []> = Tensor::scalar(w[1])
    mut remaining = 2
    while remaining > 0 || total.sum() < 0.0 {
        total *= w[0]
        remaining -= 1
    }
    return total
}
""",
        (2,),
        (1.5, 0.8),
        # The loss is the carried tensor itself, updated in place by `*=`, and the `||` runs
        # its right operand only once the counter is spent: w1 * w0^2.
        lambda x, y: (2.0 * x * y, x * x),
    ),
    TensorCase(
        "skipped_loop_and_shadowing",
        """
@grad
func skipped_loop_and_shadowing(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    mut acc = w * 3.0
    mut count = 0
    while count < 0 {
        acc = &acc * w
        count += 1
    }
    mut factor: f32 = 1.0
    mut k = 0
    while k < 3 {
        factor = factor * 2.0
        k += 1
    }
    val x = w[0]
    if x > 0.0 {
        val x = factor
        acc = &acc * x
    }
    return Tensor::scalar(acc.sum() + x)
}
""",
        (2,),
        (0.5, -1.0),
        # A loop that never runs, one whose carried float never depends on w, and an arm
        # whose `x` shadows the outer one: 24 w summed, plus the OUTER x = w0.
        lambda a, b: (25.0, 24.0),
    ),
    TensorCase(
        "kink_branch",
        """
@grad
func kink_branch(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val x = w[0]
    val y = w[1]
    if x > y { return Tensor::scalar(x * x) }
    return Tensor::scalar(x * y)
}

func kink_branch_path(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    return Tensor::scalar(w[0] * w[1])
}
""",
        (2,),
        (1.5, 1.5),
        # At x == y the condition is false, so the executed path is x * y. The two arms agree
        # on the value there and disagree on the slope in x (2x against y): a kink.
        lambda x, y: (y, x),
        path="kink_branch_path",
    ),
    TensorCase(
        "kink_trip_count",
        """
@grad
func kink_trip_count(w: &mut Tensor<f32, [1]>) -> Tensor<f32, []> {
    mut acc = w[0]
    while acc < 4.0 {
        acc = acc * w[0]
    }
    return Tensor::scalar(acc)
}

func kink_trip_count_path(w: &mut Tensor<f32, [1]>) -> Tensor<f32, []> {
    return Tensor::scalar(w[0] * w[0])
}
""",
        (1,),
        (2.0,),
        # At w = 2 the loop runs once (2 < 4, then 4 < 4 fails), so the executed path is w^2.
        # Any w just below 2 runs a second iteration: the loss jumps, and a central
        # difference of the primal would report a slope of hundreds.
        lambda w: (2.0 * w,),
        path="kink_trip_count_path",
    ),
    TensorCase(
        "transposed",
        """
@grad
func transposed(w: &mut Tensor<f32, [2, 3]>) -> Tensor<f32, []> {
    val copy = w * 1.0
    val flipped = copy.t()
    val weights: Tensor<f32, [3, 2]> = [[1.0, -2.0], [0.5, 3.0], [-1.5, 2.5]]
    val weighted = &flipped * &flipped * weights
    return Tensor::scalar(weighted.sum())
}
""",
        (2, 3),
        (1.0, -0.5, 2.0, 1.5, -1.25, 0.75),
        # loss = sum(K * W^T * W^T), so dW[i][j] = 2 * K[j][i] * W[i][j].
        lambda *w: transposed_gradient(*w),
    ),
    TensorCase(
        "reshaped",
        """
@grad
func reshaped(w: &mut Tensor<f32, [2, 3]>) -> Tensor<f32, []> {
    val scaled = w * 2.0
    val columns = scaled.reshape([3, 2])
    val weights: Tensor<f32, [3, 2]> = [[1.0, -2.0], [0.5, 3.0], [-1.5, 2.5]]
    val flat = (&columns * weights).reshape([6])
    return Tensor::scalar(flat.sum() + (&columns * &columns).sum())
}
""",
        (2, 3),
        (1.0, -0.5, 2.0, 1.5, -1.25, 0.75),
        # A reshape keeps row-major order, so element f of W meets element f of K:
        # loss = sum(2 W * K) + sum(4 W^2), and dW[f] = 2 K[f] + 8 W[f].
        lambda *w: tuple(2.0 * k + 8.0 * x for k, x in zip(RESHAPE_WEIGHTS, w)),
    ),
    TensorCase(
        "converted",
        """
@grad
func converted(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val x = w[0]
    val wide = x as f64
    val squared = (wide * wide) as f32
    val steps = (w[1] * 2.0) as i32
    val stepped = steps as f32
    return Tensor::scalar(squared + w[1] * 3.0 + stepped)
}
""",
        (2,),
        (1.25, 0.8),
        # The widening round trip is the identity, and truncating 2 * w1 = 1.6 is flat
        # around the point, so only x^2 and 3 * w1 have slopes.
        lambda x, y: (2.0 * x, 3.0),
    ),
    TensorCase(
        "computed_reads",
        """
@grad
func computed_reads(w: &mut Tensor<f32, [3]>) -> Tensor<f32, []> {
    mut i = 0
    mut s = 0.0f32
    while i < 3 {
        s = s + w[i] * w[2 - i]
        i = i + 1
    }
    return Tensor::scalar(s)
}
""",
        (3,),
        (1.25, -0.5, 2.0),
        # Positions known only at run time: loss = 2 w0 w2 + w1^2.
        lambda a, b, c: (2.0 * c, 2.0 * b, 2.0 * a),
    ),
    TensorCase(
        "sliced",
        """
@grad
func sliced(w: &mut Tensor<f32, [3, 2]>) -> Tensor<f32, []> {
    val flipped = w[(0..3).rev(), 1]
    val rows = w[1..3, ..]
    val weights: Tensor<f32, [3]> = [1.0, -2.0, 0.5]
    return Tensor::scalar((flipped * weights).sum() + (&rows * &rows).sum())
}
""",
        (3, 2),
        (1.0, -0.5, 2.0, 1.5, -1.25, 0.75),
        # flipped[r] = w[2 - r][1] meets K[r], so column 1 gets K reversed; rows 1 and 2
        # are squared.
        lambda a, b, c, d, e, f: (0.0, 0.5, 2.0 * c, -2.0 + 2.0 * d, 2.0 * e, 1.0 + 2.0 * f),
    ),
    TensorCase(
        "contracted",
        """
@grad
func contracted(w: &mut Tensor<f32, [2, 3]>) -> Tensor<f32, []> {
    val b: Tensor<f32, [3, 2]> = [[1.0, -1.0], [0.5, 2.0], [-2.0, 1.5]]
    val c = einsum("ij,jk->ik", w, &b)
    val rows = einsum("ij->i", w)
    val weights: Tensor<f32, [2]> = [2.0, -1.0]
    val square = einsum("ij,ij->", w, w)
    return Tensor::scalar((&c * &c).sum() + (rows * weights).sum() + square)
}
""",
        (2, 3),
        (1.0, -0.5, 2.0, 1.5, -1.25, 0.75),
        # dW = 2 (W B) B^T + K[i] along each row + 2 W.
        lambda *w: contracted_gradient(*w),
    ),
    TensorCase(
        "counted_loops",
        """
@grad
func counted_loops(w: &mut Tensor<f32, [3]>) -> Tensor<f32, []> {
    mut s = 0.0f32
    for i in 0..3 {
        s = s + w[i] * w[i]
    }
    mut acc = 0.0f32
    for j in (0..3).rev() {
        acc = acc * 0.5 + w[j]
    }
    val k = w[0]
    mut t = 0.0f32
    for k in 1..=2 {
        t = t + w[k] * (k as f32)
    }
    for k in 2..=1 {
        t = t + w[k] * 100.0
    }
    for j in (2..2).rev() {
        t = t + w[j] * 100.0
    }
    mut u = 0.0f32
    for (n, m) in (1..3).enumerate() {
        u = u + w[m] * (n as f32 + 2.0)
    }
    return Tensor::scalar(s + acc + t + u + k)
}
""",
        (3,),
        (1.25, -0.5, 2.0),
        # s = sum(w^2); the reversed loop is Horner's rule, acc = w0 + w1 / 2 + w2 / 4, so
        # its order shows; t = w1 + 2 w2 over the inclusive range, and the two empty ranges
        # add nothing; u = 2 w1 + 3 w2; the outer k = w0 survives the loop that shadows it.
        lambda a, b, c: (2.0 * a + 2.0, 2.0 * b + 3.5, 2.0 * c + 5.25),
    ),
    TensorCase(
        "through_calls",
        """
func squared_norm(x: &Tensor<f32, [3]>) -> f32 {
    val squares = x * x
    return squares.sum()
}

func weighted_ends(x: &mut Tensor<f32, [3]>, k: f32) -> f32 {
    x[0] * k + x[2]
}

func stretched(x: Tensor<f32, [3]>, k: f32) -> Tensor<f32, [3]> {
    x * k
}

func doubled(a: f32) -> f32 {
    a * 2.0
}

func larger_doubled(a: f32, b: f32) -> f32 {
    if a > b { return doubled(a) }
    return b
}

func power(x: f32, n: i32) -> f32 {
    mut acc = 1.0f32
    mut i = 0
    while i < n {
        acc = acc * x
        i = i + 1
    }
    acc
}

func leading_pair<N>(x: &Tensor<f32, [N]>) -> f32 {
    x[0] + x[1]
}

@grad
func through_calls(w: &mut Tensor<f32, [3]>) -> Tensor<f32, []> {
    val h = w * 1.0
    val x = squared_norm(&h)
    val ends = weighted_ends(w, 3.0)
    val t = stretched(w * 1.0, 0.5)
    val lead = leading_pair(&t)
    val larger = larger_doubled(w[0], w[1])
    val powers = power(w[2], 3) + power(w[1], 2)
    return Tensor::scalar(x + ends + lead + larger + powers)
}
""",
        (3,),
        (1.25, -0.5, 2.0),
        # sum(w^2) + (3 w0 + w2) + (w0 + w1) / 2 + 2 w0 (w0 > w1 at the point) + w2^3 + w1^2.
        # Every callee binds a parameter named `x`, the caller's own `x` included.
        lambda a, b, c: (2.0 * a + 5.5, 4.0 * b + 0.5, 3.0 * c * c + 2.0 * c + 1.0),
    ),
    TensorCase(
        # A helper that only reads the parameter takes `&`, and the differentiated `&mut`
        # is handed to it directly, as a shared reborrow, instead of through a copy.
        "read_only_helper",
        """
func read_pair(t: &Tensor<f32, [2]>) -> f32 {
    t[0] * t[1] + t[0]
}

func read_second<N>(t: &Tensor<f32, [N]>) -> f32 {
    t[1] * 3.0
}

@grad
func read_only_helper(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    return Tensor::scalar(read_pair(w) * 2.0 + read_second(w))
}
""",
        (2,),
        (1.5, -0.75),
        # 2 (w0 w1 + w0) + 3 w1.
        lambda a, b: (2.0 * (b + 1.0), 2.0 * a + 3.0),
    ),
    TensorCase(
        # Two differentiated parameters, so the bundle holds two gradients and
        # `.backward()` fills two slots; the broadcast bias gathers its partials
        # from both rows.
        "weight_and_bias",
        """
@grad
func weight_and_bias(w: &mut Tensor<f32, [2, 2]>, b: &mut Tensor<f32, [2]>, scale: f32) -> Tensor<f32, []> {
    val shifted = w + b
    val squares = &shifted * &shifted
    return Tensor::scalar(squares.sum() * scale)
}
""",
        (2, 2),
        (0.5, -1.0, 1.5, 2.0, 0.25, -0.5),
        lambda w00, w01, w10, w11, b0, b1, s: (
            2.0 * s * (w00 + b0),
            2.0 * s * (w01 + b1),
            2.0 * s * (w10 + b0),
            2.0 * s * (w11 + b1),
            2.0 * s * ((w00 + b0) + (w10 + b0)),
            2.0 * s * ((w01 + b1) + (w11 + b1)),
        ),
        constants=(1.5,),
        more_shapes=((2,),),
    ),
    TensorCase(
        # A `@grad` method under rule 3: the receiver is a constant, read for a number, a
        # nested struct's number, a loop bound, and a tensor field's element and sum.
        "method_receiver",
        """
struct Damping {
    factor: f32
}

struct Weighting {
    gain: f32,
    rounds: i32,
    offsets: Tensor<f32, [3]>,
    damping: Damping
}

impl Weighting {
    @grad
    func loss(&self, w: &mut Tensor<f32, [3]>, scale: f32) -> Tensor<f32, []> {
        mut value = w * 1.0
        mut done = 0
        while done < self.rounds {
            value = &value * self.damping.factor
            done += 1
        }
        val shifted = value - self.offsets[1]
        val squares = &shifted * &shifted
        return Tensor::scalar(squares.sum() * self.gain * scale + w[0] * self.offsets.sum())
    }
}

func method_receiver(w: &mut Tensor<f32, [3]>, scale: f32) -> Tensor<f32, []> {
    val weighting = """
        + WEIGHTING
        + """
    weighting.loss(w, scale)
}
""",
        (3,),
        (1.5, -0.75, 2.0),
        # gain * scale * sum((w / 4 + 2)^2) + w0 * (1 - 2 + 0.5), with gain 1.5.
        lambda a, b, c, s: (
            0.75 * s * (0.25 * a + 2.0) - 0.5,
            0.75 * s * (0.25 * b + 2.0),
            0.75 * s * (0.25 * c + 2.0),
        ),
        constants=(2.0,),
        callee="loss",
        receiver=WEIGHTING,
    ),
    TensorCase(
        # `wrt:` naming one parameter: the tensor it leaves out is a constant passed by
        # value, and gets no gradient.
        "selected_params",
        """
@grad(wrt: [b])
func selective(b: &mut Tensor<f32, [2]>, frozen: Tensor<f32, [2]>, scale: f32) -> Tensor<f32, []> {
    val p = b * &frozen
    val q = &p * b
    return Tensor::scalar(q.sum() * scale + frozen.sum())
}

func selected_params(b: &mut Tensor<f32, [2]>, scale: f32) -> Tensor<f32, []> {
    selective(b, make_tensor_2(0.5, -1.0), scale)
}
""",
        (2,),
        (1.25, -0.5),
        # scale * sum(b^2 * frozen) + sum(frozen), with frozen [0.5, -1].
        lambda b0, b1, s: (2.0 * b0 * 0.5 * s, 2.0 * b1 * -1.0 * s),
        constants=(2.0,),
        callee="selective",
        extra_arguments=("make_tensor_2(0.5, -1.0)",),
    ),
    TensorCase(
        # `wrt:` field paths: a nested struct's tensor copied with
        # `.clone()`, an array element read in place inside a branch, and a parameter. The
        # array's other element is read too and stays a constant.
        "selected_fields",
        """
struct Gauge {
    export w: Tensor<f32, [2]>,
    export gain: f32
}

struct Stack {
    export gauge: Gauge,
    export heads: [Tensor<f32, [2]>; 2]
}

impl Stack {
    @grad(wrt: [self.gauge.w, self.heads[1], k])
    func loss(&mut self, k: &mut Tensor<f32, [2]>, scale: f32) -> Tensor<f32, []> {
        val w = self.gauge.w.clone()
        val p = w * k
        mut total = p.sum() * self.gauge.gain
        if total > 0.0 {
            total = total + self.heads[1][0] * self.heads[1][1]
        }
        return Tensor::scalar(total * scale + self.heads[0].sum() * self.heads[1][0])
    }
}

func selected_fields(
    w: &mut Tensor<f32, [2]>,
    h: &mut Tensor<f32, [2]>,
    k: &mut Tensor<f32, [2]>,
    scale: f32
) -> Tensor<f32, []> {
    mut stack = Stack {
        gauge: Gauge { w: w.clone(), gain: 1.5 },
        heads: [make_tensor_2(0.5, -1.0), h.clone()]
    }
    stack.loss(k, scale)
}
""",
        (2,),
        (1.25, -0.5, 2.0, 0.75, 1.5, 0.5),
        # scale * (1.5 * w.k + h0 * h1) - 0.5 * h0, inside the `total > 0` arm.
        lambda w0, w1, h0, h1, k0, k1, s: (
            1.5 * s * k0,
            1.5 * s * k1,
            s * h1 - 0.5,
            s * h0,
            1.5 * s * w0,
            1.5 * s * w1,
        ),
        constants=(2.0,),
        more_shapes=((2,), (2,)),
        callee="loss",
        receiver=(
            "Stack { gauge: Gauge { w: w0, gain: 1.5 }, "
            "heads: [make_tensor_2(0.5, -1.0), w1] }"
        ),
        fields={0: "gauge.w", 1: "heads[1]"},
    ),
    TensorCase(
        # Calls through function values, each target known at compile time: a closure
        # capturing a differentiated value, a composition, a pipeline into a closure, a
        # helper taking a function (given a closure, then a composition whose second stage
        # branches), and the three traversals. The fold is order-dependent on purpose.
        "function_values",
        """
func cube(x: f32) -> f32 { x * x * x }

func halved(x: f32) -> f32 { x * 0.5 }

func larger(x: f32) -> f32 {
    if x > 0.0 { return x * 3.0 }
    return x
}

func twice(f: (f32) -> f32, x: f32) -> f32 {
    f(f(x))
}

@grad
func function_values(w: &mut Tensor<f32, [3]>) -> Tensor<f32, []> {
    val s = w[1] * 2.0
    val scaled = |x: f32| -> f32 { x * s }
    val shrink = halved >> cube
    val piped = w[2] |> |v: f32| -> f32 { v * v }
    val nested = twice(scaled, w[0])
    val composed = twice(halved >> larger, w[2])
    val mapped = w.map(|x: f32| -> f32 { x * x * s })
    val zipped = w.zip(mapped, |x: f32, y: f32| -> f32 { x * y })
    val folded = w.reduce(1.0f32, |acc: f32, x: f32| -> f32 { acc * x + x })
    val direct = scaled(w[0]) + shrink(w[2]) + piped + nested + composed
    return Tensor::scalar(direct + mapped.sum() + zipped.sum() + folded)
}
""",
        (3,),
        (1.25, -0.5, 2.0),
        # s = 2 w1. 2 w0 w1 + w2^3 / 8 + w2^2 + 4 w0 w1^2 + 2.25 w2 (both stages of
        # `halved >> larger` in its positive arm), s sum(w^2) from the map, s sum(w^3) from
        # the zip, and the fold 2 w0 w1 w2 + w1 w2 + w2.
        lambda a, b, c: (
            2.0 * b + 4.0 * b * b + 4.0 * a * b + 6.0 * a * a * b + 2.0 * b * c,
            2.0 * a
            + 8.0 * a * b
            + 4.0 * b * b
            + 2.0 * (a * a + b * b + c * c)
            + 2.0 * (a**3 + b**3 + c**3)
            + 6.0 * b**3
            + 2.0 * a * c
            + c,
            3.0 * c * c / 8.0
            + 2.0 * c
            + 2.25
            + 4.0 * b * c
            + 6.0 * b * c * c
            + 2.0 * a * b
            + b
            + 1.0,
        ),
    ),
    TensorCase(
        # A function-typed parameter of the `@grad` function itself: the `.backward()` call
        # passes a closure capturing one of its own locals, and the derivative it runs is
        # specialized to that closure, the capture passed along as an argument.
        "passed_function",
        """
@grad
func passed_inner(w: &mut Tensor<f32, [2]>, f: (f32) -> f32, scale: f32) -> Tensor<f32, []> {
    val mapped = w.map(f)
    return Tensor::scalar(mapped.sum() * scale + f(w[0] * w[1]))
}

func passed_function(w: &mut Tensor<f32, [2]>, scale: f32) -> Tensor<f32, []> {
    passed_inner(w, |x: f32| -> f32 { x * x * scale }, scale)
}
""",
        (2,),
        (0.75, -1.25),
        # f(x) = k x^2, so k^2 (w0^2 + w1^2) + k w0^2 w1^2.
        lambda a, b, k: (
            2.0 * k * k * a + 2.0 * k * a * b * b,
            2.0 * k * k * b + 2.0 * k * a * a * b,
        ),
        constants=(1.5,),
        callee="passed_inner",
        extra_arguments=("|x: f32| -> f32 { x * x * c0 }",),
    ),
    TensorCase(
        "elementwise_math",
        """
@grad
func elementwise_math(w: &mut Tensor<f32, [3]>) -> Tensor<f32, []> {
    val grown = w.exp() * 0.25
    val curved = w.tanh() + w.log()
    val rooted = w.sqrt() - w.pow(3.0) * 0.5
    return Tensor::scalar(grown.sum() + curved.sum() + rooted.sum())
}
""",
        (3,),
        (0.5, 1.25, 2.0),
        # Every tensor rule at once, each element on its own: exp(x) / 4, 1 - tanh(x)^2,
        # 1 / x, 1 / (2 sqrt x) and -1.5 x^2.
        lambda *w: tuple(
            0.25 * math.exp(x)
            + 1.0
            - math.tanh(x) ** 2
            + 1.0 / x
            + 0.5 / math.sqrt(x)
            - 1.5 * x * x
            for x in w
        ),
    ),
    TensorCase(
        # The scalar forms, reached through element reads, with an exponent that is a
        # constant argument rather than a literal: it is read by the rule, never
        # differentiated.
        "scalar_math",
        """
@grad
func scalar_math(w: &mut Tensor<f32, [2]>, p: f32) -> Tensor<f32, []> {
    val x = w[0]
    val y = w[1]
    return Tensor::scalar(x.pow(p) + y.abs() * x.sqrt() + (x * y).exp().log())
}
""",
        (2,),
        (1.5, -0.75),
        lambda x, y, p: (
            p * x ** (p - 1.0) + abs(y) / (2.0 * math.sqrt(x)) + y,
            -math.sqrt(x) + x,
        ),
        constants=(2.5,),
    ),
    TensorCase(
        # `.abs()` at exactly zero. Its derivative there is 0 by the language's rule, and a
        # central difference of |x| at 0 is 0 too, since it averages the two slopes.
        "abs_at_zero",
        """
@grad
func abs_at_zero(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val weights: Tensor<f32, [2]> = [3.0, 2.0]
    val scaled = w.abs() * weights
    return Tensor::scalar(scaled.sum())
}
""",
        (2,),
        (0.0, -1.5),
        lambda x, y: (0.0, -2.0),
    ),
    TensorCase(
        # Math whose own value its rule reads (`tanh`, `exp`), inside a loop the reverse pass
        # replays and a branch it re-runs, so the value has to be rebuilt where it is read.
        "math_in_control_flow",
        """
@grad
func math_in_control_flow(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    mut acc = w * 1.0
    mut step = 0
    while step < 2 {
        acc = (&acc * w).tanh()
        step += 1
    }
    if w[0] > 0.0 {
        acc = acc.exp()
    }
    return Tensor::scalar(acc.sum())
}
""",
        (2,),
        (0.6, 0.9),
        # Per element: a1 = tanh(w^2), a2 = tanh(a1 w), loss exp(a2).
        lambda *w: tuple(
            math.exp(math.tanh(math.tanh(x * x) * x))
            * (1.0 - math.tanh(math.tanh(x * x) * x) ** 2)
            * (math.tanh(x * x) + x * (1.0 - math.tanh(x * x) ** 2) * 2.0 * x)
            for x in w
        ),
    ),
]


RESHAPE_WEIGHTS = (1.0, -2.0, 0.5, 3.0, -1.5, 2.5)


def transposed_gradient(*w):
    k = [[1.0, -2.0], [0.5, 3.0], [-1.5, 2.5]]
    return tuple(2.0 * k[j][i] * w[3 * i + j] for i in range(2) for j in range(3))


def contracted_gradient(*w):
    b = [[1.0, -1.0], [0.5, 2.0], [-2.0, 1.5]]
    rows = (2.0, -1.0)
    wm = [[w[3 * i + j] for j in range(3)] for i in range(2)]
    c = [[sum(wm[i][l] * b[l][k] for l in range(3)) for k in range(2)] for i in range(2)]
    return tuple(
        sum(2.0 * c[i][k] * b[j][k] for k in range(2)) + rows[i] + 2.0 * wm[i][j]
        for i in range(2)
        for j in range(3)
    )


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
    shapes = dict.fromkeys(shape for case in TENSOR_CASES for shape in case.shapes)
    sources.update(dict.fromkeys(constructor_source(shape) for shape in shapes))
    sources.update(dict.fromkeys(probe_source(case) for case in TENSOR_CASES))
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


def probe_name(case):
    return f"probe_{case.name}"


def probe_source(case):
    """A Neuro function running `case`'s `.backward()` and returning one number of it.

    `probe_f(i, <elements>, <constants>) -> f64` builds each differentiated tensor from its
    elements, calls the case's function, runs `.backward()`, and returns element `i` of the
    parameters' gradients laid end to end, or the loss for `PROBE_LOSS`. Scalars in, a scalar
    out: nothing about `GradsOf_f` or the slot's layout crosses the C boundary, which is the
    point, since what is under test is exactly that machinery.
    """
    params = [f"a{k}: f64" for k in range(len(case.point))]
    params += [f"c{k}: f32" for k in range(len(case.constants))]
    lines = []
    arguments = []
    start = 0
    for index, shape in enumerate(case.shapes):
        count = math.prod(shape)
        elements = ", ".join(f"a{start + k}" for k in range(count))
        lines.append(f"    mut w{index} = {constructor_name(shape)}({elements})")
        if index not in case.fields:
            arguments.append(f"&mut w{index}")
        start += count
    arguments += list(case.extra_arguments)
    arguments += [f"c{k}" for k in range(len(case.constants))]
    callee = case.callee
    if case.receiver:
        # A receiver holding a differentiated field is borrowed `&mut` by the call.
        binding = "mut" if case.fields else "val"
        lines.append(f"    {binding} receiver = {case.receiver}")
        callee = f"receiver.{callee}"
    lines.append(f"    val loss = {callee}({', '.join(arguments)})")
    lines.append("    loss.backward()")
    lines.append(f"    if i == {PROBE_LOSS} {{ return loss.sum() as f64 }}")
    flat = 0
    for index, shape in enumerate(case.shapes):
        source = f"receiver.{case.fields[index]}" if index in case.fields else f"w{index}"
        lines.append(f"    val g{index} = {source}.grad()")
        if not shape:
            lines.append(f"    if i == {flat} {{ return g{index}.sum() as f64 }}")
            flat += 1
            continue
        for position in range(math.prod(shape)):
            coordinates = []
            rest = position
            for extent in reversed(shape):
                coordinates.append(str(rest % extent))
                rest //= extent
            element = f"g{index}[{', '.join(reversed(coordinates))}]"
            lines.append(f"    if i == {flat} {{ return {element} as f64 }}")
            flat += 1
    # Unreachable for an index the harness asks for; a NaN fails any comparison it reaches.
    lines.append("    return 0.0 / 0.0")
    body = "\n".join(lines)
    return f"\nfunc {probe_name(case)}(i: i64, {', '.join(params)}) -> f64 {{\n{body}\n}}\n"


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
    """The compiled primal, its `.backward()` probe and the input constructors of a case."""

    def __init__(self, library, case):
        self.case = case
        self.makers = []
        for shape in case.shapes:
            make = getattr(library, constructor_name(shape))
            make.restype = ctypes.c_void_p
            make.argtypes = [ctypes.c_double] * math.prod(shape)
            self.makers.append(make)
        arguments = [ctypes.POINTER(ctypes.c_void_p)] * len(case.shapes)
        arguments += [ctypes.c_float] * len(case.constants)
        self.primal = getattr(library, case.name)
        self.primal.restype = ctypes.c_void_p
        self.primal.argtypes = arguments
        self.path = getattr(library, case.path or case.name)
        self.path.restype = ctypes.c_void_p
        self.path.argtypes = arguments
        self.probe = getattr(library, probe_name(case))
        self.probe.restype = ctypes.c_double
        self.probe.argtypes = (
            [ctypes.c_int64]
            + [ctypes.c_double] * len(case.point)
            + [ctypes.c_float] * len(case.constants)
        )

    def loss(self, point):
        """The compiled primal at `point`, as the one element of its rank-0 result."""
        return self.evaluate(self.primal, point)

    def path_loss(self, point):
        """The executed path at `point`: the primal itself unless the case names a path."""
        return self.evaluate(self.path, point)

    def evaluate(self, function, point):
        tensors = [
            ctypes.c_void_p(make(*run)) for make, run in zip(self.makers, self.case.split(point))
        ]
        result = function(*(ctypes.byref(tensor) for tensor in tensors), *self.case.constants)
        try:
            (value,), _ = read_tensor(result, f"{self.case.name}'s loss")
        finally:
            release(result)
            for tensor in tensors:
                release(tensor.value)
        return value

    def derivative(self, point):
        """`.backward()` at `point`: the loss it leaves and every parameter's `.grad()`."""
        arguments = (*point, *self.case.constants)
        loss = self.probe(PROBE_LOSS, *arguments)
        gradient = [self.probe(index, *arguments) for index in range(len(point))]
        return loss, gradient


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
        "the loss `.backward()` left",
        "the primal",
        REVERSE_RELATIVE_TOLERANCE,
        REVERSE_ABSOLUTE_TOLERANCE,
    )
    compare(
        [entry.path_loss(point)],
        [primal],
        "the executed path's loss",
        "the primal",
        REVERSE_RELATIVE_TOLERANCE,
        REVERSE_ABSOLUTE_TOLERANCE,
    )
    if corrupt:
        produced = [partial + SELF_TEST_PERTURBATION for partial in produced]
    finite = fd_gradient_f32(entry.path_loss, point)
    compare(
        finite,
        expected,
        relative=F32_RELATIVE_TOLERANCE,
        absolute=F32_ABSOLUTE_TOLERANCE,
    )
    compare(
        produced,
        finite,
        "`.backward()` / `.grad()`",
        "finite differences",
        F32_RELATIVE_TOLERANCE,
        F32_ABSOLUTE_TOLERANCE,
    )
    compare(
        produced,
        expected,
        "`.backward()` / `.grad()`",
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
        for case in TENSOR_CASES:
            try:
                run_tensor_case(TensorEntry(library, case), corrupt)
            except Failure as failure:
                failures.append(f"{case.name}: {failure}")
    return failures


def case_count():
    return len(CASES) + len(TENSOR_CASES)


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
