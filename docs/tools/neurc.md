# NEURO Compiler (neurc) Reference

The `neurc` compiler is the primary tool for compiling NEURO source code. It provides a comprehensive command-line interface with multiple subcommands for different compilation stages and analysis tasks.

## Table of Contents

1. [Installation and Setup](#installation-and-setup)
2. [Command Overview](#command-overview)
3. [Compilation Commands](#compilation-commands)
4. [Analysis Commands](#analysis-commands)
5. [Development Commands](#development-commands)  
6. [Configuration](#configuration)
7. [Optimization](#optimization)
8. [Error Handling](#error-handling)

---

## Installation and Setup

### Building from Source ✅ AVAILABLE

```bash
# Clone the repository
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro

# Build the compiler
cargo build --release

# The neurc binary will be in target/release/neurc
# Or run directly with cargo:
cargo run --bin neurc -- [ARGS]
```

### Verify Installation

```bash
# Check version
cargo run --bin neurc -- version
# NEURO Compiler (neurc) v0.1.0

# Get help
cargo run --bin neurc -- --help
cargo run --bin neurc -- help [COMMAND]
```

---

## Command Overview

`neurc` provides 8 main subcommands for different tasks:

```bash
neurc [GLOBAL_OPTIONS] <COMMAND> [COMMAND_OPTIONS] <FILE>

Commands ✅ IMPLEMENTED:
  compile    - Full compilation pipeline (lexer → parser → semantic → LLVM IR)
  llvm       - Generate LLVM IR from NEURO source  
  check      - Syntax and semantic validation without compilation
  parse      - Parse source and display AST
  tokenize   - Tokenize source and display tokens
  analyze    - Perform semantic analysis and show results
  eval       - Evaluate expressions directly  
  version    - Display version information

Global Options:
  --verbose, -v    Verbose output showing compilation pipeline details
  --format FORMAT  Output format: pretty (default) or json
  --help, -h       Show help information
```

---

## Compilation Commands

### Full Compilation ✅ IMPLEMENTED

```bash
# Basic compilation
cargo run --bin neurc -- compile hello.nr

# Verbose compilation (shows pipeline stages)
cargo run --bin neurc -- --verbose compile hello.nr

# JSON output for tooling integration
cargo run --bin neurc -- --format json compile hello.nr
```

**Example Output:**
```
Compilation Results:
--------------------------------------------------
✅ Lexical Analysis: 15 tokens
✅ Syntax Parsing: AST with 3 functions  
✅ Semantic Analysis: 2 symbols, 6 variables
✅ LLVM Generation: 3 functions compiled

Functions:
  main: () -> int
  get_answer: () -> int  
  add: (int, int) -> int

No errors found.
```

### LLVM IR Generation ✅ IMPLEMENTED

Generate LLVM Intermediate Representation from NEURO source:

```bash
# Generate LLVM IR
cargo run --bin neurc -- llvm hello.nr

# Save to file
cargo run --bin neurc -- llvm hello.nr -o hello.ll

# Optimization levels
cargo run --bin neurc -- llvm hello.nr -O0  # No optimization
cargo run --bin neurc -- llvm hello.nr -O1  # Basic optimization  
cargo run --bin neurc -- llvm hello.nr -O2  # Standard optimization (default)
cargo run --bin neurc -- llvm hello.nr -O3  # Aggressive optimization

# Verbose LLVM generation
cargo run --bin neurc -- --verbose llvm hello.nr
```

**Example LLVM Output:**
```llvm
; ModuleID = 'hello.nr'
source_filename = "hello.nr"

define i32 @main() {
entry:
  ret i32 42
}

define i32 @get_answer() {
entry:
  ret i32 42
}

define i32 @add(i32 %0, i32 %1) {
entry:
  %x_addr = alloca i32
  %y_addr = alloca i32
  store i32 %0, i32* %x_addr
  store i32 %1, i32* %y_addr
  %2 = load i32, i32* %x_addr
  %3 = load i32, i32* %y_addr
  %4 = add nsw i32 %2, %3
  ret i32 %4
}
```

---

## Analysis Commands

### Syntax Checking ✅ IMPLEMENTED

```bash
# Check syntax and semantics without compilation
cargo run --bin neurc -- check hello.nr

# Verbose checking
cargo run --bin neurc -- --verbose check hello.nr
```

### Parsing ✅ IMPLEMENTED

Display the Abstract Syntax Tree (AST):

```bash
# Show AST structure
cargo run --bin neurc -- parse hello.nr

# JSON AST for tooling
cargo run --bin neurc -- --format json parse hello.nr
```

**Example AST Output:**
```
Abstract Syntax Tree:
--------------------------------------------------
Program {
  functions: [
    Function {
      name: "main",
      parameters: [],
      return_type: Some(Int),
      body: Block {
        statements: [
          Return(Some(IntegerLiteral(42)))
        ]
      }
    },
    Function {
      name: "add", 
      parameters: [
        Parameter { name: "a", type: Int },
        Parameter { name: "b", type: Int }
      ],
      return_type: Some(Int),
      body: Block {
        statements: [
          Return(Some(BinaryOp {
            left: Identifier("a"),
            operator: Add,
            right: Identifier("b")
          }))
        ]
      }
    }
  ]
}
```

### Tokenization ✅ IMPLEMENTED

Display lexical tokens:

```bash
# Show tokens
cargo run --bin neurc -- tokenize hello.nr

# JSON tokens for analysis
cargo run --bin neurc -- --format json tokenize hello.nr
```

**Example Token Output:**
```
Tokens:
--------------------------------------------------
1:1-1:3   → Keyword(Fn)
1:4-1:8   → Identifier("main")  
1:8-1:9   → LeftParen
1:9-1:10  → RightParen
1:11-1:13 → Arrow
1:14-1:17 → Keyword(Int)
1:18-1:19 → LeftBrace
2:5-2:11  → Keyword(Return)
2:12-2:14 → IntegerLiteral(42)
2:14-2:15 → Semicolon
3:1-3:2   → RightBrace
```

### Semantic Analysis ✅ IMPLEMENTED

Detailed semantic analysis results:

```bash
# Show semantic analysis
cargo run --bin neurc -- analyze hello.nr

# JSON format for IDEs
cargo run --bin neurc -- --format json analyze hello.nr
```

**Example Analysis Output:**
```
Semantic Analysis Results:
--------------------------------------------------
Symbols (3):
  main: Function { 
    name: "main", 
    params: [], 
    return_type: Int, 
    span: Span { start: 0, end: 25 } 
  }
  add: Function { 
    name: "add", 
    params: [Int, Int], 
    return_type: Int, 
    span: Span { start: 26, end: 67 } 
  }
  PI: Constant { 
    name: "PI", 
    type: Float, 
    value: 3.14159, 
    span: Span { start: 68, end: 85 } 
  }

Type Information:
  Variables (4):
    result: int (inferred)
    temp: float (explicit)
    flag: bool (inferred)
    message: string (inferred)

Scopes (2):
  Global scope: 3 symbols
  Function 'add': 2 parameters, 1 local variable

No semantic errors found.
```

---

## Development Commands

### Expression Evaluation ✅ IMPLEMENTED

Evaluate expressions directly without writing files:

```bash
# Arithmetic expressions
cargo run --bin neurc -- eval "2 + 3 * 4"        # 14
cargo run --bin neurc -- eval "42 == 42"         # true
cargo run --bin neurc -- eval "10 > 5 && 3 < 7"  # true

# String expressions
cargo run --bin neurc -- eval '"Hello" + " World"'  # "Hello World"

# Complex expressions  
cargo run --bin neurc -- eval "(2 + 3) * 4 - 8 / 2"  # 16

# Verbose evaluation (shows steps)
cargo run --bin neurc -- --verbose eval "1 + 2 * 3"
```

**Example Evaluation Output:**
```
Expression Evaluation:
--------------------------------------------------
Input: 2 + 3 * 4
Tokens: [Integer(2), Plus, Integer(3), Multiply, Integer(4)]
AST: BinaryOp(Integer(2), Add, BinaryOp(Integer(3), Multiply, Integer(4)))  
Result: 14 (type: int)
```

### Version Information ✅ IMPLEMENTED

```bash
# Version information
cargo run --bin neurc -- version

# Detailed version info
cargo run --bin neurc -- --verbose version
```

**Example Version Output:**
```
NEURO Compiler (neurc) v0.1.0
--------------------------------------------------
Build Information:
  Rust Version: rustc 1.70.0
  Target: x86_64-pc-windows-msvc
  Build Date: 2025-01-14
  Git Commit: a1b2c3d4
  
Components:
  ✅ Lexical Analysis v0.1.0
  ✅ Syntax Parsing v0.1.0  
  ✅ Semantic Analysis v0.1.0
  ✅ LLVM Backend v0.1.0
  🏗️ GPU Backend v0.1.0 (in development)
  
LLVM Version: 18.0.0
```

---

## Configuration

### Configuration Files (📅 Planned - Phase 2)

NEURO uses `neuro.toml` configuration files:

```toml
[project]
name = "my_neuro_project"
version = "1.0.0"
authors = ["Your Name <email@example.com>"]
license = "MIT"

[compilation]  
optimization_level = 2
target = "x86_64-unknown-linux-gnu"
debug_info = true
warnings_as_errors = false

[dependencies]
std = "1.0"
tensor = "1.0" 
neural = { version = "0.5", features = ["gpu"] }

[dev-dependencies]
test_utils = "0.1"

[features]
default = ["simd"]
gpu = ["cuda", "vulkan"]
cuda = []
vulkan = []
simd = []
```

### Environment Variables

```bash
# Compilation settings
export NEURO_OPT_LEVEL=3           # Optimization level
export NEURO_TARGET=native         # Target architecture
export NEURO_NUM_THREADS=8         # Parallel compilation threads

# Debug settings  
export NEURO_DEBUG=1               # Enable debug output
export NEURO_TRACE_COMPILATION=1   # Trace compilation steps
export NEURO_DUMP_AST=1            # Dump AST to file

# Memory settings
export NEURO_MEMORY_POOL_SIZE=1GB  # Tensor memory pool size
export NEURO_GC_THRESHOLD=100MB    # Garbage collection threshold
```

### Command-Line Options

```bash
# Optimization options ✅ IMPLEMENTED
-O0, --opt-level=0    # No optimization
-O1, --opt-level=1    # Basic optimization
-O2, --opt-level=2    # Standard optimization (default)
-O3, --opt-level=3    # Aggressive optimization

# Output options ✅ IMPLEMENTED  
-o, --output FILE     # Output file
--format FORMAT       # Output format (pretty, json)
--verbose, -v         # Verbose output

# Target options (📅 Planned)
--target TARGET       # Compilation target
--features FEATURES   # Enable features
--cfg CONFIG          # Configuration predicates

# Debug options (📅 Planned)
--debug-info          # Include debug information
--emit-llvm          # Emit LLVM IR
--emit-asm           # Emit assembly
--dump-ast           # Dump AST
--dump-tokens        # Dump tokens
```

---

## Optimization

### Optimization Levels ✅ IMPLEMENTED

```bash
# -O0: No optimization (fastest compilation, slowest execution)
cargo run --bin neurc -- llvm -O0 program.nr
- No function inlining
- No dead code elimination  
- No constant folding
- Fastest compilation times

# -O1: Basic optimization
cargo run --bin neurc -- llvm -O1 program.nr
- Basic function inlining
- Simple dead code elimination
- Constant folding
- Local optimizations

# -O2: Standard optimization (default)
cargo run --bin neurc -- llvm -O2 program.nr  
- Aggressive function inlining
- Advanced dead code elimination
- Loop optimizations
- Vectorization (SIMD)
- Good balance of compile time vs performance

# -O3: Aggressive optimization (slowest compilation, fastest execution)
cargo run --bin neurc -- llvm -O3 program.nr
- Maximum inlining
- Aggressive loop unrolling
- Inter-procedural optimizations
- Profile-guided optimizations
- Maximum performance
```

### Target-Specific Optimization (📅 Planned)

```bash
# CPU-specific optimizations
cargo run --bin neurc -- llvm --target=native program.nr        # Use all available CPU features
cargo run --bin neurc -- llvm --target=x86_64-v3 program.nr     # AVX2 optimizations
cargo run --bin neurc -- llvm --target=aarch64 program.nr       # ARM64 optimizations

# GPU targeting  
cargo run --bin neurc -- llvm --target=cuda program.nr          # CUDA GPU code
cargo run --bin neurc -- llvm --target=vulkan program.nr        # Vulkan compute
cargo run --bin neurc -- llvm --target=opencl program.nr        # OpenCL kernels
```

### ML-Specific Optimizations ✅ IMPLEMENTED (Infrastructure)

```bash
# Tensor operation fusion
cargo run --bin neurc -- llvm --fuse-ops program.nr
- Fuses consecutive tensor operations
- Reduces memory bandwidth requirements
- Improves cache locality

# Memory layout optimization  
cargo run --bin neurc -- llvm --optimize-layout program.nr
- Optimizes tensor memory layouts
- Improves SIMD vectorization
- Reduces memory fragmentation

# Automatic parallelization
cargo run --bin neurc -- llvm --parallelize program.nr
- Parallelizes tensor operations
- Uses all available CPU cores
- Thread-safe optimizations
```

---

## Error Handling

### Compilation Errors ✅ IMPLEMENTED

NEURO provides comprehensive error messages with source locations:

```bash
# Example error output
cargo run --bin neurc -- compile broken.nr
```

**Lexical Errors:**
```
[ERROR] Lexical Error at line 3, column 15:
--------------------------------------------------
Unexpected character: '@' in numeric literal
  3 | let invalid = 42@;
    |                 ^ unexpected character

Suggestion: Remove the '@' character or use it as an operator
```

**Syntax Errors:**  
```
[ERROR] Syntax Error at line 5, columns 10-15:
--------------------------------------------------
Expected expression, found 'else'
  5 | let x = if else 42;
    |            ^^^^ unexpected token

Expected: expression (identifier, literal, or parenthesized expression)
Found: 'else' keyword

Suggestion: Add a condition between 'if' and 'else'
```

**Semantic Errors:**
```
[ERROR] Semantic Error at line 8, columns 12-20:
--------------------------------------------------
Type mismatch: expected 'int', found 'string'
  8 | let sum = 5 + "hello";
    |               ^^^^^^^ string literal

Cannot add 'int' and 'string' types
Suggestion: Convert one operand to match the other's type, or use string interpolation
```

**Multiple Errors:**
```
[ERROR] Found 3 errors in compilation:
--------------------------------------------------
1. Line 2: Undefined variable 'x'
2. Line 5: Type mismatch: expected 'bool', found 'int'  
3. Line 8: Function 'undefined_func' not found

Use --verbose for detailed error information
```

### Warning Messages ✅ IMPLEMENTED

```
[WARNING] at line 10, column 5:
--------------------------------------------------
Unused variable 'temp'
  10 | let temp = expensive_computation();
     |     ^^^^ variable is never used

Suggestion: Use '_temp' to indicate intentionally unused variable, or remove the declaration
```

### Error Recovery ✅ IMPLEMENTED

The compiler attempts to recover from errors and continue parsing:

```
[ERROR] Syntax Error at line 3: Missing semicolon
  3 | let x = 42
    |           ^ expected ';'

[INFO] Continuing compilation with assumed semicolon...

[ERROR] Semantic Error at line 5: Undefined variable 'y'
  5 | let z = x + y;  
    |             ^ variable not found

Total: 2 errors found. Compilation failed.
```

### Debug Output ✅ IMPLEMENTED

```bash
# Verbose compilation shows internal stages
cargo run --bin neurc -- --verbose compile program.nr

[DEBUG] Lexical Analysis:
  - Input: 245 characters
  - Output: 67 tokens
  - Time: 1.2ms

[DEBUG] Syntax Parsing:
  - Input: 67 tokens
  - Output: AST with 4 functions, 12 statements
  - Time: 3.4ms

[DEBUG] Semantic Analysis:  
  - Symbol table: 8 symbols
  - Type checking: 25 expressions
  - Scope resolution: 3 scopes
  - Time: 5.1ms

[DEBUG] LLVM Generation:
  - Functions compiled: 4
  - Instructions generated: 156
  - Optimizations applied: 12
  - Time: 8.7ms

[SUCCESS] Total compilation time: 18.4ms
```

---

## Integration with Build Systems

### Cargo Integration (📅 Planned)

```toml
# Cargo.toml for mixed Rust/NEURO projects
[package]
name = "mixed_project"
version = "0.1.0"

[dependencies]
neuro-runtime = "1.0"

[build-dependencies]
neuro-build = "1.0"

# build.rs
use neuro_build::compile_neuro;

fn main() {
    compile_neuro("src/ml_models/").expect("Failed to compile NEURO files");
}
```

### Make Integration

```makefile
# Makefile for NEURO projects
NEURC = cargo run --bin neurc --

.PHONY: all clean check test

all: build

build: $(SOURCES:.nr=.ll)
	@echo "Building NEURO project..."

%.ll: %.nr
	$(NEURC) llvm $< -o $@

check:
	$(NEURC) check src/**/*.nr

test: build
	@echo "Running tests..."
	$(NEURC) eval "assert(test_function() == 42)"

clean:
	rm -f *.ll *.o
```

### IDE Integration (📅 Planned - Phase 3)

The compiler provides Language Server Protocol (LSP) support:

```bash
# Start LSP server
neurc lsp --stdio

# LSP capabilities:
# - Syntax highlighting
# - Error/warning underlining  
# - Code completion
# - Go to definition
# - Hover information
# - Symbol search
# - Refactoring support
```

This comprehensive `neurc` documentation covers all current features and planned enhancements, providing developers with everything they need to effectively use the NEURO compiler.