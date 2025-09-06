# NEURO Examples

This directory contains example NEURO programs demonstrating the current capabilities of the language.

## Current Phase 1 Features

### Implemented Features ✅

- **Complete Frontend Pipeline**: Lexical analysis → Syntax parsing → Semantic analysis
- **Working CLI Compiler**: Full `neurc` command-line interface with multiple subcommands  
- **Semantic Analysis**: Type checking, symbol resolution, scope management
- **Type System**: Type inference with comprehensive type checking for expressions and function calls
- **Error Reporting**: Comprehensive error messages with source location information
- **Developer Experience**: Multiple output formats (JSON/pretty), verbose mode, detailed analysis commands

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

You can test the complete compilation pipeline with these examples using the NEURO compiler:

```bash
# Compile with full pipeline (includes semantic analysis)
cargo run --bin neurc -- compile comprehensive_test.nr

# Generate LLVM IR from NEURO source code
cargo run --bin neurc -- llvm comprehensive_test.nr

# Generate optimized LLVM IR with output file
cargo run --bin neurc -- llvm comprehensive_test.nr -O2 -o comprehensive.ll

# Analyze semantic information in detail
cargo run --bin neurc -- analyze comprehensive_test.nr

# View tokenization results
cargo run --bin neurc -- tokenize comprehensive_test.nr --format json

# Parse and show AST structure
cargo run --bin neurc -- parse comprehensive_test.nr

# Check syntax and semantics
cargo run --bin neurc -- check comprehensive_test.nr

# Verbose mode to see pipeline details (including LLVM generation)
cargo run --bin neurc -- --verbose llvm comprehensive_test.nr

# Run complete test suite
cargo test
```

### What's Next (Future Phases)

The examples will be expanded as more features are implemented:

- **Phase 2**: Tensor operations, GPU kernels, neural network DSL
- **Phase 3**: Advanced type system, modules, imports
- **Phase 4**: Full compilation to native code, optimization passes

## Running Examples

Examples demonstrate the current compiler capabilities:

1. **Complete Compilation Pipeline**: Lexical analysis, parsing, semantic analysis, and LLVM IR generation
2. **Type System**: Type inference and checking for all expressions and function calls
3. **Symbol Resolution**: Function and variable symbol tables with scope management
4. **LLVM Backend**: Text-based LLVM IR generation with SSA form and optimization support
5. **Code Generation**: Complete expression and statement compilation to valid LLVM IR
6. **Error Reporting**: Comprehensive semantic error detection with source locations
7. **CLI Integration**: Full command-line interface for development workflows

### Current Example: `comprehensive_test.nr`

This example demonstrates all current features:

```neuro
// Namespace imports
import std::math;

// Function with typed parameters
fn calculate(x: int, y: float) -> float {
    let result = x + y;
    return result;
}

// Main function with comprehensive language features
fn main() -> int {
    // Variable declarations with type inference
    let mut counter = 0;
    let name = "NEURO"; 
    let pi = 3.14159;
    let active = true;
    
    // Function calls with type checking
    let sum = calculate(42, pi);
    
    // Control flow with semantic validation
    if active {
        counter = counter + 1;
        while counter < 10 {
            counter = counter + 1;
            if counter == 5 {
                continue;
            }
        }
    } else {
        return 1;
    }
    
    return 0;
}

// Struct definitions
struct Point {
    x: float,
    y: float,
}
```

The compiler successfully processes the program through the complete pipeline:
- **Frontend**: Lexical analysis → Syntax parsing → Semantic analysis 
- **Backend**: LLVM IR generation with complete function compilation
- **Results**: 2 functions (`calculate` and `main`) with full LLVM IR in SSA form
- **Variables**: 6 variables with inferred types (`counter: int`, `name: string`, `pi: float`, etc.)
- **Validation**: Zero semantic errors on valid code, complete symbol table and type information
- **Output**: Valid LLVM IR that can be processed by LLVM tools for native compilation

Example LLVM IR output for the `calculate` function:

```llvm
define float @calculate(i32, float) {
entry:
  %x_addr = alloca i32
  %y_addr = alloca float
  store i32 %param_0, i32* %x_addr
  store float %param_1, float* %y_addr
  %result_addr = alloca float
  %0 = load i32, i32* %x_addr
  %1 = load float, float* %y_addr
  %2 = sitofp i32 %0 to float
  %3 = fadd float %2, %1
  store float %3, float* %result_addr
  %4 = load float, float* %result_addr
  ret float %4
}
```

As the compiler develops, examples will become executable programs that demonstrate the full power of the NEURO language for AI/ML development.