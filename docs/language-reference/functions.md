# Functions

Functions are the primary unit of code organization in NEURO.

## Function Declaration

### Basic Syntax

```neuro
func function_name(param1: Type1, param2: Type2) -> ReturnType {
    // function body
    return value
}
```

**Components**:
- `func` keyword
- Function name (identifier)
- Parameter list (optional)
- Return type annotation (required unless void)
- Function body (block of statements)

### Simple Function

```neuro
func greet() -> i32 {
    return 0
}
```

### Function with Parameters

```neuro
func add(a: i32, b: i32) -> i32 {
    return a + b
}
```

### Function without Return Value

```neuro
func do_something() {
    // Implicit void return
    val x: i32 = 10
}
```

## Parameters

### Parameter Syntax

Each parameter requires:
1. Parameter name
2. Type annotation

```neuro
func process(input: i32, flag: bool) -> i32 {
    if flag {
        return input * 2
    } else {
        return input
    }
}
```

### Multiple Parameters

```neuro
func complex(a: i32, b: i32, c: i32, d: i32) -> i32 {
    return a + b + c + d
}
```

### Parameter Passing

In Phase 1, all parameters are passed by value (copied):

```neuro
func modify(x: i32) -> i32 {
    mut temp: i32 = x
    temp = temp + 10
    return temp  // Returns modified value, original unchanged
}

func test() -> i32 {
    val original: i32 = 5
    val result: i32 = modify(original)
    // original is still 5
    // result is 15
    return result
}
```

## Return Values

### Explicit Return

Use the `return` keyword to exit a function with a value:

```neuro
func explicit_return() -> i32 {
    return 42
}
```

### Multiple Return Points

```neuro
func conditional_return(x: i32) -> i32 {
    if x > 0 {
        return 1
    } else if x < 0 {
        return -1
    } else {
        return 0
    }
}
```

### Expression-Based Returns (Implicit Return)

The last expression in a function body (without semicolon) automatically becomes the return value:

```neuro
func implicit_return() -> i32 {
    42  // No semicolon, no 'return' keyword
}

func add(a: i32, b: i32) -> i32 {
    a + b  // Implicit return
}
```

**Mixing explicit and implicit returns**:

```neuro
func mixed(x: i32) -> i32 {
    if x > 10 {
        return 100  // Explicit return for early exit
    }
    x * 2  // Implicit return for normal case
}
```

**Key points**:
- Last expression without semicolon is the return value
- Must match function return type
- Can mix with explicit `return` statements
- Semicolon makes it a statement (not a return)

### Void Return

Functions without a return value:

```neuro
func no_return() {
    val x: i32 = 10
    // Implicit void return at end
}

func explicit_void() {
    val x: i32 = 10
    return  // Explicit void return
}
```

## Function Calls

### Basic Call

```neuro
func add(a: i32, b: i32) -> i32 {
    return a + b
}

func main() -> i32 {
    val result: i32 = add(5, 3)
    return result
}
```

### Nested Calls

```neuro
func double(x: i32) -> i32 {
    x * 2
}

func add(a: i32, b: i32) -> i32 {
    a + b
}

func main() -> i32 {
    val result: i32 = add(double(5), double(3))
    // double(5) = 10, double(3) = 6
    // add(10, 6) = 16
    return result
}
```

### Call Expressions

Function calls can appear anywhere an expression is expected:

```neuro
func compute() -> i32 {
    val x: i32 = add(1, 2) + add(3, 4)  // 3 + 7 = 10
    return x
}
```

## Recursion

### Basic Recursion

```neuro
func factorial(n: i32) -> i32 {
    if n <= 1 {
        1
    } else {
        n * factorial(n - 1)
    }
}

func main() -> i32 {
    factorial(5)  // Returns 120
}
```

### Tail Recursion

While NEURO doesn't yet optimize tail calls, you can write tail-recursive functions:

```neuro
func factorial_tail(n: i32, acc: i32) -> i32 {
    if n <= 1 {
        acc
    } else {
        factorial_tail(n - 1, n * acc)
    }
}

func factorial(n: i32) -> i32 {
    factorial_tail(n, 1)
}
```

### Mutual Recursion

```neuro
func is_even(n: i32) -> bool {
    if n == 0 {
        true
    } else {
        is_odd(n - 1)
    }
}

func is_odd(n: i32) -> bool {
    if n == 0 {
        false
    } else {
        is_even(n - 1)
    }
}
```

## The main Function

Every NEURO program requires a `main` function as the entry point:

```neuro
func main() -> i32 {
    // Program entry point
    return 0  // Exit code
}
```

**Requirements**:
- Must be named `main`
- Must return `i32` (exit code)
- Must not have parameters (Phase 1)

**Exit codes**:
- `0` = success
- Non-zero = error (convention)

## Function Scope

### Local Variables

Variables declared in a function are local to that function:

```neuro
func scoped() -> i32 {
    val x: i32 = 10  // Local to scoped()
    return x
}

func other() -> i32 {
    // val y: i32 = x  // Error: x not in scope
    return 0
}
```

### Parameter Scope

Parameters are local variables initialized with argument values:

```neuro
func params(a: i32, b: i32) -> i32 {
    // a and b are local variables
    val sum: i32 = a + b
    return sum
}
```

### Shadowing

Inner scopes can shadow outer scope variables:

```neuro
func shadowing() -> i32 {
    val x: i32 = 1
    if true {
        val x: i32 = 2  // Shadows outer x
        // Inner x is 2
    }
    // Outer x is still 1
    return x
}
```

## Type Checking

### Argument Type Checking

Function calls are strictly type-checked:

```neuro
func takes_i32(x: i32) -> i32 {
    return x
}

func test() -> i32 {
    val x: i64 = 100
    return takes_i32(x)  // Error: expected i32, found i64
}
```

### Return Type Checking

All return paths must match the declared return type:

```neuro
func type_checked(flag: bool) -> i32 {
    if flag {
        return 42      // OK: i32
    } else {
        return 3.14    // Error: expected i32, found f64
    }
}
```

### Argument Count Checking

```neuro
func two_params(a: i32, b: i32) -> i32 {
    return a + b
}

func wrong_count() -> i32 {
    return two_params(1)  // Error: expected 2 args, found 1
}
```

## Function Examples

### Mathematical Functions

```neuro
func abs(x: i32) -> i32 {
    if x < 0 {
        -x
    } else {
        x
    }
}

func max(a: i32, b: i32) -> i32 {
    if a > b { a } else { b }
}

func min(a: i32, b: i32) -> i32 {
    if a < b { a } else { b }
}

func clamp(x: i32, min_val: i32, max_val: i32) -> i32 {
    min(max(x, min_val), max_val)
}
```

### Boolean Logic Functions

```neuro
func and(a: bool, b: bool) -> bool {
    a && b
}

func or(a: bool, b: bool) -> bool {
    a || b
}

func xor(a: bool, b: bool) -> bool {
    (a && !b) || (!a && b)
}

func implies(a: bool, b: bool) -> bool {
    !a || b
}
```

### Recursive Functions

```neuro
func fibonacci(n: i32) -> i32 {
    if n <= 1 {
        n
    } else {
        fibonacci(n - 1) + fibonacci(n - 2)
    }
}

func gcd(a: i32, b: i32) -> i32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

func power(base: i32, exp: i32) -> i32 {
    if exp == 0 {
        1
    } else {
        base * power(base, exp - 1)
    }
}
```

## Best Practices

### 1. Keep Functions Focused

Each function should do one thing well:

```neuro
// Good: focused functions
func is_positive(x: i32) -> bool {
    x > 0
}

func absolute_value(x: i32) -> i32 {
    if is_positive(x) { x } else { -x }
}
```

### 2. Use Descriptive Names

```neuro
// Good: clear purpose
func calculate_area(width: i32, height: i32) -> i32 {
    width * height
}

// Bad: unclear
func calc(w: i32, h: i32) -> i32 {
    w * h
}
```

### 3. Use Expression Returns for Simple Functions

```neuro
// Good: concise
func double(x: i32) -> i32 {
    x * 2
}

// Verbose (but acceptable)
func double_verbose(x: i32) -> i32 {
    return x * 2
}
```

### 4. Use Explicit Returns for Complex Logic

```neuro
// Good: explicit returns for clarity
func complex_logic(x: i32) -> i32 {
    if x > 100 {
        return 100
    }
    if x < 0 {
        return 0
    }
    return x
}
```

### 5. Minimize Nesting

```neuro
// Good: early returns reduce nesting
func process(x: i32) -> i32 {
    if x < 0 {
        return 0
    }
    if x > 100 {
        return 100
    }
    return x * 2
}

// Bad: deeply nested
func process_nested(x: i32) -> i32 {
    if x >= 0 {
        if x <= 100 {
            return x * 2
        } else {
            return 100
        }
    } else {
        return 0
    }
}
```

## Common Mistakes

### Missing Return

```neuro
func missing_return(x: i32) -> i32 {
    if x > 0 {
        return x
    }
    // Error: missing return for x <= 0 case
}

// Fix: add else branch
func fixed(x: i32) -> i32 {
    if x > 0 {
        return x
    } else {
        return 0
    }
}
```

### Unreachable Code

```neuro
func unreachable() -> i32 {
    return 42
    val x: i32 = 10  // Warning: unreachable
}
```

### Semicolon on Implicit Return

```neuro
func wrong() -> i32 {
    val x: i32 = 42
    x;  // Error: semicolon makes this a statement, returns void
}

// Fix: remove semicolon
func right() -> i32 {
    val x: i32 = 42
    x  // Implicit return
}
```

## Future Features (Phase 2+)

### Default Parameters

```neuro
// Not yet implemented
func greet(name: string, greeting: string = "Hello") -> string {
    greeting + ", " + name
}
```

### Named Arguments

```neuro
// Not yet implemented
greet(name="Alice", greeting="Hi")
```

### Variadic Functions

```neuro
// Not yet implemented
func sum(values: ...i32) -> i32 {
    // Sum all arguments
}
```

### Higher-Order Functions

```neuro
// Phase 2: closures and function types
func apply(f: fn(i32) -> i32, x: i32) -> i32 {
    f(x)
}
```

### Generic Functions

```neuro
// Phase 2: generics
func identity<T>(x: T) -> T {
    x
}
```

## References

- [Types](types.md) - Function types and type checking
- [Variables](variables.md) - Local variables in functions
- [Control Flow](control-flow.md) - If/else and function control flow
- [Expressions](expressions.md) - Expression-based returns

## See Also

- Rust Book: [Functions](https://doc.rust-lang.org/book/ch03-03-how-functions-work.html)
- [Recursion](https://en.wikipedia.org/wiki/Recursion_(computer_science))
