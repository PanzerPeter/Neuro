# Functions

Functions are the primary way to organize and reuse code in NEURO. They support parameters, return values, and lexical scoping.

## Function Declaration Syntax

```neuro
fn function_name(param1: Type1, param2: Type2) -> ReturnType {
    // function body
    return expression;
}
```

## Function Components

### Function Name
- Must be a valid identifier
- Cannot conflict with keywords

### Parameters
- Zero or more parameters separated by commas
- Each parameter should have a type annotation: `param: Type`
- Parameters without explicit types are parsed as `?` (unknown) and handled permissively by the type checker
- Parameters are immutable within the function body

### Return Type
- Specified after `->` arrow: `-> ReturnType`
- Can be any valid NEURO type: primitives, tensors, function types
- **Optional**: Functions without `-> T` have implicit `-> ?` return type
- Functions without explicit return type can use `return;` for early exit

### Function Body
- Enclosed in braces `{ }`
- Can contain any valid NEURO statements
- Must return a value of the declared return type (if specified)

## Return Statements

Functions can return in two ways:
- **Value return**: `return expression;`
- **Void return**: `return;` (for functions without specific return type)

## Examples

### Function with Parameters and Return Type
```neuro
fn add(a: int, b: int) -> int {
    return a + b;
}
```

### Function without Explicit Return Type
```neuro
fn nop() {
    return;  // Void return
}
```

### Function with Complex Logic
```neuro
fn factorial(n: int) -> int {
    if n <= 1 {
        return 1;
    } else {
        return n * factorial(n - 1);
    }
}
```

### Function with Different Types
```neuro
fn is_positive(x: float) -> bool {
    return x > 0.0;
}

fn create_vector() -> Tensor<float, [5]> {
    // Placeholder implementation
    return 0;
}
```

## Function Calls

Functions are called using standard syntax:
```neuro
fn main() -> int {
    let result = add(10, 20);    // result = 30
    nop();                       // Call function with no return
    return result;
}
```

## Type Inference and Flexibility

- Parameters without type annotations are treated as unknown types (`?`)
- The type checker handles these permissively during Phase 1
- Explicit type annotations are recommended for clarity and better error messages

## Current Implementation Status

### Fully Implemented
- Function declaration parsing
- Parameter and return type parsing
- Function body parsing and execution
- Recursive function calls
- Return statement handling
- Basic type checking for functions

### Partially Implemented
- Function types as values
- Higher-order functions (parsing exists, full semantics pending)

### Not Yet Implemented
- Default parameters
- Variadic functions
- Function overloading
- Nested function definitions
- Closures and lambdas
- Generic functions (beyond parsing)
- Function attributes (`#[inline]`, etc.)

