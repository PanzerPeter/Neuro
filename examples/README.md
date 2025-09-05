# NEURO Examples

This directory contains example NEURO programs demonstrating the current capabilities of the language.

## Current Phase 1 Features

### Implemented Features ✅

- **Lexical Analysis**: Complete tokenization of NEURO source code
- **Syntax Parsing**: Full AST generation for expressions and statements
- **Expression Evaluation**: Runtime evaluation of arithmetic and logical expressions
- **Basic Control Flow**: if/else statements, while loops, break/continue

### Language Features Demonstrated

#### 1. Basic Expressions (`basic_expressions.nr`)

```neuro
// Arithmetic operations
let sum = 10 + 5;
let product = 6 * 7;
let mixed = 3 + 2.5;  // Mixed integer/float arithmetic

// String operations
let greeting = "Hello" + " World";

// Comparisons
let is_greater = 10 > 5;
let is_equal = 42 == 42;

// Complex expressions with proper precedence
let result = (2 + 3) * 4 - 8 / 2;  // = 18

// Unary operations
let negative = -42;
```

#### 2. Control Flow

```neuro
// If statements
if x > 5 {
    // do something
} else {
    // do something else
}

// While loops with break/continue
while condition {
    if some_check {
        break;
    }
    continue;
}
```

#### 3. Function Definitions

```neuro
fn calculate_area(width, height) -> float {
    return width * height;
}
```

### Data Types Supported

- **Integers**: `42`, `-10`
- **Floats**: `3.14`, `-2.5`
- **Booleans**: `true`, `false`
- **Strings**: `"Hello World"`

### Operators Supported

- **Arithmetic**: `+`, `-`, `*`, `/`, `%`
- **Comparison**: `==`, `!=`, `<`, `<=`, `>`, `>=`
- **Unary**: `-` (negation)

### Testing the Examples

You can test the parsing and evaluation of these examples using the NEURO compiler:

```bash
# Parse and validate syntax
cargo run --bin neurc -- check examples/basic_expressions.nr

# Run tests to verify functionality
cargo test
```

### What's Next (Future Phases)

The examples will be expanded as more features are implemented:

- **Phase 2**: Tensor operations, GPU kernels, neural network DSL
- **Phase 3**: Advanced type system, modules, imports
- **Phase 4**: Full compilation to native code, optimization passes

## Running Examples

Currently, examples are used primarily for:
1. **Syntax Validation**: Ensuring the parser can handle real NEURO code
2. **Expression Evaluation**: Testing the interpreter with complex expressions
3. **Documentation**: Showing language capabilities to users

As the compiler develops, examples will become executable programs that demonstrate the full power of the NEURO language for AI/ML development.