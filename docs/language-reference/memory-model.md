# Memory Model

Neuro has no garbage collector and no reference counting. Every value is either owned by one
place or borrowed from one, and the compiler inserts the release. This page records what that
machinery covers **today**, in alpha, and what it does not.

Related pages: [Types](types.md) for borrows, slices and collections, [Variables](variables.md)
for moves and reassignment, [Control Flow](control-flow.md#pool-blocks) for `pool` arena blocks,
[Strings](strings.md) for the two string representations.

## What is reclaimed

| Value | When it goes back |
|---|---|
| Stack values (`Copy` primitives, structs of them, fixed arrays, tuples) | On return from the enclosing frame |
| String literals | Never allocated: they live in `.rodata` for the life of the program |
| `Vec<T>`, `HashMap<K, V>`, `BTreeMap<K, V>`, `String`, `Tensor<T, [..]>` | Deterministic `Drop` at scope exit |
| A binding that is reassigned | The displaced value is destroyed at the assignment, before the new one is stored |
| Owners held inside a destroyed value | A struct field, array or tuple element, enum payload or newtype inner value goes back with the value that holds it |
| A value moved out of a position | Destroyed on the path it moved to, never twice |
| Allocations inside a `pool` block | One bump region, released in reverse order at the closing brace. A type with a destructor implements `PoolAware` to live in one |

An anonymous heap `string` (what `+`, interpolation and `String::to_string` produce) belongs to
no binding, so it is released at the consumer that reads and discards it: a `+` or `==` operand,
a `.len()` receiver, a `push_str` argument, a `println` argument, an interpolation hole, or a
statement whose value nothing reads. A loop that formats output holds a flat heap rather than a
growing one.

## What still leaks

A heap `string` that escapes into a position able to store it: a collection element, a struct
field, a by-value call argument, or a function's return value.

Those are the positions where releasing would hand out a dangling pointer instead, so the
ownership test answers conservatively. Freeing a `.rodata` literal, or a buffer something else
still holds, is a worse failure than holding one.

A `match` arm that binds an enum payload also disowns the whole scrutinee, so a variant the arm
did not take is not destroyed.

## Status

Completing drop coverage is sub-phase 2E in the [Quick Roadmap](../../README.md#quick-roadmap).
Until it lands, do not assume memory-safety guarantees beyond the table above. Memory-safety
semantics and backend design are where contributions land best: see
[CONTRIBUTING.md](../../CONTRIBUTING.md).
