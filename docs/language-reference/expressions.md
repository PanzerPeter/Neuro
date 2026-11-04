# Expressions

An expression is code that evaluates to a value. This page covers what expressions share: how a statement ends, which expression a block yields, and the order operands run in. The table names the page that owns each form.

## Expression Forms

| Form | Example | Owned by |
|------|---------|----------|
| Literals | `42`, `3.14`, `true`, `"hello"` | [Types](types.md) for numeric literals, [Strings](strings.md#literals) for strings |
| Names | `x`, `add` | [Variables](variables.md), [Functions](functions.md) |
| Operators | `a + b`, `-x`, `x ?? 0`, `f(x)?`, `x \|> f` | [Operators](operators.md) |
| Grouping | `(a + b) * c` | [Operators](operators.md#operator-precedence) |
| Cast | `n as f64` | [Types](types.md#type-conversion) |
| Calls | `add(5, 3)`, `Point::new(1, 2)` | [Functions](functions.md) |
| Method calls | `c.add(32)`, `"hello".len()` | [Structs](structs.md#impl-blocks) for user methods, [Strings](strings.md#string-methods) and [Types](types.md#integer-methods) for builtin receivers |
| Struct literals | `Point { x: 3.0, y: 4.0 }` | [Structs](structs.md#struct-instantiation) |
| Field access | `p.x` | [Structs](structs.md#field-access) |
| Tuples | `(12, 30)`, `pair.0` | [Types](types.md#tuples) |
| `if` | `if n >= 0 { n } else { 0 - n }` | [Control flow](control-flow.md#if-as-an-expression) |
| `match` | `match shape { ... }` | [Control flow](control-flow.md#pattern-matching) |
| `if val`, `while val` | `if val Some(x) = lookup() { x }` | [Control flow](control-flow.md#if-val-and-while-val) |
| Loops | `loop`, `while`, `for` | [Control flow](control-flow.md#loops) |
| Blocks | `{ ... }` | [Control flow](control-flow.md#blocks-as-values) |
| Closures | `\|x\| x * x` | [Functions](functions.md#closures-and-lambdas) |

String literals can also hold `{...}` interpolation holes and span lines. See [String interpolation](strings.md#string-interpolation) and [Triple-quoted strings](strings.md#triple-quoted-strings).

### If Expressions

An `if` with an `else` produces a value, and every arm must agree on its type. An arm that leaves the scope (`return`, `break`, `continue`, `panic(...)` or `unreachable()`) is exempt from that agreement. An `if` without `else` has type `void`, so `val z = if c { 1 }` fails. See [Control flow](control-flow.md#if-as-an-expression).

### Match Expressions

The first arm whose pattern and optional `if` guard hold supplies the value. [Control flow](control-flow.md#pattern-matching) shows the patterns. The rules below are the ones that page does not state:

- **Patterns.** `_`, a bare name, a literal, an inclusive `a..=b` or exclusive `a..b` range over an ordered scalar, an enum variant pattern, `name @ pattern`, or an array pattern. Payloads, array elements and the right of `@` take any pattern.
- **Exhaustiveness.** The check reads inside payloads and arrays. The arms must cover every variant, both `bool` values and every slice length. An integer or `char`, at any depth, needs a binding or `_`. A guarded arm does not count. The error names a value no arm takes, as in ``no arm matches `Option::Some(Option::None)` ``. A match too complex to verify is an error.
- **Borrowed scrutinee.** A borrow is matched through at every level: `match d` with `d: &Dir` tests the `Dir`, and `Some(Dir::Up)` over `Option<&Dir>` does too. Binding an owning value out of a borrow is an error. Bind it with `_`, or match an owned value.
- **Limits.** The scrutinee must be an enum, integer, `char`, `bool`, array or borrowed slice, or a borrow of one. A `float` or `string` has no literal pattern, so compare one in a guard, as in `Some(s) if s == "x"`.

### Unsafe Block Expressions

`unsafe { }` is a block expression with the same value rules as a bare block. It has one effect: inside a `@kernel` body, indexing an output element (`out[i] = v`, `out[i] += v`, `val x = out[i]`) is legal only inside an `unsafe` block, which marks where you vouch that no other thread writes that element. See [Control flow](control-flow.md#unsafe-blocks) and [Writing a kernel](tensors.md#writing-a-kernel-kernel).

## Expression-Based Returns

The last expression in a block is its value. Neuro has no semicolons, so a statement ends at its newline, and a final expression needs no `return`:

```neuro
func add(a: i32, b: i32) -> i32 {
    a + b
}
```

Position decides, not punctuation. Earlier lines run for their effect, and only the last line yields the value. An explicit `return` is covered in [Functions](functions.md#return-values).

## Statement Boundaries

A newline ends a statement, and so does the end of a top-level item. This is why `struct A {} struct B {}` on one line is an error. An expression continues onto the next line only when the line that just ended asks for it: it ends with a binary operator, a comma, or an opening delimiter, or it is still inside an unclosed `(`, `[` or `{`:

```neuro
val total: i32 = 1 +
    2 +
    3                    // continues: each line ends with `+`

val sum: i32 = add3(
    1,
    2,
    3
)                        // continues: inside an unclosed `(`

val chained: i32 = cell
    .get()               // continues: a leading `.` cannot start a statement
```

The line that ended makes the decision, never the line after it. A line that starts with `(`, `[`, `*`, `-`, `&`, `|` or `@` begins a new statement, even when the line above could take it as a call, index, product, difference, bitwise operand or matrix product. To continue across one of those operators, end the line with it, or keep the expression inside an unclosed `(` or `[`:

```neuro
val a: i32 = f()
(2 + 3)                  // a new statement, not `f()(2 + 3)`
```

```neuro
func negated_double(a: i32) -> i32 {
    val scaled = a * 2
    -scaled              // the tail expression, not `a * 2 - scaled`
}
```

```neuro
val total = base -
    offset               // continues: the line ends with `-`
```

## Evaluation Order

Operands evaluate left to right, so the left operand of a binary operator runs before the right one. In this program `c` is read as `0` before `tick` raises it to `1`, so the result is `1`:

```neuro
func tick(c: &mut i32) -> i32 {
    *c = *c + 1
    *c
}

func main() -> i32 {
    mut c: i32 = 0
    val s = c * 10 + tick(&mut c)   // reads c first: 0 * 10 + 1
    s
}
```

Function arguments evaluate left to right, in the order they are written. Operators of equal precedence group left to right, so `a - b - c` is `(a - b) - c`. The short-circuit forms `&&`, `||` and `??` are covered in [Operators](operators.md).

## Operator Precedence

Operator precedence, including `@` and the pipeline operators, is in [Operators](operators.md#operator-precedence).

## See also

- [Operators](operators.md): operator reference and precedence
- [Control flow](control-flow.md): `if`, `match`, loops and blocks
- [Functions](functions.md): calls, closures and returns
- [Structs](structs.md): struct literals, field access and methods
- [Types](types.md): casts, tuples and literal types
