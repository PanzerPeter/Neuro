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

`impl` blocks: `parse_program` dispatches `TokenKind::Impl` → `parse_impl_def`, which accepts both
inherent `impl TypeName { method* }` and trait `impl TraitName for TypeName { method* }` (a `for`
after the first identifier selects the trait form, recording `ImplDef::trait_name`). Each method
via `parse_method_def`, which calls `try_parse_self_param` to detect `&self`/`&mut self`/`self`
before the param list.

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
- 2026-07-06: Generic structs & impls §3.8. `parse_struct_def` parses an optional generic
  parameter list after the struct name (`StructDef.generics`); `parse_impl_def` parses impl-level
  generics after `impl` and optional type arguments on the type name (`ImplDef.generics` /
  `type_args`, via the new `parse_optional_type_args`). `parse_type` now builds a
  `Type::Generic { name, args, span }` when a `<` follows a type name, so `Pair<i32, f64>`
  annotations parse. `>` closes the list (nested `Foo<Bar<T>>` lexes as two `>` tokens, no `>>`).
- 2026-07-03: Generic functions §3.8. `parse_function` now parses an optional generic parameter
  list `<T, U: Bound + Other>` (new `parse_generic_params`) between the function name and its
  parameter list, filling `FunctionDef.generics`. Bounds after `:` are `+`-separated trait names,
  recorded but not enforced. A duplicate type-parameter name is a `DuplicateParameter` error. No
  new call-site surface (turbofish is a follow-on); type arguments are inferred downstream.
- 2026-07-02: Newtype declarations §3.15. `parse_program` dispatches `TokenKind::Newtype` to
  `parse_newtype_def` (`newtype Name = InnerType`), pushing an `Item::Newtype`. Unlike a `type` alias,
  a newtype is a distinct nominal type, so it is NOT expanded away — it stays an item for semantic
  analysis. Construction `Name(value)` reuses the existing call parse and `.0` reuses tuple-index parse
  (no new expression grammar). Type-alias `rewrite_item` recurses into the newtype's inner type so an
  aliased inner (`newtype Y = SomeAlias`) still expands.
- 2026-07-02: Pattern matching §3.6 (`parser/patterns.rs`). `parse_prefix` dispatches `TokenKind::Match`
  to `parse_match_expr`, which parses the scrutinee with struct-literals suppressed, then arms. Each arm
  is `pattern ('|' pattern)* ('if' guard)? '=>' body` (`parse_match_arm`); `parse_pattern` reads
  wildcard/binding/literal/range/`E::V` variant patterns (with `(tuple)` / `{ named }` payloads). A
  leading `-` on a numeric literal and `..`/`..=` ranges are handled in `parse_pattern_literal`.
- 2026-06-30: Enums with associated data §3.5. `parse_program` dispatches `TokenKind::Enum` to
  `parse_enum_def`; `parse_enum_variant` reads unit / `(tuple)` / `{ named }` payloads. A path
  followed by `{` in the prefix parser (when struct literals are allowed) parses as
  `parse_enum_struct_literal` → `Expr::EnumStructLiteral`. New `consume_identifier` helper. Type-alias
  `rewrite_item`/`rewrite_expr` recurse into enum payload types and enum-literal field values.
- 2026-06-29: Struct + array destructuring §3.2. `parse_stmt_into` now detects `val`/`mut` followed by a
  tuple `(`, array `[`, or struct `Name {` pattern and routes to `parse_destructure_bind`, which parses
  any top-level pattern (`parse_top_pattern`), binds the RHS to a `__destructure_N` temp, and expands.
  `DestructurePattern` gained `Struct { fields }` (shorthand field binds → `FieldAccess`) and
  `Array(Vec<ArrayPatternElem>)` (positional `Index` binds + an optional trailing `Rest`). A rest
  expands to `Expr::ArrayRest { start, exact: false }`; a rest-less array adds a discarded
  `ArrayRest { exact: true }` arity assertion. Element/struct patterns nest through
  `parse_pattern_element`. Alias rewrite covers the new `ArrayRest` node.
- 2026-06-28: Tuples §3.2. `parse_type` parses the tuple type `(T1, T2, ...)` (≥2 elements; a single
  `(T)` is grouping, `()` unit is rejected). `parse_prefix`'s `(` branch produces an
  `Expr::TupleLiteral` when a comma follows the first expression, else `Expr::Paren`. The `.` infix
  reads a following integer token as a constant `Expr::TupleIndex` (`t.0`), keeping identifier dots as
  field access. Destructuring `val (a, b) = e` is a **parse-time desugar** (no AST node): block
  collectors call the new `parse_stmt_into`, which detects `val (` / `mut (` and expands the pattern
  via `parse_tuple_destructure` to a fresh `__destructure_N` temp binding plus one projection per leaf
  — supporting `_` wildcards and nested patterns through a parse-local `DestructurePattern`. Alias
  rewrite covers the new type/expr nodes.
- 2026-06-19: Arrays §3.1. `parse_type` parses `[T; N]`; `parse_prefix` parses `[..]` array literals;
  `parse_infix` + `get_precedence` parse `a[i]` indexing (call precedence); `parse_for_stmt` branches
  `Stmt::ForRange` vs `Stmt::ForEach` on the presence of a `..` / `..=`; the identifier-statement path
  builds `Stmt::IndexAssignment` for `arr[i] = v`. Alias-rewrite covers the new nodes.
- 2026-06-18: Range expressions for `string.slice` (§2.7). `parse_infix` handles `..` / `..=`
  (`TokenKind::DotDot` / `DotDotEqual`) → `Expr::Range`, at the new `Precedence::Range` (below `??`).
  `parse_for` now parses the range start bound at `Precedence::Range` so the loop's own `..` / `..=`
  separator is not swallowed — `for`-range behaviour is unchanged.
- 2026-06-15: `char` literals (§1.2). `parse_prefix` maps `TokenKind::Char(c)` → `Expr::Literal(Literal::Char(c))`.
- 2026-06-15: `loop` as a value expression (§3.7). `parse_prefix` dispatches `TokenKind::Loop` to
  `parse_loop_expr` (and `label: loop` to `parse_labeled_loop_expr`), producing `Expr::Loop`. `break`
  parsing moved to `parse_break_stmt`: a trailing identifier is read as a label only when it names an
  in-scope loop (`Parser::active_labels`, pushed by `parse_labeled_block` / `parse_labeled_loop_expr`),
  otherwise it begins the value expression; an optional same-line value follows. `continue` keeps the
  greedy `parse_optional_loop_label`.
- 2026-06-15: Loop labels (§3.7). `parse_stmt`'s identifier branch calls `try_parse_labeled_loop`,
  which dispatches `ident : <for|while|loop>` to the matching loop parser with `Some(label)` (labels
  reuse the existing `Identifier` + `Colon` tokens — no lexer change). `parse_while_stmt` /
  `parse_loop_stmt` / `parse_for_stmt` take an `Option<Identifier>` label. `break` / `continue` read
  an optional trailing same-line label via `parse_optional_loop_label` (no newline skip, so a
  line-final `break` is never a labeled break).
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
- 2026-06-04: `unsafe { }` block expressions (1C). New `parse_unsafe_expr` prefix handler
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
