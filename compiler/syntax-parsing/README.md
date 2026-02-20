# Slice: Syntax Parsing

## Business Intent
Transform token stream into Abstract Syntax Tree (AST) representing program structure.

## Public Interface
- **Trigger:** `parse(source: &str)` function called by neurc driver
- **Input:** Source code string (internally tokenizes)
- **Output:** `Result<Vec<Item>, ParseError>` - AST items or syntax error
- **Reads:** Tokens from lexical-analysis
- **Writes:** None (pure transformation)

## Data Ownership
- **Owns:** Parsing logic, grammar rules, precedence handling
- **Subscribes to:** None

## Implementation Details
Recursive descent parser with Pratt parsing for expression precedence. Supports:
- Function definitions with parameters and return types
- Variable declarations (val/mut)
- Expressions (literals, binary/unary ops, function calls)
- Statements (if/else, while, `for i in start..end`, break, continue, return, expression statements)
- Type annotations

VSA 4.0 Note: AST types extracted to infrastructure/ast-types to eliminate
cross-slice dependencies. This slice constructs AST nodes but doesn't own their
type definitions.

## Dependencies
- **ast-types**: AST node type definitions (Expr, Stmt, Item, Type)
- **shared-types**: Span, Identifier, Literal
- **lexical-analysis**: Token stream generation
