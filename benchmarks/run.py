#!/usr/bin/env python3
"""Cross-language benchmark harness: Neuro vs C++, Rust, Python, NumPy and PyTorch.

Each benchmark is a set of programs (`programs/<name>.nr`, and any of `<name>.cpp`,
`<name>.rs`, `<name>.py`, `<name>_numpy.py` and `<name>_torch.py`) that compute the
same result and print it identically. The plain Python row measures the language; the
NumPy row measures what a Python programmer would actually write where the work
vectorizes, and the PyTorch row what they would write for a GPU. C++ and Rust are
built at the same `-O` levels as Neuro, so every compiled row runs the same LLVM
pipeline level. The harness builds the compiled ones, checks they all agree on stdout,
then times each and reports wall time relative to the fastest.

Agreement is checked before any timing: a benchmark that has drifted apart
between languages measures nothing, so a mismatch is a failure, not a footnote.

Usage, from `benchmarks/` (uv builds `.venv` with NumPy and PyTorch from `pyproject.toml`):
    uv run python run.py                  # every benchmark, default levels
    uv run python run.py mandelbrot       # one benchmark
    uv run python run.py --reps 9         # more repetitions (min is reported)
    uv run python run.py --levels 0,2,3   # which -O levels to build Neuro, C++ and Rust at

Requires `neurc` (built with `cargo build --release`) and python3. A language whose
toolchain is missing (a C++ compiler, rustc, NumPy, PyTorch with CUDA) is skipped with
a note rather than failing the run, as is a `gpu_*` benchmark when neurc cannot compile
`@gpu` (off Linux) or the machine has no usable GPU.
"""

import argparse
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROGRAMS = Path(__file__).resolve().parent / "programs"
BUILD = Path(__file__).resolve().parent / "build"

# The minimum of several runs, not the mean: the fastest observed time is the
# one least polluted by scheduling noise, and noise only ever adds.
DEFAULT_REPS = 5


def neurc() -> Path | None:
    """The release `neurc`, which is the only build worth benchmarking with."""
    candidate = ROOT / "target" / "release" / ("neurc.exe" if sys.platform == "win32" else "neurc")
    return candidate if candidate.exists() else None


def cxx() -> str | None:
    for name in ("clang++", "g++"):
        if shutil.which(name):
            return name
    return None


def run(cmd: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True)


def build_neuro(name: str, level: int) -> tuple[str, Path] | None:
    compiler = neurc()
    if compiler is None:
        return None
    out = BUILD / f"{name}.neuro.O{level}"
    result = run([str(compiler), "compile", str(PROGRAMS / f"{name}.nr"), "-O", str(level), "-o", str(out)])
    if result.returncode != 0:
        print(f"  ! neurc -O{level} failed: {result.stderr.strip().splitlines()[-1:]}")
        return None
    return (f"neuro -O{level}", out)


def build_native(name: str, ext: str, level: int) -> tuple[str, Path] | None:
    """C++ or Rust at `-O{level}`, the same LLVM pipeline level Neuro is built at. Both
    target the generic CPU, as neurc does: no `-march=native` on either side."""
    source = PROGRAMS / f"{name}.{ext}"
    if not source.exists():
        return None
    out = BUILD / f"{name}.{ext}.O{level}"
    if ext == "cpp":
        compiler = cxx()
        cmd = [compiler, f"-O{level}", str(source), "-o", str(out)] if compiler else None
    else:
        compiler = "rustc" if shutil.which("rustc") else None
        cmd = ["rustc", "--edition", "2024", "-C", f"opt-level={level}", str(source), "-o", str(out)]
    if compiler is None:
        print(f"  ({ext} compiler not found, skipping its rows)")
        return None
    result = run(cmd)
    if result.returncode != 0:
        print(f"  ! {compiler} -O{level} failed: {result.stderr.strip().splitlines()[-1:]}")
        return None
    return (f"{compiler} -O{level}", out)


def importable(check: str) -> bool:
    return run([sys.executable, "-c", check]).returncode == 0


def measure(cmd: list[str], reps: int) -> tuple[float, str] | None:
    """Fastest wall time over `reps` runs, plus the stdout every run produced, or
    None for a GPU program on a machine with no usable GPU."""
    best = float("inf")
    output = ""
    for _ in range(reps):
        start = time.perf_counter()
        result = subprocess.run(cmd, capture_output=True, text=True)
        best = min(best, time.perf_counter() - start)
        if result.returncode != 0 and "GPU, and none is usable" in result.stdout + result.stderr:
            return None
        if result.returncode != 0:
            raise SystemExit(f"{cmd[0]} exited {result.returncode}: {result.stderr}")
        output = result.stdout
    return best, output


def bench(name: str, reps: int, levels: list[int]) -> None:
    print(f"\n{name}")
    BUILD.mkdir(exist_ok=True)

    entries: list[tuple[str, list[str]]] = []
    for level in levels:
        built = build_neuro(name, level)
        if built:
            entries.append((built[0], [str(built[1])]))
    for ext in ("cpp", "rs"):
        for level in levels:
            built = build_native(name, ext, level)
            if built:
                entries.append((built[0], [str(built[1])]))
    script = PROGRAMS / f"{name}.py"
    if script.exists():
        entries.append(("python3", [sys.executable, str(script)]))
    script = PROGRAMS / f"{name}_numpy.py"
    if script.exists():
        if importable("import numpy"):
            entries.append(("numpy", [sys.executable, str(script)]))
        else:
            print("  (numpy not installed, skipping its row)")
    script = PROGRAMS / f"{name}_torch.py"
    if script.exists():
        if importable("import torch; assert torch.cuda.is_available()"):
            entries.append(("pytorch", [sys.executable, str(script)]))
        else:
            print("  (pytorch with a usable CUDA GPU not found, skipping its row)")

    if not entries:
        print("  (no runnable implementation)")
        return

    results: list[tuple[str, float]] = []
    outputs: dict[str, str] = {}
    for label, cmd in entries:
        measured = measure(cmd, reps)
        if measured is None:
            print(f"  ({label}: no usable GPU, skipping its row)")
            continue
        results.append((label, measured[0]))
        outputs[label] = measured[1]

    # Every implementation must agree, or the numbers below compare different work.
    distinct = set(outputs.values())
    if len(distinct) > 1:
        print("  ! implementations disagree on output:")
        for label, output in outputs.items():
            print(f"      {label}: {output.strip()[:60]!r}")
        raise SystemExit(1)

    if not results:
        return
    fastest = min(elapsed for _, elapsed in results)
    for label, elapsed in results:
        print(f"  {label:<15}{elapsed * 1000:9.1f} ms   {elapsed / fastest:5.2f}x")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("names", nargs="*", help="benchmarks to run (default: all)")
    parser.add_argument("--reps", type=int, default=DEFAULT_REPS)
    parser.add_argument("--levels", default="0,3", help="comma-separated -O levels for Neuro, C++ and Rust")
    args = parser.parse_args()

    if neurc() is None:
        print("neurc not found; run `cargo build --release` first", file=sys.stderr)
        raise SystemExit(1)

    names = args.names or sorted({p.stem for p in PROGRAMS.glob("*.nr")})
    levels = [int(level) for level in args.levels.split(",")]
    for name in names:
        bench(name, args.reps, levels)


if __name__ == "__main__":
    main()
