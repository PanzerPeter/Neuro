# Slice: Semantic Analysis

## Business Intent
Validate program semantics and enforce type safety before code generation.

## Public Interface
- **Trigger:** `type_check(items: &[Item])` function called by neurc driver
- **Input:** AST items from syntax-parsing
- **Output:** `Result<(), SemanticError>` - Success or type/semantic error
- **Reads:** AST from syntax-parsing output
- **Writes:** None (validation only, no mutation)

## Data Ownership
- **Owns:** Type checking rules, symbol table, semantic validation logic
- **Subscribes to:** None

## Implementation Details
Performs multi-pass analysis:
1. Symbol collection (build function and variable symbol tables)
2. Type checking (validate expressions, statements, function calls)
3. Semantic validation (unused variables, unreachable code, etc.)

Supports:
- Type inference for val declarations
- Type checking for binary/unary operations
- Function signature validation
- Return type verification
- Variable shadowing detection

Current limitations (Phase 1):
- No struct/enum support yet
- No generic types
- Limited type inference (only for literals)

## Dependencies
- **ast-types**: AST node definitions for traversal
- **shared-types**: Span for error locations
- **diagnostics**: Error reporting infrastructure
