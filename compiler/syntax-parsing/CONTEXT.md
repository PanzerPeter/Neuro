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

Struct literal disambiguation: `Parser` carries `no_struct_lit: bool`, raised while parsing a
guarded header — an `if`/`else if`/`while` condition, a `for` iterable, a `match` scrutinee, a
match-arm guard, a `where`-clause value predicate — so `Identifier { ... }` does not consume the
`{` opening the body block (same strategy as Rust). Two scoped helpers own the flag and nothing
else assigns it: `guarded_header` raises it, `inside_delimiters` lifts it for `( ... )` and
`[ ... ]` (grouping, tuple literals, call and turbofish argument lists, array literals, index
brackets), where a `{` cannot be a body block. Both restore the previous value, on the error path
too, so nesting composes — `if check(Point { x: 1 }) && flag { }` reads the literal inside the
argument list and the trailing brace as the body.

`impl` blocks: `parse_program` dispatches `TokenKind::Impl` → `parse_impl_def`, which accepts both
inherent `impl TypeName { method* }` and trait `impl TraitName for TypeName { method* }` (a `for`
after the first identifier selects the trait form, recording `ImplDef::trait_name`). Each method
via `parse_method_def`, which calls `try_parse_self_param` to detect `&self`/`&mut self`/`self`
before the param list.

Path expressions: when `parse_prefix` sees `Identifier` `::`, it produces `Expr::Path { type_name,
member, span }` — the `func` of an `Expr::Call` for associated calls like `Point::new(x, y)`.
A path may carry more than two segments now that modules exist (`utils::io::read`): the loop
folds everything ahead of the final segment into one qualifier identifier whose `name` holds the
`::` separators, and `parse_type` does the same for a qualified type annotation
(`geometry::Point`). No AST node changed — `module-resolution` splits the qualifier again,
verifies it against the module that owns the name, and erases it before semantic analysis, so
no `::` name ever reaches the type checker. The `::<` turbofish check still wins at every step,
so `f::<i32>(x)` and `math::helper::<i32>(x)` both parse.

`Amp` (`&`) token added to the lexer for self-param parsing; logos longest-match keeps `&&` as
`AmpAmp`.

Compound assignment (`+=`,`-=`,`*=`,`/=`,`%=`): `parse_statement` detects the tokens via one-token
lookahead → `parse_compound_assignment_stmt`, desugaring `target OP= rhs` into
`Stmt::Assignment { target, value: Expr::Binary { target, OP, rhs } }` at parse time. No new AST nodes.

Type aliases (`type Name = Target`): `parse_program` dispatches `TokenKind::Type` →
`parse_type_alias`, collecting declarations separately from `items`. After parsing,
`expand_type_aliases` (`parser/type_aliases.rs`) resolves alias chains (rejecting cycles, duplicates,
built-in shadows) and substitutes every aliased type annotation across items/statements/expressions,
preserving the use-site span. Transparent — like compound assignment, semantic/codegen never observe
them; an unknown target hits the existing `UnknownTypeName` check. Scope: type-annotation positions
only (var/const/param/return/field/cast); alias as value constructor or path name is out of scope.

## Recent Updates
- 2026-08-24: `Type::Generic`'s span covers its closing `>`. `parse_generic_type_args` returns
  the closing token's span alongside the arguments; the span used to end at the last argument,
  so every diagnostic pointing at a generic type application underlined one byte short
  (`Box<i32, i32` for `Box<i32, i32>`).
- 2026-08-23: `val-else` takes an unqualified variant pattern (BUG-010). `starts_val_else` widened
  from `val Name::` to also accept `val Name(`: a binding name after `val` is only ever followed by
  `:`, `=`, or a newline, so a payload makes the reading unambiguous, and the prelude had made
  `val Some(v) = ... else { }` the idiomatic spelling everywhere the marker did not reach.
  Parse-only — `parse_pattern` already produced `Pattern::UnqualifiedEnum`, which module resolution
  already rewrites. `val Name {` stays a struct destructure and a payload-less `val None = ...`
  stays a binding, both for the reason a bare `None` pattern is ambiguous. A missing `else` now
  reports the missing `else` rather than an unexpected `=`.
- 2026-08-23: `@no_prelude`. `parse_item_list` checks for the marker *before* the attribute list,
  because an attribute list is claimed by the declaration that follows it and this one has none:
  `@` followed by the identifier `no_prelude` is consumed into `Item::NoPrelude(Span)`, a new
  `ast-types` node. It is accepted only as the first thing in a file — not after a declaration,
  and not inside a `module { }` block, which is not a file — reported as the new
  `ParseError::MisplacedNoPrelude`. What the marker then *means* is module resolution's question;
  the parser only records where it was written.
- 2026-08-23: Inline `module { }` blocks and `export import`. `parse_program` now delegates to
  `parse_item_list`, which runs to end of input at file level and to the closing brace inside a
  block, so a `module Name { ... }` body is parsed by the same code as the file around it and
  blocks nest for free. The block becomes `Item::Module(ModuleDef)`, a new `ast-types` node and
  a new public re-export. Type-alias declarations from every nesting level are collected into one
  list and expanded in a single pass, and trait-default injection walks blocks too — both are
  erased at parse time, so keeping them file-scoped is what makes an alias declared beside a
  block still read inside it. `export` is now *accepted* on an `import`, setting the new
  `ImportDef.exported`, and is rejected on a `module` block instead: an inline module's name is
  reached only from the file that declares it. Which names an `export import` may re-export is a
  file-system question and stays with module-resolution.
- 2026-08-22: `export` visibility. `parse_program` consumes an optional `export` marker between
  the attributes and the item keyword — the position `pub` takes in Rust, so `@derive(Copy)`
  still reads as attached to the declaration — and sets `exported` on the item it parses.
  `parse_struct_def` reads the same marker per field, since an exported struct may still keep
  a field to itself. `export` is rejected on an `impl` block (it declares no name), on a
  `type` alias (expanded at parse time, so nothing of it survives to reach another module),
  and (until 2026-08-23) on an `import`, all through the new `ParseError::ExportNotAllowed`.
  An enum struct-variant's fields are marked exported: a variant is reached through a
  pattern naming its enum. Nothing here *enforces* visibility —
  the parser only records what was written.
- 2026-08-18: `import` declarations. New `parser/item_imports.rs`: `parse_import` reads all five
  surface forms into one `Item::Import` — an optional leading `./`, a `::`-separated path, then an
  optional `{a, b as c}` list or a trailing `as` alias. `as` doubles as the cast operator, so an
  import reads it as a rename marker only when an identifier follows; a cast keeps its meaning.
  `parse_pattern` also learned the unqualified variant form: a bare identifier followed by `(` or
  `{` becomes `Pattern::UnqualifiedEnum` (the payload parser is now shared with the qualified
  `Enum::Variant` form). A bare `None` still parses as `Pattern::Binding` — nothing at parse time
  tells the two apart, and module-resolution owns the decision either way.
- 2026-08-03: Error propagation. `parse_infix` gains a `TokenKind::Question` arm producing
  `Expr::Try`, and `get_precedence` maps `?` to `Precedence::Call` — postfix, binding as tightly as
  a call or index, so `f(x)? + 1` adds to the unwrapped payload and `parse(s)?.field` reads a field
  of it. No new precedence level is needed (Appendix B row 1). The type-alias rewrite walker gained
  the `Expr::Try` arm.
- 2026-07-31: `val-else`. New `stmt_val_else.rs`: `parse_stmt`'s `Val` arm consults
  `starts_val_else` (an `Identifier` followed by `ColonColon` or `LeftParen` after the keyword — see
  the 2026-08-23 entry, which added the second marker) before falling
  through to `parse_var_decl`. That two-token marker is unambiguous — a binding name is followed by
  `:`, `=`, or a newline — and it is checked ahead of `starts_destructure_pattern`, so
  `val Point { x, y } = p` still desugars as a struct destructure while `val Shape::Circle { r } = s
  else { ... }` parses as a `val-else`. `parse_pattern` is now `pub(super)` and shared with
  `patterns.rs`. The `|name|` after `else` is a dedicated production, not a closure literal.
  `stmt_span` and the type-alias rewrite walker gained `Stmt::ValElse` arms.
- 2026-07-28: `parse_loop_stmt` returns `Stmt::Expr(Expr::Loop { .. })` instead of the removed
  `Stmt::Loop`; `stmt_span` and the type-alias rewrite walker lose their `Stmt::Loop` arms (both
  already reach the node through `Stmt::Expr`). `items.rs` and `statements.rs` are split by item and
  statement kind into `item_{functions,structs,enums,impls}.rs` and
  `stmt_{loops,assignments,destructure}.rs`; `parse_program`, attributes, `parse_stmt` dispatch,
  and `stmt_span` stay in the original files. No parser behaviour changed by the split.
- 2026-07-26: Generic enums. `parse_enum_def` now parses an optional `<...>` list after the enum
  name via the existing `parse_generic_params`, filling `EnumDef.generics`; a lifetime parameter is
  rejected with the new `ParseError::EnumLifetimeParam` (enum payloads are scalars this phase, so
  there is nothing for a lifetime to annotate). Payload annotations naming a type parameter are
  plain `Type::Named`, resolved by later passes against the enum's generics.
- 2026-07-24: Closures and lambdas. `parse_prefix` now handles a leading `|` / `||` / `move` as a closure literal via a new `parse_closure` helper, producing `Expr::Closure`. Parameters take an optional `: T` annotation; an optional `-> R` follows; the body is a brace block (`Expr::Block`) or a single expression (`Precedence::Lowest`, so it stops at a `,`/`)`/newline). `parse_type` now parses a parenthesized type list followed by `->` as a function type `Type::Function` (accepting zero-plus params), keeping the ≥2-element tuple form for a list with no arrow. The alias-substitution walker in `type_aliases.rs` recurses into both new nodes.
- 2026-07-19: Static & dynamic dispatch. `parse_type` now accepts `impl Trait` and `dyn Trait`, producing `Type::ImplTrait` / `Type::DynTrait` via the new `parse_trait_ref_name` helper. `parse_function` then desugars ARGUMENT-position `impl Trait` (including nested under `&`/`&mut`, arrays, and tuples) into fresh anonymous generic parameters `__implN: Trait` appended to the function's `generics`, replacing each occurrence with a plain `Type::Named` — so static dispatch reuses the existing monomorphized-generic machinery unchanged and each `impl Trait` parameter is independently inferred. Return-position `impl Trait` is deliberately NOT desugared (it is one concrete type chosen by the body, not a caller-inferred parameter) and is resolved by semantic-analysis instead.
- 2026-07-18: Operator traits — scalar path. `parse_impl_def`'s body loop now accepts an
  associated-type binding `type Name = Type` (via `parse_assoc_type_binding`) alongside methods,
  collected into the new `ImplDef.assoc_types`. This lets an operator-trait impl declare its
  `type Output = T`. No new keyword — reuses `TokenKind::Type`. Trait declarations themselves are
  unchanged (operator traits are compiler-known lang-items, so the user writes only the `impl`).
- 2026-07-16: Trait declarations. `parse_program` dispatches `TokenKind::Trait` →
  `parse_trait_def`, which reads method signatures via `parse_trait_method_def` (a `{` after the
  return type opens a default-method body; otherwise the method is required and ends at the
  newline). After parsing, the whole-program `inject_trait_defaults` pass copies each trait's
  default methods into the `impl Trait for Type` blocks that omit them — a parse-time desugar
  (like type-alias expansion) run *before* `expand_type_aliases` so the injected bodies are
  alias-expanded. A method the implementor wrote explicitly is never replaced. Downstream passes
  therefore see trait methods as ordinary inherent methods.
- 2026-07-13: Explicit lifetime annotations. `parse_generic_params` now returns
  `(Vec<GenericParam>, Vec<Identifier>)` — a leading `Lifetime` token in a `<...>` list is
  collected into a separate `lifetimes` vector (kept apart from type/const generics because a
  lifetime does not monomorphize), with a duplicate-lifetime check. `FunctionDef`, `StructDef`,
  and `ImplDef` gained a `lifetimes` field, populated at their three call sites. `parse_type`'s
  reference branch parses an optional lifetime after `&` (before `mut`), setting
  `Type::Reference.lifetime`: `&'a T` / `&'a mut T`. Purely a parse surface — lifetimes are
  validated then erased in semantic analysis.
- 2026-07-06: Generic structs & impls. `parse_struct_def` parses an optional generic
  parameter list after the struct name (`StructDef.generics`); `parse_impl_def` parses impl-level
  generics after `impl` and optional type arguments on the type name (`ImplDef.generics` /
  `type_args`, via the new `parse_optional_type_args`). `parse_type` now builds a
  `Type::Generic { name, args, span }` when a `<` follows a type name, so `Pair<i32, f64>`
  annotations parse. `>` closes the list (nested `Foo<Bar<T>>` lexes as two `>` tokens, no `>>`).
- 2026-07-03: Generic functions. `parse_function` now parses an optional generic parameter
  list `<T, U: Bound + Other>` (new `parse_generic_params`) between the function name and its
  parameter list, filling `FunctionDef.generics`. Bounds after `:` are `+`-separated trait names,
  recorded but not enforced. A duplicate type-parameter name is a `DuplicateParameter` error. No
  new call-site surface (turbofish is a follow-on); type arguments are inferred downstream.
- 2026-07-02: Newtype declarations. `parse_program` dispatches `TokenKind::Newtype` to
  `parse_newtype_def` (`newtype Name = InnerType`), pushing an `Item::Newtype`. Unlike a `type` alias,
  a newtype is a distinct nominal type, so it is NOT expanded away — it stays an item for semantic
  analysis. Construction `Name(value)` reuses the existing call parse and `.0` reuses tuple-index parse
  (no new expression grammar). Type-alias `rewrite_item` recurses into the newtype's inner type so an
  aliased inner (`newtype Y = SomeAlias`) still expands.
- 2026-07-02: Pattern matching (`parser/patterns.rs`). `parse_prefix` dispatches `TokenKind::Match`
  to `parse_match_expr`, which parses the scrutinee with struct-literals suppressed, then arms. Each arm
  is `pattern ('|' pattern)* ('if' guard)? '=>' body` (`parse_match_arm`); `parse_pattern` reads
  wildcard/binding/literal/range/`E::V` variant patterns (with `(tuple)` / `{ named }` payloads). A
  leading `-` on a numeric literal and `..`/`..=` ranges are handled in `parse_pattern_literal`.
- 2026-06-30: Enums with associated data. `parse_program` dispatches `TokenKind::Enum` to
  `parse_enum_def`; `parse_enum_variant` reads unit / `(tuple)` / `{ named }` payloads. A path
  followed by `{` in the prefix parser (when struct literals are allowed) parses as
  `parse_enum_struct_literal` → `Expr::EnumStructLiteral`. New `consume_identifier` helper. Type-alias
  `rewrite_item`/`rewrite_expr` recurse into enum payload types and enum-literal field values.
- 2026-06-29: Struct + array destructuring. `parse_stmt_into` now detects `val`/`mut` followed by a
  tuple `(`, array `[`, or struct `Name {` pattern and routes to `parse_destructure_bind`, which parses
  any top-level pattern (`parse_top_pattern`), binds the RHS to a `__destructure_N` temp, and expands.
  `DestructurePattern` gained `Struct { fields }` (shorthand field binds → `FieldAccess`) and
  `Array(Vec<ArrayPatternElem>)` (positional `Index` binds + an optional trailing `Rest`). A rest
  expands to `Expr::ArrayRest { start, exact: false }`; a rest-less array adds a discarded
  `ArrayRest { exact: true }` arity assertion. Element/struct patterns nest through
  `parse_pattern_element`. Alias rewrite covers the new `ArrayRest` node.
- 2026-06-28: Tuples. `parse_type` parses the tuple type `(T1, T2, ...)` (≥2 elements; a single
  `(T)` is grouping, `()` unit is rejected). `parse_prefix`'s `(` branch produces an
  `Expr::TupleLiteral` when a comma follows the first expression, else `Expr::Paren`. The `.` infix
  reads a following integer token as a constant `Expr::TupleIndex` (`t.0`), keeping identifier dots as
  field access. Destructuring `val (a, b) = e` is a **parse-time desugar** (no AST node): block
  collectors call the new `parse_stmt_into`, which detects `val (` / `mut (` and expands the pattern
  via `parse_tuple_destructure` to a fresh `__destructure_N` temp binding plus one projection per leaf
  — supporting `_` wildcards and nested patterns through a parse-local `DestructurePattern`. Alias
  rewrite covers the new type/expr nodes.
- 2026-06-19: Arrays. `parse_type` parses `[T; N]`; `parse_prefix` parses `[..]` array literals;
  `parse_infix` + `get_precedence` parse `a[i]` indexing (call precedence); `parse_for_stmt` branches
  `Stmt::ForRange` vs `Stmt::ForEach` on the presence of a `..` / `..=`; the identifier-statement path
  builds `Stmt::IndexAssignment` for `arr[i] = v`. Alias-rewrite covers the new nodes.
- 2026-06-18: Range expressions for `string.slice`. `parse_infix` handles `..` / `..=`
  (`TokenKind::DotDot` / `DotDotEqual`) → `Expr::Range`, at the new `Precedence::Range` (below `??`).
  `parse_for` now parses the range start bound at `Precedence::Range` so the loop's own `..` / `..=`
  separator is not swallowed — `for`-range behaviour is unchanged.
- 2026-06-15: `char` literals. `parse_prefix` maps `TokenKind::Char(c)` → `Expr::Literal(Literal::Char(c))`.
- 2026-06-15: `loop` as a value expression. `parse_prefix` dispatches `TokenKind::Loop` to
  `parse_loop_expr` (and `label: loop` to `parse_labeled_loop_expr`), producing `Expr::Loop`. `break`
  parsing moved to `parse_break_stmt`: a trailing identifier is read as a label only when it names an
  in-scope loop (`Parser::active_labels`, pushed by `parse_labeled_block` / `parse_labeled_loop_expr`),
  otherwise it begins the value expression; an optional same-line value follows. `continue` keeps the
  greedy `parse_optional_loop_label`.
- 2026-06-15: Loop labels. `parse_stmt`'s identifier branch calls `try_parse_labeled_loop`,
  which dispatches `ident : <for|while|loop>` to the matching loop parser with `Some(label)` (labels
  reuse the existing `Identifier` + `Colon` tokens — no lexer change). `parse_while_stmt` /
  `parse_loop_stmt` / `parse_for_stmt` take an `Option<Identifier>` label. `break` / `continue` read
  an optional trailing same-line label via `parse_optional_loop_label` (no newline skip, so a
  line-final `break` is never a labeled break).
- 2026-06-09: `loop { ... }` infinite-loop statement. `parse_stmt` dispatches `TokenKind::Loop`
  to `parse_loop_stmt`, which parses a block body into `Stmt::Loop { body, span }` (no condition).
  `stmt_span` and the type-alias `rewrite_stmt` cover the new node.
- 2026-06-09: Mutable borrows `&mut T` + deref `*`. `parse_type` and the prefix-`&` borrow
  accept an optional `mut` after `&`, setting `mutable` on `Type::Reference` / `Expr::Reference`.
  Prefix `TokenKind::Star` now parses a dereference `Expr::Deref { operand, span }` (operand at
  `Precedence::Unary`); infix `*` stays multiply. `parse_stmt` handles a leading `*` as either a
  deref expression statement or, when followed by `=`, a `Stmt::DerefAssignment { pointer, value }`.
  Continuation fix: `parse_expr_inner` treats a newline followed by `*` as a statement boundary (via
  `peek_next_nonnewline_kind`), so `*r = v` after an expression-ending line is not glued as a
  multiplication. `stmt_span` and the type-alias rewrite cover the new nodes.
- 2026-06-08: Immutable borrows — `parse_type` parses a leading `&` recursively into
  `Type::Reference { inner, span }`; `parse_prefix` handles `TokenKind::Amp` in prefix position as a
  borrow `Expr::Reference { operand, span }` (operand at `Precedence::Unary`). Infix `&` is still
  `BinaryOp::BitAnd`, so prefix vs. infix `&` are disambiguated purely by parser position. Param /
  field span computation switched to `Type::span()` to cover the new variant.
- 2026-06-07: `@derive(...)` attaches to struct definitions. `parse_program` passes the
  collected `Vec<Attribute>` into `parse_struct_def(attributes)` → `StructDef.attributes`. The
  "attribute before non-function item" rejection now fires only when an attribute precedes neither
  `func` nor `struct`. Semantics (Copy/Clone) live in semantic-analysis; parser accepts any name.
- 2026-06-05: Struct shorthand + functional-update in `parse_struct_literal`. A field with no
  `: value` desugars to `field: field` (`FieldInit { value: Expr::Identifier(name) }`); a trailing
  `..expr` sets `StructLiteral.base` and ends the field list. `rewrite_expr` recurses into `base`.
  Parse-time desugaring; semantic/codegen see only `base`.
- 2026-06-04: `unsafe { }` block expressions (1C). New `parse_unsafe_expr` prefix handler
  (`TokenKind::Unsafe`) → `Expr::Unsafe { stmts, span }`; body parses as a statement block; inert.
  `rewrite_expr` recurses into the block. Reaches the prefix parser via expression-statement
  fallthrough (no statement-parser change).
- 2026-06-03: Type-alias declarations — `TokenKind::Type` dispatch; `parse_type_alias` +
  `expand_type_aliases` (`parser/type_aliases.rs`). New `ParseError::{DuplicateTypeAlias,
  TypeAliasShadowsBuiltin, CyclicTypeAlias}`. Parse-time desugaring, no AST node.
- 2026-05-20: Attribute parsing — `parse_attributes` collects `@name`/`@name(arg,...)` ahead of every
  `func` (free + impl methods) → `Vec<Attribute>` on `FunctionDef`/`MethodDef`. Attributes before
  non-function items rejected (`UnexpectedToken`). Semantics live in semantic-analysis; any name
  accepted so future `@grad`/`@gpu`/`@no_prelude` need no grammar churn.
- 2026-05-18: `??` (null/error coalescing) — `Precedence::NullCoalesce` between `Lowest` and
  `LogicalOr` (Appendix B row 14); R-to-L associativity via recursing on the right operand at
  `Precedence::Lowest`. The parse surface is unchanged since; semantic analysis and HIR
  lowering gave it meaning on 2026-07-28 (it desugars to a `match`).
- 2026-05-25: Float literal suffixes — `parse_prefix` handles `TokenKind::FloatSuffix` →
  `Literal::Float(val, Some(suffix))`; plain `Float(f)` → `None`.
- 2026-04-18: Integer literal suffixes — `parse_prefix` handles `TokenKind::IntegerSuffix` →
  `Literal::Integer(val, Some(suffix))`; plain `Integer(n)` → `None`.
- 2026-04-18: Bitwise operators — new `Precedence` variants `Shift`/`BitwiseAnd`/`BitwiseXor`/
  `BitwiseOr` (Appendix B levels 7–10); `Amp` → `BinaryOp::BitAnd`; `Tilde` → unary `BitNot` at
  `Precedence::Unary`.
- 2026-04-16: `parse_const_def` (module-level) + `parse_const_stmt` (body); `parse_program`/`parse_stmt`
  dispatch on `TokenKind::Const`.
- 2026-04-04: Parse `..=` for inclusive `for` ranges.
- 2026-07-10: Const generics, `where` clauses & turbofish. `parse_generic_params` accepts
  `const N: T` (sets `GenericParamKind::Const`); `parse_where_clause` folds trait bounds into the
  matching param and collects value predicates (`FunctionDef`/`StructDef`/`ImplDef.where_predicates`);
  array-type sizes accept a const-param identifier (`ArraySize::Const`); a turbofish `f::<T, N>(x)`
  parses in `parse_infix` (`ColonColon` at `Precedence::Call`, prefix `::` skips a following `<`) into
  `Expr::Call.type_args`; `Type::Generic.args` is now `Vec<GenericArg>` (types or const values, e.g.
  `Ring<i32, 4>`). New public re-exports: `ArraySize`, `GenericArg`, `GenericParamKind`.
