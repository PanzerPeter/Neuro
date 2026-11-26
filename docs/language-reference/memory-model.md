# Memory Model

Neuro has no garbage collector and no reference counting. Every value is either owned by one
place or borrowed from one, and the compiler inserts the release. This page records what that
machinery covers **today**, in alpha, and what it does not.

Related pages: [Ownership and Borrowing](ownership.md) for moves, borrows and `Drop`,
[Types](types.md) for slices and collections, [Control Flow](control-flow.md#pool-blocks) for
`pool` arena blocks, [Strings](strings.md) for the two string representations.

## What is reclaimed

| Value | When it goes back |
|---|---|
| Stack values (`Copy` primitives, structs of them, fixed arrays, tuples) | On return from the enclosing frame |
| String literals | Never allocated: they live in `.rodata` for the life of the program |
| `Vec<T>`, `HashMap<K, V>`, `BTreeMap<K, V>`, `StringBuilder`, `Tensor<T, [..]>` | Deterministic `Drop` at scope exit |
| A binding that is reassigned | The displaced value is destroyed at the assignment, before the new one is stored |
| Owners held inside a destroyed value | A struct field, array or tuple element, enum payload or newtype inner value goes back with the value that holds it |
| A value moved out of a position | Destroyed on the path it moved to, never twice |
| Allocations inside a `pool` block | One bump region, released in reverse order at the closing brace. A type with a destructor implements `PoolAware` to live in one. An allocation stored into something that outlives the block is routed to the heap instead |
| A heap `string`, wherever it is held | Like any other owner: with its binding, its holder (a struct field, an array or tuple element, an enum payload), its callee for a by-value argument, its caller for a return value, or the store that displaces it, through `&mut` included |
| A value in a collection slot | By the collection |

A `string` records in the top bit of its stored length whether it owns its buffer. A literal and a
view (a `&string`, a `.slice` or `.char_slice` result) own nothing, and a release skips them, so
the same rules cover a literal and a heap buffer in every position.

An anonymous heap `string` (what `+`, interpolation and `StringBuilder::to_string` produce) belongs to
no binding, so it is released at the end of the statement that made it unless the statement
moved it somewhere: at the consumer that reads and discards it (a `+` or `==` operand, a `.len()`
receiver, a `push_str` argument, a `println` argument, an interpolation hole, a statement whose
value nothing reads), or at the statement's end when a view of it is read there
(`(a + b).slice(0..2)`). A `for` over a view of one (`for c in (a + b).chars()`) keeps it for the
whole loop. Binding such a view to a name is a compile error, since it would outlive the buffer. A
loop that formats output holds a flat heap rather than a growing one.

A collection owns what its slots hold. `v.push(x)` and `m.insert(k, v)` move their arguments in.
Reads do not copy: `v[i]` names the slot, and `get` / `get_mut` lend it. `pop`,
`remove`, `swap_remove`, a displacing `insert` and a consuming `for x in v` hand a value over with
its ownership. A slot that is overwritten, removed, or cleared drops what it held, and the
collection drops its live slots when it is dropped.

## What still leaks

The shapes that still leak are open defects, each registered in [Known Bugs](../BUGS.md) with
a repro and a workaround. A `match` arm that binds part of what it matched
takes the scrutinee over, and once it is taken (its guard passed) it destroys the parts its
pattern left unbound: `A(x, _)` destroys the second field there. An arm that binds nothing leaves
a place scrutinee owning everything, destroyed at its own scope exit, and destroys a temporary
one on the spot.

## Open defects

Leaks and double frees found after a release are registered in [Known Bugs](../BUGS.md), each
with a repro and a workaround. Do not assume memory-safety guarantees beyond the table above
and that register. Memory-safety semantics and backend design are where contributions land
best: see [CONTRIBUTING.md](../../CONTRIBUTING.md).
