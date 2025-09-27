# Expressions

NEURO supports a comprehensive set of expressions for computation and program logic.

## Literal Expressions

### Integer Literals
- Decimal integers: `42`, `0`, `-15`
- Range: 32-bit signed integers (-2^31 to 2^31-1)

### Float Literals
- Decimal notation: `3.14`, `0.0`, `-2.5`
- Precision: 64-bit IEEE 754 double-precision

### String Literals
- Double-quoted strings: `"hello"`, `"world"`
- Escape sequences supported: `\n`, `\t`, `\"`, `\\`
- Unicode: UTF-8 encoded

### Boolean Literals
- True: `true`
- False: `false`

## Identifier Expressions

Variable and function name references:
```neuro
let x = 42;
let y = x;  // identifier expression referring to x
```

## Unary Operations

- Arithmetic negation: `-expr`
- Logical NOT: `!expr`

## Binary Operations

### Arithmetic Operations
- Addition: `+`
- Subtraction: `-`
- Multiplication: `*`
- Division: `/`
- Modulo: `%`

### Comparison Operations
- Less than: `<`
- Less than or equal: `<=`
- Greater than: `>`
- Greater than or equal: `>=`
- Equality: `==`
- Inequality: `!=`

### Logical Operations
- Logical AND: `&&`
- Logical OR: `||`

## Function Calls

Function call expressions with zero or more arguments:
- No arguments: `f()`
- Multiple arguments: `g(a, b)`
- Nested calls supported

## Parentheses

Parenthesized expressions for controlling evaluation order:
- `(expr)` - groups expressions
- Overrides operator precedence

## Operator Precedence

From highest to lowest precedence:
1. Unary operators: `-`, `!`
2. Multiplicative: `*`, `/`, `%`
3. Additive: `+`, `-`
4. Comparisons: `<`, `<=`, `>`, `>=`
5. Equality: `==`, `!=`
6. Logical AND: `&&`
7. Logical OR: `||`

## Match Expressions

Pattern matching expressions allow conditional logic based on value matching:

```neuro
fn classify(x: int) -> int {
    return match x {
        0 => 100,
        1 => 200,
        2 => 300,
        _ => 999
    };
}
```

Match expressions:
- Evaluate the input expression once
- Compare against patterns in order
- Return the result of the first matching pattern
- Support literal patterns and wildcard (`_`)
- Can be nested and used in any expression context

## Examples

```neuro
fn main() -> int {
    // Arithmetic with precedence
    let x = (2 + 3) * 4;        // x = 20

    // Logical expressions
    let ok = (x > 10) && (x == 20);  // ok = true

    // Pattern matching
    let category = match x {
        0 => 1,
        20 => 2,
        _ => 3
    };

    // Match in function calls
    let result = add(
        match x { 20 => 10, _ => 0 },
        match category { 2 => 5, _ => 1 }
    );

    // Complex expressions
    let complex = -x + y / 2;
    let valid = !finished || (count > 0 && ready);

    return result;
}
```

## Current Limitations

- Member access (`.`) expressions are parsed but not yet fully implemented
- Array/tensor indexing (`[]`) expressions are parsed but not yet fully implemented
- Method call expressions are not yet implemented
- Lambda expressions are not yet implemented

