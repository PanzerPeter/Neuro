# Strings

Neuro has two text types: the immutable `string`, which programs pass around, and the growable `StringBuilder`, which they assemble text in. This page covers literals, interpolation, the string methods, slicing, character iteration and `StringBuilder`.

## String Type

A `string` is an immutable UTF-8 value: a pointer to its bytes plus a stored byte length. `==` and `!=` compare byte content, and `+` concatenates two strings into a new owned `string`. The ordering operators are not defined on `string`:

```neuro
val ordered = "a" < "b"   // error: cannot apply binary operator < to types string and string
```

String literals live in read-only program memory and are never allocated. A `string` produced at run time (by `+`, by an interpolation, or by `StringBuilder::to_string`) lives on the heap. [Memory model](memory-model.md) says which owner releases each one.

The stored length is authoritative. `len` counts the bytes of the content and excludes the trailing NUL that keeps the pointer usable as a C string. Interior NUL bytes are ordinary content, so `"a\0b".len()` is `3`.

## Literals

A `"..."` literal must close on the line it opens. Text across lines is written as a [triple-quoted](#triple-quoted-strings) literal or with `\n`.

### Escape Sequences

| Escape | Produces |
|---|---|
| `\n`, `\r`, `\t` | newline, carriage return, tab |
| `\\`, `\"` | backslash, double quote |
| `\0` | the NUL character (legal content, see [String Type](#string-type)) |
| `\{`, `\}` | a literal brace, since `{` opens an [interpolation hole](#string-interpolation) |
| `\xNN` | the code point U+00NN, two hex digits. Above `\x7F` it is encoded as two UTF-8 bytes, so `"\xFF".len()` is `2` |
| `\u{N...}` | the Unicode scalar value written in hex between the braces |

Any other character after `\` is a lexical error:

```neuro
val bad = "a\qb"   // error: invalid escape sequence '\q'
```

Triple-quoted literals take the same escapes.

A single-quoted `char` literal holds one code point, such as `'A'` or `'\u{1F44D}'`. See [Types](types.md) for `char`.

## String Interpolation

A `{...}` hole in a string literal holds an expression. The whole literal is a `string` built fresh each time it is reached:

```neuro
val name = "Neuro"
val version: i32 = 3
val message = "Welcome to {name} v{version}, next is {version + 1}"
```

A hole holds any expression: a call, a field access, an `if`, or a block. Two limits apply. A hole cannot contain a `"` literal, because the quote ends the enclosing literal and the hole is reported as ``interpolation hole opened with `{` is never closed``. An interpolated literal is not a constant, so it cannot appear in a pattern. Write a literal brace as `\{` or `\}`. An unescaped `}` outside a hole is a lexical error.

An optional `:spec` after the expression chooses the rendering. The shape is `[< > ^] [+] [0] [width] [.precision] [kind]`, where *kind* is one of `? e d x X b o`:

| Hole | Value | Renders |
|---|---|---|
| `{pi:.2}` | `3.14159` | `3.14` |
| `{pi:e}` | `3.14159` | `3.14159e0` |
| `{n:x}` | `255` | `ff` |
| `{n:X}` | `255` | `FF` |
| `{n:b}` | `255` | `11111111` |
| `{n:o}` | `255` | `377` |
| `{n:08d}` | `255` | `00000255` |
| `{delta:+d}` | `-42` | `-42` (`+` always shows the sign) |
| `{delta:06d}` | `-42` | `-00042` (zero fill goes after the sign) |
| `{name:<8}` | `"Neuro"` | `Neuro   ` |
| `{name:>8}` | `"Neuro"` | `   Neuro` |
| `{name:^9}` | `"Neuro"` | `  Neuro  ` |
| `{name:?}` | `"Neuro"` | `"Neuro"` |

Which specifiers a value accepts depends on its type. Radix kinds (`d x X b o`) need an integer. `.N` and `e` need a float. `+` needs a signed integer or a float. A mismatch is a compile error:

```neuro
val n: i32 = 7
println("{n:.2}")   // error: format specifier `.N` does not apply to a value of type i32: fixed-point and scientific notation apply to floats
```

Integers, floats (including `f16` and `bf16`), `bool`, `char` and `string` render directly. A struct renders only through its debug form, `{p:?}`, and only when it derives or implements `Debug`:

```neuro
@derive(Debug)
struct Point { x: i32, y: i32 }

func show() {
    val p = Point { x: 1, y: 2 }
    println("{p:?}")   // Point { x: 1, y: 2 }
}
```

Arrays, tuples and enums do not render yet. A `StringBuilder` is not a hole either: call `.to_string()` first.

See [`examples/strings/string_interpolation.nr`](../../examples/strings/string_interpolation.nr) for every specifier checked against its expected output.

## Triple-Quoted Strings

A `"""..."""` literal spans lines. Its value is dedented to the column of the closing `"""`, so a block can be indented to match the code around it:

```neuro
val name = "Neuro"
val block = """
    Hello from {name}.
    This spans multiple lines.
    """
// "Hello from Neuro.\nThis spans multiple lines."
```

The rules:

- The newlines that touch a delimiter are punctuation and are dropped: the one after the opening `"""` and the one before the closing line. To end a block with a newline, leave a blank line before the closing delimiter.
- Text after the opening `"""` on the same line is content. It is exempt from the dedent rule, since it sits flush against the delimiter.
- The closing `"""` must be alone on its line. Its indentation is stripped from every content line. Indentation deeper than that survives, so nested structure is kept.
- A blank line needs no indentation and normalizes to empty.
- A non-blank line indented less than the closing delimiter is an error:

```neuro
func main() -> i32 {
    val block = """
  shallow
    """
    return 0
}
// error: is indented less than the closing `"""`
```

- A `"` or `""` inside the block needs no escape. Only `"""` ends it.

Escapes and `{...}` holes work as in a `"..."` literal. A block string produces the same `string` value, and the same run-time cost.

See [`examples/strings/triple_quoted.nr`](../../examples/strings/triple_quoted.nr) for each rule checked against its expected text.

## String Methods

Builtin methods dispatch on a `string` receiver with the usual `receiver.method()` syntax:

```neuro
val s: string = "hello, world"
val n: u64 = s.len()                  // 12
val copy: string = s.clone()          // equal to s
val hello: &string = s.slice(0..5)    // "hello", borrowed, no copy
val world: &string = s.slice(7..=11)  // "world", inclusive upper bound
```

| Method | Returns | Notes |
|---|---|---|
| `.len()` | `u64` | Byte count, read from the stored length in O(1). Excludes the NUL terminator. |
| `.clone()` | `string` | A fresh copy with its own bytes. Takes no arguments, so it chains: `"hi".clone().len()`. It is the way to keep using a value after a [move](ownership.md#moves). |
| `.slice(range)` | `&string` | Borrowed view by byte offsets. See [Slicing](#slicing). |
| `.char_slice(range)` | `&string` | Borrowed view by code point offsets. See [Slicing](#slicing). |
| `.chars()` | `Chars` | Iterator over Unicode scalar values. See [Iterating Characters](#iterating-characters). |
| `.char_indices()` | pair in a `for` head | Scalar with its byte offset. See [Iterating Characters](#iterating-characters). |
| `.parse::<T>()` | `Result<T, ParseError>` | The text read as a number or a `bool`. See [Parsing Text](#parsing-text). |

## Slicing

### `.slice(range)`

`.slice(range)` returns a borrowed `&string` into the receiver's bytes, with no copy. The range is exclusive (`a..b`) or inclusive (`a..=b`), and its indices are **byte offsets**. The result is an ordinary `&string`, so it chains (`s.slice(0..5).len()`) and compares byte-wise (`s.slice(0..5) == "hello"`).

Two rules are checked at run time, in debug and release builds alike. A violation panics, which aborts without unwinding:

- **Bounds:** the range must satisfy `0 <= start <= end <= len`. Otherwise the panic reads `string slice out of bounds`.
- **Code point alignment:** each endpoint must fall on a UTF-8 code point boundary. Otherwise the panic reads `string slice splits a UTF-8 code point`.

### `.char_slice(range)`

`.char_slice(range)` takes the same kind of range, but its indices count **Unicode code points**. It walks the UTF-8 data to find each endpoint, so it costs O(n) where `.slice` costs O(1). Use it when the indices came from counting characters, as in tokenizers. Use `.slice` when the offsets are already byte offsets or the text is known to be ASCII.

```neuro
val s = "héllo"                       // 5 characters, 6 bytes: 'é' takes two
val by_char = s.char_slice(0..3)      // "hél": three characters, four bytes
val by_byte = s.slice(0..3)           // "hé": three bytes
val tail = s.char_slice(3..=4)        // "lo", inclusive upper bound
val empty = s.char_slice(5..5)        // "", the character count is a legal bound
```

Only the bounds rule applies here, and a violation panics with `string char slice out of bounds`. A code point index cannot name a position inside a code point, so there is no alignment rule.

A range `a..b` is not a value. It may be the argument of `.slice` or `.char_slice`, a `for` head, a tensor index or a `match` pattern, so `val r = 0..5` is a compile error.

## Iterating Characters

### `.chars()`

`text.chars()` iterates over the Unicode scalar values of a `string`. Each step decodes the code point at its byte offset and advances by that code point's own width, so no part of the text is read twice. The receiver is borrowed, so the text stays usable afterwards.

```neuro
val text = "héllo"             // 5 scalars, 6 bytes

mut scalars: i32 = 0
for c in text.chars() {
    scalars = scalars + 1
}

for (position, c) in text.chars().enumerate() {
    println("{position}: {c}")   // position counts code points
}
```

`Chars` is an ordinary prelude iterator, so it can also be held and stepped by hand. `.next()` returns `Option<char>`:

```neuro
mut walk = "ok".chars()
val first = walk.next() ?? '?'        // 'o'
```

In a `for` head, `.filter(p)` and `.map(f)` may follow the iterable.

### `.char_indices()`

`for (offset, c) in text.char_indices()` walks the same scalars and binds the **byte offset** of each one. Those offsets are what `.slice(range)` takes, so the pair suits tokenizers: find a position by reading characters, then cut by bytes. An offset names the code point its own step yields.

```neuro
mut cut: u64 = 0
for (offset, c) in "aé漢".char_indices() {
    if c == '漢' { cut = offset }     // 3: 'a' is one byte, 'é' two
}
```

`.char_indices()` is a `for`-head form, not a method that returns an iterator. It binds a pair there, and it takes no `.enumerate()` and no adapters, since it already carries a position. Use `.chars()` when you want a chain.

```neuro
val t = "abc"
for c in t.char_indices() {   // error: `.char_indices()` yields a position and a value; bind both with a pair pattern `for (index, value) in ...`
}
```

## Parsing Text

`.parse::<T>()` reads a string as a value of `T`, which is any integer type, `f16`, `bf16`, `f32`, `f64` or `bool`. It returns `Result<T, ParseError>`, so it works with `?`, `??`, `match` and [`val ... else`](control-flow.md):

```neuro
val port = "8080".parse::<i32>() ?? 80           // 8080
val fallback = "not a port".parse::<i32>() ?? 80 // 80

func area(width: &string, height: &string) -> Result<f64, ParseError> {
    val w = width.parse::<f64>()?
    val h = height.parse::<f64>()?
    Ok(w * h)
}
```

The receiver is read, not moved. A `&string`, a `.slice` view and a temporary all work. The turbofish is required, and `T` must be a concrete type, so `.parse::<T>()` on a type parameter is an error.

The text must be the whole value. Whitespace around it is `Invalid`.

| `T` | Accepted text |
|---|---|
| Integers | An optional `+` or `-`, then ASCII decimal digits. A `0x` prefix or a `_` is `Invalid`. |
| Floats | An optional sign, digits with an optional `.` (at least one digit), an optional `e` or `E` exponent. Also `inf`, `infinity` and `nan` in any case, the words `{x}` prints. |
| `bool` | Exactly `true` or `false`. |

A float is rounded once, to nearest with ties to even, directly to `T`. An `f16` or `bf16` does not pass through a wider type on the way, and every platform gives the same bits. A value past `T`'s range rounds to an infinity or a zero as IEEE 754 says, so `"1e400".parse::<f64>()` is `Ok` with infinity.

A failure names its reason:

| `ParseError` variant | Meaning |
|---|---|
| `ParseError::Empty` | The text is empty. |
| `ParseError::Invalid` | The text is not a `T`. |
| `ParseError::OutOfRange` | The text is a number that `T` cannot hold: `"300"` as a `u8`, or `"-1"` as any unsigned type. |

```neuro
match "300".parse::<u8>() {
    Ok(n) => println("{n}"),
    Err(ParseError::OutOfRange) => println("too large"),
    Err(e) => println("failed: {e:?}")
}
```

`{e:?}` prints the variant's name. The variants always take the `ParseError::` prefix. They are not bare names the way `Some` and `Ok` are.

A program may declare its own `ParseError` or `Result`, as with any prelude name. A call to `.parse` in that program is then a compile error, unless the declaration has the same variants as the prelude's.

## Growable Strings (`StringBuilder`)

`string` is immutable, which makes it cheap to pass, slice and share, but wrong for text that is assembled. Writing `s = s + piece` in a loop copies everything accumulated so far on every step, so building an n-piece string copies O(n²) bytes. `StringBuilder` is the growable counterpart: an owned, mutable, heap-backed UTF-8 buffer that appends in amortized O(1). It mirrors `[T; N]` and `Vec<T>` for the same reason.

```neuro
func build_report(name: string) -> string {
    mut report = StringBuilder::new()     // no annotation: it takes no type arguments
    report.push_str("run: ")
    report.push_str(name)                 // `name` is read, not moved
    report.push_str(" ok")
    return report.to_string()
}
```

`StringBuilder` is a compiler-known library type, the same status `Vec<T>` has. It is not a keyword, and a program that declares its own `StringBuilder` shadows it.

| | `string` | `StringBuilder` |
|---|---|---|
| Contents | immutable | mutable, appendable |
| Representation | `{ ptr, i64 }` fat pointer, by value | header owning a heap buffer |
| Grows | never | amortized O(1) per append |
| Role | the text a program passes around | the buffer a program builds text in |

### `StringBuilder` Methods

| Method | Returns | Notes |
|---|---|---|
| `StringBuilder::new()` | `StringBuilder` | Empty. Allocates nothing until the first append. |
| `.push_str(text)` | nothing | Appends a `string` or a `&string`. The argument is read, not moved. Needs a `mut` binding or a `&mut StringBuilder`. |
| `.len()` | `u64` | Byte length, read from the header in O(1). |
| `.clear()` | nothing | Resets the length and keeps the buffer, so refilling does not reallocate. |
| `.to_string()` | `string` | Copies the bytes into a fresh owned immutable `string`. |

`.to_string()` copies because a borrowed view into the buffer would dangle after a later `push_str` reallocates, and the borrow checker does not yet track a builder's outstanding views. The copy is one allocation at the end of a build, not one per append.

### Ownership

`StringBuilder` owns its buffer and follows the ordinary rules. It is never `Copy`, so assignment and argument passing move it. `&StringBuilder` and `&mut StringBuilder` borrow it. The buffer is freed when the owner leaves scope. A `Vec<StringBuilder>` may hold builders as elements, and the vector owns them.

```neuro
mut buf = StringBuilder::new()
buf.push_str("a")
val moved = buf
buf.push_str("b")   // error: use of moved value 'buf'
```

### Limitations

- No `.push(char)`, `StringBuilder::with_capacity(n)`, `StringBuilder::from(s)`, `.is_empty()` or `.as_str()`. Use `.to_string()` where a `string` is needed.
- A `StringBuilder` cannot be a map key: ``StringBuilder is not a valid `HashMap` key``.
- A `StringBuilder` is not an interpolation hole or a `+` operand. Call `.to_string()` first.

## See also

- [Types](types.md): where `string` and `char` sit among the primitives
- [Operators](operators.md): `+`, `==` and `!=` on strings
- [Ownership and Borrowing](ownership.md): moves, `.clone()` and borrows of strings
- [Memory model](memory-model.md): which owner releases each heap string and builder
- [Control flow](control-flow.md): `for` heads and iterators
- [`examples/strings/`](../../examples/strings/): a runnable program per feature on this page
