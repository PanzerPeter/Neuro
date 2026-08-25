# ast-types

## Purpose
Provide the canonical Abstract Syntax Tree (AST) node definitions shared by all compiler stages that produce or consume the AST — without coupling them to each other.

## Entry Point
- Type: Library (no entry function — pure data)
- Public types: `Item`, `Expr`, `Stmt`, `BinaryOp`, `UnaryOp`, `TypeAnnotation`, `FunctionParam`,
  `ImplDef`, `MethodDef`, `SelfParam`, `Attribute`

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- shared-types — `Span`, `Identifier`, `Literal` embedded in every AST node

## Notes
Extracted from `syntax-parsing` to eliminate the cross-slice dependency that `semantic-analysis` and `llvm-backend` previously had on `syntax-parsing`. All three consumer slices now depend only on this infrastructure crate, not on each other. `syntax-parsing/src/ast/mod.rs` re-exports all types from here for backwards compatibility.

`Item::Impl` carries an `ImplDef` (optional `trait_name` + type name + list of `MethodDef`).
`trait_name` is `Some` for a trait implementation (`impl Drop for T`) and `None` for an
inherent block (`impl T`). Each `MethodDef` holds an
`Option<SelfParam>` distinguishing associated functions (`None`) from instance methods (`Some`).
`SelfParam::Ref` (`&self`) is the only variant currently supported end-to-end; `RefMut` and
`Owned` are parsed but rejected by semantic analysis until ownership semantics land.

`Expr::Path { type_name, member, span }` represents `TypeName::member` path expressions used as
the callee of associated-function calls (`Point::new(args)`).

## Recent Updates
- 2026-08-25: Added `Expr::InterpString { parts, span }` and `InterpPart::{Text, Formatted}`
  for interpolated string literals. A `Formatted` hole carries an already-parsed `Expr`
  and an optional `FormatSpec` from shared-types, so consumers see ordinary typed
  expressions rather than raw text.
- 2026-08-23: Implicit prelude. Added `Item::NoPrelude(Span)` for the file-scope `@no_prelude`
  marker. Like `Item::Import` and `Item::Module` it is parse-only: module resolution reads it off
  the file it opens and drops it, so no pass after that sees the variant. It carries only its
  span — everything the marker means is a question for the module graph, not for the tree.
- 2026-08-23: Inline modules and re-exports. Added `ModuleDef { name, items, span }` and
  `Item::Module(ModuleDef)` for an inline `module Name { ... }` block — a module with no file
  of its own, which module-resolution lifts into a graph module of its own and erases, so no
  pass after it sees the variant. Added `exported: bool` to `ImportDef` for the `export import`
  re-export form; which of the bound names may actually be re-exported is a file-system
  question and is settled during module resolution.
- 2026-08-22: Visibility. Added `exported: bool` to `FunctionDef`, `StructDef`, `EnumDef`,
  `TraitDef`, `ConstDef`, `NewtypeDef`, and `FieldDef` — `false` is the private default, and
  the parser sets it from a leading `export`. An enum struct-variant's `FieldDef` is always
  `exported: true`: a variant is reached through a pattern naming its enum, so its fields
  carry no visibility of their own. Added the `ModuleId` alias (`u32`) and a `module:
  ModuleId` field on `FunctionDef`, `StructDef`, `ImplDef`, and `ConstDef` — the file a
  declaration was loaded from, stamped by module-resolution and 0 for everything the parser
  produces alone. The merge is flat, so this stamp is the only surviving trace of which file
  a declaration came from; field visibility needs the receiver's type and is therefore
  checked by semantic-analysis, which has nothing else to read it from. `ImplDef` carries
  `module` but no `exported`: an `impl` declares no name of its own.
- 2026-08-18: Imports. Added `Item::Import(ImportDef)` — `relative` (the explicit `./` form),
  the `::`-separated `path`, and an `ImportSelection` of `Module` / `Alias(Identifier)` /
  `List(Vec<ImportName>)`. Whether a path segment names a module file, an item inside one, or
  an enum is a file-system question, so the node records only what was written and
  module-resolution settles it; nothing downstream ever sees an `Item::Import`. Also added
  `Pattern::UnqualifiedEnum { variant, payload, span }` — `Some(n)` written without its enum,
  which module-resolution rewrites into `Pattern::Enum` against the importing file's table.
  A payload-*less* variant (`None`) is indistinguishable from a binding at parse time and
  arrives as `Pattern::Binding`, resolved by the same table.
- 2026-08-03: Error propagation. Added `Expr::Try { operand, span }` — the postfix `expr?`.
  A node of its own rather than a `BinaryOp`: it has one operand, and its type comes from that
  operand's success payload while its *failure* path is typed by the enclosing function's return
  type. Interpreted by semantic-analysis (which enforces both) and desugared to a `match` by
  hir-lowering, so the HIR has no counterpart node.
- 2026-07-31: `val-else`. Added `Stmt::ValElse { pattern, value, else_binding, else_block, span }` —
  `val PATTERN = value else |binding| { ... }`. Unlike the tuple/struct/array destructuring binds
  (parse-time desugars that never reach the AST), this one survives: its pattern is refutable, so the
  test and the failure branch have to be represented. `pattern` reuses the existing `Pattern` node set
  from `match`; `else_binding` is the optional `|name|` (an `Identifier` named `_` is the written
  wildcard, distinct from `None`). Also added `Pattern::binding_names()`, a pure structural query used
  by both closure free-variable walkers. Interpreted by semantic-analysis (type-directed else binding
  + divergence rule) and hir-lowering.
- 2026-07-28: One `loop` node. `Stmt::Loop` is removed; `Expr::Loop` is the sole loop node and now
  carries the `label`, so a statement-position `loop` is `Stmt::Expr(Expr::Loop { .. })`. Two shapes
  for one construct meant every "is the tail statement value-producing?" test in the pipeline — all
  of which key on `Stmt::Expr` — silently missed a trailing `loop`, which is how a tail `loop` used
  as a function's implicit return came to be compiled as a discarded value (BUG-005).
- 2026-07-26: Generic enums. `EnumDef` gains `generics: Vec<GenericParam>` (empty for a plain
  enum), so `enum Option<T> { Some(T), None }` and `enum Result<T, E> { Ok(T), Err(E) }` are
  ordinary declarations. A generic enum is a template: semantic-analysis and hir-lowering
  monomorphize it into one distinct nominal enum per set of type arguments. A generic-enum *type*
  annotation reuses `Type::Generic { name, args }` (the same node a generic struct application
  uses); construction and patterns reuse the existing `Expr::Path` / `Expr::Call(Path)` /
  `Expr::EnumStructLiteral` / `Pattern::Enum` nodes with the base name. Enums carry no
  `lifetimes` field — the parser rejects a lifetime parameter on an enum.
- 2026-07-24: Closures and lambdas. Added `Expr::Closure { params, ret, body, is_move, span }`
  (a closure literal `|p| body` / `|p| -> R { body }` / `move |p| ...`) and the `ClosureParam
  { name, ty, span }` struct, plus `Type::Function { params, ret, span }` for the closure/function
  type `(T1, ...) -> R`. `ret` is `None` when inferred; `body` is a single expression (a bare
  block for the multi-statement form). Interpreted by semantic-analysis (capture analysis, Copy-only
  capture) and hir-lowering (each literal lifted to a `HirItem::Closure`).
- 2026-07-19: Static & dynamic dispatch. Added `Type::ImplTrait { trait_name, span }` and `Type::DynTrait { trait_name, span }`. `ImplTrait` survives parsing only in RETURN position — in argument position the parser rewrites it into a fresh trait-bounded `GenericParam`, so downstream slices see an ordinary generic. `DynTrait` always survives; semantic-analysis resolves it to a trait-object type and rejects it outside a reference.
- 2026-07-18: Operator traits — scalar path. `ImplDef` gains
  `assoc_types: Vec<(Identifier, Type)>` for associated-type bindings (`type Output = T`) inside a
  block. `BinaryOp` and `UnaryOp` now derive `Hash` so the checker/lowering can key operator-dispatch
  maps on `(struct, operator)`. Interpreted by semantic-analysis (operator lang-item recognition) and
  hir-lowering (operator desugaring); no new AST variants — an overloaded operator stays an ordinary
  `Expr::Binary` / `Expr::Unary` and is desugared to a method call downstream.
- 2026-07-16: Trait declarations. Added `Item::Trait(TraitDef)`; `TraitDef { name, methods,
  span }` and `TraitMethod { name, self_param, params, return_type, default_body, span }`. A
  `default_body` of `None` is a required method, `Some(body)` a default (provided) method. Traits
  are fully erased: the parser injects each omitted default into the matching `impl Trait for Type`
  block, so `ImplDef` is unchanged and downstream passes treat trait methods as ordinary methods.
  Interpreted by semantic-analysis (conformance + generic trait-bound enforcement); hir-lowering and
  the backends skip the trait item entirely.
- 2026-07-13: Explicit lifetime annotations. Added a `lifetimes: Vec<Identifier>` field to
  `FunctionDef`, `StructDef`, and `ImplDef` — the `'a` names from a `<...>` list, kept separate
  from `generics` because lifetimes are a distinct namespace and do not drive monomorphization
  (a lifetime-only function is an ordinary concrete function). Added `lifetime: Option<Identifier>`
  to `Type::Reference` — the `'a` in `&'a T`, `None` when elided. Both are validated then erased by
  semantic analysis; a reference type's identity does not depend on its lifetime.
- 2026-07-06: Generic structs & impls. Added `StructDef.generics: Vec<GenericParam>`
  (empty for a non-generic struct), `ImplDef.generics: Vec<GenericParam>` + `ImplDef.type_args:
  Vec<Type>` (the `<T>` of `impl<T> Wrapper<T>`), and a new `Type::Generic { name, args, span }`
  variant for a generic type application `Name<T1, ...>`. A bare type-parameter reference stays a
  plain `Type::Named`. Interpreted by semantic-analysis and hir-lowering (monomorphization).
- 2026-07-03: Generic functions. Added `GenericParam { name, bounds, span }` and
  `FunctionDef.generics: Vec<GenericParam>` (empty for a non-generic function). A generic
  function is a template; a type-parameter reference in an annotation is a plain `Type::Named`
  (resolved against the generics in scope by later passes). `bounds` records trait names for
  forward compatibility but is not enforced (no trait system yet). Interpreted by
  semantic-analysis (inference/monomorphization checking) and hir-lowering (monomorphization).
- 2026-07-02: Newtype declarations. Added `NewtypeDef { name, inner, span }` and
  `Item::Newtype(NewtypeDef)`. Unlike a `type` alias (expanded at parse time), a newtype survives as
  its own item; a newtype *type* annotation is a plain `Type::Named`, construction reuses
  `Expr::Call(Identifier)`, and inner access reuses `Expr::TupleIndex` (`.0`). Interpreted by
  semantic-analysis, hir-lowering, and both backends.
- 2026-07-02: Pattern matching. Added `Expr::Match { scrutinee, arms, span }` (with its `span()`
  arm) and the pattern types `MatchArm { patterns, guard, body, span }`, `Pattern::{Wildcard, Binding,
  Literal, Range, Enum}` (with `Pattern::span()`), `EnumPatternPayload::{Unit, Tuple, Struct}`, and
  `FieldPattern { field, pattern, span }`. Payload sub-patterns are restricted to bindings/`_` in this
  phase; or-patterns cannot bind (enforced in semantic analysis).
- 2026-06-30: Enums with associated data. Added `Item::Enum(EnumDef)`; `EnumDef { name, variants,
  span }`, `EnumVariant { name, payload, span }`, and `VariantPayload::{Unit, Tuple(Vec<Type>),
  Struct(Vec<FieldDef>)}`. Added `Expr::EnumStructLiteral { enum_name, variant, fields, span }` for the
  brace construction form (`E::V { f: x }`); unit/tuple variants reuse `Expr::Path` / `Expr::Call(Path)`
  and are disambiguated against the enum table in later passes. An enum *type* annotation is a plain
  `Type::Named`. Interpreted by semantic-analysis, hir-lowering, and llvm-backend.
- 2026-06-29: Struct + array destructuring. Added `Expr::ArrayRest { array, start, exact, span }`,
  a compiler-internal node the array-pattern desugar produces for the trailing `..rest` sub-slice
  (`[T; N - start]`), with its `span()` arm. `exact` records a rest-less pattern (length must match
  exactly). Struct destructuring `val Point { x, y } = p` needs no AST node (desugars to field-access
  bindings); only the array remainder does, because its size is known only after type checking.
- 2026-06-28: Tuples. Added `Type::Tuple { elements, span }` (the `(T1, T2, ...)` type),
  `Expr::TupleLiteral { elements, span }` (the `(e0, e1, ...)` literal, always ≥2 elements), and
  `Expr::TupleIndex { object, index, span }` (the `t.0` / `t.1` constant-index access, distinct from
  `FieldAccess` which names a struct field), with their `span()` arms. Destructuring `val (a, b) = e`
  needs no AST node — the parser desugars it to a temp binding plus indexed bindings.
- 2026-06-19: Arrays. Added `Type::Array { element, size, span }`, `Expr::ArrayLiteral { elements, span }`,
  `Expr::Index { object, index, span }`, `Stmt::ForEach { label, iterator, iterable, body, span }`, and
  `Stmt::IndexAssignment { target, index, value, span }`, with their `span()` arms.
- 2026-06-18: String `.slice(range)`. Added `Expr::Range { start, end, inclusive, span }`,
  the `a..b` / `a..=b` node. Not a first-class value: it is only valid as a `string.slice`
  argument (semantic-analysis rejects it elsewhere). `for`-range loops keep their bounds on
  `Stmt::ForRange` and never produce this node.
- 2026-06-15: `loop` as a value expression. Added `Expr::Loop { label, body, span }` — the
  value-producing form, distinct from `Stmt::Loop` (statement form, value discarded). `Stmt::Break`
  gained `value: Option<Expr>` for `break v`. The targeted `loop` evaluates to its value-`break`s
  (which must agree on type); `while`/`for` stay unit and have no expression form.
- 2026-06-15: Loop labels. `Stmt::While` / `ForRange` / `Loop` each gained `label:
  Option<Identifier>` (the `outer:` prefix); `Stmt::Break` / `Continue` each gained `label:
  Option<Identifier>` (`break outer`). `None` is the unlabeled form. Resolved by semantic-analysis
  (a label stack) and llvm-backend (labeled `LoopTargets`).
- 2026-06-09: Added `Stmt::Loop { body, span }` for the infinite `loop { ... }` statement.
  Distinct from `While`: no condition, the only exit is `break`, `continue` re-enters from the top.
  The value-producing `break value` form is not modelled yet — a `loop` statement yields unit.
  Interpreted by semantic-analysis (`loop_depth` so `break`/`continue` are in-loop) and llvm-backend
  (unconditional back-edge).
- 2026-06-09: Mutable borrows `&mut T`. `Type::Reference` and `Expr::Reference` gained a
  `mutable: bool` field (`&mut T` / `&mut place`). New `Expr::Deref { operand, span }` (the prefix
  `*` dereference) and `Stmt::DerefAssignment { pointer, value, span }` (`*r = value`). Interpreted
  by semantic-analysis (`&mut` needs a `mut` binding; `*` reads/writes; `&mut T` ≠ `&T`) and
  llvm-backend (borrow → storage pointer; deref → load/store).
- 2026-06-08: Added `Type::Reference { inner, span }` (`&T`) and `Expr::Reference { operand, span }`
  (`&place`) for immutable borrows. The reference type appears in any type-annotation
  position; the borrow expression is a prefix `&` on a place expression. Interpreted by
  semantic-analysis (no move, `Copy`, auto-deref) and llvm-backend (lowered to an opaque pointer).
- 2026-06-07: `StructDef` gained `attributes: Vec<Attribute>` so `@derive(Copy, Clone)` can
  attach to struct definitions. Mirrors the existing `attributes` field on `FunctionDef` /
  `MethodDef`; interpreted by semantic-analysis. Empty when no attributes are present.
- 2026-06-05: `Expr::StructLiteral` gained `base: Option<Box<Expr>>` for functional-update syntax
  (`Point { x: 1.0, ..p }`). `None` is a plain literal (all fields listed). Field-init
  shorthand (`Point { x, y }`) needs no AST change — the parser desugars a bare field to
  `FieldInit { value: Expr::Identifier(field_name) }`.
- 2026-06-04: Added `Expr::Unsafe { stmts, span }` for `unsafe { }` block expressions (1C groundwork). Structurally identical to `Expr::Block`; the distinct node lets later phases (Phase 4 `@kernel`) attach the kernel-aliasing relaxation. Inert today — no special semantics.
- 2026-05-20: Added `Attribute { name, args, span }` struct. `FunctionDef` and `MethodDef` now carry `attributes: Vec<Attribute>`. Semantics are interpreted by later passes (e.g. `@allow(prefer_loop_over_while_true)`); unknown attribute names are accepted so the surface stays forward-compatible with future `@grad`, `@gpu`, `@no_prelude`.
- 2026-04-04: Added `inclusive: bool` to `Stmt::ForRange` to support `..=` inclusive range iteration.
- 2026-04-16: Added `ConstDef` struct and `Item::Const(ConstDef)` for module-level constants.
  Added `Stmt::Const { name, ty, value, span }` for function-body constants.
- 2026-05-18: Added `BinaryOp::NullCoalesce` (`??`) variant. Carries no semantics here — semantic-analysis rejects it until 1G lands Option/Result. Defined now so the AST shape is final for the parser's R-to-L associativity test.
- 2026-04-28: Added `Expr::If { condition, then_block, else_if_blocks, else_block, span }` and
  `Expr::Block { stmts, span }` for value-producing if-expressions and block expressions.
  `expressions.rs` now `use super::statements::Stmt` for the block payload types.
- 2026-07-10: Const generics / `where` / turbofish. New public types `ArraySize`
  (`Literal`/`Const`), `GenericArg` (`Type`/`Const`), `GenericParamKind` (`Type`/`Const`). `GenericParam`
  gains `kind`; `Type::Array.size` is `ArraySize`; `Type::Generic.args` is `Vec<GenericArg>`;
  `Expr::Call` gains `type_args`; `FunctionDef`/`StructDef`/`ImplDef` gain `where_predicates`.
