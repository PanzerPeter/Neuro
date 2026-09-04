# hir-lowering

## Purpose
Lower a type-checked surface AST into the typed High-Level IR (`neuro-hir`), re-deriving every expression's resolved type so each backend consumes HIR instead of the AST.

## Entry Point
- Type: Library function
- Input: `items: &[ast_types::Item]` — a program that already passed
  `semantic_analysis::type_check`
- Output: `Result<neuro_hir::HirProgram, LoweringError>`

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- ast-types — read-only traversal of the surface `Item` / `Stmt` / `Expr` / `Type` nodes
- neuro-hir — the typed HIR node set produced as output
- shared-types — `Span`, `Literal`, `IntSuffix`, `FloatSuffix`, `Identifier` reused in nodes
- thiserror — `LoweringError` derivation

`syntax-parsing` is a `[dev-dependencies]` entry only (tests build ASTs through the parser); it
is never a production cross-slice dependency.

## Notes

### The governing rule
Lowering **re-derives** each expression's type rather than importing the checker's `Type`, which
would couple two feature slices (VSA: duplicate over couple). Several tables here are therefore
deliberate duplicates of the checker's — the collection method surface, the `val-else` binding
table, return-position `impl Trait` resolution — and a divergence between the two surfaces as a
`LoweringError`, which is the point. Lowering assumes well-typedness: a shape the checker should
have rejected is a `LoweringError`, never a panic.

A registration pre-pass mirrors the checker's: struct field tables (plus `@derive(Copy/Clone)`
and `@derive(PartialEq)` intent, in `clone_structs` / `partial_eq_structs`), `impl` method
signatures under mangled `Struct__method` keys, free-function signatures,
trait method order, and module constants. Bodies then lower under a lexical scope stack and a
loop-context stack.

`expressions/` holds the expression work — `mod.rs` the dispatch and the block-value tail rule,
with `calls`, `enums`, `structs`, `matches`, `sequences`, `coercion`, `try_op`, `coalesce`, and
`interpolation` beside it.

### Contextual typing
Two derivations take their type from context and faithfully mirror the checker:
- **Literals** take a suffix type, else the expected type when it fits the literal's family, else
  the default `i32` / `f64`.
- **A body's trailing expression** is an implicit return, typed against the declared return type.
  Nested block and `if`-arm tails inherit the same way: `lower_if_expr` takes the expected type if
  there is one, else the first arm's type, and `lower_block_value` threads it to the tail — the
  rule `lower_match` already applied to arm bodies.

**A trailing `Stmt::If` carrying an `else` is a value at every depth.** Both tail rules —
`lower_body_stmts` (`items.rs`, the implicit return) and `lower_block_value_inner`
(`expressions/mod.rs`, every nested block) — route it through `lower_if_expr` and emit a
`HirStmt::Expr`. When only the function body recognised it, a nested tail `if` lowered via
`lower_stmt_block` and its branches' values were discarded, and a generic-enum construction
inside one also lost its payload (a branch that is not a value position never reaches the
`current_return` instance fallback). **The rule lives only here** — the LLVM backend no longer
carries a copy.

**A divergent arm contributes no type.** `lower_if_expr` and `lower_match` take the result type
from the first arm that is not `HirType::Void`. A `panic` / `unreachable` with no context type
lowers to `void`, so taking the first arm unconditionally made the whole `if` or `match` void and
the backend rejected it as "void type cannot be used as a value" — but only when the divergent
arm happened to be written first.

### Nodes with a deliberately chosen type
Three have no first-class source form: a `loop` value-expression takes its `break v` type (or
`void`); a method-name callee `FieldAccess` carries the *call's* result type, since there is no
method value; a `Range` carries `void`, being valid only as a `string.slice` /
`string.char_slice` argument whose lowering reads its bounds directly.

Divergent panic-family calls (`panic` / `assert` / `unreachable`) adopt their context's expected
type, or `void` in statement position. The standard-output builtins (`print` / `println`,
`IO_BUILTINS` in `expressions/mod.rs`) are recognized in the same `lower_ident_call` fallback but
do **not** take their type from context: they return, so the call is always `HirType::Void` with
one `HirType::String` parameter. Both arms sit at the end of `lower_ident_call`, after every
declared function, generic template, and local binding has been tried, so a user function of the
same name still wins.

An `Expr::Loop` with no `break` targeting it lowers to the **expected** type rather than unit
(`LoopCtx.has_break`, set by `record_break_target`), so the dead exit-block result slot is still
typed for the position the loop sits in.

### Desugars this slice owns
Each produces existing HIR nodes, so no backend learns the construct exists.

- **`?` error propagation** (`expressions/try_op.rs`). `operand?` becomes a
  `HirExprKind::Match`: arm 0 tests the `Some`/`Ok` tag and yields payload slot 0 as `__try_N`
  (off `try_counter`); arm 1 is a `Wildcard` whose body is a `Block` holding a single
  `HirStmt::Return` of the failure variant. No backend change is needed — the arm terminates, and
  `codegen_arm_body` already skips the result-slot store for a terminated block. The failure value
  is rebuilt against `current_return`'s instance, **not** the operand's: a `Result<u8, E>`
  propagating out of a `-> Result<i32, E>` function must produce the latter. The `Err` payload is
  bound out of the operand's slot 0 and passed straight through, which is the "no implicit
  conversion" rule made structural.
- **`??` null coalescing** (`expressions/coalesce.rs`). `lhs ?? fallback` becomes a `Match`: arm 0
  tests the `Some`/`Ok` tag and binds payload slot 0 as `__coalesce_N` (off `coalesce_counter`, so
  nested coalesces cannot shadow each other); arm 1 is a `Wildcard` whose body is the fallback.
  The fallback's laziness comes free from the per-arm basic-block chain codegen already emits, and
  it lowers with the payload type as its expected type, which is what types a bare literal.
  `binary_result_type`'s `NullCoalesce` arm is unreachable by construction and says so.
  `success_variant`, `not_fallible`, and `fallible_base` are shared with the `?` desugar.
- **Operator traits** (`operator_traits.rs` holds the table: `Add`, `Sub`, …, `PartialEq`,
  `Comparable`). An operator-trait impl populates `operator_binary_impls` / `operator_unary_impls`
  during `register_impl`; a `Binary` / `Unary` whose peeled left/operand type is a struct with a
  matching entry becomes the method call `a.op(b)` — a `Call` with a `FieldAccess` callee,
  identical to an ordinary method call, so the backend needs no operator awareness. A comparison
  method's `rhs: &Rhs` parameter means the argument is wrapped in a `Reference`. Owned `self`
  methods are lowered (valid on a `Copy` receiver; the checker rejected any non-`Copy` case),
  though generic-impl paths still skip them.
- **Derived equality** (`@derive(PartialEq)`). A struct in `partial_eq_structs` has no `eq` to
  dispatch to, so `==` / `!=` on one stays an `HirExprKind::Binary` typed `Bool` and the backend
  expands it over the fields. Handled in the `Binary` arm *before* `binary_result_type`, which
  admits no aggregate operand — that is the whole difference from the operator-trait route above.
- **`val-else`** (`val_else.rs`). `lower_val_else` reuses `pattern_test` / `pattern_bindings` from
  `expressions/matches.rs` for the success test and bindings, lowers the `else` branch in a pushed
  scope with its own binding, and only then defines the pattern's bindings in the **enclosing**
  scope so later statements type against them. `resolve_else_binding` mirrors the checker's
  binding table off `enum_instance_base`: a `Result`'s `Err` payload (slot 0), `None` for
  `Option`, else the whole scrutinee.
- **The `for`-loop iteration protocol** (`iteration.rs`). A `Stmt::ForEach` whose head is a
  nominal type carrying an `IntoIterator` or `Iterator` impl (`trait_impls`, this slice's own
  copy of the checker's table) becomes a `HirStmt::Expr` wrapping a `Block`: a mutable
  `__iter_N` binding initialized from `head.into_iter()` — or from the head itself, when it is
  already an `Iterator` — followed by a `while true` whose single statement is a two-arm
  `Match` on `__iter_N.next()`. Arm 0 tests the `Some` tag and binds payload slot 0 to the loop
  variable; arm 1 is a `Wildcard` whose body breaks, exactly as the `?` desugar's failure arm
  terminates its block. The label rides on the emitted `while`, so `break`/`continue`, labeled
  or not, resolve against it with no backend change.
  Two details are load-bearing. The `next` receiver carries the ITERATOR's type, not the call's
  result: the backend recovers the method symbol from the receiver, so typing it as the result
  sends `next` looking for a builtin on `Option`. And the element type is read out of the
  `Some` payload of `next`'s return (`success_variant`) rather than off the impl's `type Item`
  binding, so the binding and the storage the backend decodes cannot drift apart. An enumerated
  head gets a `__iter_pos_N` cursor declared beside the iterator and advanced at the TOP of the
  yielding arm, so a `continue` cannot skip the advance and repeat an index.
  The built-in sequence heads never reach this path: a range, an array, a `Vec`, and a borrowed
  slice keep their `HirStmt::ForRange` / `ForEach` counted-loop nodes, which is what leaves
  their generated code unchanged.
  A `LoopPosition` picks where the position binding's value comes from. `Step` is the counter
  above; `ByteOffset` is what a `text.char_indices()` head takes (`char_indices_receiver`
  recognises it on the AST iterable, ahead of lowering it) — the loop samples the iterator's own
  `offset` field into `__iter_pos_N` as the FIRST statement of the `while` body, because `next`
  advances that field past the code point it returns and a sample taken afterwards would name
  the following one. Nothing increments it: the iterator owns it.
- **The `.map(f)` / `.filter(p)` desugar** (`loop_adapters.rs`). A head's `adapters` chain is
  folded into the loop it decorates rather than materialized as an iterator value, so all four
  head shapes and the protocol path share one implementation. `plan_loop_adapters` binds each
  adapter function ONCE, ahead of the loop (`__adapt_fn_N_k`), and the loop binds
  `__adapt_elem_N` instead of the user's name; `apply_loop_adapters` then opens the body with the
  chain — a filter as `if !p(cur) { continue }`, a map as a `__adapt_v_N_k` binding — and closes
  it with the user's binding over whatever the last adapter produced. The whole thing is wrapped
  in a `Block`, which is what scopes the function bindings to the loop.
  An enumerated adapted head takes its position from a `__adapt_pos_N` cursor rather than the
  counted loop's `index`: that index counts SOURCE steps, so a filtered chain would leave gaps in
  it. The cursor is read and advanced after the filters and before the user's statements, for the
  same reason the protocol path's `__iter_pos_N` is.
- **Traits.** The parser has already injected each trait's default methods into the matching
  `impl Trait for Type` blocks, so trait impls and their inherited defaults lower through the
  ordinary inherent-impl path, and a trait-bounded generic monomorphizes to concrete dispatch with
  no trait awareness needed.

### Monomorphization
The HIR has no generic node, so every template is erased into concrete instances here.

A generic `FunctionDef` goes to `generic_templates` (not `functions`) and is never lowered
directly. `lower_generic_call` infers its type arguments by unifying the template's parameter
annotations against the lowered arguments' types (`unify_ast_hir`), resolves the concrete
signature under a `type_subst` map (consulted by `resolve_type` for a parameter name), mangles a
per-instance name, enqueues the instance if unseen, and emits a `Call` to the mangled name. A
worklist drains after the ordinary items. The backend pre-declares all functions, so emission
order is irrelevant.

Generic structs and impls work the same way, through `generic_structs` / `generic_impls` and
`instantiate_generic_struct(base, args)` — called from `resolve_type` for a `Type::Generic`
annotation and from `lower_generic_struct_literal` after inferring the arguments from field
values. Each instance registers concrete fields plus impl-method signatures and emits one
`HirItem::Struct` plus one `HirItem::Impl` per generic impl, with method bodies lowered under the
impl's `type_subst` and `self` bound to the instance. Because these are ordinary struct/impl HIR
items, the backend needs no generic awareness.

**The mangling scheme is load-bearing.** `mangle_instance` (`name_g_<type…>`) and
`mangle_struct_instance` (`Base_g_<type…>`) use a **single**-underscore marker, deliberately
avoiding `__`, because the backend recovers a method's receiver struct by splitting the method
symbol on `__`. Semantic analysis rejects a user name containing `__`, so `<instance>__<method>`
holds exactly one separator from both sides.

`instantiate_generic_impls` also records each instance's `trait_impls` entry, mirroring the
checker: without it a generic iterator adapter would satisfy `Iterator` under its base name
only, and a `for` head over an instance would find no protocol on the type it actually has.

Const generics ride the same machinery: a `const_subst` (name → value) and `const_types`
(name → int type) are active while an instance body lowers, parallel to `type_subst`. `MonoArg`
(Type | Const) is the positional instance-argument kind and `split_mono_args` builds the two maps;
`unify_ast_hir` binds a const param from an array-length position, `resolve_array_size` resolves
`[T; N]` to a concrete length, a const-param reference lowers to a typed integer literal, and
mangles include const values (`_cN`). Turbofish `type_args` seed the substitution before
inference. Backends are unaffected — every instance reaching the HIR has concrete `usize` array
lengths.

**Generic enums** (`Option<T>` / `Result<T, E>`) use `generic_enums` (base → template),
`enum_instance_base` / `enum_instance_args` (instance → base + arguments), and the
`mono_enum_pending` worklist, drained ahead of the struct worklist. A generic `Item::Enum` is
registered as a template and never lowered directly; `instantiate_generic_enum` resolves its
variants under the argument substitution and emits one ordinary `HirItem::Enum` per instance
(named by `mangle_struct_instance`, e.g. `Opt_g_i32`), so backends stay generic-unaware.
Construction mirrors the checker: the instance comes from the expected type, else the payload is
unified against the template, else any remaining parameter is taken from the enclosing return
instance. To make that fallback available, `lower_body` tracks the declared return type in
`current_return`, which also gives a `return` operand its contextual type. An enum pattern written
with a generic base resolves against the scrutinee's instance (`pattern_enum_name`) for both its
tag test and its payload bindings.

### Closures
`closures.rs` lifts each `Expr::Closure` to a `HirItem::Closure` named `__closure_N` — the `__`
prefix is a reserved generated-symbol marker the checker forbids in user names — collected in
`closure_items` and appended after the monomorphization worklists. Captures are the body's free
variables that resolve to an enclosing **local** binding (`lookup_local` excludes module
constants), snapshotted in first-seen order with their types. A block body lowers through
`lower_body` (the annotation supplies the return type); a single-expression body infers it. The
value site emits `HirExprKind::Closure { name, captures }` typed as `HirType::Function`, and
`lower_ident_call` dispatches a call on a local function-typed binding indirectly (the callee is
a `HirExprKind::Variable` of `HirType::Function`).

### Dynamic dispatch
A `traits` table (name → methods in declaration order, with their visible parameter and return
types) is registered before impls, and each `Item::Trait` lowers to a `HirItem::Trait` carrying
that order — the canonical vtable slot layout backends need. `resolve_type` delegates to
`resolve_type_ctx(ty, behind_ref)` so `&dyn Trait` resolves to `Reference(DynObject)` while a bare
`dyn` is an internal error (the checker rejects it first). `[T]` rides the same flag: it lowers to
`HirType::Slice` only behind a reference.

`lower_expr` is a thin wrapper that lowers via `lower_expr_uncoerced` and then applies
`apply_unsizing_coercion`. That is the **single site** where both unsizings are inserted —
`&T` → `&dyn Trait` (`DynCoerce`) and `&[T; N]` / `&Vec<T>` → `&[T]` (`SliceCoerce`) — so every
context supplying an expected type (call arguments, returns, annotated bindings) gets them
uniformly, and a value that already has the target shape is never re-coerced. A method call on a
`DynObject` receiver types from the trait declaration, naming no implementor. Return-position
`impl Trait` resolves to its concrete type through `declared_return_type` /
`shallow_result_type`.

An **associated type** reaches lowering as a `Type::Named` spelled `Self::Item`. `enter_impl_assoc`
adds each `impl` block's bindings to `type_subst` under that spelling for the block's signature
registration and its bodies — a name standing for a concrete type over one block is what the
type-parameter substitution already is, so annotations resolve through the one path. A trait's own
declaration has no implementor, so `resolve_trait_sig_type` gives such a position `Void` in the
`traits` table; nothing reads it, because a trait declaring an associated type is not object-safe.

### Per-construct lowering notes
- **Slices** — `.slice(range)` is routed to `lower_sequence_slice` *ahead of* the collection
  method surface, because a `Vec` receiver's `.slice` borrows its buffer rather than acting on the
  header; `sliceable_element` names the three receivers that permit it (`[T; N]`, `Vec<T>`,
  `[T]`). Indexing, `for x in xs`, and `IndexAssignment` each read a slice's element type
  alongside the array's, and `slice.len()` is `u64`.
- **Enumerated loops** — `ForRange` / `ForEach` carry the position binding through as
  `index: Option<String>` and define it in the loop scope as `LOOP_INDEX_TYPE` (`u64`), ahead of
  the element binding so the two collide rather than shadow. The free-variable walker binds it
  too, or a closure in the body captures it.
- **Tuples** — `resolve_type` gives `HirType::Tuple`; a literal is typed by lowering each element
  (hinted by the expected tuple's element type when annotated) and `t.N` reads the N-th element
  type off the auto-derefed tuple type. Destructuring is parser-desugared.
- **Enums** — a pre-pass records each enum's variants and resolved payload fields. All three
  construction forms normalize to one `HirExprKind::EnumConstruct`: a unit `E::V` carries an empty
  payload, a tuple `E::V(..)` the positional args, and a struct `E::V { .. }` **reorders its
  provided fields into declared order** so codegen sees a single positional layout. `tag` is the
  variant's declaration index.
- **Array rest** — `Expr::ArrayRest { array, start, exact }` lowers to
  `HirExprKind::ArrayRest { array, start }` typed `[T; N - start]`, re-derived from the source
  array's `HirType`. A defensive arity re-check (`exact ⇒ N == start`, else `start <= N`) raises
  `Malformed` rather than underflowing `N - start`.
- **Pattern matching** — `pattern_test` maps a pattern to a `HirMatchTest` (variant tag / `IntEq`
  / `IntRange`, with `char`/`bool` literals as scalar codepoints or 0-1, and an exclusive `a..b`
  normalized to `a..=b-1`); `pattern_bindings` resolves an arm's bindings to
  `HirBindingSource::Scrutinee` or `EnumPayload { slot }` (slot = declared field position).
  Bindings are defined in a per-arm scope so the guard and body lower correctly. A
  `Pattern::UnqualifiedEnum` reaching `pattern_test` means neither module resolution nor the
  checker ran, and surfaces as `LoweringError::Malformed`.
- **Newtypes** — a pre-pass records each newtype's inner AST type; `resolve_type` maps the name to
  `HirType::Newtype { name, inner }`, resolving the inner recursively (the checker already
  rejected cycles). A `Call` whose identifier callee names a newtype lowers to
  `NewtypeConstruct` (value hinted by the inner type), taking precedence over a same-named
  function exactly as the checker does; `.0` on a newtype-typed object lowers to `NewtypeAccess`.
  No `HirItem` is emitted — the backends erase the wrapper.
- **Collections** (`collections.rs`) — `lower_collection_new` lowers `Vec::new()` and its siblings
  to `HirExprKind::CollectionNew` typed from the annotated target, and `lower_collection_method`
  re-derives each method's argument and result types, instantiating `Option<T>` through the
  ordinary generic-enum path so backends see a real `HirItem::Enum`. `resolve_type` maps the
  `Vec` / `HashMap` / `BTreeMap` generic application to `HirType::Collection` rather than
  monomorphizing a nominal instance (a user-declared type of that name still shadows it), and
  indexing, index assignment, and `for`-in resolve a `Vec` element alongside an array's.
- **`String`** — `collection_kind` recognizes the name and the `nullary_collection` helper lets
  `resolve_type` accept it as a complete type, checked **after** the struct/enum/newtype arms so a
  user declaration shadows it. `lower_collection_new` builds a nullary kind's type itself instead
  of requiring an annotated target; `lower_collection_method` adds `push_str` (`string` parameter)
  and `to_string` (`string` result). `mangle_type` uses `HirCollectionKind::mangle_tag()`, so
  `String` mangles as `strbuf` — which the primitive `string` cannot collide with — and yields the
  bare tag when there are no arguments.
- **`.slice(range)` / `.char_slice(range)`** — `lower_builtin_method` gives both the same
  `&string` result type and lowers the range argument unchanged. The two differ only in the unit
  their indices count (bytes vs. code points), which is settled in the backend, so this slice does
  not distinguish them beyond the method name reaching codegen.
- **`.chars()`** — intercepted in `lower_method_call` ahead of every other dispatch and lowered
  to the prelude iterator's own `StructLiteral`: `Chars { source: &receiver, offset: 0 }`. No
  backend learns that `.chars()` exists. The borrow is emitted even for a temporary receiver,
  which is sound because an immutable borrow of a `string` IS the fat pointer by value.
- **`__char_at(offset)`** — `lower_builtin_method` types the prelude's decode step as
  `HirType::Char` with a `u64` argument. The semantic pass has already refused it to every
  module but the prelude's, so no gate is repeated here.
- **`.is_nan()`** — `lower_builtin_method` types it as `HirType::Bool` on a full-precision float
  receiver (`is_full_float`, so `f16`/`bf16` are excluded), with no arguments to lower.
- **`checked_{add,sub,mul}`** — `lower_builtin_method` types these on any integer receiver as
  `Option<T>` over that receiver, reusing `collections.rs`'s `option_of` so the instance is
  materialized as an ordinary `HirItem::Enum` exactly like `Vec::pop`'s. They are the only builtin
  intrinsics whose result is a monomorphized enum rather than a fixed type.
- **Interpolated strings** (`expressions/interpolation.rs`) — `Expr::InterpString` lowers to
  `HirExprKind::InterpString`, typed `string`. Each hole lowers with **no** expected type: the
  hole's own expression decides its type, and the rendering follows from that.

### Two guards against a wrong program
- **A surviving argument label is refused.** Lowering rejects an `Expr::Call` whose `arg_labels`
  is non-empty. Every label is bound to a parameter by `argument-binding` before type checking;
  one surviving to here would mean that pass never reached the call, and the arguments would then
  lower in the order they were *written* rather than the callee's — a wrong program rather than a
  failed build. The check costs one `is_empty` per call.
- **An operand the backend cannot lower is refused** (`LoweringError::UnsupportedOperand`,
  BUG-015). `binary_result_type` (`expressions/coercion.rs`) asks `has_operator_lowering` whether
  the operand type has an instruction sequence: the scalars, `string`, a `&string` slice, and a
  newtype forwarding one of those. The checker rejects a concrete aggregate operand itself; the
  path that reaches here is a **generic body**, which types `a == b` as its type parameter and is
  checked once as a template, so an instantiation with an aggregate argument used to arrive at
  codegen and abort it. An operator-trait impl is dispatched to a method call before operand types
  are ever combined, so a struct with `impl PartialEq` never reaches the check — and a struct with
  `@derive(PartialEq)` is returned before it, for the same reason.
