# Variables

Neuro binds names with three keywords: `const` for compile-time constants, `val` for immutable bindings and `mut` for mutable ones. This page covers declaration, reassignment, destructuring, type annotations, scope and initialization. Moving a value from one binding to another is covered in [Ownership and Borrowing](ownership.md#moves).

## Constants

`const` declares a compile-time constant, either at module level or inside a function body. It needs an explicit type annotation, and its value must be a constant expression.

```neuro
const MAX_RETRIES: i32 = 3
const BASE: i32 = 10
const DOUBLED: i32 = BASE * 2

func train() -> i32 {
    const FACTOR: i32 = 4
    return MAX_RETRIES * FACTOR
}
```

- Module-level constants are visible to every function, whatever the source order.
- A function-body constant is visible from its declaration to the end of its block.
- A constant may use the names of constants declared before it.
- An integer constant must fit its type. An initializer that overflows, or divides by zero, is refused with `constant expression has no value`.
- Constants take no part in ownership or borrow checking.

### Constant Expressions

A constant's value is built from these forms:

| Form | Example |
|---|---|
| Integer literal | `42`, `0xFF`, `0b1010` |
| Float literal | `3.14`, `1.0e-3` |
| Bool literal | `true`, `false` |
| String literal | `"hello"` |
| Unary op on a constant | `-MAX`, `!FLAG` |
| Binary op on constants | `A * 2`, `A + B` |
| Cast of a constant | `BASE as f32` |
| Another `const` name | `DOUBLED` |

Function calls and run-time values are not constant expressions.

### Constants and Variables

| | `const` | `val` | `mut` |
|---|---|---|---|
| Mutable | no | no | yes |
| Scope | module or function | function/block | function/block |
| Initializer | constant expression only | any expression | any expression |
| Ownership | none | yes | yes |
| Emitted as | global constant | stack slot | stack slot |

## Declaring Variables

`val` declares an immutable binding and `mut` a mutable one. Both are statements inside a function body; a module-level binding is always a `const`.

```neuro
func main() -> i32 {
    val limit: i32 = 10
    mut total: i32 = 0
    total = total + limit
    return total
}
```

- Every `val` and `mut` takes an initializer.
- A name cannot be a keyword, or one of the words held back for later: `async`, `await`, `parallel`, `spawn`, `defer`, `extern`, `static`, `macro` and `yield`.
- The type annotation is optional when the initializer determines the type (see [Type Annotations](#type-annotations)).
- `mut` makes the binding reassignable. It does not change its type.
- A `mut` pattern makes every binding in it mutable (see [Destructuring](#destructuring)).

## Reassignment

A `mut` binding takes a new value with `=`. The new value must have the binding's type.

```neuro
func counter() -> i32 {
    mut count = 0
    count = count + 1
    count = count * 10
    return count
}
```

A binding is one kind of assignment target. Fields, elements and tensor coordinates are targets too: see [places](operators.md#places-what-may-sit-on-the-left).

## Destructuring

A `val` or `mut` may bind a pattern instead of a single name:

```neuro
struct Point { x: i32, y: i32 }

func destructure() -> i32 {
    val (a, b) = (3, 4)
    val p = Point { x: 5, y: 6 }
    val Point { x, y } = p
    val [first, second, ..rest] = [1, 2, 3, 4]
    val [head, ..] = [7, 8]
    val [_, mid, _] = [1, 2, 3]
    mut (lo, hi) = (1, 2)
    lo = lo + 1
    return a + b + x + y + first + second + rest[0] + head + mid + lo + hi
}
```

- Tuple patterns bind positionally.
- Struct patterns bind each named field under its own name. There is no renaming form: `Point { x: px }` is a parse error.
- Array patterns bind positionally. A rest-less array pattern must bind every element. Add `..rest` to capture the remainder as a fresh array, or a bare `..` to ignore it. `_` discards one element.
- Patterns nest, so an element may itself be a tuple, struct or array pattern.

### Refutable Patterns

A `val` pattern may fail to match when the value is an `Option` or a similar enum. The `else` branch runs when it does not match, and must leave the scope:

```neuro
func first_or_zero(optional: Option<i32>) -> i32 {
    val Some(value) = optional else {
        return 0
    }
    return value
}
```

The full form, including the optional `|binding|`, is described in [`val-else`](control-flow.md#val-else-unwrap-or-leave-the-scope).

## Type Annotations

Annotations are optional when the initializer fixes the type:

```neuro
struct Point { x: i32, y: i32 }

func annotations() -> i32 {
    val x: i32 = 42           // explicit
    val n = 100               // i32: the default for integer literals
    val ratio = 3.14          // f64: the default for float literals
    mut count = 0             // i32
    val p = Point { x: 1, y: 2 }   // Point
    return x + n + count + p.x
}
```

A `bool`, `string` or struct initializer gives the binding its own type. An integer literal with nothing else to go on is `i32`, and a float literal is `f64`. An annotation must fit its value: `val x: u8 = 300` is refused with `integer literal 300 out of range for type u8`.

Function parameters always need type annotations.

## Scope and Shadowing

A name is visible from its declaration to the end of the block that contains it. A function's locals are not visible to other functions.

```neuro
func blocks() -> i32 {
    val x: i32 = 1
    if true {
        val y: i32 = 2
    }
    return x
}
```

### Shadowing

A `val` or `mut` whose name is already in scope declares a new binding. The new binding may have a different type, and it may reuse a name from the same block:

```neuro
func reparse() -> i32 {
    val n = "42"
    val n = n.len() as i32   // the initializer still reads the first `n`
    return n                 // 2
}
```

A `val` inside a block shadows an outer `mut` of the same name. It does not assign to it:

```neuro
func shadow_mut() -> i32 {
    mut total: i32 = 0
    if true {
        val total: i32 = 10   // a new binding, gone at this brace
    }
    return total              // 0
}
```

To assign to the outer `mut`, drop the `val` and write `total = 10`.

A shadowed binding is not dropped when it is shadowed. It stays alive until its block ends, including under any borrow taken of it. The new initializer runs before the new name exists, so `val s = s` moves the outer `s` into the new binding.

## Initialization

Every binding is initialized where it is declared. The initializer may be any expression, including `if` and blocks:

```neuro
func pick(flag: bool) -> i32 {
    val x: i32 = if flag { 42 } else { 0 }
    return x
}
```

## Common Mistakes

Each example shows the diagnostic the compiler reports, with its exact text.

```neuro
val x: i32 = 10
x = 20   // error: cannot assign to immutable variable 'x'
```

```neuro
mut y: i32 = 10
y = 3.14   // error: type mismatch: expected i32, found f64
```

```neuro
func main() -> i32 {
    val z
    return 0
}   // error: variable 'z' used without initialization
```

```neuro
const N = 5   // error: unexpected token Equal, expected ':'
```

```neuro
func f() -> i32 { 3 }
const C: i32 = f()   // error: constant expression required: only literals, arithmetic on literals, and references to other constants are allowed
```

```neuro
func main() -> i32 {
    val [a, b] = [1, 2, 3]   // error: array destructuring pattern binds 2 element(s) but the array has 3: list every element or add a `..rest` pattern
    return a
}
```

```neuro
func main() -> i32 { return 0 }
val TOP: i32 = 1   // error: unexpected token Val, expected function, struct, enum, impl, const, type, newtype, module, or import definition
```

## See also

- [Ownership and Borrowing](ownership.md): moves, borrows and `.clone()`
- [Types](types.md): primitive types and inference
- [Functions](functions.md): parameters and local bindings
- [Control flow](control-flow.md): `val ... else` and block scope
- [Operators](operators.md): assignment targets
