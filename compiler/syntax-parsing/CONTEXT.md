# syntax-parsing

## Purpose
Transform a NEURO token stream into a typed Abstract Syntax Tree that subsequent compiler stages can traverse.

## Entry Point
- Type: Library function
- Input: `source: &str`
- Output: `Result<Vec<Item>, ParseError>`

## Data Ownership
- Tables: none
- Events Published: none
- Events Consumed: none
- Public Read Model: none

## Shared Kernel
- ast-types — owns AST node definitions so semantic-analysis and llvm-backend can consume the tree without depending on this slice
- shared-types — provides `Span`, `Identifier`, and `Literal` used throughout the grammar
- lexical-analysis — direct consumer relationship; `parse()` calls `tokenize()` internally so callers need only one entry point

## Notes
The direct dependency on lexical-analysis is a deliberate intra-pipeline coupling,
not a VSA cross-slice violation. syntax-parsing is the sole consumer of the token
stream; externalising tokenisation would create an unnecessary coordination step in
neurc. The architecture test carries an explicit allowlist entry for this pairing.
