#!/usr/bin/env python3
"""Check Neuro's tensor results against NumPy through the DLPack handle.

A tensor value IS a `DLManagedTensorVersioned*` (syntax spec, DLPack memory layout), so a
NumPy consumer needs no conversion step: it takes the pointer a Neuro function returned and
reads the elements in place. That is what this harness does, and it is why the comparison is
worth anything. A check that re-serialised the elements through `println` would prove the
arithmetic and prove nothing about the exchange structure; here NumPy decides the dtype, the
rank, the extents and the strides by reading the handle's own fields, so a wrong `dtype.bits`
or a stride counted in bytes instead of elements fails a case rather than passing silently.

Pipeline: write one Neuro module holding every case's function, `neurc compile --emit obj`,
link it into a shared library with the platform C compiler, `ctypes`-load it, call each
function, wrap the returned pointer in a DLPack capsule, and hand it to `numpy.from_dlpack`.

NumPy takes OWNERSHIP of each array it imports, so releasing them at the end runs the
handle's own `deleter` — the single release path the spec requires. A double free or a
deleter that frees the wrong block crashes the interpreter here instead of going unnoticed.

Run it directly, or through `cargo test -p neurc --test numpy_differential`:

    python tools/dlpack_differential.py --neurc target/debug/neurc
    python tools/dlpack_differential.py --self-test
"""

import argparse
import ctypes
import gc
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

# The exit status a missing prerequisite reports, distinct from a failed comparison so the
# Rust driver can skip rather than fail. 77 is the GNU autotools convention for "skipped".
EXIT_SKIPPED = 77

try:
    import numpy as np
except ImportError:
    print("numpy is not installed; nothing to compare against", file=sys.stderr)
    sys.exit(EXIT_SKIPPED)

# `from_dlpack` learned the versioned capsule ("dltensor_versioned") in 2.1. Older NumPy
# only accepts the deprecated unversioned structure, which this compiler does not produce.
MINIMUM_NUMPY = (2, 1)

# `kDLCPU`, the one device this backend builds buffers on.
DLPACK_DEVICE_CPU = 1

# DLPack's `data` pointer alignment guarantee, which every imported array must satisfy.
DLPACK_DATA_ALIGN = 64


def find_c_compiler():
    for candidate in ("cc", "clang", "gcc"):
        if shutil.which(candidate):
            return candidate
    return None


C_COMPILER = find_c_compiler()


class Case:
    """One Neuro function and the NumPy expression it must agree with.

    `source` declares the function; `reference` recomputes the same value with NumPy from
    literals written out again on this side. The duplication is the point: a reference that
    read its inputs from the Neuro program would test the compiler against itself.
    """

    def __init__(self, name, source, reference):
        self.name = name
        self.source = source
        self.reference = reference


CASES = [
    Case(
        "ew_add",
        """
func ew_add() -> Tensor<f32, [2, 3]> {
    val a: Tensor<f32, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
    val b: Tensor<f32, [2, 3]> = [[10.0, 20.0, 30.0], [40.0, 50.0, 60.0]]
    val sum = a + b
    return sum
}
""",
        lambda: np.array([[1, 2, 3], [4, 5, 6]], dtype=np.float32)
        + np.array([[10, 20, 30], [40, 50, 60]], dtype=np.float32),
    ),
    Case(
        "broadcast_lower_rank",
        """
func broadcast_lower_rank() -> Tensor<f32, [2, 3]> {
    val a: Tensor<f32, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
    val row: Tensor<f32, [3]> = [10.0, 20.0, 30.0]
    val sum = a + row
    return sum
}
""",
        lambda: np.array([[1, 2, 3], [4, 5, 6]], dtype=np.float32)
        + np.array([10, 20, 30], dtype=np.float32),
    ),
    Case(
        "broadcast_stretched_axis",
        """
func broadcast_stretched_axis() -> Tensor<f32, [2, 3]> {
    val a: Tensor<f32, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
    val column: Tensor<f32, [2, 1]> = [[2.0], [4.0]]
    val scaled = a * column
    return scaled
}
""",
        lambda: np.array([[1, 2, 3], [4, 5, 6]], dtype=np.float32)
        * np.array([[2], [4]], dtype=np.float32),
    ),
    Case(
        "broadcast_scalar",
        """
func broadcast_scalar() -> Tensor<f32, [2, 3]> {
    val a: Tensor<f32, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
    val scaled = a * 2.0
    return scaled
}
""",
        lambda: np.array([[1, 2, 3], [4, 5, 6]], dtype=np.float32) * np.float32(2.0),
    ),
    Case(
        "matmul",
        """
func matmul() -> Tensor<f32, [2, 4]> {
    val a: Tensor<f32, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
    val b: Tensor<f32, [3, 4]> = [
        [1.0, 2.0, 3.0, 4.0],
        [5.0, 6.0, 7.0, 8.0],
        [9.0, 10.0, 11.0, 12.0]
    ]
    val product = a @ b
    return product
}
""",
        lambda: np.array([[1, 2, 3], [4, 5, 6]], dtype=np.float32)
        @ np.array([[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]], dtype=np.float32),
    ),
    Case(
        "reduce_axis",
        """
func reduce_axis() -> Tensor<f64, [3]> {
    val frame: Tensor<f64, [2, 3]> = [[1.0, 3.0, 5.0], [7.0, 9.0, 11.0]]
    val totals = frame.sum(axis: 0)
    return totals
}
""",
        lambda: np.array([[1, 3, 5], [7, 9, 11]], dtype=np.float64).sum(axis=0),
    ),
    Case(
        "reduce_mean",
        """
func reduce_mean() -> Tensor<f64, [2]> {
    val frame: Tensor<f64, [2, 3]> = [[1.0, 2.0, 3.0], [10.0, 20.0, 30.0]]
    val means = frame.mean(axis: 1)
    return means
}
""",
        lambda: np.array([[1, 2, 3], [10, 20, 30]], dtype=np.float64).mean(axis=1),
    ),
    Case(
        "integer_elements",
        """
func integer_elements() -> Tensor<i32, [2, 3]> {
    val a: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val b: Tensor<i32, [2, 3]> = [[10, 20, 30], [40, 50, 60]]
    val sum = a + b
    return sum
}
""",
        lambda: np.array([[1, 2, 3], [4, 5, 6]], dtype=np.int32)
        + np.array([[10, 20, 30], [40, 50, 60]], dtype=np.int32),
    ),
    Case(
        "wide_integer_elements",
        """
func wide_integer_elements() -> Tensor<i64, [4]> {
    val a: Tensor<i64, [4]> = [1, 2, 3, 4]
    val b: Tensor<i64, [4]> = [100, 200, 300, 400]
    val sum = a * b
    return sum
}
""",
        lambda: np.array([1, 2, 3, 4], dtype=np.int64)
        * np.array([100, 200, 300, 400], dtype=np.int64),
    ),
    Case(
        "unsigned_elements",
        """
func unsigned_elements() -> Tensor<u8, [4]> {
    val a: Tensor<u8, [4]> = [1, 2, 3, 4]
    val b: Tensor<u8, [4]> = [10, 20, 30, 40]
    val sum = a + b
    return sum
}
""",
        lambda: np.array([1, 2, 3, 4], dtype=np.uint8)
        + np.array([10, 20, 30, 40], dtype=np.uint8),
    ),
    Case(
        "transpose",
        """
func transpose() -> Tensor<i32, [3, 2]> {
    val m: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val flipped = m.t()
    return flipped
}
""",
        # `.t()` moves the elements rather than relabelling the axes, so the result is
        # row-major in its OWN shape. That is why the expectation is a copy: an array whose
        # strides NumPy read straight off the handle must agree with a contiguous [3, 2],
        # not with a transposed view of a [2, 3].
        lambda: np.ascontiguousarray(
            np.array([[1, 2, 3], [4, 5, 6]], dtype=np.int32).T
        ),
    ),
    Case(
        "reshape",
        """
func reshape() -> Tensor<i32, [3, 2]> {
    val m: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val rows = m.reshape([3, -1])
    return rows
}
""",
        lambda: np.array([[1, 2, 3], [4, 5, 6]], dtype=np.int32).reshape(3, 2),
    ),
    Case(
        "rank_zero",
        """
func rank_zero() -> Tensor<f32, []> {
    val loss: Tensor<f32, []> = Tensor::scalar(0.5)
    return loss
}
""",
        # Rank 0 is the one shape whose `shape` and `strides` fields are null. NumPy has to
        # read the rank first and stop, so an implementation that dereferenced either
        # pointer would fault here rather than elsewhere.
        lambda: np.array(0.5, dtype=np.float32),
    ),
    Case(
        "chained_operators",
        """
func chained_operators() -> Tensor<f32, [2, 2]> {
    val a: Tensor<f32, [2, 2]> = [[1.0, 2.0], [3.0, 4.0]]
    val b: Tensor<f32, [2, 2]> = [[5.0, 6.0], [7.0, 8.0]]
    val bias: Tensor<f32, [2]> = [100.0, 200.0]
    val result = a @ b + bias
    return result
}
""",
        lambda: np.array([[1, 2], [3, 4]], dtype=np.float32)
        @ np.array([[5, 6], [7, 8]], dtype=np.float32)
        + np.array([100, 200], dtype=np.float32),
    ),
]


_capsule_new = ctypes.pythonapi.PyCapsule_New
_capsule_new.restype = ctypes.py_object
_capsule_new.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_void_p]

_capsule_is_valid = ctypes.pythonapi.PyCapsule_IsValid
_capsule_is_valid.restype = ctypes.c_int
_capsule_is_valid.argtypes = [ctypes.py_object, ctypes.c_char_p]

# The name a DLPack producer gives a versioned capsule, and the name a consumer renames it
# to once it has taken ownership of the handle. The rename is the protocol's ownership
# signal: after it, releasing the handle is the consumer's job and the producer must not
# free anything.
CAPSULE_NAME = b"dltensor_versioned"
CONSUMED_CAPSULE_NAME = b"used_dltensor_versioned"


class NeuroTensor:
    """The DLPack producer side of a pointer a Neuro function returned.

    `numpy.from_dlpack` asks any object for `__dlpack__`, so wrapping the raw pointer in
    this is the whole import path — no copy, no intermediate buffer. The capsule carries no
    destructor of its own: once NumPy consumes it, the array owns the handle and calls the
    handle's `deleter` when it is collected.
    """

    def __init__(self, pointer):
        self._pointer = pointer
        self.capsule = None

    def __dlpack__(self, *, stream=None, max_version=None, dl_device=None, copy=None):
        if max_version is None or max_version[0] < 1:
            raise BufferError(
                "this consumer predates DLPack 1.0; a Neuro tensor is a versioned handle"
            )
        if dl_device is not None and tuple(dl_device) != (DLPACK_DEVICE_CPU, 0):
            raise BufferError(f"a Neuro tensor is host memory, not device {dl_device}")
        if copy:
            raise BufferError("a Neuro tensor is exported in place, never copied")
        self.capsule = _capsule_new(self._pointer, CAPSULE_NAME, None)
        return self.capsule

    def __dlpack_device__(self):
        return (DLPACK_DEVICE_CPU, 0)

    def was_consumed(self):
        """Whether the importer renamed the capsule, taking ownership of the handle."""
        return bool(_capsule_is_valid(self.capsule, CONSUMED_CAPSULE_NAME))


class Failure(Exception):
    """A case whose result disagreed with NumPy, carrying the report to print."""


def check(produced, expected):
    """Assert every property NumPy read off the handle, not only the elements.

    Shape, dtype and strides all come from the exchange structure rather than from anything
    this harness told NumPy, so each is a separate assertion about the layout the compiler
    emitted.
    """
    if produced.dtype != expected.dtype:
        raise Failure(f"dtype: handle says {produced.dtype}, NumPy computed {expected.dtype}")
    if produced.shape != expected.shape:
        raise Failure(f"shape: handle says {produced.shape}, NumPy computed {expected.shape}")
    if produced.strides != expected.strides:
        raise Failure(
            f"strides: handle says {produced.strides}, row-major would be {expected.strides}"
        )
    if produced.ndim and not produced.flags["C_CONTIGUOUS"]:
        raise Failure("strides describe a non-contiguous buffer; the layout is row-major")
    address = produced.__array_interface__["data"][0]
    if address % DLPACK_DATA_ALIGN:
        raise Failure(f"data pointer {address:#x} is not {DLPACK_DATA_ALIGN}-byte aligned")
    if not np.array_equal(produced, expected):
        raise Failure(f"values:\n  neuro = {produced!r}\n  numpy = {expected!r}")


def build_library(neurc, work_dir):
    """Compile every case into one shared library and return its path."""
    source_path = work_dir / "dlpack_cases.nr"
    source_path.write_text("".join(case.source for case in CASES), encoding="utf-8")

    object_path = work_dir / "dlpack_cases.o"
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

    library_path = work_dir / "libdlpack_cases.so"
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


def import_tensor(library, name):
    """Call one exported Neuro function and import its handle into NumPy.

    The ownership assertion is not incidental. If the importer had copied instead of
    consuming, every value below would still match while the handle's `deleter` never ran,
    and the spec's single-release-path guarantee would go untested.
    """
    entry = getattr(library, name)
    entry.restype = ctypes.c_void_p
    entry.argtypes = []
    pointer = entry()
    if not pointer:
        raise Failure(f"{name} returned a null handle")
    handle = NeuroTensor(pointer)
    imported = np.from_dlpack(handle)
    if not handle.was_consumed():
        raise Failure("NumPy did not take ownership of the handle; nothing will release it")
    return imported


def run(neurc, corrupt):
    """Compile, import and compare every case. Returns the list of failure reports."""
    failures = []
    imported = []
    with tempfile.TemporaryDirectory() as work_dir:
        library = ctypes.CDLL(str(build_library(neurc, Path(work_dir))))
        for case in CASES:
            expected = case.reference()
            if corrupt:
                expected = expected + expected.dtype.type(1)
            try:
                produced = import_tensor(library, case.name)
                imported.append(produced)
                check(produced, expected)
            except Failure as failure:
                failures.append(f"{case.name}: {failure}")

    # Dropping every imported array runs each handle's own `deleter`, the single release
    # path a foreign consumer uses. Collecting here rather than at interpreter shutdown is
    # what makes a fault in that path this harness's failure and not a stray crash report.
    imported.clear()
    gc.collect()
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
        help="corrupt every expectation and require the comparison to reject all of them",
    )
    args = parser.parse_args()

    if tuple(int(part) for part in np.__version__.split(".")[:2]) < MINIMUM_NUMPY:
        version = ".".join(str(part) for part in MINIMUM_NUMPY)
        print(
            f"numpy {np.__version__} predates {version} and cannot import a versioned "
            "DLPack capsule",
            file=sys.stderr,
        )
        return EXIT_SKIPPED

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
        missed = len(CASES) - len(failures)
        if missed:
            print(
                f"self-test: {missed} corrupted case(s) were accepted; the comparison "
                "does not actually compare",
                file=sys.stderr,
            )
            return 1
        print(f"self-test: all {len(CASES)} corrupted cases rejected")
        return 0

    for failure in failures:
        print(failure, file=sys.stderr)
    if failures:
        print(f"{len(failures)} of {len(CASES)} cases disagree with NumPy", file=sys.stderr)
        return 1

    print(f"{len(CASES)} tensor results agree with NumPy through the DLPack handle")
    return 0


if __name__ == "__main__":
    sys.exit(main())
