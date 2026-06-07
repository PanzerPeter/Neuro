# Neuro Example Programs

Runnable `.nr` programs demonstrating the language. Each program returns an
`i32` from `main`, which becomes the process **exit code** — that is how every
example asserts its own result, and how the test harness checks it.

## Layout

Examples are grouped by topic so the set stays navigable as it grows:

| Directory        | What it covers                                                         |
| ---------------- | ---------------------------------------------------------------------- |
| `basics/`        | First programs: functions, variables, arithmetic, recursion, inference |
| `types/`         | Primitive types, literal suffixes, separators, casts, overflow, strings, move semantics, `@derive(Copy, Clone)`, type aliases |
| `operators/`     | Bitwise ops, compound assignment, integer intrinsic methods            |
| `control_flow/`  | `if`/`else`, `for`-ranges, `while`, block & `unsafe` expressions, lints, `panic`/`assert`/`unreachable` |
| `structs/`       | Struct definition, field access/mutation, `impl` methods               |
| `showcase/`      | **Bigger programs that combine many features at once**                 |

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

- No arrays yet (Phase 2+).
- Ownership/borrow checker not yet implemented (Phase 1.7).
- `&mut self` and consuming `self` on methods not yet supported — methods are
  `&self` only, so struct state is mutated by assigning to fields directly on a
  `mut` binding (see `showcase/running_stats.nr`).
- Right shift is the `.shr(n)` method, not a `>>` operator (Phase 2+).
- Prefer `return` over a tail-position `if`/`else` *expression* as a function's
  implicit return value; assign it to a `val` first if you need the value form
  (`val r = if c { a } else { b }`). The examples follow this convention.

## See also

- [Language Reference](../docs/language-reference/types.md)
- [CHANGELOG](../CHANGELOG.md)
- [Compiler Documentation](../docs/README.md)
