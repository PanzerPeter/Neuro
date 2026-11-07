# Neuro Example Programs

Runnable `.nr` programs, grouped by topic. The example test harness compiles and runs each
one and checks two things:

- the `i32` returned from `main` becomes the process **exit code**, registered in
  [`expected.txt`](expected.txt);
- what the program writes to **standard output** matches a sibling `.out` file byte for
  byte: `basics/hello.nr` is pinned by `basics/hello.out`. A program with no `.out` file
  must print nothing. Every program with a `main` has one today.

## Layout

| Directory       | What it covers | Reference |
| --------------- | -------------- | --------- |
| `basics/`       | First programs: functions, variables, arithmetic, recursion, inference, `print` / `println` | [Functions](../docs/language-reference/functions.md) |
| `types/`        | Primitives, literal suffixes and separators, casts, overflow, half precision, arrays, tuples, destructuring, newtypes, aliases, enum methods and trait impls, `Option` / `Result`, collections, dispatch | [Types](../docs/language-reference/types.md), [Generics and traits](../docs/language-reference/generics-and-traits.md) |
| `strings/`      | `string` literals and slices, `char`, interpolation, triple-quoted blocks, codepoint iteration, `.parse::<T>()`, the growable `StringBuilder` | [Strings](../docs/language-reference/strings.md) |
| `tensors/`      | `Tensor<T, [dims]>`: construction, element-wise operators and broadcasting, matrix multiplication, indexing and slicing, shape generics, named dimensions, reshaping, reductions, sorting, Einstein notation, functional traversals, elementwise math, dynamic axes, `@gpu` kernels, hand-written `@kernel` functions and their `partition` write form, device transfer with `.to` and the operations that then run on the device | [Tensors](../docs/language-reference/tensors.md) |
| `ownership/`    | Moves, `Copy` / `.clone()`, consuming `self` receivers, immutable and mutable borrows, borrow exclusivity, returned references, collections of owned values, deterministic `Drop`, `pool` arena blocks | [Ownership](../docs/language-reference/ownership.md) |
| `operators/`    | Bitwise ops, compound assignment, integer intrinsics, operator overloading, `??` coalescing, `?` propagation, `\|>` pipelines, `>>` composition | [Operators](../docs/language-reference/operators.md) |
| `control_flow/` | `if` / `else`, `for` over ranges and adapters, the iterator protocol, `while`, `loop`, block and `unsafe` expressions, `match` and its nested, `@` and array patterns, `val-else`, `if val` / `while val`, panics, lints | [Control Flow](../docs/language-reference/control-flow.md) |
| `structs/`      | Struct definition, field access and mutation, `&self` and `&mut self` methods, generic methods, supertraits, `Self`, derives, hand-written `Clone` / `Copy` / `Debug` and the bounds that name them | [Structs and Enums](../docs/language-reference/structs.md) |
| `modules/`      | Multi-file programs, `mod.nr` directory modules, qualified paths, `import`, inline `module` blocks, re-export facades, the prelude and its opt-out | [Modules](../docs/language-reference/modules.md) |
| `showcase/`     | Bigger programs proving many features work **together** | see the index below |

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

Compiled binaries under `examples/` are git-ignored, but `-o` pointing outside the tree
keeps them out of the source tree entirely.

## Testing

One integration test, [`compiler/neurc/tests/examples.rs`](../compiler/neurc/tests/examples.rs),
compiles and runs every example. It finds the programs by walking `examples/` recursively:

```bash
cargo test --workspace                 # runs all examples (among everything else)
cargo test -p neurc --test examples    # just the example harness
```

Each `expected.txt` line is a path and a marker. The marker is an exit code, optionally
followed by `gpu`, or the word `module` in place of the exit code:

- `module` marks a non-root module of a multi-file program. It has no `main` of its own,
  so the harness records it and does not run it alone. The root program that uses it
  compiles it.
- `gpu` marks a program with bare `@gpu` or `@kernel` functions. Only a Linux compiler
  builds it, and only a machine with an NVIDIA GPU runs it. Where a GPU runs it, the exit
  code and output are checked like any other program. Where no GPU is usable, it must abort
  at startup with the runtime's `panic:` diagnostic and print nothing. Off Linux, it must
  type-check. A program whose `@gpu` functions all take `fallback: true` runs everywhere and
  has no marker.

The harness collects every problem across all examples and reports them together. It fails
when:

- a `.nr` file has no line in `expected.txt`;
- a line in `expected.txt` names a file that does not exist;
- a `.out` file has no `.nr` beside it;
- a program fails to compile, or is killed by a signal;
- the exit code differs from the registered one;
- the output differs from the `.out` file, or a program with no `.out` file prints anything.

A malformed `expected.txt` line (a duplicate path, a bad exit code, an unknown marker)
fails the run before any example is checked. A stdout mismatch shows the first differing
line, then both texts in full with every line quoted, so trailing whitespace is visible.

## Adding an example

1. Drop a `.nr` file into the topic directory it fits, or create a new directory if none
   does. Open it with a header comment saying what it shows.
2. Add one line to [`expected.txt`](expected.txt), in that directory's section:
   `path/from/examples.nr  <exit-code>`.
3. If it prints, save its output beside it as `<name>.out`:
   `cargo run -p neurc -- compile examples/<name>.nr -o /tmp/ex && /tmp/ex > examples/<name>.out`.
   Read the file before committing it. It is an assertion, so it is only worth having if the
   text is what the program should produce.
4. Run `cargo test -p neurc --test examples`. No Rust edits are needed.

## Showcase index

Each showcase program with a `main` is listed once, under its theme. The header comment
in each program has the full description. The three module files under
`showcase/telemetry/` have no `main` and are not listed.

### Training and autodiff

- [`activation_fit.nr`](showcase/activation_fit.nr): One neuron trained under two activations passed as values into a `@grad` loss, using closures and a `>>` composition.
- [`composed_loss.nr`](showcase/composed_loss.nr): Two `@grad` losses built from helper functions, one shape-generic and one branching, evaluated in a `pool` over candidate weights.
- [`frozen_features.nr`](showcase/frozen_features.nr): Fine-tuning only a model's head with `@grad(wrt: [...])`, so `.backward()` fills the head's gradients while the feature layer stays fixed.
- [`gradient_loss.nr`](showcase/gradient_loss.nr): A least-squares fit by gradient descent: `@grad`, `.backward()`, `.grad()`, `.zero_grad()` and `.item()` in a `pool` per step.
- [`logistic_fit.nr`](showcase/logistic_fit.nr): Logistic regression with an elastic-net penalty, trained by its own gradient, with `.tanh()`, `.log()`, `.abs()` and `.pow(2.0)` in the loss.
- [`newton_fit.nr`](showcase/newton_fit.nr): One logistic fit solved by Newton's method (`@grad(order: 2)` and `.hessian()`) and by gradient descent, for comparison.
- [`ridge_objective.nr`](showcase/ridge_objective.nr): A training objective carried as a value, whose `@grad` loss method reads a ridge penalty from a constant receiver.
- [`robust_fit.nr`](showcase/robust_fit.nr): A `@grad` loss with a `while` loop and an `if` expression, where the derivative follows the path each call took.
- [`td_value_fit.nr`](showcase/td_value_fit.nr): Temporal-difference learning, with the bootstrapped target fenced by `.detach()` or `@no_grad` and compared with the unfenced residual gradient.

### GPU and kernels

- [`gpu_fallback.nr`](showcase/gpu_fallback.nr): A two-layer classifier with `@gpu(fallback: true)` layers that run as kernels where a GPU is usable and on the CPU otherwise, with identical output.
- [`gpu_layer.nr`](showcase/gpu_layer.nr): A dense layer whose forward pass runs as `@gpu` kernels on device-resident weights, checked against the host. Stops without a usable NVIDIA GPU.
- [`heat_diffusion.nr`](showcase/heat_diffusion.nr): Heat along a rod, one GPU thread per cell in a `@kernel` with `partition` writes, checked against a host run. Stops without a usable NVIDIA GPU.
- [`train_predict.nr`](showcase/train_predict.nr): A `@grad` fit trained on the CPU, then a `@gpu` prediction from the learned weights. The program in the README demo. Stops without a usable NVIDIA GPU.

### Tensors

- [`attention_head.nr`](showcase/attention_head.nr): One scaled dot-product attention head written as two `einsum` contractions, with a `>>` composition rescaling the scores.
- [`mixed_precision.nr`](showcase/mixed_precision.nr): `bf16` updates that round back to their weight beside an `f32` master copy that moves, with `.topk` and `.to_checked` over `bf16` data.
- [`model_shapes.nr`](showcase/model_shapes.nr): A network's layer stack declared with real tensor parameters, where shapes are part of the types and a second owner of a tensor needs `.clone()`.
- [`named_axes.nr`](showcase/named_axes.nr): A batch of token embeddings with every axis named, so an argument with its axes swapped is a compile error.
- [`optimizer_step.nr`](showcase/optimizer_step.nr): A weight update written in place (`w -= g`) and by value (`w = w - g`), with broadcasting and an `@` forward pass.
- [`ranked_batch.nr`](showcase/ranked_batch.nr): Ordering a tensor axis with `.sort()`, `.argsort()` and `.topk(k:, axis:)`; an argsort index reads back into the tensor it ranks.
- [`replay_buffer.nr`](showcase/replay_buffer.nr): A replay buffer read newest first with `.rev()` and `.step(n)` on a `for` head and a tensor index axis.

### Ownership, borrows and Drop

- [`batch_arena.nr`](showcase/batch_arena.nr): A batched forward pass in nested `pool` arenas, with a `PoolAware` type and rules for which values may cross a block.
- [`borrow_discipline.nr`](showcase/borrow_discipline.nr): What a live `&mut` forbids (reading, writing or moving the borrowed place by name), and the code written around it.
- [`borrowed_records.nr`](showcase/borrowed_records.nr): Structs and an enum holding `&'a string` views into strings `main` owns, read with `match`, `??`, `.map` and a `&mut` field.
- [`borrowed_text.nr`](showcase/borrowed_text.nr): Lifetime annotations on borrowed text: `<'a>` on parameters and returns, with `.slice` and `.char_slice` views.
- [`displaced_owners.nr`](showcase/displaced_owners.nr): Reassigning a binding releases the value it displaces, counted through a `Drop` type, a `string` and a `Vec<string>`.
- [`grid_update.nr`](showcase/grid_update.nr): A heat grid updated in place, each mutation naming its storage: a field, a tuple element, an array element, a `Vec` element or a tensor cell.
- [`held_destruction.nr`](showcase/held_destruction.nr): Values held in a struct field, an element, an enum payload or a newtype, each destroyed with its holder, exactly once.
- [`incident_desk.nr`](showcase/incident_desk.nr): A `Vec` and `HashMap` owning non-`Copy` tickets, with `&mut self` methods on slots, `get_mut`, and a `Drop` that fires once per ticket.
- [`mutable_borrows.nr`](showcase/mutable_borrows.nr): Mutable borrows `&mut T` and the dereference `*`, where writes through the borrow reach the caller's storage.
- [`owned_aggregates.nr`](showcase/owned_aggregates.nr): Arrays, tuples, enum payloads, newtypes and a generic struct holding an owned `string`, destructured one element at a time.
- [`owned_catalog.nr`](showcase/owned_catalog.nr): A catalog whose `Vec`, `HashMap` and `BTreeMap` own their text: `push` and `insert` move values in, and `.clone()` keeps a copy.
- [`stored_text.nr`](showcase/stored_text.nr): Text stored in a struct field, an element, a return value or an argument, each released by the holder that stores it.
- [`transient_text.nr`](showcase/transient_text.nr): Strings nothing binds to a name (`+` chains, interpolations, `StringBuilder::to_string`), released where they are read.

### Collections, iteration and text

- [`buffered_report.nr`](showcase/buffered_report.nr): A shift report long enough to drain the stdout buffer several times, pinning the output order across drains.
- [`config_manifest.nr`](showcase/config_manifest.nr): A config manifest rendered from typed records, with triple-quoted block strings dedented to their closing `"""`.
- [`field_report.nr`](showcase/field_report.nr): Standard I/O and iteration: `&[T]` parameters that accept an array or a `Vec`, a user `Iterator`, and `.char_indices()`.
- [`inventory_ledger.nr`](showcase/inventory_ledger.nr): A ledger on `Vec`, `HashMap` and `BTreeMap`, walked with `for` in all three receiver modes: by value, `&` and `&mut`.
- [`log_builder.nr`](showcase/log_builder.nr): A run transcript built in one growable `StringBuilder`, finished by a consuming `self` method.
- [`ranked_finish.nr`](showcase/ranked_finish.nr): `.enumerate()` on arrays, `Vec` and ranges, with a `u64` position that indexes back into the array it came from.
- [`sensor_windows.nr`](showcase/sensor_windows.nr): One `&[T]` and `&mut [T]` signature over an array and a `Vec`, windowed with `.slice(range)` without copying elements.
- [`status_report.nr`](showcase/status_report.nr): A status report built from live readings, with format specs `:.N`, `:x`, `:>W`, `:<W` and `:+d`, checked against their exact text.
- [`stream_pipeline.nr`](showcase/stream_pipeline.nr): `Iterator` with associated types and `.map` / `.filter` adapters, chained with `|>` and `>>` into a stream with no intermediate array.
- [`word_scanner.nr`](showcase/word_scanner.nr): A small tokenizer on `.chars()` and `.char_indices()` that cuts tokens out by byte offset, on non-ASCII input.

### Traits, generics and closures

- [`bin_tally.nr`](showcase/bin_tally.nr): Derivable traits as bounds (`Copy`, `Clone`, `Debug`, `PartialEq`, `Hashable`), with a `HashMap` keyed by a derived struct.
- [`closures.nr`](showcase/closures.nr): Closures and higher-order functions: capture by copy, by reference, by `move` or by a move the body makes, `(i32) -> i32` parameters, closures returned or stored in fields, and borrowing closures kept in an array and a struct and chained by a `compose` that returns one.
- [`derived_records.nr`](showcase/derived_records.nr): `@derive(Debug, PartialEq)` and `@derive(Copy, Clone)` on nested structs, with the `:?` spec and structural `==`.
- [`generic_toolkit.nr`](showcase/generic_toolkit.nr): Generic structs and functions with const parameters, `where` clauses, turbofish, a non-`Copy` type argument, and `?` at a generic parameter.
- [`shared_views.nr`](showcase/shared_views.nr): References as generic arguments: one `Slot<T>` and one `pick` over owned records and borrows of them, where `.clone()` copies the record or only the reference, with a closure parameter and an enum payload.
- [`shape_traits.nr`](showcase/shape_traits.nr): Trait declarations with default methods, `impl Trait` and `dyn Trait` dispatch, a generic bounded by two traits, and a supertrait reached through a default body, a bound and a trait object.
- [`typed_channels.nr`](showcase/typed_channels.nr): Associated types (`type Sample`) with one trait and three implementors, `Channel<Sample = i32>` bounds, and a generic trait method called through one.

### Structs, enums and numbers

- [`enum_records.nr`](showcase/enum_records.nr): `match` over enum variants (payloads, `|`, ranges, guards) with structs, arrays and loops.
- [`num_algorithms.nr`](showcase/num_algorithms.nr): Five integer routines using recursion, `while`, saturating and checked overflow methods, and `Option` matched into a value.
- [`perceptron.nr`](showcase/perceptron.nr): A two-neuron feed-forward pass with structs, `impl` blocks, associated functions, `&self` methods and `as` casts.
- [`quantized_readings.nr`](showcase/quantized_readings.nr): Sensor readings quantized with `.to_checked::<u8>()`, which rejects out-of-range and NaN values where `as u8` would clamp them.
- [`render_settings.nr`](showcase/render_settings.nr): A render pipeline called with named arguments in a different order from the declaration, plus a positional `_` parameter.
- [`running_stats.nr`](showcase/running_stats.nr): An online mean accumulator with mixed `i32` and `f64` fields, direct field mutation and an `.is_nan()` guard.
- [`season_table.nr`](showcase/season_table.nr): A league table of non-`Copy` records: `+` moves two half-seasons into one, `+=` and `-=` update a `Vec` element, a field and a `&mut` parameter in place, and one `lt` supplies `<=`, `>` and a generic `T: Comparable`, beside closures and `Option`.
- [`simulation.nr`](showcase/simulation.nr): A bit-flag state machine on a struct field, using `|=`, `&=`, `^=` and `<<=`, and `~` through a `BitNot` impl.
- [`unit_types.nr`](showcase/unit_types.nr): Newtype units of measure (`Meters` and `Seconds` both wrap `i32`) that the compiler keeps apart.
- [`vector_physics.nr`](showcase/vector_physics.nr): Operator traits (`+`, `-`, unary `-`, `==`, and `+=` through `Add`) on a `Copy` struct, driving a physics step.

### Control flow and errors

- [`job_queue.nr`](showcase/job_queue.nr): `val-else` in its three forms, with `if val`, a `while val` loop that drains a retry queue with `continue`, nested and `@` patterns over `Option<Result<..>>`, and an array pattern walking the costs.
- [`sample_audit.nr`](showcase/sample_audit.nr): `?` on `Result` and `Option` that leaves the whole function from inside a loop, with a `val-else` error binding.
- [`scan_guard.nr`](showcase/scan_guard.nr): Deterministic `Drop` with labeled exits: `break 'label` out of nested loops, and a labeled `loop` whose `break` carries a value.
- [`sensor_pipeline.nr`](showcase/sensor_pipeline.nr): `Option` and `Result` used as generic enums with structs, borrowed parameters, a generic `choose<T>` and a guarded `match`.
- [`settings_loader.nr`](showcase/settings_loader.nr): Typed settings read from raw text: `.parse::<i32>()`, `.ok_or`, `.map_err`, `?` lifting errors into one enum, and `??` with a default.

### Multi-file

- [`telemetry/main.nr`](showcase/telemetry/main.nr): The root of a multi-file program: sibling and directory modules, the three `import` forms, `export import`, and an inline `module` block.

## See also

- [Language Reference](../docs/language-reference/types.md), and the [documentation index](../docs/README.md)
- [Known Bugs](../docs/BUGS.md): what is currently broken
- [CHANGELOG](../CHANGELOG.md): what each release added
