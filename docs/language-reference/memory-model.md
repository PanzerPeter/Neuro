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
| `Vec<T>`, `HashMap<K, V>`, `BTreeMap<K, V>`, `StringBuilder`, `Tensor<T, [..]>` | Deterministic `Drop` at scope exit |
| A binding that is reassigned | The displaced value is destroyed at the assignment, before the new one is stored |
| Owners held inside a destroyed value | A struct field, array or tuple element, enum payload or newtype inner value goes back with the value that holds it |
| A value moved out of a position | Destroyed on the path it moved to, never twice |
| Allocations inside a `pool` block | One bump region, released in reverse order at the closing brace. A type with a destructor implements `PoolAware` to live in one. An allocation stored into something that outlives the block is routed to the heap instead |
| A heap `string` stored into a struct field, array element or tuple element | With the holder, when the store into that position provably allocated or moved in a buffer its source binding owned |
| A heap `string` a function returns | By the caller, when every one of the function's return paths allocates |
| A heap `string` passed by value to a parameter the callee only reads | At the call it was built for |
| A value in a collection slot | By the collection; every `string` inside a slot owns its bytes, and a `.rodata` literal is copied on the way in |

An anonymous heap `string` (what `+`, interpolation and `StringBuilder::to_string` produce) belongs to
no binding, so it is released at the consumer that reads and discards it: a `+` or `==` operand,
a `.len()` receiver, a `push_str` argument, a `println` argument, an interpolation hole, or a
statement whose value nothing reads. A loop that formats output holds a flat heap rather than a
growing one.

A collection owns what its slots hold. `v.push(x)` and `m.insert(k, v)` move their arguments in;
a `string` the argument owned is adopted, and a literal is copied, so every `string` in a slot owns
its bytes. Reads do not copy: `v[i]` names the slot, and `get` / `get_mut` lend it. `pop`,
`remove`, `swap_remove`, a displacing `insert` and a consuming `for x in v` hand a value over with
its ownership. A slot that is overwritten, removed, or cleared drops what it held, and the
collection drops its live slots when it is dropped.

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

An anonymous `string` handed into a view-producing method (`(a + b).slice(...)`, `.chars()`)
leaks, because the release belongs at the end of the view's life, which nothing tracks yet.
`.clone()` is not such a method: it copies the bytes into a buffer of its own.

A struct taken out of a collection through an `Option` (`match v.pop()`, a map's `remove`) or
returned from a function that took it out does not release its `string` fields; one bound
straight from `v.remove(i)` does.

A `match` arm that binds part of an enum payload disowns the whole scrutinee, so a part of that
variant the arm did not bind is not destroyed. An arm that binds nothing leaves the scrutinee
owning everything, and it is destroyed at its own scope exit.

## Open defects

Leaks and double frees found after a release are registered in [Known Bugs](../BUGS.md), each
with a repro and a workaround. Do not assume memory-safety guarantees beyond the table above
and that register. Memory-safety semantics and backend design are where contributions land
best: see [CONTRIBUTING.md](../../CONTRIBUTING.md).
