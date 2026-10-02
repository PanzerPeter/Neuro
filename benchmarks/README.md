# Benchmarks

A cross-language harness comparing Neuro against C++, Rust, Python, NumPy and PyTorch on
the same program, so a claim about speed can be checked rather than asserted.

## Running

```bash
cargo build --release          # the harness benchmarks the release neurc
cd benchmarks
uv run python run.py           # every benchmark
uv run python run.py mandelbrot --reps 9 --levels 0,2,3
```

`uv run` builds `benchmarks/.venv` from `pyproject.toml` on first use: Python 3.14, NumPy
and PyTorch, all PyPI wheels. That matters for NumPy, whose rows are only as fast as the BLAS
library it links. A distribution package may link the reference BLAS, about a hundred times
slower at a large matrix product; the wheel bundles OpenBLAS. PyTorch's Linux wheel bundles
CUDA, so it needs only the NVIDIA driver.

The harness builds each benchmark with `neurc`, the system C++ compiler (`clang++`, else
`g++`) and `rustc`, runs every implementation, and **fails if their output differs**. A
benchmark whose implementations have drifted apart is measuring different work. It reports the
fastest of several runs, since noise only ever adds time. A missing toolchain is skipped with a
note, so the harness still runs where only some of them are installed.

The `gpu_*` benchmarks run their Neuro side as `@gpu` kernels, which needs a Linux `neurc` and
an NVIDIA GPU; `gpu_mlp` also needs the CUDA toolkit when compiling, for libdevice. Without
them the Neuro rows are skipped with a note. They have no C++ or Rust row: a CPU loop says
nothing about a GPU kernel. Their comparisons are PyTorch on the same GPU and NumPy on the CPU.

## Adding a benchmark

Drop the files in `programs/`: `<name>.nr`, and any of `<name>.cpp`, `<name>.rs`,
`<name>.py`, `<name>_numpy.py` and `<name>_torch.py`. They must compute the same thing and
print it identically. Write each one the way a programmer of that language would write it for
speed: the i-k-j loop order in a C++ matrix product, a `BufWriter` around Rust's bulk output,
builtins over index loops in Python. The plain `.py` is the language itself; add a
`_numpy.py` where the work vectorizes, since that is what a Python programmer would write
there, and a `_torch.py` for a GPU benchmark, moving the same data between host and device as
the Neuro side does. Prefer a program whose result depends on every iteration. A loop the
optimizer can fold into a constant measures the optimizer, not the language.

## Reading the results

The `x` column is time relative to the fastest implementation of that benchmark. Neuro, C++
and Rust are built at the same `-O` levels, so `neuro -O3`, `clang++ -O3` and `rustc -O3` all
run LLVM's `-O3` pipeline, for the generic CPU of the target. `neuro -O0` is expected to be
slow: it selects trapping arithmetic and runs no optimization pipeline, much like `rustc -O0`
with its debug assertions.

Every row is a whole-program wall time, startup included. That is a fixed cost worth knowing
on the GPU rows: about 220 ms for a Neuro program, most of it the CUDA driver, and about
1100 ms for PyTorch's import and CUDA setup, which is most of each PyTorch row.
