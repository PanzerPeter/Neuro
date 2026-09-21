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
| Allocations inside a `pool` block | One bump region, released in reverse order at the closing brace. A type with a destructor implements `PoolAware` to live in one. An allocation stored into something that outlives the block is routed to the heap instead |
| A heap `string` stored into a struct field, array element or tuple element | With the holder, when the store into that position provably allocated |
| A heap `string` a function returns | By the caller, when every one of the function's return paths allocates |
| A heap `string` passed by value to a parameter the callee only reads | At the call it was built for |
| A `string` in a collection slot | By the collection, which owns a copy of the bytes rather than the operand's buffer |

An anonymous heap `string` (what `+`, interpolation and `String::to_string` produce) belongs to
no binding, so it is released at the consumer that reads and discards it: a `+` or `==` operand,
a `.len()` receiver, a `push_str` argument, a `println` argument, an interpolation hole, or a
statement whose value nothing reads. A loop that formats output holds a flat heap rather than a
growing one.

A collection never shares a buffer across its own boundary. `v.push(s)` and `m.insert(k, v)`
copy the bytes into slots of their own, which is why they read their arguments rather than moving
them and why the binding they came from is still usable afterwards; `v[i]`, `for x in v`,
`m.get(k)` and `m.keys()` copy back out, so a value read from a collection outlives it. `v.pop()`
is the one read that hands over the slot's own buffer, because the slot is gone with it. A slot
that is overwritten, removed, or cleared releases what it held.

## What still leaks

The storing positions above are covered only where the compiler can *prove* who owns the buffer.
Three shapes it cannot prove, each leaking one buffer rather than dangling one:

- a holder a call built (`val e = wrap(a + b)`), because the call says nothing about which of
  its positions own what;
- a function with one return path that hands back a literal, which disqualifies the whole
  function;
- a parameter the callee may store, return, or write through a reference.

Freeing a `.rodata` literal, or a buffer something else still holds, is a worse failure than
holding one, so every unproven case answers the same way.

A `string` a collection read hands into a view-producing method (`v[0].slice(...)`, `.chars()`,
`.clone()`) leaks its copy, as any other anonymous string does there, and a
`val PATTERN = m.get(k) else ...` binds a payload nothing releases.

A `match` arm that binds an enum payload also disowns the whole scrutinee, so a variant the arm
did not take is not destroyed.

## Status

Completing drop coverage is sub-phase 2E in the [Quick Roadmap](../../README.md#quick-roadmap).
Until it lands, do not assume memory-safety guarantees beyond the table above. Memory-safety
semantics and backend design are where contributions land best: see
[CONTRIBUTING.md](../../CONTRIBUTING.md).
