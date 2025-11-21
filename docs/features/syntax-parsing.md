# Syntax Parsing

**Status**: ✅ Complete (Phase 1)
**Crate**: `compiler/syntax-parsing`
**Entry Point**: `pub fn parse(source: &str) -> Result<Vec<Item>, ParseError>`

## Overview

The syntax parsing feature slice transforms a token stream from the lexer into an Abstract Syntax Tree (AST). It implements a Pratt parser for expressions with correct operator precedence, and a recursive descent parser for statements and declarations.

## Architecture

This slice follows the **Vertical Slice Architecture** pattern:
- **Dependencies**: `lexical-analysis` (for tokenization), `shared-types` (for common types)
- **Public API**: Single entry point (`parse`)
- **Internal implementation**: All parser internals are `pub(crate)`
- **AST exports**: AST types (`Expr`, `Stmt`, `Item`) are public for downstream consumers

## Features

### AST Node Types

#### Items (Top-level declarations)

```rust
pub enum Item {
    Function(FunctionDef),
    // Future: Struct, Enum, Trait, Import, etc.
}

pub struct FunctionDef {
    pub name: Identifier,
    pub params: Vec<Parameter>,
    pub return_type: Option<Type>,
    pub body: Vec<Stmt>,
    pub span: Span,
}
```

#### Statements

```rust
pub enum Stmt {
    VarDecl {
        name: Identifier,
        ty: Option<Type>,
        init: Option<Expr>,
        mutable: bool,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    If {
        condition: Expr,
        then_block: Vec<Stmt>,
        else_if_blocks: Vec<(Expr, Vec<Stmt>)>,
        else_block: Option<Vec<Stmt>>,
        span: Span,
    },
    Expr(Expr),
}
```

#### Expressions

```rust
pub enum Expr {
    Literal(Literal, Span),
    Identifier(Identifier),
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    Paren(Box<Expr>, Span),
}
```

### Operator Precedence

The parser uses **Pratt parsing** (precedence climbing) for correct operator precedence:

| Precedence | Operators | Associativity |
|------------|-----------|---------------|
| 1 (Lowest) | `\|\|` | Left |
| 2 | `&&` | Left |
| 3 | `==`, `!=` | Left |
| 4 | `<`, `>`, `<=`, `>=` | Left |
| 5 | `+`, `-` | Left |
| 6 | `*`, `/`, `%` | Left |
| 7 (Highest) | `-` (unary), `!` | Right |

Example:
```neuro
a + b * c       // Parsed as: a + (b * c)
a < b == c < d  // Parsed as: (a < b) == (c < d)
!a && b         // Parsed as: (!a) && b
```

## Usage

### Basic Parsing

```rust
use syntax_parsing::parse;

let source = r#"
    func add(a: i32, b: i32) -> i32 {
        return a + b
    }
"#;

let ast = parse(source)?;
for item in ast {
    match item {
        Item::Function(func_def) => {
            println!("Function: {}", func_def.name.name);
            println!("  Parameters: {}", func_def.params.len());
            println!("  Body statements: {}", func_def.body.len());
        }
    }
}
```

### Expression Parsing

```rust
let source = "func test() -> i32 { return (a + b) * c }";
let ast = parse(source)?;

// AST structure:
// Binary {
//     left: Paren(Binary { left: "a", op: Add, right: "b" }),
//     op: Multiply,
//     right: "c"
// }
```

### Statement Parsing

```rust
let source = r#"
    func example() -> i32 {
        val x: i32 = 10
        if x > 5 {
            return x * 2
        } else {
            return 0
        }
    }
"#;

let ast = parse(source)?;
// Parses variable declarations, if/else, and return statements
```

## Parsing Algorithm

### Expression Parsing: Pratt Parser

The Pratt parser algorithm:

1. **Parse prefix**: Handle unary operators and atoms (literals, identifiers, parentheses)
2. **Parse infix**: Loop while next operator has higher precedence
3. **Recursively parse right side** with adjusted precedence
4. **Build binary expression node**

Key advantages:
- Correct operator precedence without separate grammar rules
- Simple to implement and maintain
- Efficient (single-pass, no backtracking)

### Statement Parsing: Recursive Descent

Statements are parsed using traditional recursive descent:

1. **Look ahead** at current token
2. **Dispatch** to appropriate statement parser
3. **Recursively parse** nested structures
4. **Validate syntax** and build AST nodes

## Error Handling

### Error Types

```rust
pub enum ParseError {
    UnexpectedToken {
        found: TokenKind,
        expected: String,
        span: Span,
    },
    UnexpectedEof {
        expected: String,
    },
    LexError(LexError),  // Propagated from lexer
}
```

### Error Recovery

Current implementation (Phase 1):
- **Fail-fast**: Stop at first error
- **Precise error messages**: Include what was expected
- **Span information**: Exact location of error

Example error:
```
Error: unexpected token `}`, expected expression
  at line 5, column 12
```

Future (Phase 2+):
- Error recovery to report multiple errors
- Suggestion system for common mistakes
- Better recovery from missing delimiters

## Implementation Details

### Technology

- **Parser type**: Pratt parser (expressions) + Recursive descent (statements)
- **Dependencies**:
  - `lexical-analysis` - Token stream
  - `shared-types` - Common types (Span, Identifier, Literal)

### Design Patterns

**Visitor Pattern** (for future AST traversal):
```rust
impl Expr {
    pub fn span(&self) -> Span {
        // Every expression knows its span
    }
}
```

**Builder Pattern** (for complex AST nodes):
```rust
let binary_expr = Expr::Binary {
    left: Box::new(left_expr),
    op: BinaryOp::Add,
    right: Box::new(right_expr),
    span: left_span.merge(right_span),
};
```

### Testing

**Test coverage**: 65 comprehensive tests

Test categories:
- Expression parsing (all operators, precedence)
- Statement parsing (var decl, return, if/else)
- Function definitions
- Error cases (syntax errors, unexpected tokens)
- Edge cases (nested expressions, complex control flow)

Example test:
```rust
#[test]
fn test_operator_precedence() {
    let source = "func test() -> i32 { return a + b * c }";
    let ast = parse(source).unwrap();

    // Verify that multiplication binds tighter than addition
    let Item::Function(func) = &ast[0];
    let Stmt::Return { value: Some(expr), .. } = &func.body[0];

    match expr {
        Expr::Binary { op: BinaryOp::Add, right, .. } => {
            // Right side should be b * c
            assert!(matches!(**right, Expr::Binary { op: BinaryOp::Multiply, .. }));
        }
        _ => panic!("Expected addition at top level"),
    }
}
```

## Design Decisions

### Why Pratt Parsing for Expressions?

**Alternatives considered:**
- Precedence climbing
- Operator precedence parser
- PEG parser (chumsky, nom)

**Why Pratt:**
- ✅ Simple to implement and understand
- ✅ Easy to extend with new operators
- ✅ Correct precedence handling
- ✅ Efficient (single pass)
- ❌ Less composable than PEG parsers (accepted trade-off for simplicity)

### Why Recursive Descent for Statements?

- Natural fit for statement grammar
- Easy to add error recovery
- Clear mapping from grammar to code
- Debuggable and maintainable

### AST Design Choices

**Boxed sub-expressions**:
- Reduces enum size
- Prevents recursive type definition
- Slight heap allocation cost (acceptable for Phase 1)

**Span on every node**:
- Enables precise error reporting
- Supports IDE features (go-to-definition, hover)
- Required for debugging information

**Separate `Paren` node**:
- Preserves source formatting information
- Helps with error messages
- Can be eliminated in later passes if needed

## API Reference

### Public Functions

```rust
/// Parse NEURO source code into an AST
pub fn parse(source: &str) -> Result<Vec<Item>, ParseError>
```

### Public Types

```rust
// AST node types
pub enum Item { ... }
pub enum Stmt { ... }
pub enum Expr { ... }
pub enum Type { ... }

// Operators
pub enum BinaryOp { Add, Subtract, Multiply, ... }
pub enum UnaryOp { Negate, Not }

// Supporting types
pub struct FunctionDef { ... }
pub struct Parameter { ... }
```

## Integration Points

### Upstream Dependencies

- **lexical-analysis**: Token stream generation
- **shared-types**: Common types (Span, Identifier, Literal)

### Downstream Consumers

- **semantic-analysis**: Type checking the AST
- **llvm-backend**: Code generation from AST
- **LSP server** (Phase 7): AST-based features

## Grammar Reference

### EBNF Grammar (Phase 1)

```ebnf
program        ::= item*
item           ::= function_def

function_def   ::= "func" IDENTIFIER "(" parameters? ")" ("->" type)? "{" statement* "}"
parameters     ::= parameter ("," parameter)*
parameter      ::= IDENTIFIER ":" type

statement      ::= var_decl
                 | return_stmt
                 | if_stmt
                 | expr_stmt

var_decl       ::= ("val" | "mut") IDENTIFIER (":" type)? ("=" expression)? ";"
return_stmt    ::= "return" expression? ";"
if_stmt        ::= "if" expression "{" statement* "}"
                   ("else" "if" expression "{" statement* "}")*
                   ("else" "{" statement* "}")?
expr_stmt      ::= expression ";"

expression     ::= assignment
assignment     ::= logical_or (("=") logical_or)*
logical_or     ::= logical_and (("||") logical_and)*
logical_and    ::= equality (("&&") equality)*
equality       ::= comparison (("==" | "!=") comparison)*
comparison     ::= term (("<" | ">" | "<=" | ">=") term)*
term           ::= factor (("+" | "-") factor)*
factor         ::= unary (("*" | "/" | "%") unary)*
unary          ::= ("!" | "-") unary | call
call           ::= primary ("(" arguments? ")")*
primary        ::= INTEGER | FLOAT | BOOLEAN | STRING
                 | IDENTIFIER
                 | "(" expression ")"

arguments      ::= expression ("," expression)*
type           ::= IDENTIFIER
```

## Examples

### Function with If/Else

```neuro
func max(a: i32, b: i32) -> i32 {
    if a > b {
        return a
    } else {
        return b
    }
}
```

**AST**:
```rust
Item::Function(FunctionDef {
    name: Identifier { name: "max", span: ... },
    params: [
        Parameter { name: "a", ty: Type::Named("i32"), ... },
        Parameter { name: "b", ty: Type::Named("i32"), ... },
    ],
    return_type: Some(Type::Named("i32")),
    body: [
        Stmt::If {
            condition: Expr::Binary {
                left: Expr::Identifier("a"),
                op: BinaryOp::Greater,
                right: Expr::Identifier("b"),
                ...
            },
            then_block: [Stmt::Return { value: Some(Expr::Identifier("a")) }],
            else_block: Some([Stmt::Return { value: Some(Expr::Identifier("b")) }]),
            ...
        }
    ],
    ...
})
```

### Complex Expression

```neuro
func calculate() -> i32 {
    val result = (a + b) * c - d / 2
    return result
}
```

**AST** (simplified):
```
Binary(Subtract)
├─ left: Binary(Multiply)
│  ├─ left: Paren(Binary(Add, "a", "b"))
│  └─ right: "c"
└─ right: Binary(Divide, "d", Literal(2))
```

## Future Enhancements (Post-Phase 1)

- [ ] **Better error recovery**: Continue parsing after errors
- [ ] **Error messages**: "Did you mean?" suggestions
- [ ] **Attributes**: `@gpu`, `@inline`, etc.
- [ ] **Pattern matching**: `match` expressions (Phase 2)
- [ ] **Loops**: `while`, `for` (Phase 2)
- [ ] **Structs**: Type definitions (Phase 2)
- [ ] **Macros**: Procedural and declarative (Phase 8)

## Troubleshooting

### "Unexpected token" errors

**Problem**: Parser expected different token

**Solution**:
- Check syntax matches NEURO grammar
- Ensure all delimiters are balanced (`{`, `}`, `(`, `)`)
- Verify operator precedence expectations

### "Unexpected EOF" errors

**Problem**: Parser reached end of file unexpectedly

**Solution**:
- Check for missing closing braces
- Ensure all statements are properly terminated
- Verify function definitions are complete

## Performance

- **Parsing speed**: ~1ms for small programs (<100 LOC)
- **Memory**: Minimal allocations (boxed expressions only)
- **Single-pass**: No backtracking or re-parsing

## References

- [Pratt Parsing](https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html)
- [Recursive Descent Parsing](https://craftinginterpreters.com/parsing-expressions.html)
- Source: [compiler/syntax-parsing/src/lib.rs](../../compiler/syntax-parsing/src/lib.rs)
