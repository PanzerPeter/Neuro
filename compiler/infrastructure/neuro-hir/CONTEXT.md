# neuro-hir

## Purpose
Provide the typed High-Level IR node definitions — the stable, backend-agnostic contract between the frontend (parser + type checker) and every backend (`llvm-backend`, `mlir-backend`).

## Entry Point
- Type: Library (no entry function — pure data)
- Public types: `HirProgram`, `HirItem`, `HirFunction`, `HirParam`, `HirStruct`, `HirField`,
  `HirEnum`, `HirEnumVariant`, `HirEnumField`, `HirImpl`, `HirMethod`, `HirSelfParam`, `HirConst`,
  `HirTrait`, `HirClosure`, `HirCapture`, `HirStmt`, `HirExpr`, `HirExprKind`, `HirFieldInit`,
  `HirType`, `HirCollectionKind`

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- shared-types — `Span`, `Literal`, `FormatSpec` embedded in HIR nodes
- ast-types — `BinaryOp` / `UnaryOp` reused unchanged (pure data enums, identical between
  surface and IR; reused rather than duplicated-plus-converted)

## Notes
This crate defines the HIR **types only**. AST → HIR lowering (`hir-lowering`) produces them and
the backends consume them; neither belongs here.

The HIR mirrors the surface AST one-to-one in structure, with two defining differences that make
it the *typed* contract:

1. **Every expression carries its resolved type.** `HirExpr` is `{ kind, ty, span }` and `ty` is
   a fully resolved `HirType`. `HirType` has **no `Unknown` variant** — reaching the HIR implies
   the program type-checked. Its variant set mirrors what the semantic analyzer produces today;
   no generic variants until the language gains them (No Speculative Generality).
   `HirType::Tensor { element, shape }` carries the statically shaped `Tensor<T, [d0, ...]>`,
   and the four construction kinds beside it — `TensorLiteral` (elements already flattened
   row-major), `TensorFill`, `TensorIdentity`, `TensorRandomNormal` — are the only ways to
   produce one. A fill and an identity stay separate nodes rather than expanding to elements,
   so a large tensor is one node and one loop instead of one node per element.
2. **Syntactic noise is normalized away.** The AST's `Expr::Paren` is dropped (tree structure
   already encodes grouping) and identifiers are resolved to their `String` name, with the source
   span on the enclosing node.

Nothing frontend-only survives into the HIR: lint-suppression attributes such as `@allow` are
consumed before lowering. Backend attributes (`@grad`, `@gpu`) belong here when the features that
need them land, and not before.

### Nodes that carry a deliberate design decision

**Generics do not exist here.** `hir-lowering` monomorphizes every template into concrete items,
so there is no generic node to define.

**Traits are erased except for one thing.** `HirItem::Trait(HirTrait { name, methods, span })`
exists ONLY to give dynamic dispatch a canonical vtable slot order — the trait's declaration
order. Static-dispatch traits are fully erased. `HirType::DynObject(String)` is valid only as a
`HirType::Reference` referent (backends lower `&dyn T` to a `{ data ptr, vtable ptr }` fat
pointer), and `HirExprKind::DynCoerce { value }` is the `&T` → `&dyn Trait` unsizing: `value.ty`
names the concrete type that selects the vtable, the node's `ty` is the trait-object reference.

**Slices are the second unsized type, and the second coercion.** `HirType::Slice(Box<HirType>)`
is `[T]`; like `DynObject` it is valid only as a `HirType::Reference` referent, which backends
lower to a `{ buffer ptr, i64 len }` fat pointer held by value — for `&mut [T]` as well, since a
write goes to the buffer the pointer names rather than to the pair.
`HirExprKind::SliceCoerce { value }` is the `&[T; N]` / `&Vec<T>` → `&[T]` unsizing: `value.ty`
names the container the length comes from, the node's `ty` is the slice reference. It mirrors
`DynCoerce` deliberately — these two are the language's only implicit conversions, and
`hir-lowering` applies both at one site.

**Closures are lifted items.** `HirItem::Closure(HirClosure { name, captures, params,
return_type, body, span })` is one lifted item per closure literal, whose first (implicit)
parameter at codegen is the captured-environment pointer. `HirExprKind::Closure { name,
captures }` is the value referencing it, listing the enclosing variables to snapshot in
capture-layout order (`HirCapture { name, ty }`). The value's `ty` is the ordinary
`HirType::Function { params, ret }`.

**Match arms are fully resolved, so backends need no pattern logic.** `HirExprKind::Match
{ scrutinee, arms }` carries `HirMatchArm { tests, bindings, guard, body }`.
`HirMatchTest::{Wildcard, Tag, IntEq, IntRange}` are the refutable tests — an exclusive `a..b`
is pre-normalized to `a..=b-1` — and `HirMatchBinding { name, ty, source }` with
`HirBindingSource::{Scrutinee, EnumPayload { slot }}` describes each binding. No exhaustiveness
reasoning reaches a backend.

**`val-else` is a statement, not a `Match` variant.** `HirStmt::ValElse { scrutinee, test,
bindings, else_binding, else_block, span }` reuses `HirMatchTest` / `HirMatchBinding` verbatim
rather than growing a parallel vocabulary. It is a statement because its `bindings` belong to
the ENCLOSING scope and stay live for every following statement, where a `Match` arm's bindings
die with the arm. `else_binding` is scoped to `else_block` alone and is `None` for `Option`
(whose failure variant has no payload) and for the omitted / `|_|` forms. The frontend
guarantees `else_block` diverges, so a backend may terminate it with `unreachable`.

**One loop node.** `HirExprKind::Loop` is the only loop; a statement-position loop is a
`HirStmt::Expr` wrapping it, typed `void`. Two shapes for one construct is what let a tail
`loop` be silently compiled as a discarded value.

**Enumerated loops are an option on the loop, not an adapter.** `HirStmt::ForRange` and
`HirStmt::ForEach` carry `index: Option<String>`, the `u64` position binding of
`for (i, x) in xs.enumerate()`. There is no iterator value in the HIR for an adapter to wrap, and
a counted loop already computes the position it would yield.

**Enums normalize three construction forms to one.** `HirType::Enum(String)` is nominal;
`HirExprKind::EnumConstruct { enum_name, variant, tag, payload }` is what unit, tuple, and
struct-variant syntax all become — payload in declared field order, `tag` the variant's
declaration index.

**Newtypes produce no item.** `HirType::Newtype { name, inner }` carries its resolved inner type
and the transparent `HirExprKind::NewtypeConstruct { name, value }` / `NewtypeAccess { object }`
lower straight through, so backends erase the wrapper entirely.

**Collections are a type, not a family of nodes.** `HirType::Collection { kind, args }` with
`HirCollectionKind::{Vec, HashMap, BTreeMap, String}` (Display renders `Vec<i32>`), plus
`HirExprKind::CollectionNew` — the typed mirror of `Vec::new()` and its siblings, whose `ty` is
the collection being built. Collection *methods* need no node: they reach backends as an
ordinary `Call` with a `FieldAccess` callee whose `ty` carries the call's resolved result (the
`Option<T>` a fallible reader returns, the `Vec<K>` `keys()` builds).

`String` is the one **nullary** kind — its buffer is a byte run, so it carries no type
arguments and needs no new `HirType` variant: `Collection { kind: String, args: [] }` is the
whole representation. `HirCollectionKind::arity()` (0/1/2) and `mangle_tag()` serve that;
`String`'s tag is `strbuf` rather than the lowercased surface name, which would collide with
the primitive `string` in a mangled instance name.

**Tuples and array rests** are the typed mirrors of their AST nodes: `HirType::Tuple(Vec<HirType>)`
(Display `(T1, T2, ...)`) with `HirExprKind::TupleLiteral` / `TupleIndex`, and
`HirExprKind::ArrayRest { array, start }` whose `ty` carries the resolved `[T; N - start]`
remainder. Tuple, struct, and array *destructuring* carry no HIR node — the parser desugars them.

**Interpolated strings carry a uniform hole shape.** `HirExprKind::InterpString { parts }` with
`HirInterpPart::{Text, Formatted}`; a hole written without a spec carries `FormatSpec::default()`,
so a backend sees one shape for every hole rather than two.

**A struct carries the name it was written under.** `HirStruct` has both `name` — the key every
`HirType::Struct` refers to it by — and `written_name`, which differs only for a monomorphized
generic instance, whose key is mangled (`Wrapper_g_i32`). The mangled key appears in no source
text, so anything a program can *see* — the `@derive(Debug)` rendering above all — uses the
written name. A backend cannot recover it from the key: the template is not emitted.
