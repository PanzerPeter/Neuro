# Quick Start Guide

A tour of the `neurc` workflow: check a program, compile it, run it, and read the errors it
reports. Build the compiler first with the [Installation Guide](installation.md). The
[Your First Program](first-program.md) tutorial covers the language step by step.

The commands below call `neurc`. Without `cargo install`, run `cargo run -p neurc --` in its
place, or call `target/release/neurc` directly. Run them from the repository root so the example
paths resolve.

## Check a program

`check` parses and type-checks a file without producing a binary:

```bash
neurc check examples/basics/hello.nr
```

```text
Type checking passed for "examples/basics/hello.nr" (1 module(s), 24 HIR items)
```

The HIR count includes the implicit prelude, so it is larger than the number of declarations in
the file. See [`check`](../guides/cli-usage.md#check).

## Compile a program

`compile` writes a native executable beside the source, named after it without the extension:

```bash
neurc compile examples/basics/hello.nr
```

```text
Successfully compiled examples/basics/hello.nr -> examples/basics/hello
```

On Windows the executable is `examples\basics\hello.exe`. Run it and read the exit code:

```bash
./examples/basics/hello                  # Windows: .\examples\basics\hello.exe
echo $?                                  # PowerShell: echo $LASTEXITCODE
```

```text
add(5, 3)    = 8
calculate(8) = 26
```

`echo` prints `26`, the `i32` that `main` returned. On Unix the shell keeps only the low 8 bits of
an exit code. Every example in this repository has its exit code pinned in
[`examples/expected.txt`](../../examples/expected.txt), and whatever it prints is pinned byte for
byte in a sibling `.out` file.

## Run without a binary

`run` compiles the program into a temporary directory, runs it, and exits with the program's own
status. Nothing is left beside the source, so it is the command to reach for while iterating:

```bash
neurc run examples/basics/hello.nr
```

Its output is the program's output alone, with no compiler banner.

## Flags

The commands take a few flags. The [CLI guide](../guides/cli-usage.md) has the full list with
defaults:

```bash
neurc compile -o hello_custom examples/basics/hello.nr   # output path
neurc compile -O2 examples/basics/hello.nr               # optimization level, 0 to 3
neurc compile --emit llvm-ir examples/basics/hello.nr    # textual LLVM IR, not an executable
neurc run -O2 examples/basics/hello.nr
```

The default `-O0` is a debug build, where integer overflow panics. `-O1` and up wrap.

## Errors

Diagnostics go to standard error. A failed command exits with `1`. A command line the compiler
rejects, such as an unknown flag or `-O4`, exits with `2`.

A file `bad.nr` with a missing expression fails to parse:

<!-- expect-error -->
```neuro
func main() -> i32 {
    val x: i32 =
}
```

`neurc check bad.nr` prints:

```text
error: unexpected token RightBrace, expected expression
 --> bad.nr:3:1
  |
3 | }
  | ^

Error: Parsing failed
```

A file `mismatch.nr` parses but does not type-check:

<!-- expect-error -->
```neuro
func main() -> i32 {
    val x: i32 = true
    return x
}
```

`neurc check mismatch.nr` prints:

```text
Type errors found in "mismatch.nr":
error: type mismatch: expected i32, found bool
 --> mismatch.nr:2:5
  |
2 |     val x: i32 = true
  |     ^^^^^^^^^^^^^^^^^

Error: 1 type error(s) found
```

Every type error in the file is reported, not just the first.

## Workflow

Use `check` while you write, since it gives the fastest feedback. Use `run` to try a change, and
`compile` when you want a binary to keep:

```bash
neurc check program.nr
neurc run program.nr
neurc compile program.nr -o program
./program
```

To see each compilation stage, set `RUST_LOG`. See
[environment variables](../guides/cli-usage.md#rust_log).

## See also

- [Your First Program](first-program.md): a step-by-step tutorial
- [Language Reference](../language-reference/types.md): the full language
- [CLI Usage](../guides/cli-usage.md): every command and flag
- [Troubleshooting](../guides/troubleshooting.md): common problems, including linker errors and slow builds
