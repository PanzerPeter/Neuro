# ast-types

## Purpose
Provide the canonical Abstract Syntax Tree node definitions shared by every stage that produces or consumes the AST — without coupling those stages to each other.

## Entry Point
- Type: Library (no entry function — pure data)
- Public surface, re-exported from the crate root in four groups:
  - `expressions` — `Expr`, `BinaryOp`, `UnaryOp`, `Pattern`, `MatchArm`, `EnumPatternPayload`,
    `FieldPattern`, `FieldInit`, `ClosureParam`, `InterpPart`
  - `items` — `Item`, `FunctionDef`, `StructDef`, `EnumDef`, `EnumVariant`, `VariantPayload`,
    `FieldDef`, `ImplDef`, `MethodDef`, `SelfParam`, `TraitDef`, `TraitMethod`, `ConstDef`,
    `NewtypeDef`, `ModuleDef`, `ModuleId`, `ImportDef`, `ImportName`, `ImportSelection`,
    `Parameter`, `ParamLabel`, `GenericParam`, `GenericParamKind`, `TraitBound`, `Attribute`
  - `statements` — `Stmt`
  - `types` — `Type`, `ArraySize`, `GenericArg`

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- shared-types — `Span`, `Identifier`, `Literal`, `FormatSpec` embedded in AST nodes

## Notes
These types live in infrastructure rather than in `syntax-parsing` so that `semantic-analysis`,
`hir-lowering`, and `llvm-backend` can consume the tree without a cross-slice dependency on the
parser. `syntax-parsing/src/ast/mod.rs` re-exports them so parser-internal code can keep saying
`crate::ast::…`; it is a naming convenience, not a compatibility shim.

Node *shapes* are self-describing in the source. What follows is only what the definitions
cannot state.

### Parse-only nodes: written down here, gone before type checking
Three `Item` variants never reach `semantic-analysis` — module resolution reads them off the
file it opens and drops them. Each therefore records **only what was written**, because what it
*means* is a file-system question:
- `Item::Import(ImportDef)` — `relative` (the explicit `./` form), the `::`-separated `path`, an
  `ImportSelection` of `Module` / `Alias` / `List(Vec<ImportName>)`, and `exported` for the
  `export import` re-export form. Whether a path segment names a module file, an item inside one,
  or an enum is not decidable at parse time.
- `Item::Module(ModuleDef { name, items, span })` — an inline `module Name { … }` block, which
  module resolution lifts into a graph module of its own and erases.
- `Item::NoPrelude(Span)` — the file-scope `@no_prelude` marker, carrying only its span.

`Pattern::UnqualifiedEnum { variant, payload, span }` is the pattern-side counterpart: `Some(n)`
written without its enum, which module resolution rewrites into `Pattern::Enum` against the
importing file's table. A payload-*less* variant (`None`) is indistinguishable from a binding at
parse time and arrives as `Pattern::Binding`, resolved by the same table.

### What the parser desugars, and what earns a node
Most sugar never reaches this crate: tuple / struct / array destructuring, compound assignment,
type aliases, trait default-method injection, struct field-init shorthand, and
argument-position `impl Trait` are all expanded at parse time. Two exceptions earn a node, and
the reason is the same each time — the information is not available yet:
- `Stmt::ValElse { pattern, value, else_binding, else_block, span }` survives because its pattern
  is **refutable**: the test and the failure branch have to be represented. `pattern` reuses the
  `Pattern` set from `match`; `else_binding` is the optional `|name|`, where an `Identifier` named
  `_` is the written wildcard, distinct from `None`.
- `Expr::ArrayRest { array, start, exact, span }` survives because the trailing `..rest`
  sub-slice's size (`[T; N - start]`) is known only after type checking. `exact` records a
  rest-less pattern, whose length must match exactly.

`Pattern::binding_names()` is a pure structural query shared by both closure free-variable
walkers.

### Nodes whose form is a decision
- `Expr::Loop { label, body, span }` is the **sole** loop node; a statement-position `loop` is
  `Stmt::Expr(Expr::Loop { .. })`. Two shapes for one construct meant every "is the tail
  statement value-producing?" test in the pipeline — all of which key on `Stmt::Expr` — silently
  missed a trailing `loop`, which is how a tail `loop` used as an implicit return came to be
  compiled as a discarded value (BUG-005). `Stmt::Break` carries `value: Option<Expr>`;
  `while`/`for` stay unit and have no expression form.
- `Expr::Try { operand, span }` (postfix `?`) is a node of its own rather than a `BinaryOp`: it
  has one operand, and its type comes from that operand's success payload while its *failure*
  path is typed by the enclosing function's return type.
- `Expr::Range { start, end, inclusive, span }` is **not** a first-class value — it is valid only
  as a `.slice` / `.char_slice` argument, and semantic analysis rejects it elsewhere. `for`-range
  loops keep their bounds on `Stmt::ForRange` and never produce it.
- `Type::Slice { element, span }` is `[T]`, the unsized run behind `&[T]` / `&mut [T]`. It shares
  its opening bracket with `Type::Array`, and the `;` (or its absence before `]`) is what the
  parser selects on. Like `Type::DynTrait` it is valid only as a reference referent; semantic
  analysis rejects a bare one.
- `Stmt::ForRange` / `Stmt::ForEach` carry `index: Option<Identifier>` — the position binding of
  `for (i, x) in xs.enumerate()`, a `u64` counting from zero. `.enumerate()` is an *arity* on the
  loop node rather than an adapter expression because there is no iterator protocol to return one
  from, and because a range has no value form to call a method on. Both walkers that bind loop
  variables must bind this one too, or a closure in the body captures it as a free variable.
- `Expr::Unsafe { stmts, span }` is structurally identical to `Expr::Block`. The distinct node
  exists so a later phase can attach the `@kernel` aliasing relaxation to it; it carries no
  special semantics today.
- **`Type::Named` is the catch-all, deliberately.** A bare type-parameter reference, an enum
  annotation, and a newtype annotation are all plain `Type::Named` — later passes resolve the name
  against the generics in scope, the enum table, or the newtype table. Only a generic *application*
  (`Name<T1, ...>`) becomes `Type::Generic`, which serves generic structs and generic enums alike.
- `Expr::Path { type_name, member, span }` is the `TypeName::member` callee of an
  associated-function call. A qualified module path folds its leading segments into the
  `type_name` identifier's `name`; module resolution splits and erases it.
- `Type::ImplTrait` survives parsing **only in return position** — in argument position the
  parser rewrites it into a fresh trait-bounded `GenericParam`, so downstream slices see an
  ordinary generic. Its `assoc_bindings` carry the `impl Trait<Assoc = T>` constraint through
  that rewrite, which is why the node holds the same binding list a `TraitBound` does.
  `Type::DynTrait` always survives; semantic analysis resolves it to a
  trait-object type and rejects it outside a reference.

### Fields that encode a cross-slice contract
- **Named arguments.** `Parameter.label: ParamLabel` is `Implicit` for the ordinary `name: T`,
  `External(label)` for `external internal: T` (the caller must write the label), `Suppressed`
  for `_ internal: T` (the caller must not). `Expr::Call.arg_labels: Vec<Option<Identifier>>`
  sits beside `args`, one entry per argument or empty when the call named none. The label list
  is deliberately a **sibling** of `args` rather than a wrapper around each argument: it is
  parse-only surface that `argument-binding` empties before type checking, so every pass
  downstream keeps reading `args` as the positional `Vec<Expr>` it always was.
- **Visibility.** `exported: bool` on `FunctionDef`, `StructDef`, `EnumDef`, `TraitDef`,
  `ConstDef`, `NewtypeDef`, and `FieldDef` — `false` is the private default, set from a leading
  `export`. An enum struct-variant's `FieldDef` is always `exported: true`: a variant is reached
  through a pattern naming its enum, so its fields carry no visibility of their own.
- **Provenance.** `module: ModuleId` (a `u32`) on `FunctionDef`, `StructDef`, `ImplDef`, and
  `ConstDef` — the file a declaration was loaded from, stamped by module resolution and `0` for
  everything the parser produces alone. The merge is flat, so this stamp is the only surviving
  trace of which file a declaration came from, and it exists because field visibility needs the
  receiver's type and is therefore checked by `semantic-analysis`, which has nothing else to
  read it from. `ImplDef` carries `module` but no `exported`: an `impl` declares no name.
- **Lifetimes.** `lifetimes: Vec<Identifier>` on `FunctionDef` / `StructDef` / `ImplDef` and
  `lifetime: Option<Identifier>` on `Type::Reference` are kept apart from `generics` because
  lifetimes are a distinct namespace and do not drive monomorphization. Both are validated and
  then erased — a reference type's identity does not depend on its lifetime. `EnumDef` has no
  `lifetimes` field at all; the parser rejects a lifetime parameter on an enum.
- **Attributes.** `Attribute { name, args, span }` on `FunctionDef` / `MethodDef` / `StructDef`.
  Unknown names are accepted so the surface stays forward-compatible; semantics are interpreted
  by later passes (`@derive(Copy, Clone)`, `@allow(...)`, and eventually `@grad` / `@gpu`).
- **Const generics.** `GenericParamKind` (`Type` / `Const`) on `GenericParam`, `ArraySize`
  (`Literal` / `Const`) on `Type::Array`, `GenericArg` (`Type` / `Const`) in `Type::Generic.args`,
  `Expr::Call.type_args` for turbofish, and `where_predicates` on
  `FunctionDef` / `StructDef` / `ImplDef`.

### Method receivers
`ImplDef` holds an optional `trait_name` — `Some` for a trait implementation (`impl Drop for T`),
`None` for an inherent block — plus `assoc_types: Vec<(Identifier, Type)>` for `type Name = T`
bindings: the operator traits' `type Output = T`, and the answer to whatever a user trait's
`TraitDef.assoc_types: Vec<Identifier>` declares. The two sides sit on different nodes because a
declaration carries a name only. `GenericParam.bounds: Vec<TraitBound>` is the third place the
same `(name, type)` binding shape appears — a bound constrains an associated type
(`T: Channel<Sample = i32>`) exactly as an impl answers it. An associated type is named in a
signature as
`Type::Named("Self::Item")` — the qualifier rides in the name exactly as a module prefix does, and
only the type checker, which is the first pass that knows the implementing type, resolves it. Each
`MethodDef` holds an `Option<SelfParam>` distinguishing associated functions (`None`) from
instance methods (`Some`).

All three `SelfParam` variants reach codegen, with different support:
`Ref` (`&self`) passes the struct by value; `RefMut` (`&mut self`) passes it by pointer and
carries an exclusive-borrow rule at the call site; `Owned` (`self`) is accepted **only on a
`Copy` receiver** (ABI-identical to `&self`) and is otherwise rejected with
`TypeError::UnsupportedSelfParam` — a by-value non-`Copy` struct ABI does not exist yet.

### Interpolated strings
`Expr::InterpString { parts, span }` with `InterpPart::{Text, Formatted}`. A `Formatted` hole
carries an already-parsed `Expr` plus an optional `FormatSpec`, so consumers see ordinary typed
expressions rather than raw text to re-parse.
