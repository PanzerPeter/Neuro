# NEURO Parser Architecture

## Overview

The NEURO parser is a complete recursive descent parser that transforms tokenized input into an Abstract Syntax Tree (AST). Located in `compiler/syntax-parsing/`, it implements the full NEURO language grammar with robust error handling and recovery.

## Architecture

### Core Components

#### `parser.rs` - Main Parser Implementation
- **Recursive descent parser** with lookahead
- **Operator precedence parsing** for expressions
- **Error recovery** with detailed diagnostics
- **Span tracking** for precise error locations

#### `error.rs` - Error Handling
- **ParseError** enum with context-specific variants
- **Source location tracking** via spans
- **Recovery mechanisms** for continuing parsing after errors

#### `lib.rs` - Public Interface
- **Parser struct** with configuration options
- **Public parsing methods** for different input types
- **Result types** with comprehensive error reporting

## Parsing Capabilities

### 1. Expressions

#### Binary Expressions
```neuro
a + b * c         // Arithmetic with precedence
x == y && z       // Comparison and logical
tensor[i][j]      // Indexing operations
```

**Supported Operators:**
- **Arithmetic**: `+`, `-`, `*`, `/`, `%`
- **Comparison**: `==`, `!=`, `<`, `<=`, `>`, `>=`
- **Logical**: `&&`, `||`
- **Assignment**: `=`

#### Unary Expressions
```neuro
-value           // Negation
!condition       // Logical NOT
```

#### Complex Expressions
```neuro
func(a, b, c)                    // Function calls
(a + b) * c                      // Parenthesized expressions
Tensor<f32, [3, 3]>::zeros()     // Method calls with generics
```

### 2. Statements

#### Variable Declarations
```neuro
let x: int = 42;                 // Typed with initializer
let y = 3.14;                    // Type inferred
let tensor: Tensor<f32, [3]>;    // Tensor type declaration
```

#### Control Flow
```neuro
if condition {
    // then branch
} else {
    // else branch
}

while condition {
    // loop body
    break;
    continue;
}
```

#### Function Definitions
```neuro
fn add(a: int, b: int) -> int {
    return a + b;
}

fn main() -> int {
    let result = add(1, 2);
    return result;
}
```

### 3. Top-Level Items

#### Functions
```neuro
fn neural_network(input: Tensor<f32, [784]>) -> Tensor<f32, [10]> {
    let hidden = relu(linear(input, weights1));
    linear(hidden, weights2)
}
```

#### Structs
```neuro
struct Point {
    x: float,
    y: float,
}

struct Model {
    weights: Tensor<f32, [128, 10]>,
    bias: Tensor<f32, [10]>,
}
```

#### Imports
```neuro
import std::math;
import neural::layers::*;
import ./local_module;
```

### 4. Type System Support

#### Basic Types
- `int` - 32-bit integers
- `float` - 32-bit floating point
- `bool` - Boolean values
- `string` - UTF-8 strings

#### Tensor Types
```neuro
Tensor<f32, [3]>           // 1D tensor
Tensor<i32, [2, 3]>        // 2D tensor
Tensor<f64, [batch, 784]>  // Named dimensions
```

#### Function Types
```neuro
fn(int, int) -> int        // Function type signature
(Tensor<f32, [N]>) -> Tensor<f32, [N]>  // Generic function
```

## Parser Implementation Details

### Precedence Handling

```rust
// Operator precedence levels (highest to lowest)
LogicalOr     = 1,  // ||
LogicalAnd    = 2,  // &&
Equality      = 3,  // ==, !=
Comparison    = 4,  // <, <=, >, >=
Term         = 5,  // +, -
Factor       = 6,  // *, /, %
Unary        = 7,  // -, !
Call         = 8,  // function calls
Primary      = 9,  // literals, identifiers, ()
```

### Expression Parsing Strategy

1. **Left-associative operators** parsed with precedence climbing
2. **Function calls** have highest precedence
3. **Parentheses** override precedence naturally
4. **Type annotations** parsed in variable contexts

### Error Recovery

#### Synchronization Points
- Statement boundaries (`;`)
- Block boundaries (`{`, `}`)
- Function boundaries
- Top-level item boundaries

#### Error Reporting
```rust
ParseError::UnexpectedToken {
    expected: Vec<TokenType>,
    found: Token,
    span: Span,
}
```

### AST Structure

#### Expression AST
```rust
pub enum Expression {
    Literal(Literal),
    Identifier(Identifier),
    Binary(BinaryExpression),
    Unary(UnaryExpression),
    Call(CallExpression),
    Index(IndexExpression),
    // ... more variants
}
```

#### Statement AST
```rust
pub enum Statement {
    Let(LetStatement),
    Assignment(AssignmentStatement),
    Return(ReturnStatement),
    If(IfStatement),
    While(WhileStatement),
    Expression(ExpressionStatement),
    Block(BlockStatement),
}
```

## Integration Points

### Lexical Analysis Integration
- Consumes tokens from `lexical-analysis` crate
- Handles whitespace and comment filtering
- Preserves source location information

### Semantic Analysis Integration
- Produces AST consumed by semantic analyzer
- Maintains type annotation information
- Provides scope structure information

### Error Reporting Integration
- Uses `diagnostics` crate for error formatting
- Integrates with `source-location` for spans
- Provides actionable error messages

## Parser Modes

### 1. Program Parsing
Full program parsing for compilation:
```rust
let mut parser = Parser::new(tokens);
let program = parser.parse()?;
```

### 2. Expression Parsing
REPL/evaluation mode:
```rust
let mut parser = Parser::new(tokens);
let expr = parser.parse_expression()?;
```

### 3. Interactive Mode
Incremental parsing for development tools:
```rust
let mut parser = Parser::new(tokens);
parser.set_interactive_mode(true);
let result = parser.parse_incrementally()?;
```

## Performance Characteristics

### Time Complexity
- **Linear time** parsing: O(n) where n = number of tokens
- **Minimal backtracking** due to LL(1) design
- **Efficient precedence parsing** for expressions

### Memory Usage
- **Single-pass parsing** with minimal lookahead
- **AST nodes allocated** on demand
- **Span information** preserved for diagnostics

## Error Handling Strategy

### 1. Immediate Errors
Critical syntax errors that prevent continuation:
```neuro
fn missing_paren(a: int b: int) -> int  // Missing comma
```

### 2. Recoverable Errors
Errors where parsing can continue:
```neuro
let x =  // Missing initializer, continue to next statement
let y = 42;
```

### 3. Semantic Hints
Provide helpful suggestions:
```neuro
fn add(a: integer) -> int  // Suggest 'int' instead of 'integer'
```

## Future Enhancements

### Phase 2 Parser Extensions
- **Macro syntax** parsing
- **Attribute parsing** (`#[grad]`, `#[kernel]`)
- **Pattern matching** syntax
- **Advanced tensor operations**

### Development Tools Support
- **Incremental parsing** for IDE integration
- **Syntax highlighting** support
- **Auto-completion** AST analysis
- **Refactoring** support via AST manipulation

## Testing Strategy

### Unit Tests
- **Individual parsing functions** tested
- **Error case coverage** comprehensive
- **Edge case handling** verified

### Integration Tests
- **Full program parsing** tested
- **Error recovery** scenarios covered
- **Performance benchmarks** maintained

The NEURO parser provides a solid foundation for the language with robust error handling, comprehensive language support, and clean integration with the rest of the compiler pipeline.