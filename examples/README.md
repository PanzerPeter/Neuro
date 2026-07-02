# Neuro Example Programs

Runnable `.nr` programs demonstrating the language. Each program returns an
`i32` from `main`, which becomes the process **exit code** — that is how every
example asserts its own result, and how the test harness checks it.

## Layout

Examples are grouped by topic so the set stays navigable as it grows:

| Directory        | What it covers                                                         |
| ---------------- | ---------------------------------------------------------------------- |
| `basics/`        | First programs: functions, variables, arithmetic, recursion, inference |
| `types/`         | Primitive types, `char` literals, `f16`/`bf16` half-precision, literal suffixes, separators, casts, overflow, strings, string concatenation (`+`), string slices (`&string`), `.slice(range)` sub-slices, move semantics, deterministic `Drop` (scope-exit destructors), immutable borrows (`&T`), borrow exclusivity (`&`/`&mut` aliasing rules), returned references / lifetime elision, `@derive(Copy, Clone)`, type aliases, fixed-size arrays `[T; N]` (indexing, `.len()`, `for x in arr`) |
| `operators/`     | Bitwise ops, compound assignment, integer intrinsic methods            |
| `control_flow/`  | `if`/`else`, `for`-ranges, `while`, `loop`, block & `unsafe` expressions, lints, `panic`/`assert`/`unreachable` |
| `structs/`       | Struct definition, field access/mutation, `impl` methods (`&self` and in-place `&mut self`) |
| `showcase/`      | **Bigger programs that combine many features at once** — incl. mutable borrows `&mut T` + `*` deref (`mutable_borrows.nr`) |

The single source of truth for each program's expected exit code is
[`expected.txt`](expected.txt).

## Showcase programs

These exist specifically to prove features work *together*, not just in
isolation:

- [`showcase/perceptron.nr`](showcase/perceptron.nr) — a two-neuron feed-forward
  pass. Structs + `impl` (method calling method) + `f64` math + ReLU branch +
  `while` loop + `as` cast. Exit `8`.
- [`showcase/num_algorithms.nr`](showcase/num_algorithms.nr) — `isqrt`, `gcd`
  (recursion), `is_prime`, `ipow` (saturating multiply). Loops + recursion +
  modulo + compound assignment. Exit `32`.
- [`showcase/running_stats.nr`](showcase/running_stats.nr) — an online mean
  accumulator. Struct state, direct field mutation, `&self` query method, `f64`
  division, `as` casts. Exit `5`.
- [`showcase/simulation.nr`](showcase/simulation.nr) — a bit-flag state machine.
  Bitwise `<<`/`|`/`&`/`^`, `.shr(n)`, struct state, `&self` predicate +
  popcount methods, `while` with `break`. Exit `2`.
- [`showcase/enum_records.nr`](showcase/enum_records.nr) — pattern matching
  (§3.6) deconstructing enums with associated data (§3.5, all three variant
  forms) alongside a struct with an enum field, `impl` methods, a fixed-size
  array + `for`-in loop, plus value/or/range/guard patterns. Exit `46`.
- [`showcase/unit_types.nr`](showcase/unit_types.nr) — newtype units of measure
  (§3.15) flowing through a struct with newtype fields, `impl` methods, an enum
  + `match`, and a fixed-size array + `for`-in loop. Exit `94`.

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
  its inner type. The inner type is limited to `Copy` types for now (§3.15).
- Struct and array destructuring patterns are supported (`types/destructuring.nr`):
  `val Point { x, y } = p` binds each field by name; `val [a, b, c] = arr` binds
  array elements positionally; `val [first, ..rest] = arr` captures the remainder as
  a fresh `[T; N - k]` array, and a bare `..` ignores it. A rest-less array pattern
  must match the array's length exactly.
- Move semantics, borrows (`&T`/`&mut T`), borrow exclusivity, lifetime elision, and deterministic `Drop` are implemented (sub-phase 1C). Growable heap strings (`String` builder) and owning collections (`Vec`, `HashMap`) are not yet implemented (1G).
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
