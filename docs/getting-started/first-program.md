# Your First Neuro Program

This tutorial takes you from an empty file to a program with functions, control flow and each
basic type. Each step builds on the one before, so work through them in order. You need the
compiler from the [Installation Guide](installation.md) and a text editor.

The commands use `neurc`. Without `cargo install`, substitute `cargo run -p neurc --`, as the
[Quick Start](quick-start.md) explains.

## Step 1: Create a Project Directory

```bash
mkdir neuro-projects
cd neuro-projects
```

## Step 2: Write Your First Program

Create a file named `hello.nr`:

```neuro
func main() -> i32 {
    return 0
}
```

This is the simplest valid Neuro program:

- `func` declares a function
- `main` is the entry point every executable needs
- `-> i32` is the return type, a 32-bit signed integer
- `return 0` exits with the success code

## Step 3: Check the Program

```bash
neurc check hello.nr
```

```text
Type checking passed for "hello.nr" (1 module(s), 22 HIR items)
```

The HIR item count includes the implicit prelude, so it is larger than the number of
declarations in your file.

## Step 4: Compile the Program

```bash
neurc compile hello.nr
```

```text
Successfully compiled hello.nr -> hello
```

On Windows the executable is `hello.exe`.

## Step 5: Run the Program

```bash
./hello                # Windows: .\hello.exe
echo $?                # PowerShell: echo $LASTEXITCODE
```

`echo` prints `0`, the exit code of a successful run.

## Variables

Bindings are declared with `val`, which is immutable, and the type can be annotated or left to
inference:

```neuro
func main() -> i32 {
    val x: i32 = 10
    val y: i32 = 20
    return x + y
}
```

`val count = 42` is an `i32` too. Annotate a binding when its type is not obvious at a glance.

Replace the contents of `hello.nr` with this program and run it:

```bash
neurc run hello.nr
echo $?                # 30
```

## Mutable Variables

`mut` declares a binding that can be reassigned:

```neuro
func main() -> i32 {
    mut counter: i32 = 0
    counter = counter + 1
    counter = counter + 2
    return counter
}
```

The program exits with `3`. Assigning to a `val` is an error (see [Common mistakes](#common-mistakes)),
and a reassigned value must match the variable's type.

## Functions

Parameters are `name: type` pairs. A call must pass one argument per parameter, each of the
matching type:

```neuro
func add(a: i32, b: i32) -> i32 {
    return a + b
}

func main() -> i32 {
    val result: i32 = add(5, 3)
    return result
}
```

This exits with `8`.

## Implicit Returns

The last expression in a function body is its return value, so `return` can be left out:

```neuro
func add(a: i32, b: i32) -> i32 {
    a + b
}

func main() -> i32 {
    add(5, 3)
}
```

The last expression has no `return` and no `;`. It must match the return type, and explicit
`return` statements can appear elsewhere in the body.

## Control Flow: If and Else

An `if` condition must be a `bool`. An `i32` there is a type error (`expected bool, found i32`).
`else` is optional, and `else if` chains conditions:

```neuro
func max(a: i32, b: i32) -> i32 {
    if a > b {
        return a
    } else {
        return b
    }
}

func classify(x: i32) -> i32 {
    if x > 0 {
        return 1
    } else if x < 0 {
        return -1
    } else {
        return 0
    }
}

func main() -> i32 {
    return classify(max(10, 5))
}
```

This exits with `1`. For loops, `match` and the other control forms, see the
[control flow reference](../language-reference/control-flow.md).

## Integers, Floats and Booleans

Integer types are `i8`, `i16`, `i32`, `i64` and their unsigned forms `u8`, `u16`, `u32`, `u64`.
`i32` is the default for an unannotated integer literal:

```neuro
func main() -> i32 {
    val tiny: i8 = 127
    val small: i16 = 32767
    val normal: i32 = 42
    val big: i64 = 9999999

    val byte: u8 = 255
    val word: u16 = 65535
    val dword: u32 = 42
    val qword: u64 = 99999

    return normal
}
```

Floats are `f32` and `f64`. A float literal is `f64` unless an annotated type gives it context,
so `2.0` below becomes `f32`. Mixing widths needs an explicit `as` cast:

```neuro
func main() -> i32 {
    val pi: f32 = 3.14159
    val e: f64 = 2.71828
    val result: f32 = pi * 2.0
    val wide: f64 = pi as f64 * 2.0
    return 0
}
```

Booleans are `true` and `false`, combined with `&&`, `||` and `!`:

```neuro
func main() -> i32 {
    val is_ready: bool = true
    val is_error: bool = false

    if is_ready && !is_error {
        return 1
    } else {
        return 0
    }
}
```

## Operators

Arithmetic is `+ - * / %`, comparisons are `== != < > <= >=`, and logic is `&& || !`. Integer
division truncates. The [operators reference](../language-reference/operators.md) covers
precedence and the rest of the operator set.

```neuro
func main() -> i32 {
    val a: i32 = 10
    val b: i32 = 3

    val sum: i32 = a + b       // 13
    val quot: i32 = a / b      // 3
    val rem: i32 = a % b       // 1
    val ge: bool = a >= 10     // true
    val both: bool = ge && b != 5

    if both {
        return sum
    }
    return 0
}
```

## Complete Example: Factorial

Combining the pieces so far:

```neuro
func factorial(n: i32) -> i32 {
    if n <= 1 {
        1
    } else {
        n * factorial(n - 1)
    }
}

func main() -> i32 {
    val result: i32 = factorial(5)
    result
}
```

This exits with `120`. [`examples/basics/factorial.nr`](../../examples/basics/factorial.nr) is the
same function with explicit returns, and it prints each multiplication as the recursion unwinds.

## Common Mistakes

### Returning a value from a function without a return type

A function with no `->` returns nothing:

```neuro
// error: return type mismatch: expected void, found i32
func bad() {
    return 0
}

func good() -> i32 {
    return 0
}
```

### Returning the wrong type

```neuro
// error: return type mismatch: expected i32, found bool
func wrong() -> i32 {
    return true
}

func right() -> i32 {
    return 42
}
```

### Assigning to an immutable variable

```neuro
// error: cannot assign to immutable variable 'x'
func bad() -> i32 {
    val x: i32 = 0
    x = 10
    return x
}

func good() -> i32 {
    mut x: i32 = 0
    x = 10
    return x
}
```

### Ending a statement with a semicolon

Neuro has no semicolons: a newline ends a statement, and a `;` is a parse error. A block's last
expression, with no `return`, is its value.

```neuro
func example() -> i32 {
    val x: i32 = 10
    x
}

// error: unexpected token Semicolon, expected a newline or '}' to end the statement
func wrong() -> i32 {
    val x: i32 = 10
    x;
}
```

## Exercises

### Fibonacci

Write `fibonacci(n)`, the nth Fibonacci number. `fibonacci(10)` is 55, so the program exits with
`55`.

<details>
<summary>Solution</summary>

```neuro
func fibonacci(n: i32) -> i32 {
    if n <= 1 {
        n
    } else {
        fibonacci(n - 1) + fibonacci(n - 2)
    }
}

func main() -> i32 {
    fibonacci(10)
}
```
</details>

### Power

Write `power(base, exp)` for x to the power of n. `power(3, 4)` is 81. On Unix the exit code keeps
only the low 8 bits, so a larger result such as `power(2, 8)`, which is 256, exits with `0`.

<details>
<summary>Solution</summary>

```neuro
func power(base: i32, exp: i32) -> i32 {
    if exp == 0 {
        1
    } else {
        base * power(base, exp - 1)
    }
}

func main() -> i32 {
    power(3, 4)
}
```
</details>

### Is Prime

Write `is_prime(n)` that returns 1 for a prime and 0 otherwise. `is_prime(17)` exits with `1`.

<details>
<summary>Solution</summary>

```neuro
func is_prime_helper(n: i32, divisor: i32) -> bool {
    if divisor * divisor > n {
        true
    } else if n % divisor == 0 {
        false
    } else {
        is_prime_helper(n, divisor + 1)
    }
}

func is_prime(n: i32) -> bool {
    if n <= 1 {
        false
    } else {
        is_prime_helper(n, 2)
    }
}

func main() -> i32 {
    if is_prime(17) {
        1
    } else {
        0
    }
}
```
</details>

## See also

- [Types](../language-reference/types.md): the type system
- [Functions](../language-reference/functions.md): declarations, closures and calls
- [Generics and traits](../language-reference/generics-and-traits.md): generic functions and trait bounds
- [Control flow](../language-reference/control-flow.md): loops, `match` and `val`-else
- [CLI Usage](../guides/cli-usage.md): every command and flag
