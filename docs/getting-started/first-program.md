# Your First NEURO Program

This tutorial walks you through writing, compiling, and running your first NEURO program.

## Prerequisites

- NEURO compiler installed ([Installation Guide](installation.md))
- Text editor or IDE
- Basic programming knowledge

## Step 1: Create a Project Directory

Create a directory for your NEURO programs:

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

This is the simplest valid NEURO program:
- `func` declares a function
- `main` is the entry point (required)
- `-> i32` specifies the return type (32-bit signed integer)
- `return 0` exits with success code

## Step 3: Check the Program

Validate syntax and types:

```bash
cargo run -p neurc -- check hello.nr
```

Expected output:
```
Type checking passed!
```

## Step 4: Compile the Program

Generate a native executable:

```bash
cargo run -p neurc -- compile hello.nr
```

Expected output:
```
Compilation successful: hello.exe
```

## Step 5: Run the Program

Execute your program:

```bash
# Windows
.\hello.exe

# Unix
./hello
```

The program runs and returns exit code 0 (success).

Check the exit code:

```bash
# Windows (PowerShell)
echo $LASTEXITCODE

# Unix
echo $?
```

Output: `0`

## Understanding the Program

Let's break down the hello.nr program:

```neuro
func main() -> i32 {
    return 0
}
```

- **`func`**: Keyword to declare a function
- **`main`**: Function name (required entry point)
- **`()`**: Empty parameter list
- **`-> i32`**: Return type annotation
- **`{...}`**: Function body
- **`return 0`**: Return statement with value

## Adding Variables

Let's make the program more interesting:

```neuro
func main() -> i32 {
    val x: i32 = 10
    val y: i32 = 20
    return x + y
}
```

**New concepts**:
- **`val`**: Declares an immutable variable
- **`x: i32`**: Variable name and type annotation
- **`= 10`**: Initializer expression
- **`x + y`**: Arithmetic expression

Compile and run:

```bash
cargo run -p neurc -- compile hello.nr
.\hello.exe  # Windows
echo $LASTEXITCODE  # Output: 30
```

## Using Mutable Variables

NEURO supports mutable variables with the `mut` keyword:

```neuro
func main() -> i32 {
    mut counter: i32 = 0
    counter = counter + 1
    counter = counter + 2
    return counter  // Returns 3
}
```

**New concepts**:
- **`mut`**: Declares a mutable variable
- **`counter = ...`**: Variable reassignment (only for mut variables)

**Key points**:
- Immutable by default (`val`)
- Explicit mutability (`mut`)
- Type safety: reassigned value must match variable type

## Adding Functions

Let's create a helper function:

```neuro
func add(a: i32, b: i32) -> i32 {
    return a + b
}

func main() -> i32 {
    val result: i32 = add(5, 3)
    return result
}
```

**New concepts**:
- **Function parameters**: `a: i32, b: i32`
- **Function calls**: `add(5, 3)`
- **Type checking**: Arguments must match parameter types

Compile and run:

```bash
cargo run -p neurc -- compile hello.nr
.\hello.exe
echo $LASTEXITCODE  # Output: 8
```

## Expression-Based Returns

NEURO supports implicit returns (Rust-style):

```neuro
func add(a: i32, b: i32) -> i32 {
    a + b  // Implicit return (no semicolon, no 'return' keyword)
}

func main() -> i32 {
    add(5, 3)  // Implicit return
}
```

**Key points**:
- Last expression without semicolon becomes the return value
- Must match function return type
- Can mix explicit `return` and implicit returns

## Control Flow: If/Else

Add conditional logic:

```neuro
func max(a: i32, b: i32) -> i32 {
    if a > b {
        return a
    } else {
        return b
    }
}

func main() -> i32 {
    val result: i32 = max(10, 5)
    return result
}
```

**New concepts**:
- **`if` conditions**: Must be boolean expressions
- **`else` blocks**: Optional alternative path
- **`else if`**: Chain multiple conditions

With else-if:

```neuro
func classify(x: i32) -> i32 {
    if x > 0 {
        return 1  // Positive
    } else if x < 0 {
        return -1  // Negative
    } else {
        return 0  // Zero
    }
}

func main() -> i32 {
    return classify(5)  // Returns 1
}
```

## Working with Different Types

### Integers

NEURO supports multiple integer types:

```neuro
func main() -> i32 {
    val tiny: i8 = 127        // 8-bit signed
    val small: i16 = 32767    // 16-bit signed
    val normal: i32 = 42      // 32-bit signed (default)
    val big: i64 = 9999999    // 64-bit signed

    val byte: u8 = 255        // 8-bit unsigned
    val word: u16 = 65535     // 16-bit unsigned
    val dword: u32 = 42       // 32-bit unsigned
    val qword: u64 = 99999    // 64-bit unsigned

    return normal
}
```

### Floats

```neuro
func main() -> i32 {
    val pi: f32 = 3.14159     // 32-bit float
    val e: f64 = 2.71828      // 64-bit float (default)

    val result: f64 = pi * 2.0
    return 0
}
```

Note: Currently, integer literals default to i32 and float literals default to f64. Type inference for numeric literals is pending.

### Booleans

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

### Arithmetic

```neuro
func main() -> i32 {
    val a: i32 = 10
    val b: i32 = 3

    val sum: i32 = a + b       // 13
    val diff: i32 = a - b      // 7
    val prod: i32 = a * b      // 30
    val quot: i32 = a / b      // 3
    val rem: i32 = a % b       // 1

    return sum
}
```

### Comparison

```neuro
func main() -> i32 {
    val x: i32 = 10

    val eq: bool = x == 10     // true
    val ne: bool = x != 5      // true
    val lt: bool = x < 20      // true
    val gt: bool = x > 5       // true
    val le: bool = x <= 10     // true
    val ge: bool = x >= 10     // true

    if eq {
        return 1
    } else {
        return 0
    }
}
```

### Logical

```neuro
func main() -> i32 {
    val a: bool = true
    val b: bool = false

    val and_result: bool = a && b   // false
    val or_result: bool = a || b    // true
    val not_result: bool = !a       // false

    if a && !b {
        return 1  // This executes
    } else {
        return 0
    }
}
```

## Complete Example: Factorial

Combining everything we've learned:

```neuro
func factorial(n: i32) -> i32 {
    if n <= 1 {
        1  // Implicit return
    } else {
        n * factorial(n - 1)  // Implicit return with recursion
    }
}

func main() -> i32 {
    val result: i32 = factorial(5)
    result  // Implicit return: 120
}
```

Compile and run:

```bash
cargo run -p neurc -- compile factorial.nr
.\factorial.exe
echo $LASTEXITCODE  # Output: 120
```

## Best Practices

### 1. Prefer Immutable Variables

```neuro
// Good
val x: i32 = 10

// Only use mut when necessary
mut counter: i32 = 0
counter = counter + 1
```

### 2. Use Explicit Types

While type inference is planned, currently explicit types are clearer:

```neuro
// Explicit and clear
val count: i32 = 42
```

### 3. Use Implicit Returns for Simple Functions

```neuro
// Clean and concise
func add(a: i32, b: i32) -> i32 {
    a + b
}

// Use explicit return for complex logic
func complex(x: i32) -> i32 {
    if x > 0 {
        return x * 2
    }
    return 0
}
```

### 4. Keep Functions Small

```neuro
// Good: focused functions
func is_positive(x: i32) -> bool {
    x > 0
}

func is_negative(x: i32) -> bool {
    x < 0
}

func classify(x: i32) -> i32 {
    if is_positive(x) {
        return 1
    } else if is_negative(x) {
        return -1
    } else {
        return 0
    }
}
```

## Common Mistakes

### 1. Forgetting Return Type

```neuro
// Error: missing return type
func bad() {
    return 0
}

// Correct
func good() -> i32 {
    return 0
}
```

### 2. Type Mismatch

```neuro
// Error: type mismatch
func wrong() -> i32 {
    return true  // bool, not i32
}

// Correct
func right() -> i32 {
    return 42
}
```

### 3. Assigning to Immutable Variable

```neuro
// Error: cannot assign to immutable variable
func bad() -> i32 {
    val x: i32 = 0
    x = 10  // Error: x is immutable
    return x
}

// Correct
func good() -> i32 {
    mut x: i32 = 0
    x = 10  // OK: x is mutable
    return x
}
```

### 4. Missing Semicolon vs Implicit Return

```neuro
func example() -> i32 {
    val x: i32 = 10  // Semicolon required for statements
    x  // No semicolon for implicit return
}

// Common mistake: semicolon on last expression
func wrong() -> i32 {
    val x: i32 = 10
    x;  // Error: returns void, not i32
}
```

## What's Next?

Now that you've written your first programs, explore:

1. **[Language Reference](../language-reference/types.md)** - Complete type system documentation
2. **[Functions Guide](../language-reference/functions.md)** - Advanced function features
3. **[Control Flow](../language-reference/control-flow.md)** - Conditional logic in detail
4. **[CLI Usage](../guides/cli-usage.md)** - Advanced compiler usage

## Practice Exercises

Try implementing these programs:

### Exercise 1: Fibonacci

Write a function that computes the nth Fibonacci number.

Expected output for `fibonacci(10)`: 55

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

### Exercise 2: Power

Write a function that computes x^n (x to the power of n).

Expected output for `power(2, 8)`: 256

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
    power(2, 8)
}
```
</details>

### Exercise 3: Is Prime

Write a function that checks if a number is prime.

Return 1 for prime, 0 for not prime.

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

## Getting Help

If you encounter issues:

1. Check the error message carefully - it includes the exact location and problem
2. Review the [Troubleshooting Guide](../guides/troubleshooting.md)
3. Consult the [Language Reference](../language-reference/types.md)
4. Report bugs: https://github.com/PanzerPeter/Neuro/issues
