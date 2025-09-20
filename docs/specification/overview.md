# NEURO Language Overview

NEURO is a statically typed, compiled programming language specifically designed for machine learning and AI workloads. It combines the performance of systems programming languages with the expressiveness needed for numerical computation and tensor operations.

## Design Philosophy

- **Performance-First**: Compiles to efficient native code via LLVM
- **Type Safety**: Strong static typing with sophisticated type inference
- **ML/AI Focused**: First-class tensor types and operations
- **Modern Syntax**: Clean, readable syntax inspired by Rust and Python
- **Memory Efficient**: Automatic reference counting with explicit memory pools for performance-critical sections

## Phase 1 Implementation Status

The current implementation includes the following features:

### Lexical Analysis
Complete tokenization support for:
- **Literals**: integers, floats, strings (with escape sequences), booleans
- **Identifiers**: variables, functions, types
- **Keywords**: `fn`, `let`, `mut`, `if`, `else`, `while`, `return`, `break`, `continue`, `struct`, `import`
- **Operators**: arithmetic (`+`, `-`, `*`, `/`, `%`), comparison (`==`, `!=`, `<`, `<=`, `>`, `>=`), logical (`&&`, `||`, `!`)
- **Punctuation**: parentheses, braces, brackets, semicolons, colons, arrows, commas

### Type System
- **Primitive Types**: `int` (32-bit), `float` (64-bit), `bool`, `string`
- **Tensor Types**: `Tensor<T, [dims]>` with compile-time known dimensions
- **Function Types**: `fn(param_types...) -> return_type`
- **Type Inference**: Sophisticated inference engine reduces need for explicit annotations
- **Generic Placeholders**: `?` for unknown types (internal use)

### Declarations
- **Functions**: `fn name(params) -> return_type { body }`
- **Variables**: `let name = value` and `let mut name = value`
- **Optional Type Annotations**: `let x: int = 42`
- **Structs**: Basic struct declaration parsing (limited semantic analysis)

### Statements
- **Expression Statements**: Any expression can be a statement
- **Variable Declarations**: `let` and `let mut` bindings
- **Assignment**: `variable = expression`
- **Return Statements**: `return;` and `return expression;`
- **Control Flow**: `if { } else { }`, `while { }`
- **Block Statements**: `{ ... }` for scoping
- **Flow Control**: `break;` and `continue;` in loops

### Expressions
- **Literals**: All primitive literal types
- **Identifiers**: Variable and function references
- **Unary Operations**: arithmetic negation (`-`), logical NOT (`!`)
- **Binary Operations**: Full arithmetic, comparison, and logical operator support
- **Function Calls**: `function(arg1, arg2, ...)`
- **Parentheses**: For expression grouping and precedence control
- **Proper Precedence**: Complete operator precedence implementation

### Module System (Partial)
- **Import Parsing**: `import` statements are parsed
- **Module Resolution**: Basic scaffolding in place
- **Dependency Analysis**: Framework for circular dependency detection

### Compilation Pipeline
- **Lexical Analysis**: Complete tokenization
- **Syntax Parsing**: Full AST generation for implemented features
- **Semantic Analysis**: Type checking and scope resolution
- **LLVM Backend**: Code generation to LLVM IR
- **Binary Generation**: Executable output via LLVM

## Not Yet Implemented

The following features are planned but not yet available:
- **Attributes**: `#[grad]`, `#[kernel]`, `#[gpu]` for ML-specific code generation
- **For Loops**: `for item in collection` syntax
- **Enums**: Algebraic data types
- **Pattern Matching**: `match` expressions
- **Member Access**: `object.field` expressions (parsed but not semantically analyzed)
- **Array/Tensor Indexing**: `array[index]` expressions (parsed but not semantically analyzed)
- **Method Calls**: Object-oriented method syntax
- **Lambdas/Closures**: Anonymous functions
- **Generics**: Full generic type system
- **Traits/Interfaces**: Behavioral contracts

## Compiler Tools

The `neurc` compiler provides several useful subcommands:

- `neurc compile <file>`: Full compilation to executable
- `neurc check <file>`: Syntax and semantic checking without compilation
- `neurc tokenize <file>`: Show tokenization output
- `neurc parse <file>`: Show parsed AST structure
- `neurc analyze <file>`: Detailed semantic analysis output
- `neurc llvm <file>`: Generate LLVM IR
- `neurc eval <expr>`: Evaluate simple expressions

## Example Program

```neuro
// A simple NEURO program demonstrating current features
fn fibonacci(n: int) -> int {
    if n <= 1 {
        return n;
    } else {
        return fibonacci(n - 1) + fibonacci(n - 2);
    }
}

fn main() -> int {
    let mut i = 0;
    while i < 10 {
        let result = fibonacci(i);
        i = i + 1;
    }
    return 0;
}
```

This overview reflects the current state of the NEURO compiler implementation and will be updated as new features are added.

