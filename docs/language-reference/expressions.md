# Expressions

Expressions are code constructs that evaluate to a value.

## Expression Types

### Literals

Values written directly in code:

```neuro
42              // Integer literal (i32 by default)
3.14            // Float literal (f64 by default)
true            // Boolean literal
false           // Boolean literal
"hello"         // String literal (type: string)
```

### String Interpolation

A string literal may embed expressions in `{...}` holes. The whole literal is an
expression of type `string`, evaluated to a fresh owned string each time it is
reached:

```neuro
val message = "Welcome to {name} v{version}"
val report  = "Sum: {a + b}, Product: {a * b}"
```

A hole holds any expression (a call, a field access, a struct literal, an `if`,
a nested block): the lexer only finds the hole's bounds and the parser
re-parses its text as an ordinary expression. Write a literal brace as `\{` or
`\}`; an unescaped `}` outside a hole is an error, so a dropped `{` is caught
where it goes missing.

An optional `:spec` after the expression chooses the rendering. The shape is
`[< > ^] [+] [0] [width] [.precision] [kind]`, where *kind* is one of
`? e d x X b o`:

```neuro
"{pi:.2}"     // 3.14        (fixed point, 2 decimals)
"{pi:e}"      // 3.14159e0   (scientific)
"{n:x}"       // ff          (lowercase hex, n = 255)
"{n:08d}"     // 00000255    (zero-padded to width 8)
"{s:^10}"     // centred in a field of width 10
"{delta:+d}"  // +42 or -42  (always show the sign)
```

Which specifiers a value accepts depends on its type, and a mismatch is a
compile error rather than a surprising rendering: radix kinds (`d x X b o`)
require an integer, fixed-point and scientific (`.N`, `e`) require a float, and
the `+` flag requires a signed integer or a float. Interpolation renders
integers, floats, `bool`, `char`, and `string` directly, and a struct that
derives `Debug` under the `:?` specifier — `"{p:?}"` yields `Point { x: 1, y: 2 }`,
recursing into a nested struct and quoting a `string` or `char` field. A struct
has no display form, so the bare `"{p}"` is a compile error either way. Other
aggregates (arrays, tuples, enums) do not render yet.

Two limits are worth knowing. A hole may not contain a `"` string literal: the
quote ends the enclosing literal, and the hole is reported as unterminated. And
an interpolated literal is not a constant, so it cannot appear in a pattern.

See [`examples/types/string_interpolation.nr`](../../examples/types/string_interpolation.nr)
for every specifier checked against its expected output.

### Triple-Quoted Strings

A `"""…"""` literal spans lines. Its value is dedented to the column the closing
`"""` sits at, so a block can be indented to match the code around it without
that indentation ending up in the string:

```neuro
val block = """
    Hello from {name}.
    This spans multiple lines.
    """
// "Hello from Neuro.\nThis spans multiple lines.\n"
```

The rules:

- The newline directly after the opening `"""` is punctuation and is dropped.
  Text trailing the opening delimiter on the same line is content, and is exempt
  from the dedent rule — it sits flush against the delimiter and cannot be
  indented.
- The closing `"""` must be alone on its line. The whitespace before it is the
  prefix stripped from every content line; indentation beyond that prefix
  survives, so nested structure inside the block is preserved.
- A blank line needs no indentation of its own and normalizes to empty.
- A non-blank line indented less than the closing delimiter is a compile error.
- A single `"` or `""` inside the block needs no escape; only `"""` ends it.

Escapes and `{...}` interpolation holes work exactly as in a `"…"` literal — a
block string produces the same `string` value and carries no runtime cost of its
own.

See [`examples/types/triple_quoted.nr`](../../examples/types/triple_quoted.nr)
for each rule checked against its expected text.

### Identifiers

Variable and function names:

```neuro
x               // Variable reference
add             // Function reference
```

### Binary Expressions

Two operands with an operator:

```neuro
a + b           // Addition
x * y           // Multiplication
result == 42    // Comparison
flag && ready   // Logical AND
```

### Unary Expressions

Single operand with operator:

```neuro
-x              // Arithmetic negation
!flag           // Logical NOT
~mask           // Bitwise NOT (integer types only)
```

### Cast Expressions

Explicit numeric type conversion:

```neuro
n as f64        // Integer to float
pi as i32       // Float to integer (truncates toward zero)
flag as i32     // Boolean to integer (false → 0, true → 1)
```

### Function Call Expressions

```neuro
add(5, 3)                   // Free function call
double(square(x))           // Nested calls
max(min(a, b), c)           // Multiple nesting
c.add(32)                   // Method call (instance method)
Point::new(1, 2)            // Associated function call
"hello".len()               // Builtin method on a string receiver
```

Method-call syntax `receiver.method(args)` resolves against user-defined `impl` methods
when the receiver is a struct, and against a fixed, compiler-known set of intrinsic methods
when the receiver is a builtin type (`string.len()` and `.clone()`, the integer overflow
methods, `.slice()`, and more). See [types.md](types.md#string-methods) for the string set
and [types.md](types.md#integer-methods) for the integer set.

### Struct Literal Expressions

Construct a struct value by naming all fields:

```neuro
val p = Point { x: 3.0, y: 4.0 }
val c = Counter { value: 0, step: 1 }
```

All fields must be present; duplicate or unknown fields are compile errors.

### Field Access Expressions

Read a field from a struct value:

```neuro
val x = p.x          // reads field x, type is f64
val total = c.value + c.step
```

Field access binds tighter than function calls in the precedence table.

### Tuple Literals and Index Access

A tuple literal groups two or more values; element access uses a constant index:

```neuro
val pair = (12, 30)       // (i32, i32)
val mixed = (5, true)     // (i32, bool)
val a = pair.0            // 12
val b = pair.1            // 30
```

A single parenthesized expression `(x)` is grouping, not a one-element tuple.
Tuple index `t.N` binds like field access; because `t.0.1` lexes as the float
`0.1`, write a nested access as `(t.0).1`. Tuple destructuring (`val (a, b) = t`)
is a binding form covered under [Variables](variables.md) and [Types](types.md).

### If Expressions

`if`/`else` chains are expressions when every arm has an `else` branch:

```neuro
val abs_n: i32 = if n >= 0 { n } else { 0 - n }
val sign: i32  = if n < 0 { -1 } else if n == 0 { 0 } else { 1 }
```

All arms must produce the same type. An `if` without `else` has type `Void` and cannot be used as a value.

An arm that **leaves the scope** rather than producing a value, `return`, `break`,
`continue`, `panic(...)`, or `unreachable()`, is exempt: it never reaches the point
where the `if` has a value, so it neither supplies the type nor has to match it. The
remaining arms decide:

```neuro
func first_positive(n: i32) -> i32 {
    val v = if n > 0 { return 1 } else { 2 }   // the `if` is an i32; the then-arm returns
    v
}
```

### Match Expressions

`match` is an expression that exhaustively deconstructs a value. The first arm
whose pattern matches, and whose optional `if` guard holds, supplies the
value; all arm bodies must have the same type, except those that leave the scope
(`{ return ... }`, `{ break }`, `{ continue }`, `panic(...)`, `unreachable()`), which
supply no type at all.

```neuro
enum Shape { Circle(i32), Rect { w: i32, h: i32 }, Unit }

func area(s: Shape) -> i32 {
    match s {
        Shape::Circle(r)       => r * r * 3,   // tuple variant, binds `r`
        Shape::Rect { w, h }   => w * h,        // struct variant, binds `w`, `h`
        Shape::Unit            => 0
    }
}

func classify(n: i32) -> i32 {
    match n {
        0            => 1,        // literal
        1 | 2        => 2,        // or-pattern
        3..=9        => 3,        // inclusive range (`..` is exclusive)
        n if n < 0   => 4,        // bare binding + guard
        _            => 9         // wildcard catch-all
    }
}
```

**Patterns**: `_` (wildcard), a bare identifier (binds the whole scrutinee), a
literal, an inclusive `a..=b` / exclusive `a..b` range over an ordered scalar,
or an enum variant pattern (`E::Unit`, `E::Tuple(a, b)`, `E::Struct { field }`).

**Exhaustiveness**: every case must be handled. An enum match must cover every
variant or include a `_` arm; an integer/`char` match requires a `_` arm; a
`bool` match needs both `true` and `false` (or `_`). A guarded arm does not
count toward exhaustiveness.

**Phase 1E limits**: the scrutinee must be an enum, integer, `char`, or `bool`;
enum-payload sub-patterns must be bindings or `_` (match a payload *value* with a
guard, e.g. `Some(n) if n == 0`); and alternatives of an `|`-pattern may not
bind.

### Block Expressions

A `{ … }` block is an expression whose value is its final (trailing) expression:

```neuro
val area: i32 = {
    val w: i32 = 6
    val h: i32 = 7
    w * h           // trailing expression, this is the block's value
}
```

Locals declared inside a block are scoped to that block.

### Unsafe Block Expressions

An `unsafe { … }` block is a block expression prefixed with the reserved
`unsafe` keyword. It evaluates exactly like a bare block, its value is the
trailing expression, and its locals are block-scoped:

```neuro
val x: i32 = unsafe {
    val a: i32 = 20
    a + 22          // trailing expression, this is the block's value
}
```

`unsafe` is currently **inert**: it is a reserved keyword and produces a
distinct AST node, but carries no special semantics yet. It exists as 1C
groundwork for the GPU-kernel aliasing model (Phase 4), where `unsafe { }` will
gate raw `KernelOut` index writes. Until then it behaves identically to `{ }`.

### Parenthesized Expressions

Control evaluation order:

```neuro
(a + b) * c     // Force addition first
x / (y + z)     // Force addition before division
```

## Operator Precedence

Higher precedence operators bind first. Full table from highest to lowest, matching the
Pratt parser's ladder exactly:

| Level | Operators | Associativity | Description |
|-------|-----------|---------------|-------------|
| 16 (highest) | `.` | Left | Field / method access |
| 15 | call `f(…)`, index `a[i]`, postfix `?`, turbofish `::<…>` | Left | Postfix forms |
| 14 | `-` (unary), `!`, `~` | Right | Negation, logical NOT, bitwise NOT |
| 13 | `as` | Left | Type cast |
| 12 | `*`, `/`, `%` | Left | Multiply, divide, modulo |
| 11 | `+`, `-` | Left | Addition, subtraction |
| 10 | `<<` | Left | Left shift |
| 9 | `<`, `>`, `<=`, `>=` | Left | Comparison |
| 8 | `==`, `!=` | Left | Equality |
| 7 | `&` | Left | Bitwise AND |
| 6 | `^` | Left | Bitwise XOR |
| 5 | `\|` | Left | Bitwise OR |
| 4 | `&&` | Left | Logical AND |
| 3 | `\|\|` | Left | Logical OR |
| 2 | `??` | Right | Null/error coalescing |
| 1 (lowest) | `..`, `..=` | Left | Ranges |

Comparison binds tighter than equality: `x < y == z` parses as `(x < y) == z`. There is no
`>>` operator; right shift is the `.shr(n)` method because `>>` is reserved for function
composition.

**Examples**:

```neuro
a + b * c         // Parsed as: a + (b * c)
a & b + c         // Parsed as: a & (b + c)   (arithmetic before bitwise)
a | b & c         // Parsed as: a | (b & c)   (AND before OR)
a < b == c < d    // Parsed as: (a < b) == (c < d)
!a && b           // Parsed as: (!a) && b
a || b && c       // Parsed as: a || (b && c)
n as f64 + 1.0    // Parsed as: (n as f64) + 1.0
f(x)? + 1         // Parsed as: (f(x)?) + 1
```

## Expression-Based Returns

The last expression in a function body is the return value (Neuro has no semicolons; statements are newline-terminated):

```neuro
func add(a: i32, b: i32) -> i32 {
    a + b  // Implicit return
}

func max(a: i32, b: i32) -> i32 {
    if a > b {
        a  // Implicit return from if branch
    } else {
        b  // Implicit return from else branch
    }
}
```

**Key distinction**: it is positional, not punctuation-based:
- A bare expression in the **last** position of a block is its return value
- Any earlier line (a `val`/`mut` binding, assignment, or non-final expression) is a statement evaluated for its effect

```neuro
func example() -> i32 {
    val x: i32 = 42  // Statement (binding)
    x  // Last expression, implicit return
}
```

## Statement Boundaries

There are no semicolons: a newline ends a statement. An expression continues onto the
next line only when the line that just ended asks it to: it ends with a binary
operator, a comma, or an opening delimiter, or the expression is still inside an
unclosed `(`, `[`, or `{`:

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

The decision belongs to the line that ended, never to the line that follows. A line
*starting* with `(`, `[`, or `*` therefore begins a new statement: a parenthesized
expression, an array literal, or a dereference. It can never be a call, an index,
or a multiplication continuing the line above:

```neuro
val a: i32 = f()
(2 + 3)                  // a new statement, not `f()(2 + 3)`
```

## Type Checking

All expressions are type-checked at compile time:

```neuro
val x: i32 = 42 + 10        // OK: i32 + i32 = i32
val y: f64 = 3.14 * 2.0     // OK: f64 * f64 = f64
val z: bool = x > y         // Error: cannot compare i32 and f64
```

## Evaluation Order

Left-to-right evaluation for operators of same precedence:

```neuro
a + b + c       // Evaluates as: (a + b) + c
a - b - c       // Evaluates as: (a - b) - c
a * b / c       // Evaluates as: (a * b) / c
```

Function arguments are evaluated left-to-right:

```neuro
add(first(), second())  // first() called before second()
```

## Expression Examples

### Arithmetic Expressions

```neuro
val sum: i32 = a + b
val product: i32 = x * y * z
val average: i32 = (a + b + c) / 3
val remainder: i32 = n % 10
```

### Comparison Expressions

```neuro
val is_equal: bool = x == y
val is_greater: bool = a > b
val in_range: bool = x >= 0 && x <= 100
```

### Logical Expressions

```neuro
val both_true: bool = flag1 && flag2
val either_true: bool = flag1 || flag2
val inverted: bool = !flag
val complex: bool = (a && b) || (c && d)
```

### Nested Function Calls

```neuro
val result: i32 = add(mul(2, 3), div(10, 2))
val distance: i32 = abs(x1 - x2) + abs(y1 - y2)
```

## Common Patterns

### Chained Comparisons

```neuro
val in_range: bool = min <= value && value <= max
val outside_range: bool = value < min || value > max
```

### Conditional Values

```neuro
val sign: i32 = if x > 0 { 1 } else if x < 0 { -1 } else { 0 }
val abs_value: i32 = if x >= 0 { x } else { -x }
```

### Expression Composition

```neuro
val result: i32 = square(x) + square(y)
val total: i32 = sum(a, b) + sum(c, d)
```

## References

- [Operators](operators.md) - Detailed operator documentation
- [Types](types.md) - Expression type checking
- [Functions](functions.md) - Function call expressions
