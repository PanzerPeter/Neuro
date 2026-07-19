# Functions

Functions are the primary unit of code organization in Neuro.

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

The last expression in a function body automatically becomes the return value (Neuro has no semicolons; statements are newline-terminated):

```neuro
func implicit_return() -> i32 {
    42  // No 'return' keyword needed
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
- The last expression in the body is the return value
- Must match function return type
- Can mix with explicit `return` statements
- There are no semicolons; a stray `;` is a parse error

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

While Neuro doesn't yet optimize tail calls, you can write tail-recursive functions:

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

Every Neuro program requires a `main` function as the entry point:

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

### Stray Semicolon

```neuro
func wrong() -> i32 {
    val x: i32 = 42
    x;  // Error: `;` is not a valid token (Neuro has no semicolons)
}

// Fix: remove the semicolon
func right() -> i32 {
    val x: i32 = 42
    x  // Implicit return
}
```

## Future Features (Phase 1+)

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
// Phase 1: closures and function types
func apply(f: fn(i32) -> i32, x: i32) -> i32 {
    f(x)
}
```

### Generic Functions

```neuro
// Phase 1: generics
func identity<T>(x: T) -> T {
    x
}
```

## Panic Builtins (§1.2)

Three compiler-known builtins terminate the program when an unrecoverable condition is
reached. They follow the **abort, no unwinding** model: each prints a diagnostic
(`message at file:line:col`) to standard error and calls `abort()`. The stack is *not*
unwound, so future `Drop` / `defer` cleanup runs only on normal scope exit, never during a
panic.

```neuro
func divide(a: i32, b: i32) -> i32 {
    if b == 0 {
        panic("division by zero")
    }
    a / b
}

func main() -> i32 {
    assert(divide(10, 2) == 5)   // passes silently
    val x = divide(1, 0)         // prints "panic: division by zero at main.nr:3:9" and aborts
    return 0
}
```

| Builtin | Signature | Behaviour |
|---|---|---|
| `panic` | `panic(msg: string)` | Print `panic: <msg> at <loc>` and abort. |
| `assert` | `assert(cond: bool)` | Abort with `assertion failed at <loc>` only when `cond` is false; otherwise continue. |
| `unreachable` | `unreachable()` | Print `internal error: entered unreachable code at <loc>` and abort. |

Because these builtins **diverge** (they never return), a call may appear anywhere a value is
expected — including the implicit-return (tail) position of a non-`void` function:

```neuro
func parse_digit(c: i32) -> i32 {
    if c >= 48 && c <= 57 { c - 48 } else { panic("not a digit") }
}
```

A user-defined function whose name is `panic`, `assert`, or `unreachable` shadows the builtin
within the program.

## Generic Functions

A function may declare **type parameters** in angle brackets after its name. Each type
parameter stands for a type supplied at the call site; the compiler generates one specialized
copy of the function per distinct set of concrete type arguments (**monomorphization**), so a
type parameter carries **zero runtime cost**.

```neuro
// A single template, instantiated at each concrete type it is called with.
func identity<T>(x: T) -> T {
    x
}

// Multiple type parameters.
func second<T, U>(a: T, b: U) -> U {
    b
}

func main() -> i32 {
    val a = identity(41)     // identity<i32>
    val f = identity(2.5)    // identity<f64> — a separate specialized copy
    val s = second(1.5, 7)   // second<f64, i32> -> 7
    return a + s             // 41 + 7 = 48
}
```

**Type-argument inference.** Type arguments are inferred from the call's value arguments — you do
not write them explicitly. Every type parameter must therefore appear in at least one parameter
so it can be inferred (a parameter used only in return position needs explicit type arguments,
which are not yet supported).

**What a generic body may do.** Because trait bounds are not yet enforced (the trait system is a
separate, later feature), the body of a generic function may use only operations valid for *any*
type: binding a value, returning it, passing it to another function, and building or observing
tuples. Operations that need a known concrete type — arithmetic, comparison, field access, method
calls — are rejected on a bare type parameter:

```neuro
func bad<T>(a: T, b: T) -> T {
    a + b   // error: `+` is not defined on the unbounded type parameter T
}
```

**Bounds.** A bound may be written (`func f<T: Ord + Eq>(...)`) and parses for forward
compatibility, but it is **not enforced** yet.

**Restrictions (this phase).** Type arguments are restricted to `Copy` types. Generic structs and
`impl` blocks are supported too (see [Structs](structs.md#generic-structs-and-impls)).

### Const (value) parameters

A generic parameter list may also declare a **const parameter** — a compile-time *value* (of an
integer type), written `const NAME: T`. A const parameter is usable as an array length and as a
value in the body, and each distinct value is monomorphized into its own specialized code (zero
runtime cost). Const parameters are inferred from array-argument lengths:

```neuro
func sum<const N: u32>(a: [i32; N]) -> i32 {
    mut total: i32 = 0
    for x in a {
        total = total + x
    }
    total          // N is inferred from the array's length
}

val xs: [i32; 3] = [10, 20, 12]
val s = sum(xs)    // sum<3>  ->  42
```

### `where` clauses

For a readable signature, constraints may move into a `where` clause after the return type. A
`where` clause carries trait bounds (parsed, still unenforced) and **value predicates** over const
parameters — a boolean expression checked at every instantiation and reported at the offending call:

```neuro
func head<const N: u32>(a: [i32; N]) -> i32 where N > 0 {
    a[0]           // guaranteed non-empty by `where N > 0`
}
```

### Turbofish — explicit generic arguments

When inference cannot reach a parameter (or you want to be explicit), supply the arguments at the
call with a **turbofish** `::<...>`. This is the only call-site form for explicit generic arguments;
arguments may be types or const values:

```neuro
val a = identity::<i32>(5)   // explicit type argument
val b = zeros::<4>()         // explicit const argument
```

## Static and Dynamic Dispatch

A trait bound can be satisfied two ways, and the keyword chooses which.

### `impl Trait` — static dispatch

`impl Trait` is anonymous-generic sugar. Each concrete type flowing through it produces a
specialized copy of the function, exactly as a named type parameter does, so it carries
**zero runtime cost** and no pointer indirection.

```neuro
// These two signatures compile to the same code:
func train(model: &impl Model) -> i32 { model.step() }
func train<T: Model>(model: &T) -> i32 { model.step() }
```

Each `impl Trait` parameter is its *own* anonymous parameter, so one call may bind two
different concrete types — unlike a single shared `<T>`:

```neuro
func total(a: &impl Shape, b: &impl Shape) -> i32 { a.area() + b.area() }
total(&square, &rect)   // valid: two different types
```

In **return** position, `impl Trait` names the one concrete type the body produces. It
resolves transparently to that type, and the compiler verifies the type implements the
trait:

```neuro
func make() -> impl Shape { Square { side: 3 } }
```

The body's result must be a direct constructor (a struct literal or enum value) for the
concrete type to be inferable; richer forms arrive with closures and iterators.

### `dyn Trait` — dynamic dispatch

`dyn Trait` is a single *runtime* type that can hold any implementor. Method calls go
through a **vtable**: the reference carries a pointer to the value plus a pointer to that
concrete type's method table, and the call jumps through a fixed slot.

A trait object is **unsized**, so it only appears behind a reference — `&dyn Trait` or
`&mut dyn Trait`. A bare `dyn Trait` is a compile error.

```neuro
// One function body serves every implementor.
func measure(s: &dyn Shape) -> i32 { s.area() }

measure(&square)   // &Square coerces to &dyn Shape
measure(&rect)     // &Rect   coerces to &dyn Shape
```

Use `dyn` when values of *different* concrete types must be handled behind one
interface; use `impl Trait` (or a named bound) when each call site has one concrete type.

| Form         | Dispatch | Cost                   | Use when                                       |
| ------------ | -------- | ---------------------- | ---------------------------------------------- |
| `impl Trait` | static   | zero (monomorphized)   | one concrete type per call site                |
| `&dyn Trait` | dynamic  | one vtable indirection | a heterogeneous set behind a uniform interface |

### Object safety

A trait is usable as `dyn` only if it is **object-safe**: every method must dispatch on a
`&self` or `&mut self` receiver, so the vtable has a fixed layout. A method that consumes
`self` by value, or one with no receiver at all, makes the trait unusable as a trait
object (it remains fully usable through `impl Trait` and named bounds).

```neuro
trait Consume { func take(self) -> i32 }   // not object-safe: consumes self
trait Maker   { func build() -> i32 }      // not object-safe: no receiver
```

A `&mut self` method requires a `&mut dyn Trait` receiver; mutations through it are
visible to the caller.

## References

- [Types](types.md) - Function types and type checking
- [Variables](variables.md) - Local variables in functions
- [Control Flow](control-flow.md) - If/else and function control flow
- [Expressions](expressions.md) - Expression-based returns

## See Also

- Rust Book: [Functions](https://doc.rust-lang.org/book/ch03-03-how-functions-work.html)
- [Recursion](https://en.wikipedia.org/wiki/Recursion_(computer_science))
