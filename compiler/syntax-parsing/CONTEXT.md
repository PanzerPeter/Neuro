# syntax-parsing

## Purpose
Transform a Neuro token stream into a typed Abstract Syntax Tree for later compiler stages.

## Entry Point
- Type: Library function
- Input: `source: &str`
- Output: `Result<Vec<Item>, ParseError>`

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- ast-types — owns AST node definitions so semantic-analysis/llvm-backend consume the tree without
  depending on this slice
- shared-types — `Span`, `Identifier`, `Literal` used throughout the grammar
- lexical-analysis — direct consumer; `parse()` calls `tokenize()` internally (callers need one entry)

## Notes
The lexical-analysis dependency is deliberate intra-pipeline coupling, not a VSA violation:
syntax-parsing is the sole token-stream consumer, and externalising tokenisation would add an
unnecessary neurc coordination step. The architecture test allowlists this pairing.

Struct literal disambiguation: `Parser` carries `no_struct_lit: bool`, set `true` while parsing
if/while/for and `else if` conditions, so `Identifier { ... }` does not consume the `{` opening the
body block (same strategy as Rust). All condition sites set/clear the flag symmetrically.

`impl` blocks: `parse_program` dispatches `TokenKind::Impl` → `parse_impl_def` (`impl TypeName
{ method* }`); each method via `parse_method_def`, which calls `try_parse_self_param` to detect
`&self`/`&mut self`/`self` before the param list.

Path expressions: when `parse_prefix` sees `Identifier` `::`, it produces `Expr::Path { type_name,
member, span }` — the `func` of an `Expr::Call` for associated calls like `Point::new(x, y)`.

`Amp` (`&`) token added to the lexer for self-param parsing; logos longest-match keeps `&&` as
`AmpAmp`.

Compound assignment (`+=`,`-=`,`*=`,`/=`,`%=`): `parse_statement` detects the tokens via one-token
lookahead → `parse_compound_assignment_stmt`, desugaring `target OP= rhs` into
`Stmt::Assignment { target, value: Expr::Binary { target, OP, rhs } }` at parse time. No new AST nodes.

Type aliases (`type Name = Target`, §3.14): `parse_program` dispatches `TokenKind::Type` →
`parse_type_alias`, collecting declarations separately from `items`. After parsing,
`expand_type_aliases` (`parser/type_aliases.rs`) resolves alias chains (rejecting cycles, duplicates,
built-in shadows) and substitutes every aliased type annotation across items/statements/expressions,
preserving the use-site span. Transparent — like compound assignment, semantic/codegen never observe
them; an unknown target hits the existing `UnknownTypeName` check. Scope: type-annotation positions
only (var/const/param/return/field/cast); alias as value constructor or path name is out of scope.

## Recent Updates
- 2026-06-09: `loop { ... }` infinite-loop statement (§3.7). `parse_stmt` dispatches `TokenKind::Loop`
  to `parse_loop_stmt`, which parses a block body into `Stmt::Loop { body, span }` (no condition).
  `stmt_span` and the type-alias `rewrite_stmt` cover the new node.
- 2026-06-09: Mutable borrows `&mut T` + deref `*` (§2.5). `parse_type` and the prefix-`&` borrow
  accept an optional `mut` after `&`, setting `mutable` on `Type::Reference` / `Expr::Reference`.
  Prefix `TokenKind::Star` now parses a dereference `Expr::Deref { operand, span }` (operand at
  `Precedence::Unary`); infix `*` stays multiply. `parse_stmt` handles a leading `*` as either a
  deref expression statement or, when followed by `=`, a `Stmt::DerefAssignment { pointer, value }`.
  Continuation fix: `parse_expr_inner` treats a newline followed by `*` as a statement boundary (via
  `peek_next_nonnewline_kind`), so `*r = v` after an expression-ending line is not glued as a
  multiplication. `stmt_span` and the type-alias rewrite cover the new nodes.
- 2026-06-08: Immutable borrows §2.4 — `parse_type` parses a leading `&` recursively into
  `Type::Reference { inner, span }`; `parse_prefix` handles `TokenKind::Amp` in prefix position as a
  borrow `Expr::Reference { operand, span }` (operand at `Precedence::Unary`). Infix `&` is still
  `BinaryOp::BitAnd`, so prefix vs. infix `&` are disambiguated purely by parser position. Param /
  field span computation switched to `Type::span()` to cover the new variant.
- 2026-06-07: `@derive(...)` attaches to struct definitions (§2.3). `parse_program` passes the
  collected `Vec<Attribute>` into `parse_struct_def(attributes)` → `StructDef.attributes`. The
  "attribute before non-function item" rejection now fires only when an attribute precedes neither
  `func` nor `struct`. Semantics (Copy/Clone) live in semantic-analysis; parser accepts any name.
- 2026-06-05: Struct shorthand + functional-update (§3.3) in `parse_struct_literal`. A field with no
  `: value` desugars to `field: field` (`FieldInit { value: Expr::Identifier(name) }`); a trailing
  `..expr` sets `StructLiteral.base` and ends the field list. `rewrite_expr` recurses into `base`.
  Parse-time desugaring; semantic/codegen see only `base`.
- 2026-06-04: `unsafe { }` block expressions (Phase 1.7). New `parse_unsafe_expr` prefix handler
  (`TokenKind::Unsafe`) → `Expr::Unsafe { stmts, span }`; body parses as a statement block; inert.
  `rewrite_expr` recurses into the block. Reaches the prefix parser via expression-statement
  fallthrough (no statement-parser change).
- 2026-06-03: Type-alias declarations §3.14 — `TokenKind::Type` dispatch; `parse_type_alias` +
  `expand_type_aliases` (`parser/type_aliases.rs`). New `ParseError::{DuplicateTypeAlias,
  TypeAliasShadowsBuiltin, CyclicTypeAlias}`. Parse-time desugaring, no AST node.
- 2026-05-20: Attribute parsing — `parse_attributes` collects `@name`/`@name(arg,...)` ahead of every
  `func` (free + impl methods) → `Vec<Attribute>` on `FunctionDef`/`MethodDef`. Attributes before
  non-function items rejected (`UnexpectedToken`). Semantics live in semantic-analysis; any name
  accepted so future `@grad`/`@gpu`/`@no_prelude` need no grammar churn.
- 2026-05-18: `??` (null/error coalescing) — `Precedence::NullCoalesce` between `Lowest` and
  `LogicalOr` (Appendix B row 14); R-to-L associativity via recursing on the right operand at
  `Precedence::Lowest`. Semantic/codegen don't yet support it.
- 2026-05-25: Float literal suffixes §1.2/§1.4 — `parse_prefix` handles `TokenKind::FloatSuffix` →
  `Literal::Float(val, Some(suffix))`; plain `Float(f)` → `None`.
- 2026-04-18: Integer literal suffixes §1.4 — `parse_prefix` handles `TokenKind::IntegerSuffix` →
  `Literal::Integer(val, Some(suffix))`; plain `Integer(n)` → `None`.
- 2026-04-18: Bitwise operators §1.4 — new `Precedence` variants `Shift`/`BitwiseAnd`/`BitwiseXor`/
  `BitwiseOr` (Appendix B levels 7–10); `Amp` → `BinaryOp::BitAnd`; `Tilde` → unary `BitNot` at
  `Precedence::Unary`.
- 2026-04-16: `parse_const_def` (module-level) + `parse_const_stmt` (body); `parse_program`/`parse_stmt`
  dispatch on `TokenKind::Const`.
- 2026-04-04: Parse `..=` for inclusive `for` ranges.
