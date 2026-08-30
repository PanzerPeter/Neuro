# Benchmarks

A cross-language harness comparing Neuro against C++ and Python on the same
program, so a claim about speed can be checked rather than asserted.

## Running

```bash
cargo build --release          # the harness benchmarks the release neurc
python benchmarks/run.py       # every benchmark
python benchmarks/run.py mandelbrot --reps 9 --levels 0,2,3
```

The harness builds each benchmark with `neurc` and with the system C++ compiler,
runs all three implementations, and **fails if their output differs** — a
benchmark whose implementations have drifted apart is measuring different work.
It reports the fastest of several runs, since noise only ever adds time.

A missing toolchain is skipped with a note, so the harness still runs where only
some of the three are installed.

## Adding a benchmark

Drop three files in `programs/`: `<name>.nr`, `<name>.cpp`, `<name>.py`. They
must compute the same thing and print it identically. Prefer a program whose
result depends on every iteration — a loop the optimizer can fold into a
constant measures the optimizer, not the language.

## Reading the results

The `x` column is time relative to the fastest implementation of that benchmark.
`neuro -O0` is expected to be slow: it selects trapping arithmetic and runs no
optimization pipeline. `neuro -O3` against `c++ -O2` is the comparison that
matters.
