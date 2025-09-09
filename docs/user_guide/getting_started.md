# Getting Started with NEURO

Welcome to NEURO! This guide will help you get up and running with the NEURO programming language.

## Current Status (Phase 1 - 100% Complete ✅)

NEURO has implemented a complete compilation pipeline including:
- ✅ Lexical analysis and tokenization (COMPLETE)
- ✅ Syntax parsing with AST generation (COMPLETE)
- ✅ Semantic analysis with type checking (COMPLETE)
- ✅ Symbol resolution and scope management (COMPLETE)
- ✅ Comprehensive error reporting (COMPLETE)
- ✅ Full-featured CLI compiler (`neurc`) (COMPLETE)
- ✅ LLVM backend integration (COMPLETE)
- ✅ Working executable generation (COMPLETE)
- ✅ Memory management with ARC (COMPLETE)
- ✅ Basic tensor operations (COMPLETE)
- ✅ Module system (COMPLETE)

## Installation

### Prerequisites

- Rust 1.70+ (for building from source)
- Git

### Building from Source

```bash
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release
```

### Verify Installation

```bash
cargo run --bin neurc -- version
# NEURO Compiler (neurc) v0.1.0
```

## Your First NEURO Program

Create a file called `hello.nr`:

```neuro
// Import standard math functions
import std::math;

// Function with typed parameters  
fn calculate(x: int, y: float) -> float {
    let result = x + y;
    return result;
}

// Main function
fn main() -> int {
    // Variable declarations
    let mut counter = 0;
    let name = "NEURO";
    let pi = 3.14159; 
    let active = true;
    
    // Function calls with type checking
    let sum = calculate(42, pi);
    
    // Control flow with type-checked conditions
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

## Using the NEURO Compiler

The `neurc` compiler provides several commands to work with NEURO source code:

### Compile a Program

```bash
# Compile with full pipeline (lexer → parser → semantic analysis)
cargo run --bin neurc -- compile hello.nr

# Generate LLVM IR from NEURO source
cargo run --bin neurc -- llvm hello.nr

# Generate optimized LLVM IR with output file
cargo run --bin neurc -- llvm hello.nr -O2 -o hello.ll

# Evaluate expressions directly (NEW!)
cargo run --bin neurc -- eval "2 + 3 * 4"        # Returns: 14
cargo run --bin neurc -- eval "42 == 42"         # Returns: true
cargo run --bin neurc -- eval "\"Hello\" + \" World\""  # Returns: "Hello World"

# Verbose mode shows pipeline details  
cargo run --bin neurc -- --verbose compile hello.nr
```

### Analyze Semantic Information

The `analyze` command provides detailed semantic analysis results:

```bash
# Pretty-printed semantic analysis
cargo run --bin neurc -- analyze hello.nr

# JSON format for tooling integration
cargo run --bin neurc -- analyze hello.nr --format json
```

Example output:
```
Semantic Analysis Results:
--------------------------------------------------
Symbols (2):
  calculate: Function { name: "calculate", params: [Int, Float], return_type: Float, span: ... }
  main: Function { name: "main", params: [], return_type: Int, span: ... }

Type Information:
  sum: float
  name: string
  result: float
  active: bool
  pi: float
  counter: int
```

### Parse and View AST

```bash
# Parse source and show AST
cargo run --bin neurc -- parse hello.nr

# JSON AST for tooling
cargo run --bin neurc -- parse hello.nr --format json
```

### Tokenize Source

```bash
# Show tokenization results
cargo run --bin neurc -- tokenize hello.nr

# JSON tokens for analysis
cargo run --bin neurc -- tokenize hello.nr --format json
```

### Check Syntax and Semantics

```bash
# Syntax and semantic validation without compilation
cargo run --bin neurc -- check hello.nr
```

## Language Features

### Type System

NEURO has a static type system with type inference:

```neuro
fn example() -> int {
    let x = 42;        // Inferred as int
    let y = 3.14;      // Inferred as float  
    let z = "hello";   // Inferred as string
    let w = true;      // Inferred as bool
    
    // Type checking prevents errors
    let sum = x + y;   // int + float = float (with coercion)
    
    return x;
}
```

### Function Definitions

```neuro
// Function with parameters and return type
fn add(a: int, b: int) -> int {
    return a + b;
}

// Function with no parameters
fn get_answer() -> int {
    return 42;
}

// Function with no return value (implicit void)
fn print_hello() {
    // Implementation would go here
}
```

### Variables and Mutability

```neuro
fn variables() {
    let x = 10;        // Immutable variable
    let mut y = 20;    // Mutable variable
    
    // y = 30;         // OK - y is mutable
    // x = 15;         // Error - x is immutable
}
```

### Control Flow

```neuro
fn control_flow(condition: bool) -> int {
    // If statements require boolean conditions
    if condition {
        let mut i = 0;
        
        // While loops with break/continue
        while i < 10 {
            i = i + 1;
            if i == 5 {
                continue;
            }
            if i == 8 {
                break;
            }
        }
        return i;
    } else {
        return 0;
    }
}
```

### Module System

```neuro
// Import modules
import std::math;
import my_module::utilities;

// Your code here
```

## Error Handling

NEURO provides comprehensive error reporting with source locations:

```bash
# If you have a type error in your code:
cargo run --bin neurc -- compile broken.nr
```

Example error output:
```
[ERROR] Semantic Error(s):
--------------------------------------------------
1: Type mismatch: expected 'int', found 'string' at Span { start: 45, end: 52 }
```

## Next Steps

1. **Explore Examples**: Check out the `examples/` directory for more code samples
2. **Read the Specification**: See `docs/specification/` for complete language reference
3. **Try Different Commands**: Experiment with all the `neurc` subcommands
4. **Write Your Own Code**: Create NEURO programs and see the semantic analysis in action

## Current Status & Roadmap

Since NEURO Phase 1 is now 100% complete:

**✅ COMPLETED IN PHASE 1:**
- ✅ Complete compilation pipeline (lexer → parser → semantic → LLVM IR)
- ✅ Expression evaluation system
- ✅ Type checking and symbol resolution
- ✅ LLVM IR generation for functions, variables, and expressions
- ✅ Native binary generation (LLVM IR → executable)
- ✅ Memory management (ARC implementation)
- ✅ Basic tensor operations
- ✅ 160+ comprehensive tests

**📅 COMING IN PHASE 2:**
- 🏗️ Advanced GPU support (#[kernel], #[gpu] attributes)
- 🏗️ Full automatic differentiation (#[grad] attribute)
- 🏗️ Advanced type features (generics, complex tensors)
- 🏗️ Neural network DSL and model definition
- 🏗️ CUDA and Vulkan kernel generation

## Getting Help

- Check the [documentation](../specification/README.md)
- Look at [examples](../../examples/README.md)
- Review the [architecture guide](../architecture/README.md)

Happy coding with NEURO! 🚀