# Standard Library: Files, Weights, Command Line, Input and Time

The standard library is Neuro source compiled with every program (see
[Modules](modules.md#the-implicit-prelude)). Five of its modules reach the operating system:

| Module | What it holds |
| --- | --- |
| `std::fs` | Whole-file reads and writes, and removing a file |
| `std::safetensors` | Tensors in `.safetensors` files, the format PyTorch, JAX and Hugging Face share |
| `std::env` | The command line and environment variables |
| `std::io` | `read_line`, for standard input. Output is `print` / `println` |
| `std::time` | `Instant` and `Duration`, on a monotonic clock |

Each call is written with its path (`std::fs::read_to_string(...)`), or brought in with an
`import` like any other module's item. A program that calls none of them links none of the runtime
behind them.

## Errors: `IoError`

Every file and stream call fails with one type, so `?` carries a failure from any of them out of a
function that returns `Result<_, IoError>`. Both types are in the prelude:

```neuro
export enum IoErrorKind { NotFound, PermissionDenied, InvalidData, Other }

export struct IoError {
    export kind: IoErrorKind,
    export message: string,
}
```

`kind` is what a program branches on. `message` is written for a person and names the operation,
the file and the system's reason, as in `cannot read data.txt: No such file or directory`. The
reason comes from the operating system, so its wording varies between systems. The variants are
written with the enum's name (`IoErrorKind::NotFound`), as `ParseError`'s are, and `{e:?}` prints an
`IoError` with both fields.

## Files: `std::fs`

```neuro
std::fs::read_to_string(path: &string) -> Result<string, IoError>
std::fs::read_bytes(path: &string) -> Result<Vec<u8>, IoError>
std::fs::write(path: &string, text: &string) -> Result<(), IoError>
std::fs::write_bytes(path: &string, bytes: &[u8]) -> Result<(), IoError>
std::fs::remove_file(path: &string) -> Result<(), IoError>
```

A path is a `&string`, so a literal is passed as it is. A relative path starts at the working
directory. `write` and `write_bytes` create the file, or replace what it held.

```neuro
func load_settings(path: &string) -> Result<string, IoError> {
    std::fs::write(path, "epochs = 3\n")?
    std::fs::read_to_string(path)
}
```

| Failure | Kind |
| --- | --- |
| The file or a directory on its path does not exist | `NotFound` |
| The system refused access | `PermissionDenied` |
| `read_to_string` found bytes that are not UTF-8 | `InvalidData` |
| The path holds a NUL byte, or anything else | `Other` |

`read_bytes` accepts any file. A `Vec<u8>` coerces to the `&[u8]` that `write_bytes` takes.

## Weights: `std::safetensors`

A `.safetensors` file is a JSON header naming each tensor's dtype, shape and byte range, followed by
the raw little-endian data. Neuro reads files PyTorch and NumPy wrote, and they read Neuro's.

```neuro
func load(path: &string) -> Result<f32, IoError> {
    val file = std::safetensors::open(path)?
    val w: Tensor<f32, [2, 3]> = file.tensor("layer.w")?
    val steps: Tensor<i64, [1]> = file.tensor("steps") ?? Tensor::zeros()

    mut out = std::safetensors::Writer::new()
    out.add("layer.w", &w)
    out.add_metadata("note", "copied")
    out.save("copy.safetensors")?
    Result::Ok(w.sum())
}
```

| Call | Result |
| --- | --- |
| `std::safetensors::open(path)` | `Result<File, IoError>`: the file, its header checked |
| `file.tensor(name)` | `Result<Tensor<T, S>, IoError>`, a copy, typed by its context |
| `file.metadata(key)` | `Option<string>`, from the header's `__metadata__` |
| `Writer::new()` | An empty writer |
| `out.add(name, &tensor)` | Copies a tensor of any element type and shape in; a repeated name replaces |
| `out.add_metadata(key, value)` | Sets one metadata entry |
| `out.save(path)` | `Result<(), IoError>`: writes the file, or replaces it |

`open` checks the whole header before trusting any of it: the length, the JSON, each dtype, and
that the byte ranges match each tensor's size and cover the data exactly. A file that fails is an
`InvalidData` error naming the first problem. The dtypes are the language's element types:
`BOOL`, `U8` to `U64`, `I8` to `I64`, `F16`, `BF16`, `F32` and `F64`.

`file.tensor(name)` takes its tensor type from what it is bound to, through `?`, `??`, or a
`val ... else` annotated with the whole `Result` (see
[Control flow](control-flow.md#annotating-the-scrutinee)). With nothing to read a type from, it
is a compile error. The stored dtype and shape must match that type exactly; otherwise the read is
an `Err` that names the tensor and both layouts. A name the file does not hold is `NotFound`. The
shape is fixed for now: a `?` extent is refused.

The result is an ordinary tensor that owns its buffer, so it can be moved to a GPU with `.to`,
updated in place, or trained. `out.add` reads a GPU tensor back before copying it. `save` writes
the metadata first, then the tensors in the order their names were first added.

## Command line and environment: `std::env`

```neuro
std::env::args() -> Vec<string>
std::env::var(name: &string) -> Option<string>
```

`args()` is the command line, with the program's own name first, so the first argument is at index
1. `var` returns `None` for a variable that is not set. Neither fails: a byte that does not begin
valid UTF-8 reads as U+FFFD.

```neuro
val args = std::env::args()
val epochs = args.get(1).map(|a| a.parse::<i32>() ?? 10) ?? 10
val mode = std::env::var("MODE") ?? "train"
```

`neurc run` passes everything after `--` to the program:

```bash
neurc run train.nr -- 20
```

## Standard input: `std::io::read_line`

```neuro
std::io::read_line() -> Result<Option<string>, IoError>
```

Each call returns the next line without its `\n` or `\r\n`, and `Ok(None)` once input is
exhausted. A last line with no terminator is still returned. A line that is not UTF-8 is an `Err`
of kind `InvalidData`. Buffered standard output is drained before the read, so a prompt written
with `print` is visible while the program waits:

```neuro
print("name? ")
loop {
    match std::io::read_line() {
        Ok(Some(line)) => println("hello, {line}"),
        Ok(None) => break,
        Err(e) => panic(e.message),
    }
}
```

## Time: `std::time`

`Instant` is one reading of a monotonic clock, which a change to the wall clock does not move.
`Duration` is the span between two readings, to the nanosecond. Both are `Copy`.

| Call | Result |
| --- | --- |
| `Instant::now()` | The current reading |
| `i.elapsed()` | The `Duration` since `i` |
| `later.duration_since(&earlier)` | The `Duration` between them, zero if `earlier` is the later one |
| `d.as_nanos()`, `d.as_millis()` | The span as a `u64` |
| `d.as_secs_f64()` | The span in seconds, as an `f64` |

```neuro
val start = std::time::Instant::now()
train(&mut model)
println("took {start.elapsed().as_secs_f64():.3} s")
```

`std::time` does not read the wall-clock date.

## See also

- [`examples/system/files.nr`](../../examples/system/files.nr): reading and writing a file, and
  branching on an `IoError`'s kind
- [`examples/system/command_line.nr`](../../examples/system/command_line.nr): arguments, a variable,
  standard input and timing
- [`examples/system/weights.nr`](../../examples/system/weights.nr): saving tensors, reading them back
  typed by their annotations, and a read that does not match
- [`examples/showcase/checkpoint_resume.nr`](../../examples/showcase/checkpoint_resume.nr): a training
  run that checkpoints its weights and resumes to the same result
- [`examples/integration/training/run_journal.nr`](../../examples/integration/training/run_journal.nr): a run that saves its
  journal, reloads it and resumes
- [Strings](strings.md#parsing-text): `.parse::<T>()`, for turning text read from a file into values
