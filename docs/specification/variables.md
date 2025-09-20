# Variables

NEURO uses `let` declarations to create variables with optional mutability and type annotations. Variables provide lexically scoped storage for values.

## Variable Declaration Syntax

### Basic Syntax Forms
```neuro
let name = expression;              // Immutable variable with inferred type
let name: Type = expression;        // Immutable variable with explicit type
let mut name = expression;          // Mutable variable with inferred type
let mut name: Type = expression;    // Mutable variable with explicit type
```

### Declaration Without Initialization
```neuro
let name;                          // Immutable variable without initialization
let name: Type;                    // Immutable variable with explicit type
let mut name: Type;                // Mutable variable with explicit type
```

**Note**: Variables declared without initialization are parsed but may require initialization before use in semantic analysis.

## Mutability

### Immutable Variables (Default)
Variables declared with `let` are immutable by default:
```neuro
let x = 42;
// x = 43;  // ERROR: Cannot assign to immutable variable
```

### Mutable Variables
Variables declared with `let mut` can be reassigned:
```neuro
let mut counter = 0;
counter = counter + 1;  // OK: counter is mutable
counter = 42;           // OK: can assign different values
```

## Type Annotations

### Explicit Type Annotations
```neuro
let age: int = 25;
let height: float = 5.9;
let name: string = "Alice";
let ready: bool = true;
```

### Type Inference
NEURO's type inference system can deduce types from initializer expressions:
```neuro
let count = 10;         // Inferred as int
let ratio = 3.14;       // Inferred as float
let message = "hello";  // Inferred as string
let active = true;      // Inferred as bool
```

### Tensor Variables
```neuro
let vector: Tensor<float, [3]> = create_vector();
let matrix: Tensor<int, [2, 2]> = create_matrix();
```

## Variable Scope

Variables follow lexical scoping rules:

### Function Scope
```neuro
fn example() -> int {
    let x = 10;    // x visible throughout function
    {
        let y = 20;    // y only visible in this block
        let result = x + y;  // Can access outer x and inner y
    }
    // y not accessible here
    return x;
}
```

### Block Scope
```neuro
fn scoped_example() -> int {
    let x = 1;
    {
        let x = 2;     // Shadows outer x within this block
        // Inner x = 2
    }
    // Outer x = 1 (shadowing ends)
    return x;
}
```

## Assignment

### Initial Assignment
All variables must be assigned during declaration or before first use:
```neuro
let x: int = 42;       // Assigned during declaration
let mut y: int;        // Declared without assignment
y = 100;               // Must assign before use
```

### Reassignment (Mutable Variables Only)
```neuro
let mut counter = 0;
counter = 1;           // Basic reassignment
counter = counter + 5; // Using current value
counter = other_var;   // Assigning from another variable
```

## Complete Example

```neuro
fn main() -> int {
    // Different variable declaration styles
    let mut y = 2;          // Mutable with type inference
    let x: int = 1;         // Immutable with explicit type

    // Assignment to mutable variable
    y = y + x;              // y becomes 3

    // Block scoping
    {
        let z = x * 2;      // Block-scoped variable
        y = y + z;          // y becomes 5
    }
    // z not accessible here

    return y;               // Returns 5
}
```

## Current Implementation Status

### Fully Implemented ✅
- `let` declarations with and without type annotations
- `let mut` declarations for mutable variables
- Type inference for primitive types
- Variable assignment and reassignment
- Lexical scoping rules
- Variable shadowing

### Partially Implemented ⚠️
- Uninitialized variable declarations (parsed but semantic checking pending)
- Complex tensor variable initialization

### Limitations
- Variables cannot be declared without some form of type information (either annotation or initializer)
- Pattern matching in variable declarations not yet implemented
- Destructuring assignment not available
- Global variables not yet supported

