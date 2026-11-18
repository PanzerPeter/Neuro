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
| `basics/`       | First programs: functions, variables, arithmetic, recursion, inference, `print` / `println`, `@test` functions run by `neuroc test` | [Functions](../docs/language-reference/functions.md) |
| `types/`        | Primitives, literal suffixes and separators, casts, overflow, half precision, arrays, tuples, destructuring, newtypes, aliases, enum methods and trait impls, `Option` / `Result`, collections, dispatch | [Types](../docs/language-reference/types.md), [Generics and traits](../docs/language-reference/generics-and-traits.md) |
| `strings/`      | `string` literals, also where `&string` is expected, and slices, `char`, interpolation, triple-quoted blocks, codepoint iteration, `.lines()` and `.split(sep)`, `.parse::<T>()`, the growable `StringBuilder` | [Strings](../docs/language-reference/strings.md) |
| `tensors/`      | `Tensor<T, [dims]>`: construction, element-wise operators and broadcasting, matrix multiplication, indexing and slicing, shape generics over extents and whole shapes (`Tensor<T, S>`, `[..S, N]`), structs and `impl`s sized by bare names, element types generic under `Numeric` / `Integer` / `Float`, named dimensions, reshaping, reductions, sorting, Einstein notation, functional traversals, elementwise math, dynamic axes, `@gpu` kernels, hand-written `@kernel` functions and their `partition` write form, device transfer with `.to` and the operations that then run on the device | [Tensors](../docs/language-reference/tensors.md) |
| `ownership/`    | Moves, `Copy` / `.clone()`, consuming `self` receivers, immutable and mutable borrows, borrow exclusivity, returned references, collections of owned values, owned strings in fields, payloads and arguments, deterministic `Drop`, `pool` arena blocks | [Ownership](../docs/language-reference/ownership.md) |
| `operators/`    | Bitwise ops, compound assignment, integer intrinsics, operator overloading, `??` coalescing, `?` propagation, `\|>` pipelines, `>>` composition | [Operators](../docs/language-reference/operators.md) |
| `control_flow/` | `if` / `else`, `for` over ranges and adapters, `for` over maps, ranges as values, borrowing iterators (`.iter()`, `.keys()`, `.values()`) as values, the iterator protocol, `while`, `loop`, block and `unsafe` expressions, `match` and its nested, `@` and array patterns, `val-else`, `if val` / `while val`, panics, lints | [Control Flow](../docs/language-reference/control-flow.md) |
| `structs/`      | Struct definition, field access and mutation, `&self` and `&mut self` methods, generic methods, supertraits, `Self`, derives, hand-written `Clone` / `Copy` / `Debug` and the bounds that name them | [Structs and Enums](../docs/language-reference/structs.md) |
| `system/`       | `std::fs` file reads and writes and the `IoError` they fail with, `std::safetensors` weights files, `std::env` arguments and variables, `std::io::read_line`, `std::time::Instant` | [Standard library](../docs/language-reference/standard-library.md) |
| `modules/`      | Multi-file programs, `mod.nr` directory modules, qualified paths, `import`, per-module namespaces, inline `module` blocks, re-export facades, the prelude and its opt-out | [Modules](../docs/language-reference/modules.md) |
| `showcase/`     | About ten flagship programs worth reading first | see the showcase list below |
| `integration/`  | Bigger programs proving many features work **together**, in theme folders (`training/`, `gpu/`, `tensors/`, `ownership/`, `text_collections/`, `traits_generics/`, `records_numbers/`, `errors_flow/`, `multi_file/`) | see the integration index below |

## Compiling and running

```bash
# Type-check only
cargo run -p neuroc -- check examples/basics/hello.nr

# Compile to an executable (choose an output path outside the source tree)
cargo run -p neuroc -- compile examples/basics/hello.nr -o /tmp/hello
/tmp/hello; echo "exit: $?"

# What it prints is exactly what the golden file holds
/tmp/hello | diff - examples/basics/hello.out && echo "output matches"
```

Compiled binaries under `examples/` are git-ignored, but `-o` pointing outside the tree
keeps them out of the source tree entirely.

## Testing

One integration test, [`compiler/neuroc/tests/examples.rs`](../compiler/neuroc/tests/examples.rs),
compiles and runs every example. It finds the programs by walking `examples/` recursively:

```bash
cargo test --workspace                 # runs all examples (among everything else)
cargo test -p neuroc --test examples    # just the example harness
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
   does. A program that combines features goes under `integration/<theme>/`. Never add to
   `showcase/` directly: it is curated. Open it with a header comment saying what it shows.
2. Add one line to [`expected.txt`](expected.txt), in that directory's section:
   `path/from/examples.nr  <exit-code>`.
3. If it prints, save its output beside it as `<name>.out`:
   `cargo run -p neuroc -- compile examples/<name>.nr -o /tmp/ex && /tmp/ex > examples/<name>.out`.
   Read the file before committing it. It is an assertion, so it is only worth having if the
   text is what the program should produce.
4. Run `cargo test -p neuroc --test examples`. No Rust edits are needed.

## Showcase

`showcase/` holds the few programs worth reading first: each shows something the language does that
most do not (shapes in types, autodiff, GPU kernels, compile-time borrow checks) and stays short.
The header comment in each has the full description. A program joins this folder by hand-picked
promotion, never because a feature needed a combination test (that goes under `integration/`).

- [`train_predict.nr`](showcase/train_predict.nr): A `@grad` fit trained on the CPU, then a `@gpu` prediction from the learned weights. The program in the README demo. Stops without a usable NVIDIA GPU.
- [`newton_fit.nr`](showcase/newton_fit.nr): One logistic fit solved by Newton's method (`@grad(order: 2)` and `.hessian()`) and by gradient descent, for comparison.
- [`checkpoint_resume.nr`](showcase/checkpoint_resume.nr): A least-squares fit trained with `@grad` that saves its weights halfway with `std::safetensors`, the step count in the metadata, reads them back through an annotated `val ... else`, and resumes to the same bits as a run that never stopped.
- [`rank_free_layers.nr`](showcase/rank_free_layers.nr): One `Affine<T, S>` layer, generic over its element type and its whole shape, trained at a vector, a matrix and a batch; a trailing bias broadcast over `[..S, N]`.
- [`attention_head.nr`](showcase/attention_head.nr): One scaled dot-product attention head written as two `einsum` contractions, with a `>>` composition rescaling the scores.
- [`named_axes.nr`](showcase/named_axes.nr): A batch of token embeddings with every axis named, so an argument with its axes swapped is a compile error.
- [`heat_diffusion.nr`](showcase/heat_diffusion.nr): Heat along a rod, one GPU thread per cell in a `@kernel` with `partition` writes, checked against a host run. Stops without a usable NVIDIA GPU.
- [`gpu_fallback.nr`](showcase/gpu_fallback.nr): A two-layer classifier with `@gpu(fallback: true)` layers that run as kernels where a GPU is usable and on the CPU otherwise, with identical output.
- [`borrow_discipline.nr`](showcase/borrow_discipline.nr): What a live `&mut` forbids (reading, writing or moving the borrowed place by name), and the code written around it.
- [`trainer/main.nr`](showcase/trainer/main.nr): A small training project in six files: `data`, `model` (a `@grad` loss), `optim` (a directory module with a child and a re-export) and `train`. Tensor shapes cross every module boundary in the types, and `Sgd` keeps its rate private so only `Sgd::new` can build one.

## Integration index

`integration/` holds the larger programs that prove features work **together**, one folder per theme.
A new feature extends one of these (or adds one in the theme that fits) so it is exercised beside at
least two earlier features. Each program with a `main` is listed once. The header comment in each has
the full description.

### Training and autodiff

- [`training/activation_fit.nr`](integration/training/activation_fit.nr): One neuron trained under two activations passed as values into a `@grad` loss, using closures and a `>>` composition.
- [`training/composed_loss.nr`](integration/training/composed_loss.nr): Two `@grad` losses built from helper functions, one shape-generic and one branching, evaluated in a `pool` over candidate weights.
- [`training/frozen_features.nr`](integration/training/frozen_features.nr): Fine-tuning only a model's head with `@grad(wrt: [...])`, so `.backward()` fills the head's gradients while the feature layer stays fixed.
- [`training/gradient_loss.nr`](integration/training/gradient_loss.nr): A least-squares fit by gradient descent: `@grad`, `.backward()`, `.grad()`, `.zero_grad()` and `.item()` in a `pool` per step.
- [`training/logistic_fit.nr`](integration/training/logistic_fit.nr): Logistic regression with an elastic-net penalty that leaves the bias alone, trained by its own gradient, with `.tanh()`, `.log()`, `.abs()` and `.pow(2.0)` in the loss and a perplexity taken from the likelihood alone.
- [`training/ridge_objective.nr`](integration/training/ridge_objective.nr): A training objective carried as a value, whose `@grad` loss method reads a ridge penalty from a constant receiver.
- [`training/robust_fit.nr`](integration/training/robust_fit.nr): A `@grad` loss with a `while` loop and an `if` expression, where the derivative follows the path each call took.
- [`training/run_journal.nr`](integration/training/run_journal.nr): A run that saves its journal with `std::fs`, reloads it and resumes from the last epoch the file records, taking its epoch count from `std::env::args()` and timing itself with `std::time::Instant`; a damaged journal is reported through `.map_err` and `?`.
- [`training/td_value_fit.nr`](integration/training/td_value_fit.nr): Temporal-difference learning, with the bootstrapped target fenced by `.detach()` or `@no_grad` and compared with the unfenced residual gradient.

### GPU and kernels

- [`gpu/file_training.nr`](integration/gpu/file_training.nr): A linear model trained from a batch read with `std::safetensors`, checkpointed with its step count, then resumed on the GPU from device tensors, landing on the same bits as the host and scored by a `@gpu` function. Stops without a usable NVIDIA GPU.
- [`gpu/gpu_layer.nr`](integration/gpu/gpu_layer.nr): A dense layer whose forward pass runs as `@gpu` kernels on device-resident weights, checked against the host. Stops without a usable NVIDIA GPU.

### Tensors

- [`tensors/batched_heads.nr`](integration/tensors/batched_heads.nr): Two attention heads as one batch: `einsum` with `...` over a shape parameter, batched `@` with a projection shared across heads, and a gradient that sums both heads.
- [`tensors/mixed_precision.nr`](integration/tensors/mixed_precision.nr): `bf16` updates that round back to their weight beside an `f32` master copy that moves, with `.topk` and `.to_checked` over `bf16` data.
- [`tensors/model_shapes.nr`](integration/tensors/model_shapes.nr): A network's layer stack declared with real tensor parameters, where shapes are part of the types and a second owner of a tensor needs `.clone()`.
- [`tensors/optimizer_step.nr`](integration/tensors/optimizer_step.nr): A weight update written in place (`w -= g`) and by value (`w = w - g`), with broadcasting and an `@` forward pass.
- [`tensors/precision_sweep.nr`](integration/tensors/precision_sweep.nr): One gradient-descent fit written once over a `T: Float` element type and run at `f64`, `f32` and `bf16`, where `bf16` stalls short of the line.
- [`tensors/ranked_batch.nr`](integration/tensors/ranked_batch.nr): Ordering a tensor axis with `.sort()`, `.argsort()` and `.topk(k:, axis:)`; an argsort index reads back into the tensor it ranks.
- [`tensors/replay_buffer.nr`](integration/tensors/replay_buffer.nr): A replay buffer read newest first with `.rev()` and `.step(n)` on a `for` head and a tensor index axis.

### Ownership, borrows and Drop

- [`ownership/batch_arena.nr`](integration/ownership/batch_arena.nr): A batched forward pass in nested `pool` arenas, with a `PoolAware` type and rules for which values may cross a block.
- [`ownership/borrowed_records.nr`](integration/ownership/borrowed_records.nr): Structs and an enum holding `&'a string` views into strings `main` owns, read with `match`, `??`, `.map` and a `&mut` field.
- [`ownership/borrowed_text.nr`](integration/ownership/borrowed_text.nr): Lifetime annotations on borrowed text: `<'a>` on parameters and returns, with `.slice` and `.char_slice` views and string literals passed as `&string`.
- [`ownership/displaced_owners.nr`](integration/ownership/displaced_owners.nr): Reassigning a binding releases the value it displaces, counted through a `Drop` type, a `string` and a `Vec<string>`.
- [`ownership/grid_update.nr`](integration/ownership/grid_update.nr): A heat grid updated in place, each mutation naming its storage: a field, a tuple element, an array element, a `Vec` element or a tensor cell.
- [`ownership/held_destruction.nr`](integration/ownership/held_destruction.nr): Values held in a struct field, an element, an enum payload or a newtype, each destroyed with its holder, exactly once.
- [`ownership/incident_desk.nr`](integration/ownership/incident_desk.nr): A `Vec` and `HashMap` owning non-`Copy` tickets, with `&mut self` methods on slots, `get_mut`, and a `Drop` that fires once per ticket.
- [`ownership/mutable_borrows.nr`](integration/ownership/mutable_borrows.nr): Mutable borrows `&mut T` and the dereference `*`, where writes through the borrow reach the caller's storage.
- [`ownership/owned_aggregates.nr`](integration/ownership/owned_aggregates.nr): Arrays, tuples, enum payloads, newtypes and a generic struct holding an owned `string`, destructured one element at a time.
- [`ownership/owned_catalog.nr`](integration/ownership/owned_catalog.nr): A catalog whose `Vec`, `HashMap` and `BTreeMap` own their text: `push` and `insert` move values in, and `.clone()` keeps a copy.
- [`ownership/stored_text.nr`](integration/ownership/stored_text.nr): Text stored in a struct field, an element, a return value or an argument, each released by the holder that stores it.
- [`ownership/transient_text.nr`](integration/ownership/transient_text.nr): Strings nothing binds to a name (`+` chains, interpolations, `StringBuilder::to_string`), released where they are read, or at the end of the statement or loop that reads a view of them.

### Collections, iteration and text

- [`text_collections/buffered_report.nr`](integration/text_collections/buffered_report.nr): A shift report long enough to drain the stdout buffer several times, pinning the output order across drains.
- [`text_collections/config_manifest.nr`](integration/text_collections/config_manifest.nr): A config manifest rendered from typed records, with triple-quoted block strings dedented to their closing `"""`.
- [`text_collections/field_report.nr`](integration/text_collections/field_report.nr): Standard I/O and iteration: `&[T]` parameters that accept an array or a `Vec`, a user `Iterator`, and `.char_indices()`.
- [`text_collections/inventory_ledger.nr`](integration/text_collections/inventory_ledger.nr): A ledger on `Vec`, `HashMap` and `BTreeMap`, each walked with `for` in all three receiver modes (by value, `&` and `&mut`), with `(key, value)` pair heads, `.keys()` / `.values()` and a head `.filter`.
- [`text_collections/log_builder.nr`](integration/text_collections/log_builder.nr): A run transcript built in one growable `StringBuilder`, finished by a consuming `self` method.
- [`text_collections/ranked_finish.nr`](integration/text_collections/ranked_finish.nr): `.enumerate()` on arrays, `Vec` and ranges, with a `u64` position that indexes back into the array it came from.
- [`text_collections/score_sheet.nr`](integration/text_collections/score_sheet.nr): A score sheet read from text with `.lines()` and `.split(sep)`, each score parsed through `?`, the lines filtered by a function returning `impl Iterator` and the scores summed by a generic consumer.
- [`text_collections/sensor_windows.nr`](integration/text_collections/sensor_windows.nr): One `&[T]` and `&mut [T]` signature over an array and a `Vec`, windowed with `.slice(range)` without copying elements.
- [`text_collections/status_report.nr`](integration/text_collections/status_report.nr): A status report built from live readings, with format specs `:.N`, `:x`, `:>W`, `:<W` and `:+d`, checked against their exact text.
- [`text_collections/stream_pipeline.nr`](integration/text_collections/stream_pipeline.nr): `Iterator` with associated types and `.map` / `.filter` adapters, both in `for` heads and as adapter values that a function returns (by type name or as `impl Iterator`) and a generic consumer drives, a reversed and strided range value and an array walked by borrow with `.iter()` fed through the same adapters and consumer, chained with `|>` and `>>` into a stream with no intermediate array.
- [`text_collections/word_scanner.nr`](integration/text_collections/word_scanner.nr): A small tokenizer on `.chars()` and `.char_indices()` that cuts tokens out by byte offset, on non-ASCII input.

### Traits, generics and closures

- [`traits_generics/bin_tally.nr`](integration/traits_generics/bin_tally.nr): Derivable traits as bounds (`Copy`, `Clone`, `Debug`, `PartialEq`, `Hashable`), with a `HashMap` keyed by a derived struct.
- [`traits_generics/closures.nr`](integration/traits_generics/closures.nr): Closures and higher-order functions: capture by copy, by reference, by `move` or by a move the body makes, `(i32) -> i32` parameters, closures returned or stored in fields, and borrowing closures kept in an array and a struct and chained by a `compose` that returns one.
- [`traits_generics/derived_records.nr`](integration/traits_generics/derived_records.nr): `@derive(Debug, PartialEq)` and `@derive(Copy, Clone)` on nested structs, with the `:?` spec and structural `==`.
- [`traits_generics/generic_toolkit.nr`](integration/traits_generics/generic_toolkit.nr): Generic structs and functions with const parameters, `where` clauses, turbofish, a non-`Copy` type argument, and `?` at a generic parameter.
- [`traits_generics/shape_traits.nr`](integration/traits_generics/shape_traits.nr): Trait declarations with default methods, `impl Trait` and `dyn Trait` dispatch, a generic bounded by two traits, and a supertrait reached through a default body, a bound and a trait object.
- [`traits_generics/shared_views.nr`](integration/traits_generics/shared_views.nr): References as generic arguments: one `Slot<T>` and one `pick` over owned records and borrows of them, where `.clone()` copies the record or only the reference, with a closure parameter and an enum payload.
- [`traits_generics/typed_channels.nr`](integration/traits_generics/typed_channels.nr): Associated types (`type Sample`) with one trait and three implementors, `Channel<Sample = i32>` bounds, and a generic trait method called through one.

### Structs, enums and numbers

- [`records_numbers/enum_records.nr`](integration/records_numbers/enum_records.nr): `match` over enum variants (payloads, `|`, ranges, guards) with structs, arrays and loops.
- [`records_numbers/num_algorithms.nr`](integration/records_numbers/num_algorithms.nr): Five integer routines using recursion, `while`, saturating and checked overflow methods, and `Option` matched into a value.
- [`records_numbers/perceptron.nr`](integration/records_numbers/perceptron.nr): A two-neuron feed-forward pass with structs, `impl` blocks, associated functions, `&self` methods and `as` casts.
- [`records_numbers/quantized_readings.nr`](integration/records_numbers/quantized_readings.nr): Sensor readings quantized with `.to_checked::<u8>()`, which rejects out-of-range and NaN values where `as u8` would clamp them.
- [`records_numbers/render_settings.nr`](integration/records_numbers/render_settings.nr): A render pipeline called with named arguments in a different order from the declaration, plus a positional `_` parameter.
- [`records_numbers/running_stats.nr`](integration/records_numbers/running_stats.nr): An online mean accumulator with mixed `i32` and `f64` fields, direct field mutation and an `.is_nan()` guard, checked by `@test` functions that `neuroc test` runs.
- [`records_numbers/season_table.nr`](integration/records_numbers/season_table.nr): A league table of non-`Copy` records: `+` moves two half-seasons into one, `+=` and `-=` update a `Vec` element, a field and a `&mut` parameter in place, and one `lt` supplies `<=`, `>` and a generic `T: Comparable`, beside closures and `Option`.
- [`records_numbers/simulation.nr`](integration/records_numbers/simulation.nr): A bit-flag state machine on a struct field, using `|=`, `&=`, `^=` and `<<=`, and `~` through a `BitNot` impl.
- [`records_numbers/unit_types.nr`](integration/records_numbers/unit_types.nr): Newtype units of measure (`Meters` and `Seconds` both wrap `i32`) that the compiler keeps apart.
- [`records_numbers/vector_physics.nr`](integration/records_numbers/vector_physics.nr): Operator traits (`+`, `-`, unary `-`, `==`, `!=` through the provided `ne`, `+=` through `Add` and `-=` through a `SubAssign` impl) on a `Copy` struct, driving a physics step.

### Control flow and errors

- [`errors_flow/job_queue.nr`](integration/errors_flow/job_queue.nr): `val-else` in its three forms, with `if val`, a `while val` loop that drains a retry queue with `continue`, nested and `@` patterns over `Option<Result<..>>`, and an array pattern walking the costs.
- [`errors_flow/sample_audit.nr`](integration/errors_flow/sample_audit.nr): `?` on `Result` and `Option` that leaves the whole function from inside a loop, with a `val-else` error binding.
- [`errors_flow/scan_guard.nr`](integration/errors_flow/scan_guard.nr): Deterministic `Drop` with labeled exits: `break 'label` out of nested loops, and a labeled `loop` whose `break` carries a value.
- [`errors_flow/sensor_pipeline.nr`](integration/errors_flow/sensor_pipeline.nr): `Option` and `Result` used as generic enums with structs, borrowed parameters, a generic `choose<T>` and a guarded `match`.
- [`errors_flow/settings_loader.nr`](integration/errors_flow/settings_loader.nr): Typed settings read from raw text: `.parse::<i32>()`, `.ok_or`, `.map_err`, `?` lifting errors into one enum, and `??` with a default.### Multi-file

- [`multi_file/telemetry/main.nr`](integration/multi_file/telemetry/main.nr): The root of a multi-file program: sibling and directory modules, the three `import` forms, `export import`, an inline `module` block, and namespaces. Two modules declare the same function name, an enum is matched and a trait implemented through their module paths, a trait default crosses modules, and a local shadows an import.

## See also

- [Language Reference](../docs/language-reference/types.md), and the [documentation index](../docs/README.md)
- [Known Bugs](../docs/BUGS.md): what is currently broken
- [CHANGELOG](../CHANGELOG.md): what each release added
