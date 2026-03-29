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

Struct literal disambiguation: the `Parser` carries a `no_struct_lit: bool` flag set
to `true` while parsing if/while/for conditions. This prevents `Identifier { ... }`
from consuming the `{` that opens the body block — the same disambiguation strategy
used by Rust.

`impl` blocks: `parse_program` dispatches on `TokenKind::Impl` → `parse_impl_def`,
which parses `impl TypeName { method* }`. Each method is parsed by `parse_method_def`,
which calls `try_parse_self_param` to detect `&self`, `&mut self`, or `self` before
the regular parameter list.

Path expressions: when `parse_prefix` sees `Identifier` followed by `::`, it
consumes both tokens and produces `Expr::Path { type_name, member, span }`. This
node appears as the `func` of an `Expr::Call` for associated function calls like
`Point::new(x, y)`.

The `Amp` (`&`) token was added to the lexer to support self-parameter parsing.
logos uses longest-match so `&&` still tokenizes as `AmpAmp`.
