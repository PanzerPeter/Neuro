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
`syntax-parsing` is a `[dev-dependencies]` entry only (tests build ASTs through the parser); it is never a production cross-slice dependency.

Lowering **re-derives** each expression's type rather than importing the checker's `Type`, which would couple two feature slices (VSA: duplicate over couple). It assumes well-typedness — a shape the checker should have rejected surfaces as a `LoweringError`, never a panic.

The lowerer runs a registration pre-pass mirroring the checker's: struct field tables (+ `@derive(Copy/Clone)` intent), `impl` method signatures under mangled `Struct__method` keys, free-function signatures, and module constants. Bodies then lower under a lexical scope stack and a loop-context stack.

Two type derivations are contextual and faithfully mirror the checker:
- **Literals** take a suffix type, else the expected type when it fits the literal's family, else the default `i32`/`f64`.
- A **function/method body's trailing expression** is an implicit return, typed against the declared return type; nested block/`if`-arm tails are typed with no hint.

Tuples (§3.2): `resolve_type` lowers the tuple type to `HirType::Tuple`; a tuple literal is typed by lowering each element (each hinted by the expected tuple's element type when annotated) and a `t.N` index reads the N-th element type off the (auto-derefed) tuple type. Destructuring is already desugared by the parser, so only the literal/index nodes reach here.

Enums (§3.5): a registration pre-pass records each enum's variants and resolved payload fields (`enums` table). `resolve_type` maps an enum name to `HirType::Enum`. All three construction forms normalize to one `HirExprKind::EnumConstruct { enum_name, variant, tag, payload }`: a unit `E::V` (Path) carries an empty payload; a tuple `E::V(..)` (Call→Path) carries the positional args; a struct `E::V { .. }` (`EnumStructLiteral`) reorders its provided fields into declared order so codegen sees a single positional layout. `tag` is the variant's declaration index.

Struct + array destructuring (§3.2): the parser desugars these, so only the array-rest node reaches lowering. `Expr::ArrayRest { array, start, exact }` lowers to `HirExprKind::ArrayRest { array, start }` typed `[T; N - start]` (re-derived from the source array's `HirType`); a defensive arity re-check (`exact ⇒ N == start`, else `start <= N`) raises `Malformed` rather than underflowing `N - start`.

Pattern matching (§3.6): `lower_match` fully resolves each arm. `pattern_test` maps a pattern to a `HirMatchTest` (variant tag / `IntEq` / `IntRange`, with `char`/`bool` literals as scalar codepoints/0-1 and an exclusive `a..b` normalized to `a..=b-1`); `pattern_bindings` resolves a single arm's bindings to `HirBindingSource::Scrutinee` (bare binding) or `EnumPayload { slot }` (payload field, slot = declared field position). Bindings are defined in a per-arm scope so the guard and body lower correctly; the body-type hint is the caller's expected type, else the first arm's type. The match type is the first arm's body type.

Newtypes (§3.15): a registration pre-pass records each newtype's inner AST type (`newtypes` table).
`resolve_type` maps a newtype name to `HirType::Newtype { name, inner }`, resolving the inner
recursively (a newtype may wrap another; the checker already rejected cycles). Construction
`Name(value)` — a `Call` whose identifier callee names a newtype — lowers to
`HirExprKind::NewtypeConstruct { name, value }` (value hinted by the inner type), taking precedence
over a same-named function like the checker. Inner access `.0` on a newtype-typed object lowers to
`HirExprKind::NewtypeAccess { object }` typed as the inner type. No `HirItem` is emitted — a newtype
is purely a type-system distinction that the backends erase.

Three nodes carry a deliberately-chosen type the source has no first-class form for: a `loop` value-expression takes its `break v` type (or `void`); a method-name callee `FieldAccess` carries the call's result type (there is no method value); a `Range` carries `void` (valid only as a `string.slice` argument — the slice lowering reads its bounds directly). Divergent panic-family calls (`panic`/`assert`/`unreachable`) adopt their context's expected type, or `void` in statement position.

Generics (§3.8): this slice performs **monomorphization** — the HIR has no generic node, so generic templates are erased into concrete instances here. A generic `FunctionDef` is stored in `generic_templates` (not `functions`) and never lowered directly. A call to a generic function (`lower_generic_call`) infers its type arguments by unifying the template's parameter annotations against the lowered arguments' types (`unify_ast_hir`), resolves the concrete signature under a `type_subst` map (consulted by `resolve_type` for a parameter name), mangles a per-instance name (`mangle_instance` → `name__g_<type…>`), enqueues the instance if unseen, and emits a `Call` to the mangled name. A worklist drains after the ordinary items: each instance's body lowers under its `type_subst`, appended as a concrete `HirItem::Function`. The backend pre-declares all functions, so instance emission order is irrelevant.

Generic structs & impls (§3.8): monomorphized the same way. A generic `StructDef` is stored in `generic_structs` (not `structs`) and a generic `impl` in `generic_impls` (keyed by base name); neither is lowered directly. `instantiate_generic_struct(base, args)` — called from `resolve_type` for a `Type::Generic` annotation and from `lower_generic_struct_literal` after inferring the arguments from the field values via `unify_ast_hir` — mangles a per-instance name, registers the instance's concrete fields + impl-method signatures, and enqueues a `MonoStruct` if unseen. The struct-instance mangle (`mangle_struct_instance` → `Base_g_<type…>`) deliberately avoids `__`, because the backend recovers a method's receiver struct by splitting the method symbol on `__`. The struct worklist drains alongside the function worklist, emitting one `HirItem::Struct` plus one `HirItem::Impl` per generic impl (method bodies lowered under the impl's `type_subst` with `self` bound to the instance). Since these are ordinary struct/impl HIR items, the backend needs no generic awareness.

- 2026-07-16: Trait declarations (§3.9). Traits are fully erased in this slice: an `Item::Trait`
  produces no HIR and needs no registration, because the parser has already injected each trait's
  default methods into the matching `impl Trait for Type` blocks — so trait impls (and their
  inherited defaults) lower through the ordinary inherent-impl path, and a trait-bounded generic
  monomorphizes to concrete dispatch on the substituted type with no trait awareness here.
- 2026-07-10: Const generics, `where` clauses & turbofish (§3.8). Monomorphization now keys on const
  *values* as well as types: a `const_subst` (name → value) and `const_types` (name → int type) are
  active while an instance body lowers, parallel to `type_subst`. `MonoArg` (Type|Const) is the
  positional instance-argument kind; `split_mono_args` builds the two subst maps. `unify_ast_hir`
  binds a const param from an array-length position; `resolve_array_size` resolves `[T; N]` to a
  concrete length; a const-param value reference lowers to a typed integer literal; mangles include
  const values (`_cN`). Turbofish `type_args` seed the substitution before inference. Backends are
  unaffected — every instance reaching HIR has concrete `usize` array lengths.
