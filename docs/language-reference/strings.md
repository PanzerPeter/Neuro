# Strings

Neuro has two text types: the immutable `string` slice, and the growable `String` buffer.
The pair mirrors `[T; N]` / `Vec<T>`, and for the same reason.

## String Type

The `string` type is an immutable, UTF-8 encoded fat pointer `{ ptr, i64 }`, a pointer to
the bytes plus a stored byte length. Equality (`==`, `!=`) compares byte content; the `+`
operator concatenates two strings into a new owned `string`.

### Storage and the `len` Guarantee

String **literals** live in read-only program memory (`.rodata`) for the lifetime of the
program; they are **not** heap-allocated, so a program that only reads literals never leaks.
**Concatenation** (`a + b`) is the first runtime heap-backed string: it `malloc`s a fresh buffer
and copies both operands' bytes in, yielding a new owned `string`. Both literal and heap-backed
forms share the same `{ ptr, i64 }` ABI, so consumers cannot tell them apart. An anonymous heap
`string` (the result of `+`, of interpolation, or of `String::to_string`) is owned by no
binding the drop machinery tracks, so it still leaks; see the alpha memory warning in the README.
A [`String`](#growable-strings-string) builder is different: it *is* a tracked binding, so its
buffer is freed at scope exit.

The pointer addresses a NUL-terminated byte sequence so it doubles as a valid C string for
future FFI, but the stored `len` field **excludes** that trailing NUL. `len` is the
**authoritative** length: it is the exact UTF-8 byte count of the content. Consumers must use
`len` and **must not** scan for a NUL terminator, because interior NUL bytes are legal content
`"a\0b".len()` is `3`, not `1`.

### String Methods

Builtin intrinsic methods dispatch on a `string` receiver via the usual `receiver.method()`
syntax:

```neuro
val s: string = "hello, world"
val n: u64 = s.len()    // 12, O(1) read of the stored byte length
val copy: string = s.clone()   // a fresh string equal to s
val hello: &string = s.slice(0..5)    // "hello", borrowed, zero copy
val world: &string = s.slice(7..=11)  // "world", inclusive upper bound
```

**`.len() -> u64`**, returns the number of UTF-8 bytes, read directly from the fat pointer
in O(1) with no scan. The length **excludes** the null terminator. Because the index is a
byte count, a multi-byte code point contributes more than one to the length.

**`.clone() -> string`**, returns a fresh `string` equal to its receiver. It is the
canonical explicit deep copy for non-`Copy` owned types and, now that move-by-default has
landed (1C, see [variables](variables.md#move-semantics-ownership)), the way to
keep using a value after it would otherwise be moved. Today strings
are immutable and `.rodata`-backed (no heap string type exists yet), so the clone copies the
`(ptr, len)` fat pointer, observationally a deep copy because the pointee bytes are
immutable and shared safely. `.clone()` takes no arguments and returns a `string`, so it
chains with other builtin methods (`"hi".clone().len()`). `Copy` scalar types
(`i8`..`u64`, `f32`/`f64`, `bool`) do not provide `.clone()`: assignment already duplicates
them.

**`.slice(range) -> &string`**, returns a borrowed `&string` view into the receiver's UTF-8
data, with no allocation: since strings are immutable, a sub-range is just a `(ptr + start,
len)` fat pointer (the analogue of Rust's `&str`). The range is exclusive (`s.slice(a..b)`)
or inclusive (`s.slice(a..=b)`). **Indices are byte offsets**, not character offsets. The
slice is itself a `&string`, so it chains (`s.slice(0..5).len()`) and compares byte-wise
(`s.slice(0..5) == "hello"`). Two boundary rules are enforced at runtime in **both** debug and
release builds and **panic** (abort, no unwinding, see [control flow](control-flow.md)) on
violation:

- **Bounds:** the range must satisfy `0 <= start <= end <= len`. An out-of-bounds or reversed
  range panics with `string slice out of bounds`.
- **Code-point alignment:** each endpoint must fall on a UTF-8 code-point boundary. A range
  that splits a multi-byte code point panics with `string slice splits a UTF-8 code point`.

**`.char_slice(range) -> &string`**, the codepoint-indexed companion to `.slice`. It returns
the same borrowed, zero-copy `&string`, but its range counts **Unicode code points** rather
than bytes, walking the UTF-8 data to locate each endpoint: O(n) on the receiver's length,
where `.slice` is O(1). Use it whenever the indices came from counting characters (tokenizer
and NLP work); use `.slice` when the offsets are already byte offsets or the text is known to
be ASCII.

```neuro
val s = "héllo"                       // 5 characters, 6 bytes: 'é' takes two
val by_char = s.char_slice(0..3)      // "hél": three characters, four bytes
val by_byte = s.slice(0..3)           // "hé": three bytes
val tail = s.char_slice(3..=4)        // "lo", inclusive upper bound
val empty = s.char_slice(5..5)        // "", the character count is a legal bound
```

Only the **bounds** rule applies: the range must satisfy `0 <= start <= end <= character
count`, and a reversed or out-of-range range panics with `string char slice out of bounds`.
There is no code-point-alignment rule to break: a code point index cannot name a position
inside a code point, which is the reason to reach for this method in the first place.

A range expression `a..b` / `a..=b` is valid **only** as a `.slice` or `.char_slice`
argument; used anywhere else it is a compile error.

**`.chars() -> Chars`**, an iterator over the receiver's Unicode scalar values. Each step is
O(1): the cursor decodes the code point standing at its byte offset and advances by that code
point's own UTF-8 width, so no part of the text is scanned twice. `Chars` is an ordinary
`Iterator` from the prelude (see [control flow](control-flow.md)), which means a `for` head
drives it, `.enumerate()` numbers the scalars, `.map(f)` / `.filter(p)` decorate them, and the
iterator itself is a value that can be held and stepped by hand. The receiver is **borrowed**,
not consumed, so the text stays usable afterwards.

```neuro
val text = "héllo"

for c in text.chars() {
    println("{c}")                    // 5 scalars, though text.len() is 6 bytes
}

for (position, c) in text.chars().enumerate() { }   // position counts code points

mut walk = text.chars()
val first = walk.next() ?? '?'        // Option::Some('h')
```

**`for (offset, c) in text.char_indices()`**, the same walk with the **byte offset** of each
scalar bound alongside it. Those are the offsets `.slice(range)` takes, which is what makes the
pair the tokenizer's tool: find a position by reading characters, then cut by bytes. An offset
names the code point its step yields, never the one after it.

```neuro
mut cut: u64 = 0
for (offset, c) in "aé漢".char_indices() {
    if c == '漢' { cut = offset }     // 3: 'a' is one byte, 'é' two
}
```

`.char_indices()` is a **`for`-head form**, like `.enumerate()`, rather than a method: it binds
a pair, and a pair cannot travel through `Iterator::next`, whose `Option` payload is limited to
scalars in this phase. So it appears only in a `for` head, it binds a pair there (never a single
variable), and it takes no `.enumerate()` and no adapters: it already carries a position of its
own. Where a chain is wanted, walk `.chars()` instead.

## Growable Strings (`String`)

`string` is immutable, which makes it cheap to pass, slice, and share, but wrong for text that is
*assembled*. Writing `s = s + piece` in a loop allocates a new buffer and recopies everything
accumulated so far on every step, so building an n-piece string costs O(n²) bytes copied.

`String` is the growable counterpart: an owned, mutable, heap-backed UTF-8 buffer that appends in
amortized O(1). The pair mirrors `[T; N]` / `Vec<T>` exactly, and for the same reason.

```neuro
mut report = String::new()          // no annotation needed: `String` takes no type arguments
report.push_str("run: ")
report.push_str(name)               // `name` is read, not moved
report.push_str(" ok")

val line: string = report.to_string()   // finished text, immutable from here
```

`String` is a **compiler-known library type**, not a keyword and not a language primitive, the
same status `Vec<T>` has: the language exposes no allocator and no raw pointers, so nothing in
`.nr` source could implement it. A program that declares its own `String` shadows this one.

| | `string` | `String` |
|---|---|---|
| Contents | immutable | mutable, appendable |
| Representation | `{ ptr, i64 }` fat pointer, by value | `{ buffer, len, cap }` header owning the buffer |
| Grows | never | amortized O(1) per append |
| Role | the text a program passes around | the buffer a program builds text in |

### `String` Methods

**`String::new() -> String`**, an empty builder. Allocates nothing until the first append, so an
unused builder costs no heap traffic. It takes no type arguments, so unlike `Vec::new()` it needs
no annotation to be inferred.

**`.push_str(text)`**, appends the bytes of a `string` or an immutable `&string`. The argument is
**read, not moved** (the same latitude a `+` operand or a map lookup key gets), so the caller's
binding stays usable afterwards. It mutates, so it needs a `mut` binding or a `&mut String`.

**`.len() -> u64`**, the byte length, read from the header in O(1). Bytes, not characters, for the
same reason `string.len()` is.

**`.clear()`**, resets the length to zero and **retains** the buffer, so refilling in a loop does
not reallocate. This is what makes one builder reusable across iterations. It mutates.

**`.to_string() -> string`**, copies the accumulated bytes into a fresh owned immutable `string`.
This is the bridge back to `string`: everything that consumes text (`+`, `==`, `.len()`, a
`Vec<string>` element, a map key) takes the result. A borrowed view into the buffer would be
zero-copy, but a later `push_str` may reallocate and leave it dangling, and the borrow checker
does not yet track a builder's outstanding views, so the copy is the sound answer. It is one
allocation at the end of a build, not one per append.

### Ownership

`String` owns a heap buffer, so it follows the ordinary rules with no exceptions: it is never
`Copy`, assignment and argument passing **move** it, `&String` / `&mut String` borrow it, and the
buffer is freed when the owner leaves scope.

```neuro
mut buf = String::new()
buf.push_str("a")
val moved = buf          // buf is MOVED
// buf.push_str("b")     // COMPILE ERROR: use of moved value 'buf'
```

### Phase 1C Limitations

- No `.push(char)`, `String::with_capacity(n)`, `String::from(s)`, or `.is_empty()`.
- No borrowed `.as_str()`; use `.to_string()`.
- A `String` cannot be a collection element or a map key.
- `String` is not an interpolation hole or a `+` operand: call `.to_string()` first.


## References

- [Types](types.md): where `string` sits among the primitives, and `&[T]` slices generally
- [Operators](operators.md): `+` concatenation and comparison on strings
- [examples/strings/](../../examples/strings/): a runnable program per feature on this page
