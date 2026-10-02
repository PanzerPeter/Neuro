# Benchmarks

A cross-language harness comparing Neuro against C++, Python and NumPy on the
same program, so a claim about speed can be checked rather than asserted.

## Running

```bash
cargo build --release          # the harness benchmarks the release neurc
python benchmarks/run.py       # every benchmark
python benchmarks/run.py mandelbrot --reps 9 --levels 0,2,3
```

The harness builds each benchmark with `neurc` and with the system C++ compiler,
runs every implementation, and **fails if their output differs**. A
benchmark whose implementations have drifted apart is measuring different work.
It reports the fastest of several runs, since noise only ever adds time.

A missing toolchain is skipped with a note, so the harness still runs where only
some of them are installed.

The `gpu_*` benchmarks run their Neuro side as `@gpu` kernels, which needs a
`neurc` built with the MLIR backend and an NVIDIA GPU:

```bash
cargo build --release -p neurc --features mlir   # see the installation guide for MLIR
python benchmarks/run.py gpu_matmul gpu_reduce gpu_relax
```

Without either, the Neuro rows are skipped with a note. They have no C++ row: a
CPU loop says nothing about a GPU kernel. Their comparison is NumPy on the CPU.

## Adding a benchmark

Drop the files in `programs/`: `<name>.nr`, and any of `<name>.cpp`,
`<name>.py` and `<name>_numpy.py`. They must compute the same thing and print it
identically. The plain `.py` is the language itself; add a `_numpy.py` where the
work vectorizes, since that is what a Python programmer would write there. Prefer a program whose
result depends on every iteration. A loop the optimizer can fold into a
constant measures the optimizer, not the language.

## Reading the results

The `x` column is time relative to the fastest implementation of that benchmark.
`neuro -O0` is expected to be slow: it selects trapping arithmetic and runs no
optimization pipeline. `neuro -O3` against `c++ -O2` is the comparison that
matters.

The NumPy rows are only as fast as the BLAS library NumPy links. A distribution
package may link the reference BLAS, which is about a hundred times slower at a
large matrix product than an optimized one. The PyPI wheel bundles OpenBLAS, so
run the harness from a virtual environment to compare against NumPy at its best:

```bash
python -m venv ~/.venvs/bench && ~/.venvs/bench/bin/pip install numpy
~/.venvs/bench/bin/python benchmarks/run.py
```
