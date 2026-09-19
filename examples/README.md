# Neuro Example Programs

Runnable `.nr` programs demonstrating the language. Each program asserts itself
two ways, and the test harness checks both:

- the `i32` returned from `main` becomes the process **exit code**, registered in
  [`expected.txt`](expected.txt);
- whatever the program writes to **standard output** is fixed byte for byte in a
  sibling `.out` file: `basics/hello.nr` is pinned by `basics/hello.out`.

Every example currently prints, so every one has a `.out` file. The rule still runs
the other way too: a program that prints nothing has no `.out` file, and that absence
is itself the expectation: its output must stay empty.

Each program explains itself in a header comment. This file says where things are,
not what each one does.

## Layout

| Directory       | What it covers | Reference |
| --------------- | -------------- | --------- |
| `basics/`       | First programs: functions, variables, arithmetic, recursion, inference, `print` / `println` | [Functions](../docs/language-reference/functions.md) |
| `types/`        | Primitives, literal suffixes and separators, casts, overflow, half precision, arrays, tuples, destructuring, newtypes, aliases, `Option` / `Result`, collections, dispatch | [Types](../docs/language-reference/types.md) |
| `strings/`      | `string` literals and slices, `char`, interpolation, triple-quoted blocks, codepoint iteration, the growable `String` | [Strings](../docs/language-reference/strings.md) |
| `tensors/`      | `Tensor<T, [dims]>`: construction, element-wise operators and broadcasting, matrix multiplication, indexing and slicing, shape generics, named dimensions, reshaping, reductions, sorting, dynamic axes | [Tensors](../docs/language-reference/tensors.md) |
| `ownership/`    | Moves, `Copy` / `.clone()`, consuming `self` receivers, immutable and mutable borrows, borrow exclusivity, returned references, deterministic `Drop`, `pool` arena blocks | [Types](../docs/language-reference/types.md#references-immutable-borrows-t) |
| `operators/`    | Bitwise ops, compound assignment, integer intrinsics, operator overloading, `??` coalescing, `?` propagation | [Operators](../docs/language-reference/operators.md) |
| `control_flow/` | `if` / `else`, `for` over ranges and adapters, the iterator protocol, `while`, `loop`, block and `unsafe` expressions, `match`, `val-else`, panics, lints | [Control Flow](../docs/language-reference/control-flow.md) |
| `structs/`      | Struct definition, field access and mutation, `&self` and `&mut self` methods, derives | [Structs](../docs/language-reference/structs.md) |
| `modules/`      | Multi-file programs, `mod.nr` directory modules, qualified paths, `import`, inline `module` blocks, re-export facades, the prelude and its opt-out | [Modules](../docs/language-reference/modules.md) |
| `showcase/`     | Bigger programs proving many features work **together** | see the index below |

The single source of truth for each program's expected exit code is
[`expected.txt`](expected.txt); for its expected output, the sibling `.out` file.
A multi-file program registers its root with an exit code and each of its other
modules with the marker `module`: those have no `main` of their own and are
compiled as part of the root that reaches into them, so only the root has output
of its own to pin.

## Compiling and running

```bash
# Type-check only
cargo run -p neurc -- check examples/basics/hello.nr

# Compile to an executable (choose an output path outside the source tree)
cargo run -p neurc -- compile examples/basics/hello.nr -o /tmp/hello
/tmp/hello; echo "exit: $?"

# What it prints is exactly what the golden file holds
/tmp/hello | diff - examples/basics/hello.out && echo "output matches"
```

> Compiled binaries are git-ignored under `examples/`, but prefer `-o /tmp/...`
> so you never leave artifacts in the source tree.

## Testing

Every example is compiled and run by a single integration test that **discovers
files automatically**:

```bash
cargo test --workspace                 # runs all examples (among everything else)
cargo test -p neurc --test examples    # just the example harness
```

The harness ([`compiler/neurc/tests/examples.rs`](../compiler/neurc/tests/examples.rs))
walks `examples/` recursively, compiles and runs every `.nr` file, and asserts both
its exit code against [`expected.txt`](expected.txt) and its standard output against
the sibling `.out` file. It fails if:

- a `.nr` file on disk has **no** entry in `expected.txt` (forces registration),
- an entry in `expected.txt` points at a file that **doesn't exist** (stale),
- a `.out` file has no `.nr` beside it (stale golden file),
- any example's exit code **doesn't match** its registered value,
- any example's output **doesn't match** its `.out` file, including a silent
  example that starts printing, which has no `.out` and so must print nothing.

An output mismatch prints the first differing line and then both texts in full,
with every line quoted so trailing whitespace stays visible.

## Adding an example

1. Drop a `.nr` file into the topic directory it belongs to (create a new
   directory if no topic fits), and open it with a comment saying what it shows.
2. Add one line to [`expected.txt`](expected.txt), in that directory's section:
   `path/from/examples.nr  <exit-code>`.
3. If it prints, save exactly what it prints beside it as `<name>.out`:
   `cargo run -p neurc -- compile examples/<name>.nr -o /tmp/ex && /tmp/ex > examples/<name>.out`.
   Read that file before committing it: it is an assertion, so it is only worth
   having if the text in it is the text the program *should* produce.
4. Run `cargo test -p neurc --test examples`.

No Rust edits are needed: discovery is automatic.

## Showcase index

One line each. The program's own header comment is the full description.

- [`batch_arena.nr`](showcase/batch_arena.nr): a batched forward pass run inside nested `pool` arenas, with a `PoolAware` scratch type the arena sweeps in reverse construction order, and a summary line a declared function builds so it may be kept past the block
- [`borrow_discipline.nr`](showcase/borrow_discipline.nr): `&mut self` methods, arrays and interpolation written around a live borrow, which freezes the borrowee's own name and forbids moving out from under it
- [`borrowed_text.nr`](showcase/borrowed_text.nr): explicit lifetime annotations over borrowed text
- [`buffered_report.nr`](showcase/buffered_report.nr): a shift report long enough to exercise buffered stdout
- [`closures.nr`](showcase/closures.nr): closures and higher-order functions
- [`config_manifest.nr`](showcase/config_manifest.nr): a config manifest rendered from typed records
- [`derived_records.nr`](showcase/derived_records.nr): derived `Debug` and `PartialEq` over earlier features
- [`displaced_owners.nr`](showcase/displaced_owners.nr): a reassigned binding releasing the value it displaces, across a `Drop` type, a heap `string` and a `Vec`
- [`enum_records.nr`](showcase/enum_records.nr): pattern matching over enums, structs, methods and arrays
- [`field_report.nr`](showcase/field_report.nr): standard I/O driving a field report
- [`generic_toolkit.nr`](showcase/generic_toolkit.nr): generics, const generics, turbofish and `where` clauses together, plus a non-`Copy` type argument and `Drop` across a generic boundary
- [`grid_update.nr`](showcase/grid_update.nr): a heat grid whose every mutation names the storage it writes to: a field, a field of a field, a nested array element, a `Vec` element and a tensor coordinate
- [`held_destruction.nr`](showcase/held_destruction.nr): a value held in a struct field, an array or tuple element, an enum payload or a newtype, destroyed with its holder and exactly once when one element is given up first
- [`inventory_ledger.nr`](showcase/inventory_ledger.nr): the standard collections carrying an inventory ledger
- [`job_queue.nr`](showcase/job_queue.nr): `val-else` early exit carrying a small job queue
- [`log_builder.nr`](showcase/log_builder.nr): a run transcript assembled in one growable `String`, finished by a consuming `self` method
- [`model_shapes.nr`](showcase/model_shapes.nr): a network's layer stack declared with real tensor parameters
- [`mutable_borrows.nr`](showcase/mutable_borrows.nr): mutable borrows `&mut T` and the dereference operator `*`
- [`named_axes.nr`](showcase/named_axes.nr): a batch of token embeddings with every tensor axis named
- [`num_algorithms.nr`](showcase/num_algorithms.nr): a tiny integer-math toolkit
- [`optimizer_step.nr`](showcase/optimizer_step.nr): a weight update written in place and by value, all three broadcast forms, plus the forward pass `@` makes of it
- [`owned_aggregates.nr`](showcase/owned_aggregates.nr): arrays, tuples, enum payloads, newtypes and a generic struct all holding an owned `string`, destructured element by element
- [`owned_catalog.nr`](showcase/owned_catalog.nr): a catalog whose `Vec`, `HashMap` and `BTreeMap` own the text they hold, closing the value model with the rest of its sub-phase
- [`perceptron.nr`](showcase/perceptron.nr): a two-neuron feed-forward pass
- [`ranked_batch.nr`](showcase/ranked_batch.nr): ordering a tensor axis alongside the rest of the tensor surface
- [`ranked_finish.nr`](showcase/ranked_finish.nr): `.enumerate()` carrying a position through earlier features
- [`render_settings.nr`](showcase/render_settings.nr): a render pipeline configured by named arguments
- [`replay_buffer.nr`](showcase/replay_buffer.nr): range `.rev()` driving a replay buffer
- [`running_stats.nr`](showcase/running_stats.nr): an online mean accumulator
- [`sample_audit.nr`](showcase/sample_audit.nr): `?` error propagation threaded through earlier features
- [`scan_guard.nr`](showcase/scan_guard.nr): deterministic `Drop` and labeled loop exit together
- [`sensor_pipeline.nr`](showcase/sensor_pipeline.nr): `Option` / `Result` over structs, methods, arrays and generics
- [`sensor_windows.nr`](showcase/sensor_windows.nr): windowed sensor readings behind one slice signature
- [`shape_traits.nr`](showcase/shape_traits.nr): trait declarations and both dispatch forms together
- [`simulation.nr`](showcase/simulation.nr): a tiny bit-flag state machine
- [`status_report.nr`](showcase/status_report.nr): a formatted status report built from live readings
- [`stored_text.nr`](showcase/stored_text.nr): text stored into a struct field, an array or tuple element, a call's argument and a call's return value, each released by whoever stored it
- [`stream_pipeline.nr`](showcase/stream_pipeline.nr): the iteration protocol carrying a small stream pipeline
- [`telemetry/main.nr`](showcase/telemetry/main.nr): multi-file compilation and `import` over prior features
- [`transient_text.nr`](showcase/transient_text.nr): headings, rows and comparisons built from strings nothing ever binds, released where they are read
- [`typed_channels.nr`](showcase/typed_channels.nr): associated types, one trait with three implementors
- [`unit_types.nr`](showcase/unit_types.nr): newtype units of measure over structs, enums and methods
- [`vector_physics.nr`](showcase/vector_physics.nr): operator traits driving a vector physics step
- [`word_scanner.nr`](showcase/word_scanner.nr): the codepoint iterators driving a small tokenizer

## See also

- [Language Reference](../docs/language-reference/types.md), and the [documentation index](../docs/README.md)
- [Known Bugs](../docs/BUGS.md): what is currently broken
- [CHANGELOG](../CHANGELOG.md): what each release added
