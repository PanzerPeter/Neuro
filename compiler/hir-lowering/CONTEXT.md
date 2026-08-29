# hir-lowering

## Purpose
Lower a type-checked surface AST into the typed High-Level IR (`neuro-hir`), re-deriving every expression's resolved type so each backend consumes HIR instead of the AST.

## Entry Point
- Type: Library function
- Input: `items: &[ast_types::Item]` (a program that already passed `semantic_analysis::type_check`)
- Output: `Result<neuro_hir::HirProgram, LoweringError>`

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- ast-types — read-only traversal of the surface `Item` / `Stmt` / `Expr` / `Type` nodes
- neuro-hir — the typed HIR node set produced as output
- shared-types — `Span`, `Literal`, `IntSuffix`, `FloatSuffix`, `Identifier` reused in nodes
- thiserror — `LoweringError` derivation

## Notes
- 2026-08-29: New public error `LoweringError::UnsupportedOperand` (BUG-015). `binary_result_type`
  (`expressions/coercion.rs`) now refuses an operand type the backend has no instruction sequence
  for, via the new `has_operator_lowering`: the scalars, `string`, a `&string` slice, and a newtype
  forwarding one of those. The checker rejects a concrete aggregate operand itself; the path that
  reaches here is a generic body, which types `a == b` as its type parameter and is checked once as
  a template, so an instantiation with an aggregate argument used to arrive at codegen and abort it.
  An operator-trait impl is dispatched to a method call in the `Expr::Binary` arm above, before
  operand types are ever combined, so a struct with `impl PartialEq` never reaches the check.
- 2026-08-29: Growable `String`. `collection_kind` recognizes `"String"`; the new
  `nullary_collection` helper lets `resolve_type` accept the bare name as a complete type, checked
  *after* the struct/enum/newtype arms so a user declaration shadows it. `lower_collection_new`
  builds a nullary kind's type itself instead of requiring an annotated target, and
  `lower_collection_method` gains `push_str` (`string` parameter) and `to_string` (`string` result).
  `mangle_type` now uses `HirCollectionKind::mangle_tag()` — `String` mangles as `strbuf`, which the
  primitive `string` cannot collide with — and yields the bare tag when there are no arguments.
- 2026-08-29: Named arguments. Lowering refuses an `Expr::Call` whose `arg_labels` is
  non-empty. Every label is bound to a parameter by `argument-binding` before type checking;
  one surviving to here would mean that pass never reached the call, and the arguments would
  then lower in the order they were written rather than the callee's — a wrong program rather
  than a failed build. The check costs one `is_empty` per call and never fires on a
  well-formed one.
- 2026-08-03: `?` error propagation. New `expressions/try_op.rs`: `lower_try` desugars `operand?`
  into `HirExprKind::Match` — arm 0 tests the `Some`/`Ok` tag and yields payload slot 0 as
  `__try_N` (named off the new `try_counter`), arm 1 is a `Wildcard` whose body is a
  `Block` holding a single `HirStmt::Return` of the failure variant. No HIR node and no backend
  change: the arm terminates, and `codegen_arm_body` already skips the result-slot store for a
  terminated block. The failure value is rebuilt against `current_return`'s instance, NOT the
  operand's — a `Result<u8, E>` propagating out of a `-> Result<i32, E>` function must produce the
  latter — and the `Err` payload is bound out of the operand's slot 0 and passed straight through,
  which is the "no implicit conversion" rule made structural. `success_variant`, `not_fallible`, and
  the new `fallible_base` are shared with the `??` desugar in `coalesce.rs`.
- 2026-07-31: A trailing `Stmt::If` is a value at every depth. `Expr::If` lowering moved into
  `lower_if_expr` (`expressions/mod.rs`), and both tail rules — `lower_body_stmts` (`items.rs`, the
  implicit return) and `lower_block_value_inner` (`expressions/mod.rs`, every nested block) — now
  route a trailing `Stmt::If` carrying an `else` through it, emitting `HirStmt::Expr`. Only the
  function body used to recognise it, so a nested tail `if` lowered via `lower_stmt_block` and its
  branches' values were discarded; a generic-enum construction inside one also lost its payload,
  since a branch that is not a value position never reaches the `current_return` instance fallback.
  The rule now lives only here — the LLVM backend's `codegen_body` no longer carries its own copy.
- 2026-07-31: `val-else`. New `val_else.rs`: `lower_val_else` reuses `pattern_test` /
  `pattern_bindings` from `expressions/matches.rs` (both now `pub(crate)`) for the success test and
  bindings, lowers the `else` branch in a pushed scope with its own binding, then defines the
  pattern's bindings in the ENCLOSING scope so later statements type against them.
  `resolve_else_binding` mirrors the checker's binding table off `enum_instance_base`: a `Result`'s
  `Err` payload (slot 0), `None` for `Option`, else the whole scrutinee. `enum_variant` is now
  `pub(crate)`. The table is duplicated from the checker's rather than shared — the two slices stay
  independent, and a divergence surfaces as a `LoweringError`.
- 2026-07-28: `??` full implementation. New `expressions/coalesce.rs`: the `Expr::Binary` dispatch
  hands `BinaryOp::NullCoalesce` to `lower_null_coalesce`, which desugars `lhs ?? fallback` into
  `HirExprKind::Match` — arm 0 tests the `Some`/`Ok` tag and binds payload slot 0 as
  `__coalesce_N` (named off the new `coalesce_counter`, so nested coalesces cannot shadow each
  other), arm 1 is a `Wildcard` whose body is the fallback. No HIR node and no backend change: the
  fallback's laziness comes free from the per-arm basic-block chain codegen already emits. The
  fallback lowers with the payload type as its expected type, which is what types a bare literal.
  `binary_result_type`'s `NullCoalesce` arm is now unreachable-by-construction and says so.
- 2026-07-28: Divergent `loop`, and expression module split. `LoopCtx` gains `has_break`, set by
  `record_break_target`; `Expr::Loop` lowers to its value-break type, or to the *expected* type when
  no `break` targets it, so the (dead) exit-block result slot is still typed for the position the
  loop sits in. `lower_stmt` loses its `Stmt::Loop` arm — the node is gone, so a statement-position
  loop arrives as `Stmt::Expr(Expr::Loop)` and `lower_body_stmts`' existing tail rule types it
  against the declared return type. `expressions.rs` is now `expressions/` (`mod.rs` holds the
  dispatch and the block-value tail rule; `calls`, `enums`, `structs`, `matches`, `sequences`,
  `coercion` hold the rest) and `tests.rs` is split by subject. Behaviour is unchanged by the split.
- 2026-07-28: `checked_{add,sub,mul}`. `lower_builtin_method` (`expressions.rs`) types these on any
  integer receiver as `Option<T>` over that receiver, reusing `collections.rs`'s `option_of` so the
  instance is materialized as an ordinary `HirItem::Enum` exactly like `Vec::pop`'s. They are the
  only builtin intrinsics whose result is a monomorphized enum rather than a fixed type.
- 2026-07-27: Standard collections. New `collections.rs`: `lower_collection_new` lowers
  `Vec::new()` / `HashMap::new()` / `BTreeMap::new()` to `HirExprKind::CollectionNew` typed from the
  annotated target, and `lower_collection_method` re-derives each method's argument and result types
  — instantiating `Option<T>` through the ordinary generic-enum path, so backends see a real
  `HirItem::Enum`. `resolve_type` maps the `Vec` / `HashMap` / `BTreeMap` generic application to
  `HirType::Collection` instead of monomorphizing a nominal instance (a user-declared type of that
  name still shadows it), and indexing / index assignment / `for`-in resolve a `Vec` element
  alongside an array's. The method table is duplicated from the checker's rather than shared: the two
  slices stay independent, and a divergence surfaces as a `LoweringError`.
- 2026-07-26: Generic enums (`Option<T>` / `Result<T, E>`). New `Lowerer` state: `generic_enums`
  (base -> template), `enum_instance_base` / `enum_instance_args` (instance -> base + arguments),
  and the `mono_enum_pending` worklist, drained in `lower_program` ahead of the struct worklist. A
  generic `Item::Enum` is registered as a template and never lowered directly;
  `instantiate_generic_enum` resolves its variants under the argument substitution and emits one
  ordinary `HirItem::Enum` per instance, named by `mangle_struct_instance` (`Opt_g_i32`), so the
  backends stay generic-unaware. `resolve_type` maps a `Type::Generic` application naming a generic
  enum to `HirType::Enum(<instance>)`. Construction mirrors the checker: the instance comes from the
  expected type, else the payload is unified against the template (`unify_ast_hir`) and any
  remaining parameter is taken from the enclosing return instance. To make that fallback available,
  `lower_body` now tracks the body's declared return type in `current_return` (previously left at
  `Void`), which also gives a `return` operand its contextual type. An enum pattern written with a
  generic base resolves against the scrutinee's instance (`pattern_enum_name`) for both its tag test
  and its payload bindings.
- 2026-07-24: Closures and lambdas. New `closures.rs`: `lower_closure` lifts each `Expr::Closure`
  to a `HirItem::Closure` (named `__closure_N` via a `closure_counter`; the `__` prefix is a
  reserved generated-symbol marker the checker forbids in user names) collected in `closure_items`
  and appended after the monomorphization worklists in `lower_program`. Captures are the body's free
  variables that resolve to an enclosing *local* binding (a new `lookup_local` excludes module
  constants), snapshotted in first-seen order with their types; a block body lowers through
  `lower_body` (annotation supplies the return type), a single-expression body infers it. The value
  site emits `HirExprKind::Closure { name, captures }` typed as `HirType::Function`. `lower_ident_call`
  now dispatches a call on a local function-typed binding indirectly (callee is an `HirExprKind::Variable`
  of `HirType::Function`). `resolve_type` handles `Type::Function`.
- 2026-07-19: Static & dynamic dispatch. Traits are no longer fully erased here: a new
  `traits` table (name → methods in declaration order, with their visible parameter and return
  types) is registered before impls, and each `Item::Trait` now lowers to a `HirItem::Trait`
  carrying that method order — the canonical vtable slot layout backends need. `resolve_type`
  delegates to `resolve_type_ctx(ty, behind_ref)` so `&dyn Trait` resolves to
  `Reference(DynObject)` while a bare `dyn` is an internal error (the checker rejects it first).
  `lower_expr` is now a thin wrapper that lowers via `lower_expr_uncoerced` and then applies
  `apply_dyn_coercion`: this is the single site where `&T` -> `&dyn Trait` unsizing is inserted, so
  every context that supplies an expected type (call arguments, returns, annotated bindings) gets
  the coercion uniformly and an existing trait object is never re-coerced. A method call on a
  `DynObject` receiver types from the trait declaration, naming no implementor. Return-position
  `impl Trait` resolves to its concrete type through `declared_return_type` / `shallow_result_type`,
  mirroring the checker (duplicated, not shared — separate type tables per VSA).

`syntax-parsing` is a `[dev-dependencies]` entry only (tests build ASTs through the parser); it is never a production cross-slice dependency.

Lowering **re-derives** each expression's type rather than importing the checker's `Type`, which would couple two feature slices (VSA: duplicate over couple). It assumes well-typedness — a shape the checker should have rejected surfaces as a `LoweringError`, never a panic.

The lowerer runs a registration pre-pass mirroring the checker's: struct field tables (+ `@derive(Copy/Clone)` intent), `impl` method signatures under mangled `Struct__method` keys, free-function signatures, and module constants. Bodies then lower under a lexical scope stack and a loop-context stack.

Two type derivations are contextual and faithfully mirror the checker:
- **Literals** take a suffix type, else the expected type when it fits the literal's family, else the default `i32`/`f64`.
- A **function/method body's trailing expression** is an implicit return, typed against the declared return type. Nested block and `if`-arm tails inherit the context the same way: `lower_if_expr` takes the expected type if there is one, else the first arm's type, and `lower_block_value` threads it to the tail — the rule `lower_match` already applied to arm bodies.

Tuples: `resolve_type` lowers the tuple type to `HirType::Tuple`; a tuple literal is typed by lowering each element (each hinted by the expected tuple's element type when annotated) and a `t.N` index reads the N-th element type off the (auto-derefed) tuple type. Destructuring is already desugared by the parser, so only the literal/index nodes reach here.

Enums: a registration pre-pass records each enum's variants and resolved payload fields (`enums` table). `resolve_type` maps an enum name to `HirType::Enum`. All three construction forms normalize to one `HirExprKind::EnumConstruct { enum_name, variant, tag, payload }`: a unit `E::V` (Path) carries an empty payload; a tuple `E::V(..)` (Call→Path) carries the positional args; a struct `E::V { .. }` (`EnumStructLiteral`) reorders its provided fields into declared order so codegen sees a single positional layout. `tag` is the variant's declaration index.

Struct + array destructuring: the parser desugars these, so only the array-rest node reaches lowering. `Expr::ArrayRest { array, start, exact }` lowers to `HirExprKind::ArrayRest { array, start }` typed `[T; N - start]` (re-derived from the source array's `HirType`); a defensive arity re-check (`exact ⇒ N == start`, else `start <= N`) raises `Malformed` rather than underflowing `N - start`.

Pattern matching: `lower_match` fully resolves each arm. `pattern_test` maps a pattern to a `HirMatchTest` (variant tag / `IntEq` / `IntRange`, with `char`/`bool` literals as scalar codepoints/0-1 and an exclusive `a..b` normalized to `a..=b-1`); `pattern_bindings` resolves a single arm's bindings to `HirBindingSource::Scrutinee` (bare binding) or `EnumPayload { slot }` (payload field, slot = declared field position). Bindings are defined in a per-arm scope so the guard and body lower correctly; the body-type hint is the caller's expected type, else the first arm's type. The match type is the first arm's body type.

Newtypes: a registration pre-pass records each newtype's inner AST type (`newtypes` table).
`resolve_type` maps a newtype name to `HirType::Newtype { name, inner }`, resolving the inner
recursively (a newtype may wrap another; the checker already rejected cycles). Construction
`Name(value)` — a `Call` whose identifier callee names a newtype — lowers to
`HirExprKind::NewtypeConstruct { name, value }` (value hinted by the inner type), taking precedence
over a same-named function like the checker. Inner access `.0` on a newtype-typed object lowers to
`HirExprKind::NewtypeAccess { object }` typed as the inner type. No `HirItem` is emitted — a newtype
is purely a type-system distinction that the backends erase.

Three nodes carry a deliberately-chosen type the source has no first-class form for: a `loop` value-expression takes its `break v` type (or `void`); a method-name callee `FieldAccess` carries the call's result type (there is no method value); a `Range` carries `void` (valid only as a `string.slice` argument — the slice lowering reads its bounds directly). Divergent panic-family calls (`panic`/`assert`/`unreachable`) adopt their context's expected type, or `void` in statement position. The standard-output builtins (`print`/`println`, `IO_BUILTINS` in `expressions/mod.rs`) are recognized in the same `lower_ident_call` fallback but do **not** take their type from context: they return, so the call is always `HirType::Void` with one `HirType::String` parameter.

Generics: this slice performs **monomorphization** — the HIR has no generic node, so generic templates are erased into concrete instances here. A generic `FunctionDef` is stored in `generic_templates` (not `functions`) and never lowered directly. A call to a generic function (`lower_generic_call`) infers its type arguments by unifying the template's parameter annotations against the lowered arguments' types (`unify_ast_hir`), resolves the concrete signature under a `type_subst` map (consulted by `resolve_type` for a parameter name), mangles a per-instance name (`mangle_instance` → `name_g_<type…>`, single-underscore marker so the symbol never contains `__`), enqueues the instance if unseen, and emits a `Call` to the mangled name. A worklist drains after the ordinary items: each instance's body lowers under its `type_subst`, appended as a concrete `HirItem::Function`. The backend pre-declares all functions, so instance emission order is irrelevant.

Generic structs & impls: monomorphized the same way. A generic `StructDef` is stored in `generic_structs` (not `structs`) and a generic `impl` in `generic_impls` (keyed by base name); neither is lowered directly. `instantiate_generic_struct(base, args)` — called from `resolve_type` for a `Type::Generic` annotation and from `lower_generic_struct_literal` after inferring the arguments from the field values via `unify_ast_hir` — mangles a per-instance name, registers the instance's concrete fields + impl-method signatures, and enqueues a `MonoStruct` if unseen. The struct-instance mangle (`mangle_struct_instance` → `Base_g_<type…>`) deliberately avoids `__`, because the backend recovers a method's receiver struct by splitting the method symbol on `__`; `mangle_instance` uses the same marker for the same reason, and semantic analysis rejects a user name containing `__`, so `<instance>__<method>` holds exactly one separator from both sides. The struct worklist drains alongside the function worklist, emitting one `HirItem::Struct` plus one `HirItem::Impl` per generic impl (method bodies lowered under the impl's `type_subst` with `self` bound to the instance). Since these are ordinary struct/impl HIR items, the backend needs no generic awareness.

- 2026-07-18: Operator traits — scalar path. Operator overloading is desugared here: an
  operator trait (`Add`, `Sub`, …, `PartialEq`, `Comparable`; table in `operator_traits.rs`) impl
  populates `operator_binary_impls` / `operator_unary_impls` during `register_impl`. In `lower_expr`,
  a `Binary` / `Unary` whose (peeled) left/operand type is a struct with a matching entry desugars to
  the method call `a.op(b)` via `build_operator_call` / `build_unary_operator_call` — a `Call` with a
  `FieldAccess` callee, identical to an ordinary method call, so the backend needs no operator
  awareness. A comparison method's `rhs: &Rhs` parameter means the argument is wrapped in a
  `Reference`. Owned `self` methods are no longer skipped in `register_impl` / `lower_impl` (valid on
  a `Copy` receiver; the checker rejected any non-`Copy` case). Generic-impl paths still skip owned
  `self` (operator traits on generic structs are out of scope this phase).
- 2026-07-16: Trait declarations. Traits are fully erased in this slice: an `Item::Trait`
  produces no HIR and needs no registration, because the parser has already injected each trait's
  default methods into the matching `impl Trait for Type` blocks — so trait impls (and their
  inherited defaults) lower through the ordinary inherent-impl path, and a trait-bounded generic
  monomorphizes to concrete dispatch on the substituted type with no trait awareness here.
- 2026-07-10: Const generics, `where` clauses & turbofish. Monomorphization now keys on const
  *values* as well as types: a `const_subst` (name → value) and `const_types` (name → int type) are
  active while an instance body lowers, parallel to `type_subst`. `MonoArg` (Type|Const) is the
  positional instance-argument kind; `split_mono_args` builds the two subst maps. `unify_ast_hir`
  binds a const param from an array-length position; `resolve_array_size` resolves `[T; N]` to a
  concrete length; a const-param value reference lowers to a typed integer literal; mangles include
  const values (`_cN`). Turbofish `type_args` seed the substitution before inference. Backends are
  unaffected — every instance reaching HIR has concrete `usize` array lengths.

## Recent Updates
- 2026-08-29: Standard-output builtins (2A). `IO_BUILTINS` (`expressions/mod.rs`) and a
  matching arm at the end of `lower_ident_call` (`expressions/calls.rs`) resolve `print` /
  `println` once every declared function, generic template and local binding has been tried,
  so a user function of that name still wins. Unlike the panic arm beside it the call type is
  fixed at `HirType::Void` rather than adopted from `expected`. Backend lowering is in
  `llvm-backend`; no new HIR node.
- 2026-08-25: `expressions/interpolation.rs` lowers `Expr::InterpString` to
  `HirExprKind::InterpString`, typed `string`. Each hole lowers with no expected type —
  the hole's own expression decides its type, and the rendering follows from that.
- 2026-08-24: Divergent arms no longer decide a lowered expression's type. `lower_if_expr`
  and `lower_match` take the result type from the first arm that is not `HirType::Void`,
  mirroring the checker. A `panic` / `unreachable` with no context type lowers to `void`, so
  taking the first arm unconditionally made the whole `if` or `match` void and the backend
  rejected it as "void type cannot be used as a value" — but only when the divergent arm was
  written first.
- 2026-08-18: `import` declarations. Lowering is unchanged in substance: `Item::Import` is
  consumed by module-resolution long before this pass, and an imported name arrives already
  rewritten to what it stands for. A `Pattern::UnqualifiedEnum` reaching `pattern_test` means
  neither the resolver nor the checker ran, so it surfaces as a `LoweringError::Malformed`.
