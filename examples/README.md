# Neuro Example Programs

Runnable `.nr` programs demonstrating the language. Each program asserts itself
two ways, and the test harness checks both:

- the `i32` returned from `main` becomes the process **exit code**, registered in
  [`expected.txt`](expected.txt);
- whatever the program writes to **standard output** is fixed byte for byte in a
  sibling `.out` file — `basics/hello.nr` is pinned by `basics/hello.out`.

Every example currently prints, so every one has a `.out` file. The rule still runs
the other way too: a program that prints nothing has no `.out` file, and that absence
is itself the expectation — its output must stay empty.

## Layout

Examples are grouped by topic so the set stays navigable as it grows:

| Directory        | What it covers                                                         |
| ---------------- | ---------------------------------------------------------------------- |
| `basics/`        | First programs: functions, variables, arithmetic, recursion, inference, `print` / `println` to stdout |
| `types/`         | Primitive types, `char` literals, `f16`/`bf16` half-precision, literal suffixes, separators, casts, overflow, strings, string concatenation (`+`), string interpolation with the format mini-language, triple-quoted block strings, string slices (`&string`), `.slice(range)` byte sub-slices and `.char_slice(range)` codepoint sub-slices, borrowed slices `&[T]` / `&mut [T]` over arrays and `Vec`s, move semantics, deterministic `Drop` (scope-exit destructors), immutable borrows (`&T`), borrow exclusivity (`&`/`&mut` aliasing rules), returned references / lifetime elision, `@derive(Copy, Clone)`, type aliases, fixed-size arrays `[T; N]` (indexing, `.len()`, `for x in arr`), static & dynamic dispatch (`impl Trait`, `&dyn Trait`), associated-type bounds (`T: Source<Item = i32>`), `Option<T>` / `Result<T, E>` and generic enums, the standard collections `Vec<T>` / `HashMap<K, V>` / `BTreeMap<K, V>`, the growable `String` text buffer |
| `operators/`     | Bitwise ops, compound assignment, integer intrinsic methods, operator overloading (`Add`/`Sub`/`Neg`/`PartialEq`), `??` coalescing on `Option`/`Result`, `?` error propagation |
| `control_flow/`  | `if`/`else`, `for`-ranges, `for (i, x) in xs.enumerate()`, the `.map(f)` / `.filter(p)` head adapters, the `IntoIterator` / `Iterator` protocol and hand-written adapters, `while`, `loop`, block & `unsafe` expressions, lints, `panic`/`assert`/`unreachable`, `match` pattern matching, `val-else` unwrap-or-exit |
| `structs/`       | Struct definition, field access/mutation, `impl` methods (`&self` and in-place `&mut self`) |
| `modules/`       | Multi-file programs: a sibling module, a `mod.nr` directory module and its child, reached through qualified paths and `import`, with `export` choosing each module's surface; plus inline `module { }` blocks, an `export import` re-export facade, the implicit prelude, and the `@no_prelude` opt-out |
| `showcase/`      | **Bigger programs that combine many features at once** — incl. mutable borrows `&mut T` + `*` deref (`mutable_borrows.nr`) |

The single source of truth for each program's expected exit code is
[`expected.txt`](expected.txt); for its expected output, the sibling `.out` file.
A multi-file program registers its root with an exit code and each of its other
modules with the marker `module`: those have no `main` of their own and are
compiled as part of the root that reaches into them, so only the root has output
of its own to pin.

## Showcase programs

These exist specifically to prove features work *together*, not just in
isolation:

- [`showcase/perceptron.nr`](showcase/perceptron.nr) — a two-neuron feed-forward
  pass. Structs + `impl` (method calling method) + `f64` math + ReLU branch +
  `while` loop + `as` cast. Exit `8`.
- [`showcase/num_algorithms.nr`](showcase/num_algorithms.nr) — `isqrt`, `gcd`
  (recursion), `is_prime`, `ipow` (saturating multiply), `pow_checked`
  (`checked_mul` reporting overflow as `Option::None`). Loops + recursion +
  modulo + compound assignment + `Option`/`match` + tuples + loop-as-value,
  plus a **nesting block comment** shelving an alternative `isqrt` whose body
  carries a `/* */` comment of its own. Exit `33`.
- [`showcase/ranked_finish.nr`](showcase/ranked_finish.nr) — a race result read
  by finishing position. `.enumerate()` over a fixed-size array, over the `Vec<i32>`
  that loop fills, and over a range, with `@derive(Copy)` structs + `&self`
  methods + `match` on the position + string interpolation. The combination is the
  point: the `u64` position indexes back into the array that produced it, so each
  runner is compared with the next — something `for runner in runners` cannot do.
  Exit `176`.
- [`showcase/running_stats.nr`](showcase/running_stats.nr) — an online mean
  accumulator. Struct state, direct field mutation, `&self` query methods, `f64`
  division, `as` casts, and `.is_nan()` screening a non-finite sample out of the
  accumulator. Exit `5`.
- [`showcase/simulation.nr`](showcase/simulation.nr) — a bit-flag state machine.
  Bitwise `<<`/`|`/`&`/`^`, `.shr(n)`, struct state, `&self` predicate +
  popcount methods, `while` with `break`. Exit `2`.
- [`showcase/status_report.nr`](showcase/status_report.nr) — a formatted status
  report, **printed to stdout with `println`**. String interpolation with the
  format mini-language (`:04x`, `:.2`, `:+d`, `:>10`) rendering values that come
  from a `@derive(Copy)` struct with `impl` methods, an enum with a payload
  matched by `match`, a fixed-size array walked by `for`-in, `f64` math, and `+`
  concatenation. Each line is printed and then checked against the exact text it
  should produce. Exit `34`.
- [`showcase/config_manifest.nr`](showcase/config_manifest.nr) — a config
  manifest rendered from typed records. **Triple-quoted block strings** carrying
  the header, footer, and the expected document verbatim, working together with a
  `@derive(Copy)` struct + `impl` methods, an enum with a payload matched by
  `match`, a fixed-size array + `for`-in loop, `+` concatenation, and string
  interpolation with the format mini-language (`:<10`, `:>4`, `:.3`). Exit `81`.
- [`showcase/enum_records.nr`](showcase/enum_records.nr) — pattern matching
  deconstructing enums with associated data (all three variant
  forms) alongside a struct with an enum field, `impl` methods, a fixed-size
  array + `for`-in loop, plus value/or/range/guard patterns. Exit `46`.
- [`showcase/unit_types.nr`](showcase/unit_types.nr) — newtype units of measure
  flowing through a struct with newtype fields, `impl` methods, an enum
  + `match`, and a fixed-size array + `for`-in loop. Exit `94`.
- [`showcase/generic_toolkit.nr`](showcase/generic_toolkit.nr) — generic structs and
  generic inherent impls, monomorphized per instance (`Pair<T, U>` inferred at
  a literal, `Cell<T>::get` used at `i32` and `bool`), working together with generic
  functions (`identity<T>`, `choose<T>`, `second<T, U>`), **const generic parameters**
  (`Buffer<T, const CAP>` and `sum_all<const N>` with a `where N > 0` predicate),
  **turbofish** (`identity::<i32>(8)`), a fixed-size array + `for`-in loop, an enum +
  pattern matching, and a tuple used as a generic type argument. Exit `85`.
- [`showcase/inventory_ledger.nr`](showcase/inventory_ledger.nr) — the **standard
  collections** carrying a small ledger: a `Vec<Item>` of `Copy` structs, a
  `Vec<string>`, a `HashMap<string, i32>` name index, and a key-ordered
  `BTreeMap<i32, i32>` report, working together with an `impl` block of `&self`
  methods, an enum + `match` classifier, `Option` matching on every fallible
  read, `??` unwrapping the reads that only need a default (including a
  right-to-left chain), fixed-size arrays, and `for`-in over both arrays and
  collections. Exit `181`.
- [`showcase/borrowed_text.nr`](showcase/borrowed_text.nr) — **explicit lifetime
  annotations** `<'a>` on the classic `longest<'a>(a: &'a string, b: &'a string)
  -> &'a string`, working together with immutable string borrows, zero-copy
  `.slice(range)` / `.char_slice(range)` / `.len()`, an if-expression, and a lifetime
  mixed with a type parameter (`tagged_len<'a, T>`) that monomorphizes on `T` only.
  The lifetime is validated then erased — zero runtime cost. The two slice methods are
  shown side by side on multi-byte text, where they part company. Exit `25`.
- [`showcase/shape_traits.nr`](showcase/shape_traits.nr) — **trait declarations**:
  a `Shape` trait with a required `area` and a **default** `is_big` method, implemented
  for two structs (`Square` inherits the default, `Rect` overrides it), dispatched
  through a **trait-bounded generic** `scaled_area<T: Shape>` monomorphized per shape,
  and combined with `&self` methods, `@derive(Copy)` structs, a fixed-size array +
  `for`-in loop, and if-expressions. Also demonstrates **both dispatch forms**:
  `describe(&impl Shape)` (static, monomorphized like the bound generic) and
  `dyn_area` / `dyn_flag` taking `&dyn Shape` (dynamic, one body serving both shapes
  through a vtable, reaching Square's inherited default and Rect's override). Exit
  `161`.
- [`showcase/typed_channels.nr`](showcase/typed_channels.nr) — **associated types**: one
  `Channel` trait whose `type Sample` each instrument binds differently (`f64`, `i32`, `bool`),
  so three unrelated measurement types share one trait — which a trait fixed to a single sample
  type could not do. Combined with a trait default method (`channel_id`, inherited by the tally
  and overridden by the others), `@derive(Copy)` structs with `&self` methods, a drain returning
  `Option<Self::Sample>` unwrapped by `match`, `Vec<i32>` + `for`-in, an if-expression, and
  interpolation with the format mini-language (`:.1`, `:>4`). A `Channel<Sample = i32>`
  bound then reads every counted channel through one generic body — the constraint the
  trait declaration alone cannot express. Exit `108`.
- [`showcase/vector_physics.nr`](showcase/vector_physics.nr) — **operator traits**:
  a `Copy` `Vec2` implementing `Add` / `Sub` / `Neg` / `PartialEq`, so `+` / `-` / unary
  `-` / `==` dispatch to the impl methods, combined with an `&self` method, compound
  assignment (`+=` desugaring through `Add`), a `while` loop, and if-expressions. The
  operators are monomorphized to plain calls — no vtable. Exit `35`.
- [`showcase/sensor_pipeline.nr`](showcase/sensor_pipeline.nr) — **`Option` / `Result`**: a
  reading looked up in an array (absent → `Option::None`) and validated (out of range →
  `Result::Err`), combined with a struct + `impl` methods (`&self`), a borrowed struct parameter
  (`&Sensor`), a fixed-size array + `for`-in loop, a generic function used at two type arguments,
  a guarded `match` arm, and compound assignment. Exit `57`.
- [`showcase/sensor_windows.nr`](showcase/sensor_windows.nr) — **borrowed slices**: one
  `Window::over(&[i32])` reader serving a fixed-size array, a `.slice(range)` window into it,
  and a `Vec<i32>`, combined with a struct + `impl` methods (`&self`), an `Option<i32>`
  unwrapped by `match`, `for (i, x) in xs.enumerate()`, an in-place clamp through a
  `&mut [i32]`, and the format mini-language. Exit `85`.
- [`showcase/closures.nr`](showcase/closures.nr) — **closures and lambdas**: a
  higher-order `map_sum(xs, f: (i32) -> i32)` applied with a closure literal that
  **captures** an enclosing Copy variable by value, a **`move`** closure with a block
  body and explicit return type, and a struct `impl` method — all combined with
  fixed-size arrays + indexed iteration and a `while` loop. Each closure is lifted to a
  `{ fn_ptr, env_ptr }` value; the environment holds the captured value. Exit `90`.
- [`showcase/job_queue.nr`](showcase/job_queue.nr) — **`val-else`**: three stages of a
  job queue each unwrap-or-exit, exercising all three `else` forms — `|reason|` naming a
  `Result`'s `Err` payload, `|verdict|` naming a plain enum's whole scrutinee for a nested
  `match`, and a bare `else { break }` draining a `Vec` through `pop`. Combined with `??`
  defaulting an absent lookup, a `@derive(Copy)` struct with an associated function and
  `&self` method, an enum + guarded `match`, a fixed-size array, a range-`for`, and
  `for`-in over the collection. Exit `139`.
- [`showcase/log_builder.nr`](showcase/log_builder.nr) — **the growable `String`**: a run
  transcript assembled into one buffer that grows in place, instead of the `+` chain that
  would reallocate and recopy the whole transcript once per event. Each line is appended
  through a `&mut String` parameter. Combined with a `Vec<Event>` + `for`-in, a
  `@derive(Copy, Clone)` struct with `&self` methods, an enum with a payload + `match`, and
  string interpolation with the format mini-language (`:>4`, `:.1`, `:+d`). The finished
  text is checked exactly, then `.clear()` proves the buffer is reusable. Exit `64`.
- [`showcase/buffered_report.nr`](showcase/buffered_report.nr) — **buffered standard
  output**: a 240-line shift report followed by a banner larger than the output buffer
  itself, so the lines held in the buffer, the drains that empty it, and the oversize
  string that skips it all have to come out in the order they were written. Combined with a
  `@derive(Copy, Clone)` struct with `&self` methods, a `Vec<Reading>` + `for`-in, the
  growable `String` with `push_str`, and interpolation with the format mini-language
  (`:>3`, `:.2`). Exit `40` — the number of readings at peak load.
- [`showcase/scan_guard.nr`](showcase/scan_guard.nr) — **deterministic `Drop` +
  labeled breaks**: two `impl Drop` scope guards sharing a `&mut i32` counter while a
  labeled `break` exits *two* nested loops at once, proving the destructors still run
  on that path. Combined with a `Copy` struct + `&self` method, a fixed-size array with
  indexed reads, and `match` over range and `_` patterns. Exit `160`.
- [`showcase/sample_audit.nr`](showcase/sample_audit.nr) — **the `?` propagation
  operator**: a validator whose `Err` carries the offending sample, propagated with `?`
  out of a `for`-in loop body (leaving the whole function, not the iteration), plus `?`
  on an `Option` rebuilt as the caller's own `None`. Combined with a `@derive(Copy)`
  struct + `&self` method, fixed-size arrays, `match` on the returned `Result`,
  `val-else` with an `|e|` error binding, and `??` defaulting the reads that only need
  a fallback. Exit `177`.
- [`showcase/stream_pipeline.nr`](showcase/stream_pipeline.nr) — **the iteration
  protocol**: a `Readings` container implementing `IntoIterator`, the `ReadingsIter`
  cursor it hands out implementing `Iterator`, and three adapters over it — a `Scaled`
  transform, an `Above` filter that may pull several elements per step, and a `Shaped`
  transform carrying a **closure in a struct field**. Two adapters are stacked over one
  source and drained by a single `for` head, so nothing between the source and the loop
  is ever materialized. Combined with an **associated-type bound**
  (`S: Iterator<Item = i32>`) that is what lets an adapter call `self.inner.next()`,
  generic structs monomorphized per instance, `@derive(Copy)` structs with `&mut self`
  methods, `Option` + `match`, `Vec<i32>` + `for`-in, `.enumerate()` over a protocol
  head, and interpolation with the format mini-language (`:>2`, `:>3`). The same
  pipeline is then rewritten with the compiler's own **`.map(f)` / `.filter(p)` head
  adapters**, which need no adapter type at all, and a filtered array head is
  enumerated to show the position counting what the chain yielded. Exit `189`.
- [`showcase/telemetry/main.nr`](showcase/telemetry/main.nr) — **multi-file
  compilation and `import`**: a root module reaching a sibling (`stats`), a `mod.nr`
  directory module (`report`), and its child (`report::format`), naming them through a
  name list, a rename, and a module alias. The **implicit prelude** carries `Some` / `None`
  into the root module and into `report` with no import at all, while `stats` — which has
  nothing fallible to say — opts out of it with **`@no_prelude`**. Also an **inline `module`
  block** (`scoring`, with a private helper the surrounding file cannot name) and an
  **`export import`** in `report/mod.nr` that re-exports its child's `clamp`, so `main`
  reaches `report::clamp` without naming `format`. Combined with a struct +
  `impl` methods built in one module and used in another, a generic function monomorphized
  at `i32` and `bool`, a fixed-size array, a heap-backed `Vec<T>` that frees its buffer at
  scope exit, an enum + `match`, and `??` defaulting an absent `Option`. Each module publishes
  a surface with `export` and keeps the rest private — `Summary.total` and `report`'s `Band`
  never leave the file that declares them. Exit `75`.

- [`showcase/render_settings.nr`](showcase/render_settings.nr) — **the sub-phase 1H
  language-cleanup features together**: **named arguments** with external labels
  (`quality q:`, `min floor:`) and a positional-only `_ factor`, **string interpolation**
  with the format mini-language (`{w:>4}`), a **triple-quoted `"""` block** dedented
  against its closing delimiter, and a **nested block comment**. Every named call is
  written in an order that differs from the declaration, so the program's answer is only
  right if the labels — not the positions — decided the binding. Combined with a
  `@derive(Copy)` struct + `&self` / `&mut self` methods, an associated function, an enum
  with a payload + `match`, a fixed-size array + `for`-in loop, and `??` defaulting an
  `Option`. Exit `99`.

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
- any example's output **doesn't match** its `.out` file — including a silent
  example that starts printing, which has no `.out` and so must print nothing.

An output mismatch prints the first differing line and then both texts in full,
with every line quoted so trailing whitespace stays visible.

## Adding an example

1. Drop a `.nr` file into the topic directory it belongs to (create a new
   directory if no topic fits).
2. Add one line to [`expected.txt`](expected.txt): `path/from/examples.nr  <exit-code>`.
3. If it prints, save exactly what it prints beside it as `<name>.out`:
   `cargo run -p neurc -- compile examples/<name>.nr -o /tmp/ex && /tmp/ex > examples/<name>.out`.
   Read that file before committing it — it is an assertion, so it is only worth
   having if the text in it is the text the program *should* produce.
4. Run `cargo test -p neurc --test examples`.

No Rust edits are needed — discovery is automatic.

## Known language limitations (affect what examples can do)

- Fixed-size arrays `[T; N]` are supported (`types/arrays.nr`): literals,
  indexing, element assignment, `.len()`, and `for x in arr` / `for x in &arr`.
  Element types are limited to `Copy` scalars for now; growable `Vec<T>` and
  `.enumerate()` are later phases.
- Tuples `(T1, T2, ...)` are supported (`types/tuples.nr`): the tuple type,
  literals, `.0` / `.1` index access, and destructuring binds `val (a, b) = t`
  (with `_` wildcards and nesting). Elements are limited to `Copy` types for now,
  so tuples holding a `string` or other non-Copy value are a later phase.
- Newtypes are supported (`types/newtype.nr`): `newtype Meters = i32` creates a
  distinct nominal type wrapping an inner type, constructed `Meters(30)` and read
  back with `.0`. Unlike a `type` alias, a newtype is *not* interchangeable with
  its inner type. The inner type is limited to `Copy` types for now.
- Struct and array destructuring patterns are supported (`types/destructuring.nr`):
  `val Point { x, y } = p` binds each field by name; `val [a, b, c] = arr` binds
  array elements positionally; `val [first, ..rest] = arr` captures the remainder as
  a fresh `[T; N - k]` array, and a bare `..` ignores it. A rest-less array pattern
  must match the array's length exactly.
- Move semantics, borrows (`&T`/`&mut T`), borrow exclusivity, lifetime elision, and deterministic `Drop` are implemented (sub-phase 1C). The owning collections `Vec<T>`, `HashMap<K, V>`, and `BTreeMap<K, V>` are implemented (1G), as is the growable `String` text buffer — all four move on assignment and free their buffers at scope exit. What still leaks is the anonymous heap `string` that `+`, interpolation, and `String::to_string` produce, which no tracked binding owns.
- `&self` and `&mut self` methods are supported; a `&mut self` method mutates
  struct state in place (see `structs/mut_self_accumulator.nr`). Consuming `self`
  is not yet supported.
- Right shift is the `.shr(n)` method, not a `>>` operator (Phase 2+).
- Prefer `return` over a tail-position `if`/`else` *expression* as a function's
  implicit return value; assign it to a `val` first if you need the value form
  (`val r = if c { a } else { b }`). The examples follow this convention.

## See also

- [Language Reference](../docs/language-reference/types.md)
- [CHANGELOG](../CHANGELOG.md)
- [Compiler Documentation](../docs/README.md)
