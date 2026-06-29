# Variables

Variables store values that can be used and manipulated in your programs.

## Compile-Time Constants

Use `const` to declare compile-time constants. Constants must have an explicit type
annotation and a *constant expression* as their value.

```neuro
const MAX_RETRIES: i32 = 3
const BASE: i32 = 10
const DOUBLED: i32 = BASE * 2   // arithmetic on other consts is allowed
```

**Key points**:
- Declared with `const` keyword
- Type annotation is required
- RHS must be a constant expression: literals, arithmetic/unary/cast on literals,
  or identifiers that refer to previously declared `const` names
- Function calls and runtime values are not allowed as `const` initializers
- No ownership or lifetime — consts do not participate in the borrow checker
- Module-level consts are visible to all functions regardless of source order
- Function-body consts are scoped to the enclosing function

### Module-Level Constants

```neuro
const LEARNING_RATE_SCALE: i32 = 100
const MAX_EPOCHS: i32 = 50

func train() -> i32 {
    return MAX_EPOCHS * LEARNING_RATE_SCALE   // 5000
}
```

### Function-Body Constants

```neuro
func compute_threshold(base: i32) -> i32 {
    const FACTOR: i32 = 4
    return base * FACTOR
}
```

### Constant Expressions

Valid RHS forms for `const`:

| Form | Example |
|---|---|
| Integer literal | `42`, `0xFF`, `0b1010` |
| Float literal | `3.14`, `1.0e-3` |
| Bool literal | `true`, `false` |
| String literal | `"hello"` |
| Unary op on const expr | `-MAX`, `!FLAG` |
| Binary op on const exprs | `A * 2`, `A + B` |
| Cast of const expr | `BASE as f32` |
| Another `const` name | `DOUBLED` (if `DOUBLED` is a `const`) |

### Constants vs Variables

| | `const` | `val` | `mut` |
|---|---|---|---|
| Keyword | `const` | `val` | `mut` |
| Mutable | no | no | yes |
| Scope | module or function | function/block | function/block |
| RHS | constant expr only | any expr | any expr |
| Ownership | none | yes | yes |
| LLVM emission | global constant | stack alloca | stack alloca |

## Variable Declaration

### Immutable Variables

Use `val` to declare immutable variables:

```neuro
val x: i32 = 42
val name: string = "Neuro"  // String type pending
val pi: f64 = 3.14159
```

**Key points**:
- Declared with `val` keyword
- Cannot be reassigned after initialization
- Type annotation optional when type can be inferred from a numeric literal
- Must be initialized at declaration

### Mutable Variables

Use `mut` to declare mutable variables:

```neuro
mut counter: i32 = 0
counter = counter + 1
counter = 42

mut flag: bool = false
flag = true
```

**Key points**:
- Declared with `mut` keyword
- Can be reassigned after initialization
- Reassigned value must match original type
- Type annotation optional when type can be inferred from a numeric literal

## Syntax

### Basic Syntax

```neuro
val name: Type = value      // Immutable
mut name: Type = value      // Mutable
```

**Components**:
- `val` or `mut` keyword
- Variable name (identifier)
- Type annotation (`: Type`)
- Initializer expression (`= value`)

### Examples

```neuro
func variables() -> i32 {
    // Immutable variables
    val x: i32 = 10
    val y: i32 = 20
    val sum: i32 = x + y

    // Mutable variables
    mut counter: i32 = 0
    counter = 5
    counter = counter + 1

    return counter
}
```

## Mutability

### Immutable by Default

Variables are immutable by default in Neuro:

```neuro
val x: i32 = 10
// x = 20  // Error: cannot assign to immutable variable
```

This prevents accidental modification and makes code easier to reason about.

### Explicit Mutability

Use `mut` when you need to modify a variable:

```neuro
mut x: i32 = 10
x = 20        // OK: x is mutable
x = x + 5     // OK: can update
```

### Why Immutable by Default?

1. **Safety**: Prevents accidental modification
2. **Clarity**: Easy to see which variables change
3. **Reasoning**: Easier to understand code flow
4. **Optimization**: Compiler can optimize better

## Variable Reassignment

### Mutable Variable Assignment

```neuro
mut x: i32 = 0
x = 10                  // Simple assignment
x = x + 5               // Update based on current value
x = x * 2               // Arithmetic update
```

### Type-Safe Reassignment

Reassigned values must match the variable's type:

```neuro
mut x: i32 = 10
x = 20        // OK: i32
// x = 3.14   // Error: expected i32, found f64
// x = true   // Error: expected i32, found bool
```

### Assignment with Expressions

```neuro
mut result: i32 = 0
result = add(5, 3)              // Function call result
result = if x > 0 { 1 } else { 0 }  // Conditional (Phase 2)
result = x * 2 + y              // Complex expression
```

### Multiple Reassignments

```neuro
func counter() -> i32 {
    mut count: i32 = 0
    count = count + 1     // count = 1
    count = count + 1     // count = 2
    count = count + 1     // count = 3
    return count
}
```

## Move Semantics (Ownership)

Every binding owns its value. For **non-`Copy`** types — today that is `string` —
placing the value somewhere new *moves* ownership out of the source binding, and the
source becomes invalid. Reading a moved binding is a compile error:

```neuro
val s1: string = "Hello"
val s2: string = s1        // s1 is MOVED into s2
// val n: u64 = s1.len()   // ERROR: use of moved value 's1'
val n: u64 = s2.len()      // OK — s2 owns the value now
```

A move happens whenever a non-`Copy` value is handed to a new owner: a `val`/`mut`
initializer, an assignment, a `return`, a struct-field store, or a by-value call
argument:

```neuro
func consume(s: string) -> u64 { s.len() }

val greeting: string = "Hi"
val len: u64 = consume(greeting)   // greeting is moved into consume()
// val again = greeting.len()      // ERROR: greeting was moved
```

**`Copy` scalars are never moved.** `i8`..`u64`, `f32`/`f64`, and `bool` are
duplicated on assignment, so the source stays valid:

```neuro
val a: i32 = 5
val b: i32 = a             // a is COPIED
val c: i32 = a + b         // both a and b still valid
```

**`.clone()` is the opt-out.** When you need an independent copy of a non-`Copy`
value, clone it — the receiver is borrowed, not moved:

```neuro
val a: string = "hello"
val b: string = a.clone()  // a is NOT moved
val ok: bool = a == b      // reading a here is fine
```

**Conditional moves don't leak.** A move that only happens inside one branch of an
`if`/`while`/`for` does not invalidate the binding on paths that never ran that
branch:

```neuro
val msg: string = "hi"
if ready {
    val r: u64 = consume(msg)   // moves msg only on this path
}
val n: u64 = msg.len()          // OK — the move above was conditional
```

> Move tracking currently applies only to `string`, the one non-`Copy` type the
> language can construct. Structs become move-tracked once the `Copy` trait and
> `@derive(Copy)` land; until then they are freely duplicable. `mut` bindings that
> were moved can be revived by reassigning them a fresh value.

## Type Annotations

Type annotations are optional when the type can be inferred from the initializer. Numeric literal inference is fully implemented:

```neuro
val x: i32 = 42           // Explicit annotation
val pi: f64 = 3.14159     // Explicit annotation
val flag: bool = true     // Explicit annotation

val n = 100               // Inferred i32 (default for integer literals)
val ratio = 3.14          // Inferred f64 (default for float literals)
mut count = 0             // Inferred i32
```

Non-numeric types (bool, string, struct) require an explicit annotation or a typed initializer. Function parameters and return types always require explicit annotations.

## Variable Scope

### Function Scope

Variables are scoped to the function where they're declared:

```neuro
func example() -> i32 {
    val x: i32 = 10
    return x  // x is in scope
}

func other() -> i32 {
    // return x  // Error: x not in scope
    return 0
}
```

### Block Scope

Variables are scoped to their enclosing block:

```neuro
func blocks() -> i32 {
    val x: i32 = 1
    if true {
        val y: i32 = 2  // y only exists in this block
        // x and y both in scope
    }
    // Only x in scope here
    // return y  // Error: y not in scope
    return x
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
    return x  // Returns 1
}
```

**Shadowing vs. Reassignment**:
- Shadowing creates a new variable (can have different type in Phase 2)
- Reassignment modifies existing variable (must have same type)

```neuro
// Shadowing (Phase 2 feature for type change)
val x: i32 = 5
val x: f64 = 3.14  // New variable, different type

// Reassignment (Phase 1)
mut x: i32 = 5
x = 10  // Same variable, must be same type
```

## Initialization

### Required Initialization

Variables must be initialized when declared (Phase 1):

```neuro
val x: i32 = 42           // OK: initialized
mut y: i32 = 0            // OK: initialized
// val z: i32             // Error: missing initializer
```

### Initialization with Expressions

```neuro
val x: i32 = 10 + 20              // Arithmetic
val y: i32 = add(5, 3)            // Function call
val z: i32 = if true { 1 } else { 0 }  // Conditional (Phase 2)
```

### Uninitialized Variables (Phase 2+)

Future phases may support uninitialized variables with explicit type:

```neuro
// Not yet implemented
val x: i32  // Declared but not initialized
// Use of x here would be an error
x = 42      // Initialize before use
```

## Common Patterns

### Accumulators

```neuro
func sum_to_n(n: i32) -> i32 {
    mut sum: i32 = 0
    for i in 1..=n {
        sum = sum + i
    }
    return sum
}
```

### State Machines

```neuro
func state_machine(input: i32) -> i32 {
    mut state: i32 = 0
    if input == 1 {
        state = 1
    } else if input == 2 {
        state = 2
    }
    return state
}
```

### Conditional Initialization

```neuro
func conditional_init(flag: bool) -> i32 {
    val x: i32 = if flag {
        42
    } else {
        0
    }
    return x
}
```

## Examples

### Counter

```neuro
func count_up() -> i32 {
    mut counter: i32 = 0
    counter = counter + 1  // 1
    counter = counter + 1  // 2
    counter = counter + 1  // 3
    return counter
}
```

### Swap (Manual)

```neuro
func swap_manual(a: i32, b: i32) -> i32 {
    mut temp: i32 = a
    mut first: i32 = b
    mut second: i32 = temp
    return first * 100 + second
}
```

### Running Total

```neuro
func running_total(a: i32, b: i32, c: i32) -> i32 {
    mut total: i32 = 0
    total = total + a
    total = total + b
    total = total + c
    return total
}
```

### Flag Toggle

```neuro
func toggle_flag(initial: bool) -> i32 {
    mut flag: bool = initial
    flag = !flag
    if flag {
        return 1
    } else {
        return 0
    }
}
```

## Best Practices

### 1. Prefer Immutable Variables

```neuro
// Good: immutable when possible
val x: i32 = 10
val y: i32 = x * 2

// Only use mut when necessary
mut counter: i32 = 0
counter = counter + 1
```

### 2. Declare Variables Close to Use

```neuro
// Good: declare near usage
func calculate() -> i32 {
    val x: i32 = get_x()
    val y: i32 = get_y()
    return x + y
}

// Bad: declare far from usage
func calculate_bad() -> i32 {
    val x: i32 = get_x()
    // ... lots of code ...
    val y: i32 = get_y()
    return x + y
}
```

### 3. Use Descriptive Names

```neuro
// Good: clear names
val user_count: i32 = 42
val is_valid: bool = true
val max_retries: i32 = 3

// Bad: unclear names
val n: i32 = 42
val f: bool = true
val x: i32 = 3
```

### 4. Initialize with Meaningful Values

```neuro
// Good: meaningful initialization
mut error_count: i32 = 0
mut is_complete: bool = false

// Avoid: magic numbers without context
mut x: i32 = 42  // What does 42 mean?
```

### 5. Group Related Variables

```neuro
// Good: related variables together
val width: i32 = 10
val height: i32 = 20
val area: i32 = width * height
```

## Common Mistakes

### Assigning to Immutable Variable

```neuro
val x: i32 = 10
// x = 20  // Error: cannot assign to immutable variable

// Fix: use mut
mut y: i32 = 10
y = 20  // OK
```

### Type Mismatch in Reassignment

```neuro
mut x: i32 = 10
// x = 3.14  // Error: expected i32, found f64

// Fix: ensure types match
mut y: f64 = 10.0
y = 3.14  // OK
```

### Using Uninitialized Variable

```neuro
// Error: uninitialized variables not allowed (Phase 1)
// val x: i32
// return x

// Fix: initialize
val x: i32 = 0
return x
```

### Shadowing Instead of Reassignment

```neuro
mut x: i32 = 10
val x: i32 = 20  // Creates new variable (shadowing), doesn't reassign

// If you meant reassignment:
mut y: i32 = 10
y = 20  // Reassigns existing variable
```

## Future Features (Phase 2+)

### Type Inference

```neuro
// Will infer types from initializers
val x = 42          // Infers i32
val pi = 3.14       // Infers f64
val flag = true     // Infers bool
```

### Destructuring

```neuro
// Tuple destructuring
val (x, y) = get_point()

// Struct destructuring — binds each named field by its own name
val Point { x, y } = point

// Array destructuring — binds positionally
val [a, b, c] = triple

// Array destructuring with a trailing rest: `rest` is a fresh `[T; N - 2]` array
val [first, second, ..rest] = numbers

// A bare `..` ignores the remainder; `_` discards a single element
val [head, ..] = numbers
val [_, mid, _] = triple
```

A rest-less array pattern must bind every element — `val [a, b] = arr` where `arr`
has more than two elements is a compile error; add a `..rest` (or `..`) to capture
the remainder. `mut` patterns make every binding mutable. Patterns nest, so an
element may itself be a tuple, struct, or array pattern.

### Pattern Matching in Declarations

```neuro
val Some(value) = optional else {
    return 0
}
```

## References

- [Types](types.md) - Type system and type checking
- [Functions](functions.md) - Local variables in functions
- [Expressions](expressions.md) - Variable initialization expressions
- [Operators](operators.md) - Assignment operator

## See Also

- Rust Book: [Variables and Mutability](https://doc.rust-lang.org/book/ch03-01-variables-and-mutability.html)
- [Immutability](https://en.wikipedia.org/wiki/Immutable_object)
