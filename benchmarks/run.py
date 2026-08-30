#!/usr/bin/env python3
"""Cross-language benchmark harness: Neuro vs C++ vs Python.

Each benchmark is a triple of programs — `programs/<name>.nr`, `<name>.cpp`,
`<name>.py` — that compute the same result and print it identically. The harness
builds the compiled ones, checks all three agree on stdout, then times each and
reports wall time relative to the fastest.

Agreement is checked before any timing: a benchmark that has drifted apart
between languages measures nothing, so a mismatch is a failure, not a footnote.

Usage:
    python benchmarks/run.py                  # every benchmark, default levels
    python benchmarks/run.py mandelbrot       # one benchmark
    python benchmarks/run.py --reps 9         # more repetitions (min is reported)
    python benchmarks/run.py --levels 0,2,3   # which -O levels to build Neuro at

Requires `neurc` (built with `cargo build --release`), a C++ compiler, and
python3. A language whose toolchain is missing is skipped with a note rather
than failing the run.
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


def build_cpp(name: str, flag: str) -> tuple[str, Path] | None:
    compiler = cxx()
    source = PROGRAMS / f"{name}.cpp"
    if compiler is None or not source.exists():
        return None
    out = BUILD / f"{name}.cpp.{flag.lstrip('-')}"
    result = run([compiler, flag, str(source), "-o", str(out)])
    if result.returncode != 0:
        print(f"  ! {compiler} {flag} failed: {result.stderr.strip().splitlines()[-1:]}")
        return None
    return (f"c++ {flag}", out)


def measure(cmd: list[str], reps: int) -> tuple[float, str]:
    """Fastest wall time over `reps` runs, plus the stdout every run produced."""
    best = float("inf")
    output = ""
    for _ in range(reps):
        start = time.perf_counter()
        result = subprocess.run(cmd, capture_output=True, text=True)
        best = min(best, time.perf_counter() - start)
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
    for flag in ("-O0", "-O2"):
        built = build_cpp(name, flag)
        if built:
            entries.append((built[0], [str(built[1])]))
    script = PROGRAMS / f"{name}.py"
    if script.exists():
        entries.append(("python3", [sys.executable, str(script)]))

    if not entries:
        print("  (no runnable implementation)")
        return

    results: list[tuple[str, float]] = []
    outputs: dict[str, str] = {}
    for label, cmd in entries:
        elapsed, output = measure(cmd, reps)
        results.append((label, elapsed))
        outputs[label] = output

    # Every implementation must agree, or the numbers below compare different work.
    distinct = set(outputs.values())
    if len(distinct) > 1:
        print("  ! implementations disagree on output:")
        for label, output in outputs.items():
            print(f"      {label}: {output.strip()[:60]!r}")
        raise SystemExit(1)

    fastest = min(elapsed for _, elapsed in results)
    for label, elapsed in results:
        print(f"  {label:<12}{elapsed * 1000:9.1f} ms   {elapsed / fastest:5.2f}x")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("names", nargs="*", help="benchmarks to run (default: all)")
    parser.add_argument("--reps", type=int, default=DEFAULT_REPS)
    parser.add_argument("--levels", default="0,3", help="comma-separated Neuro -O levels")
    args = parser.parse_args()

    if neurc() is None:
        print("neurc not found — run `cargo build --release` first", file=sys.stderr)
        raise SystemExit(1)

    names = args.names or sorted({p.stem for p in PROGRAMS.glob("*.nr")})
    levels = [int(level) for level in args.levels.split(",")]
    for name in names:
        bench(name, args.reps, levels)


if __name__ == "__main__":
    main()
