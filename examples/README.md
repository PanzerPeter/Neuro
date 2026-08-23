# Neuro Example Programs

Runnable `.nr` programs demonstrating the language. Each program returns an
`i32` from `main`, which becomes the process **exit code** — that is how every
example asserts its own result, and how the test harness checks it.

## Layout

Examples are grouped by topic so the set stays navigable as it grows:

| Directory        | What it covers                                                         |
| ---------------- | ---------------------------------------------------------------------- |
| `basics/`        | First programs: functions, variables, arithmetic, recursion, inference |
| `types/`         | Primitive types, `char` literals, `f16`/`bf16` half-precision, literal suffixes, separators, casts, overflow, strings, string concatenation (`+`), string slices (`&string`), `.slice(range)` sub-slices, move semantics, deterministic `Drop` (scope-exit destructors), immutable borrows (`&T`), borrow exclusivity (`&`/`&mut` aliasing rules), returned references / lifetime elision, `@derive(Copy, Clone)`, type aliases, fixed-size arrays `[T; N]` (indexing, `.len()`, `for x in arr`), static & dynamic dispatch (`impl Trait`, `&dyn Trait`), `Option<T>` / `Result<T, E>` and generic enums, the standard collections `Vec<T>` / `HashMap<K, V>` / `BTreeMap<K, V>` |
| `operators/`     | Bitwise ops, compound assignment, integer intrinsic methods, operator overloading (`Add`/`Sub`/`Neg`/`PartialEq`), `??` coalescing on `Option`/`Result`, `?` error propagation |
| `control_flow/`  | `if`/`else`, `for`-ranges, `while`, `loop`, block & `unsafe` expressions, lints, `panic`/`assert`/`unreachable`, `match` pattern matching, `val-else` unwrap-or-exit |
| `structs/`       | Struct definition, field access/mutation, `impl` methods (`&self` and in-place `&mut self`) |
| `modules/`       | Multi-file programs: a sibling module, a `mod.nr` directory module and its child, reached through qualified paths and `import`, with `export` choosing each module's surface; plus inline `module { }` blocks, an `export import` re-export facade, the implicit prelude, and the `@no_prelude` opt-out |
| `showcase/`      | **Bigger programs that combine many features at once** — incl. mutable borrows `&mut T` + `*` deref (`mutable_borrows.nr`) |

The single source of truth for each program's expected exit code is
[`expected.txt`](expected.txt). A multi-file program registers its root with an exit
code and each of its other modules with the marker `module`: those have no `main` of
their own and are compiled as part of the root that reaches into them.

## Showcase programs

These exist specifically to prove features work *together*, not just in
isolation:

- [`showcase/perceptron.nr`](showcase/perceptron.nr) — a two-neuron feed-forward
  pass. Structs + `impl` (method calling method) + `f64` math + ReLU branch +
  `while` loop + `as` cast. Exit `8`.
- [`showcase/num_algorithms.nr`](showcase/num_algorithms.nr) — `isqrt`, `gcd`
  (recursion), `is_prime`, `ipow` (saturating multiply), `pow_checked`
  (`checked_mul` reporting overflow as `Option::None`). Loops + recursion +
  modulo + compound assignment + `Option`/`match` + tuples + loop-as-value.
  Exit `33`.
- [`showcase/running_stats.nr`](showcase/running_stats.nr) — an online mean
  accumulator. Struct state, direct field mutation, `&self` query method, `f64`
  division, `as` casts. Exit `5`.
- [`showcase/simulation.nr`](showcase/simulation.nr) — a bit-flag state machine.
  Bitwise `<<`/`|`/`&`/`^`, `.shr(n)`, struct state, `&self` predicate +
  popcount methods, `while` with `break`. Exit `2`.
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
  `.slice(range)` / `.len()`, an if-expression, and a lifetime mixed with a type
  parameter (`tagged_len<'a, T>`) that monomorphizes on `T` only. The lifetime is
  validated then erased — zero runtime cost. Exit `18`.
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

## Compiling and running

```bash
# Type-check only
cargo run -p neurc -- check examples/basics/hello.nr

# Compile to an executable (choose an output path outside the source tree)
cargo run -p neurc -- compile examples/basics/hello.nr -o /tmp/hello
/tmp/hello; echo "exit: $?"
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
walks `examples/` recursively, compiles and runs every `.nr` file, and asserts
its exit code against [`expected.txt`](expected.txt). It fails if:

- a `.nr` file on disk has **no** entry in `expected.txt` (forces registration),
- an entry in `expected.txt` points at a file that **doesn't exist** (stale),
- any example's exit code **doesn't match** its registered value.

## Adding an example

1. Drop a `.nr` file into the topic directory it belongs to (create a new
   directory if no topic fits).
2. Add one line to [`expected.txt`](expected.txt): `path/from/examples.nr  <exit-code>`.
3. Run `cargo test -p neurc --test examples`.

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
- Move semantics, borrows (`&T`/`&mut T`), borrow exclusivity, lifetime elision, and deterministic `Drop` are implemented (sub-phase 1C). The owning collections `Vec<T>`, `HashMap<K, V>`, and `BTreeMap<K, V>` are implemented (1G) — they move on assignment and free their buffers at scope exit. A growable heap string (`String` builder) is still not implemented.
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
