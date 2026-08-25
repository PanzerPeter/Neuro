# Syntax Parsing

**Status**: Complete (Phase 1)
**Crate**: `compiler/syntax-parsing`
**Entry Point**: `pub fn parse(source: &str) -> Result<Vec<Item>, ParseError>`

## Overview

The syntax parsing feature slice transforms a token stream from the lexer into an Abstract Syntax Tree (AST). It implements a Pratt parser for expressions with correct operator precedence, and a recursive descent parser for statements and declarations.

## Architecture

This slice follows the **Vertical Slice Architecture** pattern:
- **Dependencies**: `lexical-analysis` (tokenization), `ast-types` (AST definitions), `shared-types` (common values)
- **Public API**: Single entry point (`parse`)
- **Internal implementation**: All parser internals are `pub(crate)`
- **AST exports**: AST types (`Expr`, `Stmt`, `Item`) are public for downstream consumers

## Features

### AST Node Types

The node set itself lives in the `ast-types` infrastructure crate, not here — that is what
lets semantic analysis, module resolution, and HIR lowering read the tree without depending
on this slice. Reproducing the definitions in prose only guarantees they drift, so this
section describes the shape and points at the source:
[`compiler/infrastructure/ast-types/src/`](../../../compiler/infrastructure/ast-types/src/).

**Items** (`Item`) — the top level of a file: `Function`, `Struct`, `Enum`, `Trait`, `Impl`,
`Const`, `Newtype`, `Import`, `Module` (an inline `module { }` block), and `NoPrelude` (the
file-scope `@no_prelude` marker, consumed by module resolution). A function or struct carries
its generic parameters and lifetimes; an `impl` carries an optional trait name, so the
inherent and trait forms are one node. A method's receiver is `Option<SelfParam>` — absent for
an associated function, otherwise `&self`, `&mut self`, or owned `self`.

**Statements** (`Stmt`) — bindings (`val` / `mut`), assignment and compound assignment, field
and index assignment, dereference assignment, `return`, `break`, `continue`, `if`, `while`,
`for`, `loop`, `match`, destructuring bindings, `val-else`, and a bare expression. An `if` in
statement position always parses to `Stmt::If`, never `Stmt::Expr(Expr::If)`, which is why the
type checker and HIR lowering each recognise a trailing `Stmt::If` as a block's value.

**Expressions** (`Expr`) — literals, identifiers, unary and binary operators, calls (with
optional turbofish type arguments), index, field access, `Type::member` paths, struct and enum
literals, tuples and arrays, ranges, casts, closures, blocks, `unsafe` blocks, `if`, `match`,
`loop`, the `?` propagation operator, and `Paren` grouping (dropped during lowering).

### Operator Precedence

The parser is a **Pratt parser** (precedence climbing). The ladder, loosest first:

| Precedence | Operators | Associativity |
|------------|-----------|---------------|
| 1 (loosest) | `..`, `..=` (range) | Left |
| 2 | `??` (null-coalescing) | Right |
| 3 | `\|\|` | Left |
| 4 | `&&` | Left |
| 5 | `\|` (bitwise or) | Left |
| 6 | `^` | Left |
| 7 | `&` (bitwise and) | Left |
| 8 | `==`, `!=` | Left |
| 9 | `<`, `>`, `<=`, `>=` | Left |
| 10 | `<<` | Left |
| 11 | `+`, `-` | Left |
| 12 | `*`, `/`, `%` | Left |
| 13 | `as` (cast) | Left |
| 14 | `-`, `!`, `~` (unary) | Right |
| 15 | call `f(...)`, index `a[i]`, `?`, turbofish `::<...>` | Left |
| 16 (tightest) | `.` (field / method access) | Left |

There is no `>>` operator: right shift is the `.shr(n)` method, because `>>` is reserved for
function composition. `??` associates right-to-left so `a ?? b ?? c` evaluates each fallback
only when every left-hand side before it was absent.

```neuro
a + b * c       // a + (b * c)
a < b == c < d  // (a < b) == (c < d)
!a && b         // (!a) && b
f(x)? + 1       // (f(x)?) + 1
```

**Statement boundaries.** A newline ends a statement unless the line that just ended asks to
continue — it ends with a binary operator, a comma, or an opening delimiter, or the expression
is inside an unclosed `(`, `[`, or `{`. The decision belongs to the line that ended, so a line
*starting* with `(`, `[`, or `*` opens a new statement rather than continuing the one above as
a call, an index, or a multiplication.

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
        Item::Struct(struct_def) => {
            println!("Struct: {}", struct_def.name.name);
            println!("  Fields: {}", struct_def.fields.len());
        }
        Item::Impl(impl_def) => {
            println!("Impl for: {}", impl_def.type_name.name);
            println!("  Methods: {}", impl_def.methods.len());
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

`ParseError` (see [`errors.rs`](../../../compiler/syntax-parsing/src/errors.rs)) covers the
token-level failures — `UnexpectedToken`, `UnexpectedEof`, a wrapped `LexError`, and
`MaxDepthExceeded`, which stops runaway nesting rather than overflowing the stack — plus the
grammar rules that are cheapest to enforce while parsing: `DuplicateParameter`,
`DuplicateTypeAlias`, `TypeAliasShadowsBuiltin`, `CyclicTypeAlias`, `EnumLifetimeParam`,
`ExportNotAllowed`, and `MisplacedNoPrelude`. Each carries the span of the offending token,
not the start of the enclosing construct.

### Error Recovery

- **Fail-fast**: parsing stops at the first error. Multiple-error reporting is the type
  checker's job — it collects diagnostics and keeps going.
- **Precise error messages**: each names what was expected
- **Span information**: the exact location of the offending token

Example error:
```
Error: unexpected token `}`, expected expression
  at line 5, column 12
```

Future (Phase 1+):
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
- Simple to implement and understand
- Straightforward to extend with new operators
- Correct precedence handling
- Efficient (single pass)
- Less composable than PEG parsers (accepted trade-off for this phase)

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
/// Parse Neuro source code into an AST
pub fn parse(source: &str) -> Result<Vec<Item>, ParseError>

/// Parse a single expression — used by tests and tooling, not by the driver
pub fn parse_expr(source: &str) -> Result<Expr, ParseError>
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

- **module-resolution**: expands the parsed root file into a whole program (the driver
  injects `parse` into it, so that slice does not depend on this one)
- **semantic-analysis**: type checking the AST
- **hir-lowering**: AST → typed HIR, which is what the backends consume
- **LSP server** (Phase 8): AST-based features

## Grammar Reference

### Grammar

The language surface is documented feature by feature in the
[language reference](../../language-reference/), where every construct is shown in a program
that compiles. That is the grammar's source of truth; a second copy here would only drift
from it.

What belongs in this document is the mapping from grammar to code:

| Construct | Parsed by |
|---|---|
| Top-level dispatch (`func`, `struct`, `enum`, `trait`, `impl`, `const`, `newtype`, `type`, `import`, `module`, `@no_prelude`) | `parser/items.rs` |
| Functions, parameters, generic and lifetime lists | `parser/item_functions.rs` |
| Structs and their fields | `parser/item_structs.rs` |
| Enums and their variants | `parser/item_enums.rs` |
| Traits, `impl` blocks, methods, associated types | `parser/item_impls.rs` |
| `import` / `export import` forms | `parser/item_imports.rs` |
| Statements, bindings, control flow | `parser/statements.rs`, `stmt_loops.rs`, `stmt_assignments.rs` |
| `val PATTERN = expr else { ... }` | `parser/stmt_val_else.rs` |
| Destructuring bindings | `parser/stmt_destructure.rs` |
| Match patterns | `parser/patterns.rs` |
| Expressions (Pratt) | `parser/expressions.rs` |
| Type syntax | `parser/types.rs` |
| `type` aliases | `parser/type_aliases.rs` |

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

## Future Enhancements

- [ ] **Error recovery**: continue parsing after an error so a run can report more than one
- [ ] **Suggestions**: "did you mean?" for near-miss identifiers and keywords
- [ ] **Triple-quoted strings**: the `"""..."""` block form (1H)
- [ ] **Named arguments**: the `external internal: T` parameter form (1H)
- [ ] **Macros**: procedural and declarative (Phase 7)

## Troubleshooting

### "Unexpected token" errors

**Problem**: Parser expected different token

**Solution**:
- Check syntax matches Neuro grammar
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
