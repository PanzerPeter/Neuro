# syntax-parsing

## Purpose
Transform a Neuro token stream into a typed Abstract Syntax Tree that subsequent compiler stages can traverse.

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
to `true` while parsing if/while/for conditions and `else if` conditions. This prevents
`Identifier { ... }` from consuming the `{` that opens the body block — the same
disambiguation strategy used by Rust. All condition sites (primary `if`, every `else if`
arm, and `while`) must set and clear the flag symmetrically.

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

Compound assignment (`+=`, `-=`, `*=`, `/=`, `%=`): `parse_statement` detects
compound-assignment tokens via a one-token lookahead and calls
`parse_compound_assignment_stmt`, which desugars `target OP= rhs` into
`Stmt::Assignment { target, value: Expr::Binary { target, OP, rhs } }` at parse
time. No new AST nodes; semantic analysis and codegen are unaffected.

## Recent Updates
- 2026-05-18: Added `??` (null/error coalescing) at parser level. New `Precedence::NullCoalesce` between `Lowest` and `LogicalOr` per Appendix B row 14; wired in `is_binary_op`, `token_to_binary_op`, `get_precedence`. R-to-L associativity is enforced by recursing on the right operand with `Precedence::Lowest`, so `a ?? b ?? c` parses as `a ?? (b ?? c)`. AST shape locked in by `test_null_coalesce_is_right_associative`. Semantic and codegen do not yet support the operator — see `semantic-analysis` and `llvm-backend` notes.
- 2026-04-18: Integer literal type suffixes §1.4. `parse_prefix` handles `TokenKind::IntegerSuffix(tok)` → `Literal::Integer(tok.value, Some(tok.suffix))`; plain `TokenKind::Integer(n)` now produces `Literal::Integer(n, None)`.

- 2026-04-04: Enabled parsing of `..=` for inclusive `for` ranges.
- 2026-04-16: Added `parse_const_def()` (module-level `const NAME: Type = expr`) and
  `parse_const_stmt()` (function-body const). `parse_program` dispatches on `TokenKind::Const`
  → `parse_const_def`; `parse_stmt` dispatches similarly for body consts.
- 2026-04-18: Implemented bitwise operators §1.4. New `Precedence` variants `Shift`, `BitwiseAnd`,
  `BitwiseXor`, `BitwiseOr` inserted between `LogicalAnd` and `Equality` (for bitwise OR/XOR/AND)
  and between `Comparison` and `Sum` (for `<<`), matching Appendix B precedence table levels 7–10.
  `Amp` wired as `BinaryOp::BitAnd`. `Tilde` parses as unary `UnaryOp::BitNot` at `Precedence::Unary`.
