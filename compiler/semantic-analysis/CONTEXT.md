# semantic-analysis

## Purpose
Validate type correctness and scope rules of a parsed Neuro program before code generation.

## Entry Point
- Type: Library function
- Input: `items: &[Item]`
- Output: `Result<Vec<Warning>, Vec<TypeError>>` — `Ok` carries non-fatal lint warnings, `Err`
  carries fatal type errors. Warnings are dropped when errors are present.

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- ast-types — read-only traversal of `Item`, `Expr`, `Stmt` nodes
- shared-types — `Span` embedded in every `TypeError` for diagnostic location
- diagnostics — error type infrastructure

## Notes
Fail-slow: all type errors collected in one pass so the developer sees the complete set per
compilation. `syntax-parsing` is `[dev-dependencies]` only (integration tests), not production.

Multi-pass `check_program` — numbered passes below, with lettered sub-passes slotted between them
(0z/0a/0/1b/1c/1d/2b) as later requirements landed; see the pass table in
`docs/compiler/components/semantic-analysis.md` for the full ordering and its rationale.

0z. `check_reserved_names` rejects any declared name (function, param, struct, field, enum,
   variant, trait, method, const, newtype) containing `__` with `ReservedNameSeparator`. `__` is
   the receiver/method separator in the flat function table and the backend splits method symbols
   on it, so a user name carrying its own `__` could forge another item's symbol. Runs first, before
   anything mangles.
1. Pre-register all `Item::Struct` into `struct_defs` (and `@derive` Copy/Clone intent into
   `copy_structs`/`clone_structs`). Pass 1b runs `validate_copy_derive` per struct once all are
   registered (so a Copy field that is another struct resolves regardless of order).
2. Pre-register all `Item::Impl` method signatures into `functions` (mangled `StructName__methodName`)
   and `impl_methods` (struct → method → mangled key). An `impl Drop for T` block (`trait_name ==
   "Drop"`) is additionally validated by `register_drop_impl`: it must hold exactly the
   destructor `drop(&mut self)` (no params, no return, else `InvalidDropImpl`) and `T` must not be
   `Copy` (else `DropTypeCannotBeCopy`). The backend recomputes the Drop-type set from the AST, so no
   Drop state is kept on the checker.
3. Pre-register all `Item::Const` names/types into `constants` (forward refs + cross-function
   visibility, no ordering constraint).
3b. Pre-register every `Item::Function` signature via `register_function_signature` — parameter
   and return types resolved in the function's generic scope, then recorded in `functions` (or
   `generic_funcs` for a template), along with the duplicate-name check. Runs over all functions
   before any body is checked, so a call resolves regardless of source order and mutually recursive
   functions can name each other. `check_function` reads the signature back
   (`lookup_registered_signature`) rather than resolving it a second time.
4. Full-check: `check_function` / `check_impl` / `check_const_item`.
5. Lint pass: `run_lints` walks bodies collecting non-fatal `Warning`s. Currently
   `prefer-loop-over-while-true`, silenced by `@allow(prefer_loop_over_while_true)`. Lints run
   independently of type errors (warnings still collected for tests inspecting the checker, but
   dropped from the final `Err`).

Struct types are nominal — two `Type::Struct` are compatible iff names match.

`check_impl` binds `self` as a var of the struct type — **mutable for `&mut self`**, immutable for
`&self` — then the remaining params, before checking the body. A `&mut self` body may therefore
assign to `self.field`.

Method calls (`instance.method(args)`) — recognised in `check_expr` when the `Call`'s `func` is a
`FieldAccess`; the object's struct type drives an `impl_methods` lookup for the mangled name, then
arity/arg types are validated (skipping param[0] = `self`). When the resolved method is in
`mut_self_methods` (a `&mut self` receiver), `check_mut_self_receiver` enforces the exclusive borrow:
the receiver must be a `mut` place (or reached through `&mut T`) and must not already be borrowed —
the same coexistence rule as a `&mut place` borrow — registering a transient exclusive borrow that
clears at statement end. A `&T` receiver or a non-`mut` binding is `CannotBorrowMutably`; a live
borrow is `CannotMutablyBorrowWhileBorrowed`.

Associated calls (`TypeName::func(args)`) — recognised when `func` is an `Expr::Path`; mangled name
`TypeName__funcName` looked up directly in `functions`.

Builtin method dispatch: for a non-struct (primitive/string) receiver, `resolve_builtin_method`
checks a fixed compiler-known set before `MethodNotFound`, returning the result type (and an arity
diagnostic on wrong count). Intrinsics: `string.len() -> u64`, `string.clone() -> string` (
nullary); `string.slice(a..b) -> &string` (`check_string_slice`: one `Expr::Range` arg with
integer bounds, else `SliceExpectsRange`); and on any integer receiver `wrapping_{add,sub,mul}`,
`saturating_{add,sub,mul}`, `.shr(n)` — each one same-typed arg
(`check_unary_int_intrinsic_arg`), returns the receiver type. `checked_{add,sub,mul}` take the same
argument but return `Option<T>` over the receiver type, instantiated through the shared
`option_of` (`collections.rs`) so the overflow-reporting intrinsics and the fallible collection
readers materialize the same prelude enum instance; a program without an `Option` in scope gets
`UnknownTypeName`. A bare `Expr::Range` outside a `.slice` argument is `RangeNotAllowed`.
A struct receiver's `.clone()` is a nullary builtin when the struct derives `Clone`/`Copy` and
no user `clone` method exists (user method shadows); returns the struct type.

Panic-family builtins: `check_plain_call` consults `resolve_panic_builtin` before ordinary
resolution, only when no user function of the same name is registered (user `func panic(...)`
shadows). Builtins: `panic(msg: string)`, `assert(cond: bool)`, `unreachable()`; each validates
arity/type (`ArgumentCountMismatch`/`Mismatch`) and returns `Type::Unknown` — not `Void`, because the
call **diverges** (aborts) and must satisfy any context (unit stmt, non-`void` tail return, value
binding) until a dedicated `!`/never type lands. Lowering lives in `llvm-backend`.

Standard-output builtins: `check_plain_call` consults `resolve_io_builtin`
(`expressions/builtins.rs`) after the panic family and under the same shadowing rule.
Builtins: `print(text: string)`, `println(text: string)`. Both return `Type::Void` — they
**return**, unlike the panic family, so the result is the real unit type and cannot stand
in for a value. The argument is an owned `string` or an immutable `&string` (the same fat
pointer, which is what `.slice(range)` yields); a `&mut string` is a pointer to the fat
pointer and is rejected as a `Mismatch`. No move is recorded: the text is read, not
consumed, so the binding stays usable. Lowering lives in `llvm-backend`.

Consuming `self` methods are rejected at registration with `UnsupportedSelfParam` (they need the
by-value struct ABI). `&mut self` is supported: `register_impl` records its mangled key in
`mut_self_methods` for the call-site borrow check above.

Move-by-default ownership (`type_checkers/moves.rs`): a non-`Copy` value is *moved* out of its
source binding when placed into a new owner — `val`/`mut` initializer, assignment RHS, `return`,
struct-field assignment value, or by-value call argument. `record_move` marks the source moved (only
when the consumed expr is a bare place identifier of a move-tracked type;
`TypeChecker::is_type_move_tracked` returns true for `Type::String` and any `Type::Struct` not
deriving `Copy`, via `copy_structs`). Reading a moved binding emits `UseOfMovedValue` (with the
original move span) from the `Expr::Identifier` arm. `SymbolInfo.moved_at: Option<Span>` holds
per-binding state; reassigning a `mut` clears it. `.clone()` borrows (no move) — the canonical
opt-out. Conservative: `if`/`while`/`for` bodies and if-expr arms snapshot/restore move state
(`snapshot_moves`/`restore_moves`) so a conditional move never leaks onto a non-executing path. May
miss some moves (e.g. second-iteration loop moves) but never rejects a valid program.

Every expression is checked exactly once, and the trailing bare expression of a non-void body is
the case that has to be arranged deliberately: it is skipped in `check_function`'s statement loop
and checked afterwards with the declared return type as its expected type. Checking it in both
places re-ran its effects — a by-value argument was recorded as moved twice, and the second read
then reported a use of the value the expression had moved itself — and duplicated any diagnostic
the tail produced. The method loop in `declarations.rs` follows the same rule.

A trailing `if`/`else` is a value too, at every depth. An `if` in statement position parses to
`Stmt::If`, never `Stmt::Expr(Expr::If)`, so `check_block_expr_type` matches a trailing `Stmt::If`
carrying an `else` and routes it through `check_if_expr`. Without that, an `if` written as the last
thing inside an if-branch or a bare block typed as `void`, which made
`val r = if a { x } else { if b { y } else { z } }` a spurious mismatch.

An `if`/`else` in value position carries its context into its arms, mirroring `check_match`
exactly: the arm-type hint is the caller's expected type when there is one, else the first arm's
type once known. Without it the arms were typed against nothing, so an arm naming no type of its
own — a bare `None`, an untyped integer literal — resolved against nothing even when the `val` it
initialized was annotated, and `if`/`else` disagreed with the `match` spelling of the same
computation. `check_bare_block_expr`, `check_unsafe_block_expr`, and `check_block_expr_type` thread
the same expected type down to the tail expression.

Borrow exclusivity (`symbol_table.rs` + the `Expr::Reference` arm): each binding tracks
borrows taken *against its place* — persistent counts (a borrow held by a reference binding via
`val r = &x`) plus transient counts (a borrow passed to a call, used in a condition, or returned).
`borrow_counts` sums them; at a `&place` site the `Expr::Reference` arm rejects a `&mut` while any
borrow is live (`CannotMutablyBorrowWhileBorrowed`) and a `&` while a `&mut` is live
(`CannotBorrowWhileMutablyBorrowed`); any number of shared borrows may coexist. A direct
`&place` / `&mut place` initializer is promoted to a persistent borrow held by the new binding
(`attach_borrow`), released when that binding leaves scope (`pop_scope`); reassigning a `mut`
reference releases its old borrow first (`release_borrow_of`). Transient borrows are dropped at the
end of every statement (`clear_transient_borrows`), so a borrow never outlives the statement that
took it. Lexical, not NLL: only direct-borrow initializers create tracked persistent borrows, so the
analysis never rejects a valid program — it may miss borrows that escape through compound
expressions. Read/move-while-borrowed is not yet checked (it awaits full lifetime inference).

Returned-reference outlives (lifetime elision; `declarations.rs` + `statements.rs`): a function
or method whose declared return type is a `Type::Reference` must not return a reference borrowing a
place that dies with the call. `current_fn_outliving: HashSet<String>` holds the names that outlive
the call — reference-typed parameters (single-input elision applies the input lifetime to outputs)
plus `self` for an instance method (the `&self` lifetime is applied to method outputs). It is rebuilt
per function/method and cleared on exit. At each `return` and trailing implicit-return whose type is a
reference, `check_returned_reference` walks the returned expression: a `&place` whose root place
(`root_place_name`, peeling parens/field-access/deref) is local emits `ReturnsReferenceToLocal`; a
returned reference *binding* is flagged when its `borrow_provenance` (the place a `val r = &x`
initializer recorded) is local; `if`/`else` arms and bare/`unsafe` blocks are followed into their tail
expressions. `is_local_to_function` treats a name as local when it is a live binding absent from the
outliving set, and conservatively treats an absent name (constant, out-of-scope place) as non-local so
a valid program is never rejected. Elision-only: no annotation syntax, and ambiguous multi-reference
signatures are accepted as long as the borrowee is a parameter (explicit `<'a>` lands with generics).

Const declarations (`const NAME: Type = expr`): `constants: HashMap<String, Type>` holds both
module-level and body consts. `is_const_expr` validates the RHS (literals, arithmetic on literals,
casts, identifiers referring to other known consts). Body `Stmt::Const` validated in `check_stmt`.
`Expr::Identifier` falls back to `constants` after the symbol table, so const names work in any
expression context.

## Recent Updates
- 2026-08-30: New public error `TypeError::VoidBinding` (BUG-016). A binding whose initializer
  has type `void` used to pass the checker and abort the backend, which can only answer it with
  an internal error because its value path has no representation for the absence of a value. The
  `Stmt::VarDecl` arm now rejects a `Type::Void` `final_ty` beside the existing `Type::Unknown`
  guard, and mirrors it — the error is recorded and the name is left undefined. Testing the
  binding's TYPE rather than its initializer's shape is what makes one check cover every
  spelling: a `void` call is only two of them, the others being an `if`, a `match`, a bare block,
  a `loop { break }`, and an explicit `: void` annotation. Statement position is untouched, so
  `println("hi")` on its own line still compiles; the backend's `InternalError` stays as the
  unreachable assertion it was meant to be.
- 2026-08-29: Standard-output builtins (2A) — `resolve_io_builtin` (`expressions/builtins.rs`)
  recognizes `print` / `println` in `check_plain_call` after `resolve_panic_builtin` and behind the
  same "no user function of this name" guard. One `string` / immutable `&string` argument, result
  `Type::Void`; wrong arity or a non-string argument reuse `ArgumentCountMismatch` / `Mismatch`. No
  new error variants, and no `record_move` — printing borrows its text.
- 2026-08-29: New public error `TypeError::MissingPartialEqImpl` (BUG-015). `==` / `!=` used to
  accept any two operands of compatible type, so a struct with no `impl PartialEq` — and equally an
  array, tuple, enum, collection or non-string reference — type-checked and then aborted the
  backend. `check_binary_expr`'s equality arm now asks the new `has_builtin_equality`
  (`expressions/operators.rs`) whether the operand type has equality without an impl: the scalars
  (half-precision included), `string` after `peel_string_ref`, and a newtype forwarding one of those
  (via `lookup_newtype_inner`, whose cycles are already rejected at registration). A `Type::Generic`
  answers yes — a generic body is checked once as a template, so the instantiation is `hir-lowering`'s
  to refuse. A struct operand is reported as the missing trait; every other operand reuses
  `InvalidBinaryOperator`, which is what the ordering comparisons already gave. The operator-trait
  dispatch above the arm is untouched, so an explicit `impl PartialEq` still compiles as before.
- 2026-08-29: New public error `TypeError::FunctionUsedAsValue` (BUG-013). `Expr::Identifier`
  resolution consults `functions` and `generic_funcs` before falling through to
  `UndefinedVariable`, so a function name in value position is told apart from a name that
  does not exist. No coercion was added — a function is still not a value.
- 2026-08-29: Growable `String`. `CollectionKind::String` is a fourth, **nullary** collection
  kind (`arity() == 0`), so `Type::Collection { kind: String, args: [] }` reuses every existing
  collection rule — never `Copy`, move-tracked, `Drop`-freed — with no new `Type` variant. Bare
  `String` resolves as a complete type in `resolution.rs` (the "collection needs type arguments"
  arm now applies only to `arity() > 0`, so a user-declared `struct String` still shadows it), and
  `check_collection_new` returns the type directly for a nullary kind rather than demanding an
  annotation. `collections.rs` gains `ParamSlot::Text` (accepts `string` or an immutable `&string`,
  and does not move it — the latitude `+` gives its operands) and `ResultShape::OwnedString`,
  backing `push_str` (mutating) and `to_string`; `len` / `clear` fall out of the existing
  kind-agnostic entries. `Type`'s `Display` omits `<>` for a nullary collection.
- 2026-08-25: String interpolation checking (`type_checkers/expressions/interpolation.rs`).
  Each hole's expression is checked, its type auto-dereferenced through a borrow, and its
  written spec validated against that type — radix kinds need an integer, fixed-point and
  scientific need a float, `+` needs a signed integer or float, zero fill cannot combine
  with `<`/`^`, and width/precision are bounded. The literal always types as `string`, so
  a rejected hole does not cascade. New errors: `UnformattableType`,
  `FormatSpecMismatch`, `FormatWidthTooLarge`, `FormatPrecisionTooLarge`.
- 2026-08-24: Return-path checking. A non-void function or method must produce a value on
  every path, and `check_function` / the `impl` method loop now say so with
  `TypeError::MissingReturn`. Two shared helpers in `declarations/functions.rs` state the
  rule once: `tail_is_implicit_return` recognises the implicit return — a trailing bare
  expression, or a trailing `if`/`else`, which the parser always shapes as `Stmt::If` and
  which was therefore never checked against the declared return type at all — and
  `check_implicit_return` checks it. An `if` whose every arm leaves the function
  (`val_else::stmt_diverges`, now `pub(crate)`) carries no value and is a statement, so the
  divergence check covers it instead. Without the rule the backend left the exit block
  without a return, LLVM terminated it with `unreachable` (a legal terminator, so the
  verifier stayed silent), and the program ran off the end of the function at runtime.
  `check_if_expr` is `pub(crate)` so the declaration modules can reach it.
- 2026-08-24: A parameter whose type failed to resolve is still bound, at `Type::Unknown`.
  `check_function` skipped defining it, which turned every use of it in the body into a second
  "undefined variable" report chasing an error already given. `Unknown` is compatible with
  everything, so binding it is what actually stops the cascade.
- 2026-08-24: An arm that LEAVES the scope contributes no type. `check_if_expr` routes every
  arm through `arm_value_type`, and `check_arm` checks `expr_diverges` on the arm body: a block
  ending in `return` / `break` / `continue` reports `Type::Unknown` instead of the `Void` its
  trailing statement gives it, so it neither supplies the expression's type nor has to match
  it. Without the rule `if n > 0 { return 1 } else { 2 }` was "expected void, found i32" — the
  diverging arm named as the EXPECTED type — while the same shape written with `panic` compiled,
  because the panic family was already `Unknown`. `val_else::expr_diverges` is now `pub(crate)`;
  the analysis itself is unchanged and still owned by `val_else.rs`, which needs it for the
  `else`-must-diverge rule.
- 2026-08-24: Divergent arms no longer decide an expression's type. `check_if_expr` and
  `check_match` take the result from the first arm that is not `Type::Unknown`, and compare
  every arm against it. A `panic` / `unreachable` arm is `Unknown` — the
  compatible-with-everything type — so taking it made the whole expression untyped and its
  binding vanish, purely because of the order the arms were written in.
- 2026-08-22: Struct field visibility. A field is private to its declaring module unless it
  carries `export`, and this slice is where that is enforced — the rule needs the receiver's
  type, so module-resolution (which runs before type checking) cannot state it. `register_struct`
  / `register_generic_struct` record each struct's `module` and its private field names;
  `instantiate_generic_struct` copies both onto every monomorphized instance. `current_module`
  is set from the item being checked in pass 4, and `reject_private_field` compares the two at
  the four places a field is reached: a read (`check_field_access_expr`, which also covers
  struct destructuring — the parser desugars it into field reads), a write
  (`Stmt::FieldAssignment`), and a literal's listed fields (plain and generic).
  `reject_private_update` covers `..base`, which supplies every *unlisted* field and so would
  otherwise copy private ones out. New `TypeError::PrivateField`. Nothing else in the checker
  reads `current_module`; a single-file program is one module, so the rule is inert there.
- 2026-08-18: `import` declarations. Nothing about imports reaches the checker — module-resolution
  consumes every `Item::Import` and rewrites every name it bound — so the new arms are no-ops. The
  one exception is `Pattern::UnqualifiedEnum`, which the resolver rejects when no import accounts
  for it; reaching the checker means the resolver did not run, reported as the new
  `TypeError::UnimportedVariantPattern` and contributing no exhaustiveness coverage.
- 2026-08-03: `?` error propagation. New `expressions/try_expr.rs`: `check_try_expr` types
  `Expr::Try` as the operand's success payload after two checks. The operand must be fallible —
  resolved through the new `fallible_kind` helper (`expressions/operators.rs`, the shared form of
  `fallible_payload`, so `?` and `??` accept exactly the same set of types); anything else is the
  new `TryOnNonFallible`. And `current_function_return_type` must be an instance of the SAME
  fallible enum, since that is where the failure goes; otherwise the new
  `TryOutsideFallibleFunction` (which also covers propagating an `Option` out of a `Result`
  function — the two do not convert). For `Result`, the operand's `Err` payload must already equal
  the function's, reported as an ordinary `Mismatch`: the spec forwards the error with no implicit
  `.into()`, so `.map_err(...)` is the explicit conversion path. Success payloads are unconstrained
  — only the error types must agree.
- 2026-07-31: `val-else`. New `val_else.rs`: `check_val_else` checks the scrutinee, runs the
  pattern through `matches.rs`'s `check_pattern` (now `pub(crate)`), checks the `else` branch in its
  own scope, and only THEN defines the pattern's bindings in the enclosing scope — so the branch
  cannot see bindings its own failure means were never produced. `else_binding_type` implements the
  documented binding table by resolving the scrutinee through `enum_instance_base`: `Result` binds the `Err`
  payload, `Option` is the new `ValElseBindingOnOption` (its failure variant is empty; `|_|` and an
  omitted form are filtered out before the check), any other type binds the scrutinee itself. A
  local `stmts_diverge` walk (return / break / continue, `panic` / `unreachable` calls, an
  if/else or `match` whose every branch diverges) enforces the new `ValElseMustDiverge`.
- 2026-07-28: `??` full implementation. `check_binary_expr` routes `BinaryOp::NullCoalesce` to
  `check_null_coalesce` (`expressions/operators.rs`) BEFORE the shared operand check, because the
  operator is not operand-symmetric: the right side is typed by the left's *payload*, not by the
  left. `fallible_payload` resolves the left type to an `Option`/`Result` instance through
  `enum_instance_base` (a shadowing non-generic declaration is its own base) and returns the
  `Some`/`Ok` slot-0 type; anything else is the new `NullCoalesceOnNonFallible { found, span }`.
  The `Result` error payload is deliberately unconstrained — `??` discards it. A mistyped fallback
  is an ordinary `Mismatch`. `OperatorNotYetSupported` is deleted (it had no other user).
  `OPTION_ENUM` in `collections.rs` is now `pub(crate)`, shared with the operator rule.
- 2026-07-28: Divergent `loop`, and expression/declaration module split.
  `LoopContext` gains `has_break`, set by `record_break_target` from the `Stmt::Break` arm, and
  `check_loop_body` returns `LoopExit { value_ty, has_break }`. `Expr::Loop` now yields its agreed
  value-break type; unit when only plain `break`s target it; and the *expected* type when no `break`
  targets it at all — such a loop never reaches its exit, so it satisfies any context, the same
  divergent contract the panic-family builtins carry. This keeps
  `func f() -> i32 { loop { ... return x } }` valid now that a trailing `loop` is checked as the
  implicit return. `Stmt::Loop` handling is gone (the node no longer exists); the `while true` lint
  descends through `Stmt::Expr(Expr::Loop)`.
  `type_checkers/expressions.rs` is now `expressions/` (`mod.rs` holds the `check_expr` dispatch;
  `calls`, `enum_exprs`, `struct_exprs`, `operators`, `blocks`, `places`, `sequences`, `builtins`,
  `const_predicates` hold the rest) and `declarations.rs` is now `declarations/` (`mod.rs` holds the
  reserved-name pass, generic scope, and generic unification/substitution; one module per
  declaration kind). `tests/` is likewise split by subject. Behaviour is unchanged by both splits.
- 2026-07-27: Standard collections. New `Type::Collection { kind, args }` + `CollectionKind`
  (`Vec` / `HashMap` / `BTreeMap`) — a compiler-known nominal type, never `Copy` and always
  move-tracked (`is_type_copy` / `is_type_move_tracked`). The new `type_checkers/collections.rs`
  owns the rules: `resolve_collection` resolves `Vec<T>` / `HashMap<K, V>` / `BTreeMap<K, V>` from
  the generic-application arm of `resolve_type`, validating storable elements (`Copy` or `string`)
  and map keys, and a program declaring its own generic type of that name shadows the builtin.
  `check_collection_new` types `Vec::new()` from the expected type (else
  `CollectionTypeNotInferable`); `resolve_collection_method` types the method surface, requiring a
  mutable receiver for the mutating half and taking ownership only of stored arguments (a lookup key
  is read, like a `==` operand); fallible readers instantiate the prelude `Option<T>`. Raw float keys
  are rejected toward `OrderedF32` / `OrderedF64`; a struct key requires `impl PartialEq` plus
  `impl Hashable` (hashed) or `impl Comparable` (ordered). `Hashable` joins `Drop` and the operator
  traits as a lang-item — `register_hashable_impl` enforces the single `hash(&self) -> u64` method.
  Indexing, index assignment, and `for`-in accept a `Vec` alongside an array.
  `check_mut_self_receiver` is now `pub(crate)`. New errors: `CollectionTypeNotInferable`,
  `InvalidCollectionElement`, `InvalidCollectionKey`, `InvalidHashableImpl`.
- 2026-07-27: Standard collections. New `Type::Collection { kind, args }` + `CollectionKind`
  (`Vec` / `HashMap` / `BTreeMap`) — a compiler-known nominal type, never `Copy` and always
  move-tracked (`is_type_copy` / `is_type_move_tracked`). The new `type_checkers/collections.rs`
  owns the rules: `resolve_collection` resolves `Vec<T>` / `HashMap<K, V>` / `BTreeMap<K, V>` from
  the generic-application arm of `resolve_type`, validating storable elements (`Copy` or `string`)
  and map keys, and a program declaring its own generic type of that name shadows the builtin.
  `check_collection_new` types `Vec::new()` from the expected type (else
  `CollectionTypeNotInferable`); `resolve_collection_method` types the method surface, requiring a
  mutable receiver for the mutating half and taking ownership only of stored arguments (a lookup key
  is read, like a `==` operand); fallible readers instantiate the prelude `Option<T>`. Raw float keys
  are rejected toward `OrderedF32` / `OrderedF64`; a struct key requires `impl PartialEq` plus
  `impl Hashable` (hashed) or `impl Comparable` (ordered). `Hashable` joins `Drop` and the operator
  traits as a lang-item — `register_hashable_impl` enforces the single `hash(&self) -> u64` method.
  Indexing, index assignment, and `for`-in accept a `Vec` alongside an array.
  `check_mut_self_receiver` is now `pub(crate)`. New errors: `CollectionTypeNotInferable`,
  `InvalidCollectionElement`, `InvalidCollectionKey`, `InvalidHashableImpl`.
- 2026-07-26: Generic enums (`Option<T>` / `Result<T, E>`). New `TypeChecker` state:
  `generic_enums` (base name -> template) and `enum_instances` (instance name -> base + type
  arguments). Pass 0 routes an `EnumDef` with generics to `register_generic_enum`, which resolves
  the template's variants with the parameters in scope (a `Type::Generic` payload placeholder is
  exempt from the scalar-payload rule) and keeps them in `enum_defs` under the base name so
  construction sites can infer the arguments. `instantiate_generic_enum` monomorphizes per argument
  set — substituting each payload, re-checking the scalar restriction per instance
  (`Option<string>` is rejected), and registering the instance in `enum_defs` under the mangled
  nominal name `Base<Arg, ...>`. `resolve_type` instantiates a `Type::Generic` application whose
  name is a generic enum and rejects the bare name (`GenericEnumNeedsArgs`). The three construction
  checkers (`check_enum_unit_path`, `check_enum_tuple_call`, `check_enum_struct_literal`) now take
  the expected type: the instance comes from the expected type when there is one, else the payload
  is unified against the template and any parameter still unbound is taken from the enclosing
  function's return instance (`enum_return_type_args` — the only context a tail `if` branch has), else
  `GenericEnumNotInferable`. An enum pattern written with the base name matches the scrutinee's
  instance and binds payloads at the instance's concrete types. New errors: `GenericEnumNeedsArgs`,
  `GenericEnumNotInferable`; `GenericArgCountMismatch`'s message now says "generic type" (it covers
  enums too). `Option` / `Result` themselves are not special-cased anywhere here — `neurc` injects
  their declarations.
- 2026-07-24: Closures and lambdas. New `type_checkers/closures.rs`: `check_closure` types a `Expr::Closure` as `Type::Function { params, ret }` — parameters require an annotation (`ClosureParamNeedsType`), a block body requires an explicit return type and is checked like a function body (`ClosureBlockNeedsReturnType`), and a single-expression body infers its return type. Capture analysis (a free-variable walk) rejects capturing a non-Copy enclosing local (`ClosureCapturesNonCopy`) or assigning to a captured variable (`ClosureAssignsCapture`); module constants and functions are referenced directly, not captured. The body is checked with `current_function_return_type` redirected to the closure's return type so an early `return` binds to the closure. `check_plain_call` now dispatches a call on a local binding of function type (a closure or `(T)->U` parameter). `resolve_type` resolves `Type::Function`. The pre-existing `Type::Function` variant is now produced by real programs.
- 2026-07-19: Static & dynamic dispatch. Added `Type::DynObject(String)` (nominal trait object). `resolve_type` now delegates to a private `resolve_type_ctx(ty, behind_ref)`: the flag is set only by the `Reference` arm, so a bare `dyn Trait` is rejected (`DynTraitNotBehindReference`) while `&dyn Trait` resolves, after checking the trait is declared and object-safe. New `trait_object_safety` (every method must take `&self`/`&mut self`, else `TraitNotObjectSafe`), `type_implements_trait`, and `assignable(found, expected)` — the latter is ordinary compatibility PLUS the single implicit `&T` -> `&dyn Trait` unsizing coercion, and now backs the call-argument, return, and annotated-binding checks. Return-position `impl Trait` resolves transparently in `check_function` via `resolve_impl_return`, which reads the concrete type structurally from the body's result expression (`shallow_result_type`: struct literal, enum value, newtype construction, or a block/`if` tail) and verifies it implements the trait; callers therefore see the concrete type at zero cost. A method call on a `DynObject` receiver types against the trait's declared signature. New errors: `DynTraitNotBehindReference`, `TraitNotObjectSafe`, `ImplTraitNotAllowedHere`, `ImplReturnNotInferable`, `ImplReturnDoesNotImplement`.
- 2026-07-18: Operator traits — scalar path. Operator traits (`Add`, `Sub`, `Mul`, `Div`,
  `Rem`, `Neg`, `Not`, `BitAnd`, `BitOr`, `BitXor`, `Shl`, `PartialEq`, `Comparable`) are
  compiler-known lang-items (like `Drop`) defined in `type_checkers/operator_traits.rs`; the user
  writes only the `impl`. New `TypeChecker` state: `operator_binary_impls` (`(struct, BinaryOp)` →
  `OperatorDispatch { rhs, result }`) and `operator_unary_impls` (`(struct, UnaryOp)` → result type).
  `register_impl` routes an operator-trait impl to `register_operator_impl` (instead of
  `check_trait_conformance`): the receiver must be `Copy` (`OperatorTraitRequiresCopy`), a declared
  `type Output` must equal the method return (`AssociatedTypeMismatch`), and each operator's method
  wires its result type. Owned `self` is now accepted on a `Copy` struct (ABI-identical to `&self`);
  it stays rejected on a non-`Copy` type. A new pass 2b (`check_operator_supertraits`) enforces
  `Comparable: PartialEq` order-independently (`MissingSupertraitImpl`). In `check_expr`, a binary /
  unary operator whose (peeled) left/operand type is a struct with a matching entry takes the impl's
  result type before the built-in numeric/comparison paths (which still reject other struct operands
  with `InvalidBinaryOperator`). Fully erased — HIR desugars each operator to the method call. Not
  yet: the dedicated in-place `*Assign` traits (compound assignment goes through the parse-desugar to
  the by-value operator), `MatMul`/`@`, and auto-derived trait default methods (each operator needs
  its own impl method).
- 2026-07-16: Trait declarations. New `TypeChecker` state: `traits` (name → `TraitInfo` of
  resolved method signatures), `trait_impls` (set of `(trait, type)` pairs with an impl), and
  `generic_bounds` (type-parameter → bound trait names, live inside a generic definition). A new
  pass 1d (`register_trait`) runs before impl registration. `register_impl` calls
  `check_trait_conformance` for any non-`Drop` trait impl: every required method present
  (`MissingTraitMethod`), each impl method a trait member (`NotATraitMethod`) with a matching
  signature (`TraitMethodSignatureMismatch`), or `UnknownTrait` for an undeclared trait. Method
  dispatch (`check_expr`) resolves `obj.m()` on a bounded type parameter via
  `resolve_generic_trait_method`; generic call sites enforce bounds via `check_trait_bounds`
  (`TraitBoundNotSatisfied`), keyed off `GenericFnSig.bounds`. Traits are fully erased — the parser
  injects default methods into impls, so they check as ordinary methods.
- 2026-07-13: Explicit lifetime annotations. New `lifetime_scope: HashSet<String>` on the
  `TypeChecker`, populated by `enter_generic_scope` (now takes a `lifetimes: &[Identifier]`
  argument) from each definition's `lifetimes` field and cleared by `exit_generic_scope`. In
  `resolve_type`, a `Type::Reference` carrying an explicit lifetime is validated against
  `lifetime_scope`; an unknown name records the new `TypeError::UndeclaredLifetime`. The lifetime
  is then erased — the semantic `Type::Reference` is unchanged, so `&'a T` and `&T` are the same
  type. No new outlives logic: the returned-reference check (v1.40.0) already accepts returning a
  borrowed parameter, which is exactly the `longest<'a>` case.
- 2026-07-06: Generic structs & impls. Generic structs are monomorphized by name-mangling:
  a generic `StructDef` is stored in `generic_structs` and its placeholder-typed fields are also
  kept in `struct_defs` under the base name so generic-`impl` method bodies check abstractly
  (like a generic function body); the bare name is rejected via `GenericStructNeedsArgs`. A generic
  `impl` (`impl<T> Wrapper<T>`) is stored in `generic_impls`; its method signatures register under
  the base. `instantiate_generic_struct` (called from `resolve_type` for a `Type::Generic`
  annotation and from `check_generic_struct_literal` after inferring the arguments from field
  values) materializes a distinct nominal `Type::Struct("Base<args>")` — concrete fields
  (`substitute_generic`) and per-instance methods (`remap_method_type`, renaming `Struct(base)` →
  the instance) registered on demand — so downstream field access / method dispatch reuse the
  ordinary struct machinery. Type arguments are `Copy`-restricted. New errors:
  `GenericStructNeedsArgs`, `GenericArgCountMismatch`, `NotAGenericType`, `NestedGenericTypeArg`
  (a generic instantiated with an enclosing type parameter is deferred).
- 2026-07-03: Generic functions. New `Type::Generic(String)` (a nominal type-parameter
  placeholder, compatible only with itself). A generic `FunctionDef` is registered in a
  `generic_funcs` table (signature carrying `Generic` placeholders + the ordered parameter names),
  NOT in `functions`; `generic_scope` puts its parameters in scope so `resolve_type` maps their
  names to `Generic`. Generic bodies are checked once abstractly, so only type-agnostic operations
  type-check (no bounds/trait system yet). `check_generic_call` infers each type argument by
  unifying declared parameter types against argument types (`unify_generic`), validates arity and
  the `Copy`-argument restriction, and returns the substituted return type (`substitute_generic`).
  New errors: `GenericParamShadowsBuiltin`, `GenericParamNotInferable`, `GenericArgumentNotCopy`.
- 2026-07-02: Newtype declarations. New `Type::Newtype(String)` (distinct nominal, NOT compatible
  with its inner type) plus a `newtype_defs` name→inner table. Passes: `predeclare_newtype` reserves
  each name (rejecting builtin/struct/enum/newtype collisions via `NewtypeAlreadyDefined`), then
  `resolve_newtype_inners` resolves inner types once all nominal names are known and rejects cyclic
  (`CyclicNewtype`) and non-Copy (`NewtypeInnerNotCopy`) inners — inner is restricted to Copy types this
  phase, so a newtype forwards Copy. Construction `Name(value)` is handled in `check_plain_call` (one
  inner-typed arg); `.0` on a newtype yields the inner type in the `TupleIndex` check. New `TypeError`s:
  `NewtypeAlreadyDefined`, `NewtypeInnerNotCopy`, `CyclicNewtype`.
- 2026-07-02: Pattern matching (`type_checkers/matches.rs`). `check_match` types the scrutinee
  (restricted to enum / integer / `char` / `bool`), checks each arm's patterns against it, introduces
  pattern bindings into a per-arm scope for the guard and body, unifies arm-body types (first arm drives
  literal inference), and verifies exhaustiveness (enum variant coverage, both `bool` values, or a `_`
  catch-all; guarded arms never count). New `TypeError`s: `NonExhaustiveMatch`, `UnsupportedMatchScrutinee`,
  `PatternTypeMismatch`, `MatchArmTypeMismatch`, `InvalidRangePattern`, `VariantPatternFormMismatch`,
  `OrPatternBinding`, `RefutablePayloadPattern`. `check_returned_reference` recurses into arm bodies.
- 2026-06-30: Enums with associated data. New `Type::Enum(String)` (nominal, `Copy`) and an
  `enum_defs` table (name → `Vec<EnumVariantInfo>` with `VariantForm` + resolved fields), registered
  in a pre-pass before structs. `resolve_type` resolves an enum name; `register_enum` rejects
  duplicates and non-scalar payloads (`UnsupportedEnumPayload` — payloads limited to scalar Copy
  primitives this phase). Construction type-checking: `E::V` (Path) → unit, `E::V(..)` (Call→Path) →
  tuple, `E::V { .. }` (`EnumStructLiteral`) → struct, with arity/field/form diagnostics
  (`UnknownEnumVariant`, `EnumVariantFormMismatch`, `EnumVariantArityMismatch`,
  `Unknown/Missing/DuplicateEnumField`, `EnumAlreadyDefined`).
- 2026-06-29: Struct + array destructuring. `check_expr` handles `Expr::ArrayRest { array, start,
  exact }`: the source must be an array `[T; N]`; the result is the `[T; N - start]` remainder. `exact`
  (a rest-less array pattern) requires `N == start`; otherwise `start <= N`. New diagnostic
  `ArrayPatternLengthMismatch { expected, found }`. Struct/array destructuring otherwise reaches
  semantic analysis as ordinary field-access / index bindings (parser-desugared).
- 2026-06-28: Tuples. New `Type::Tuple(Vec<Type>)` (compatible only on equal arity with each
  element matching; `(T1, T2, ...)` Display). `resolve_type` resolves the tuple type and rejects a
  non-Copy element (`NonCopyTupleElement`). `check_expr` handles tuple literals (each element checked
  against the expected tuple's element type when annotated; non-Copy element rejected) and tuple
  indexing `t.N` (`NotATuple` on a non-tuple, `TupleIndexOutOfBounds` past the arity). A tuple is
  `Copy` exactly when every element is. Destructuring `val (a, b) = e` is desugared in the parser, so
  it reaches semantic analysis as ordinary bindings. New diagnostics: `NonCopyTupleElement`,
  `NotATuple`, `TupleIndexOutOfBounds`.
- 2026-06-19: Arrays. New `Type::Array { element, size }` (compatible only on equal element type
  and length; `[T; N]` Display). `resolve_type` resolves `[T; N]` and rejects a non-Copy element
  (`NonCopyArrayElement`). `check_expr` handles array literals (homogeneous, length vs annotation) and
  indexing (`NotIndexable` / `IndexNotInteger`); `array.len()` resolves to `u64`. `Stmt::ForEach` binds
  the element type; `Stmt::IndexAssignment` requires a mutable array target. Arrays are `Copy` when the
  element is. New diagnostics: `NotIndexable`, `IndexNotInteger`, `ArrayLengthMismatch`,
  `CannotInferEmptyArray`, `NonCopyArrayElement`.
- 2026-06-17: Returned-reference outlives / lifetime elision. New `current_fn_outliving:
  HashSet<String>` on `TypeChecker` (reference params + `self`), rebuilt in `check_function` /
  `check_impl` and cleared on exit. New `SymbolTable::borrow_provenance`. New
  `check_returned_reference` (+ free fns `tail_expr` / `root_place_name`, method
  `is_local_to_function`) invoked from `Stmt::Return` and both trailing implicit-return sites when the
  return type is a `Type::Reference`. New error `ReturnsReferenceToLocal`. Elision-only — no annotation
  surface; explicit `<'a>` awaits generics. Tests in `type_checkers/tests/mod.rs`.
- 2026-06-16: Borrow exclusivity. `SymbolInfo` gained persistent/transient borrow counters
  plus a `borrows` provenance; new `SymbolTable` methods `borrow_counts` / `add_transient_borrow` /
  `attach_borrow` / `release_borrow_of` / `clear_transient_borrows`, and `pop_scope` now releases a
  dying reference binding's borrow. The `Expr::Reference` arm checks coexistence and registers the
  borrow as transient; `check_stmt` wraps `check_stmt_inner` to clear transient borrows at statement
  end; `VarDecl` / `Assignment` promote a direct `&place` initializer to a persistent borrow. New
  errors `CannotMutablyBorrowWhileBorrowed` / `CannotBorrowWhileMutablyBorrowed`. Tests in
  `type_checkers/tests/mod.rs`.
- 2026-06-16: `f16`/`bf16` half-precision primitives. New `Type::F16`/`Type::BF16`: `"f16"`/`"bf16"`
  resolve in `resolve_type`; the `FloatSuffix::F16`/`BF16` literal suffixes infer to them; `is_half_float()`
  added. Narrow contract — Copy (not move-tracked), `==`/`!=` via the compatible-type path, `as`-cast
  to/from any numeric type and to/from each other (`is_valid_cast` half arms), but **no arithmetic**:
  `+ - * / %` on a half operand emits `TypeError::HalfFloatArithmetic` ("compute in f32"). `is_float()`
  deliberately still excludes halves so arithmetic/inference paths skip them.
- 2026-06-15: `char` primitive type. New `Type::Char`: `"char"` resolves in `resolve_type`;
  `Literal::Char` infers to `Type::Char`; `is_valid_cast` permits char↔integer and char→char only
  (no float/bool); ordering comparisons (`< > <= >=`) accept `char` (built-in total order) alongside
  numerics; char is Copy (not move-tracked). Equality reuses the existing compatible-type path.
- 2026-06-15: `loop` as a value expression. `loop_labels` became `loop_stack:
  Vec<LoopContext>` (label + `is_value_loop` + accumulated `break_value_ty`). `check_loop_body`
  pushes a context for while/loop/for; only `loop` (and `Expr::Loop`) is a value loop. A value
  `break v` calls `record_break_value`: it rejects a value targeting a `while`/`for`
  (`TypeError::BreakValueInUnitLoop`), sets the loop's type on first value-break, and reports
  `Mismatch` on a disagreeing later one. `Expr::Loop` returns the agreed type (or unit).
- 2026-06-15: Loop labels. The `loop_depth: u32` counter is replaced by `loop_labels:
  Vec<Option<String>>` (innermost last). Each loop pushes its label (or `None`) for the duration of
  its body; `check_loop_control_label` validates `break`/`continue`: an unlabeled one needs a
  non-empty stack (else `BreakOutsideLoop` / `ContinueOutsideLoop`), a labeled one needs a matching
  active label (else the new `TypeError::UndefinedLabel`).
- 2026-06-09: `loop { ... }` statement. `check_stmt` handles `Stmt::Loop` like `while`'s body
  (increments `loop_depth` so `break`/`continue` inside are in-loop; snapshot/restore moves around
  the body per), minus the condition. The `prefer-loop-over-while-true` lint walker recurses
  into `loop` bodies. No new error code — the construct is unconditionally valid.
- 2026-06-09: Mutable borrows `&mut T` + deref `*`. `Type::Reference` is now
  `{ inner, mutable }` (Display `&mut T`; compatible only when mutability **and** referents
  match — no `&mut T`→`&T` coercion). `resolve_type` carries `mutable` through. The
  `Expr::Reference` arm rejects `&mut` of a non-`mut` binding (`CannotBorrowMutably`). New
  `Expr::Deref` arm: types `*r` to the referent, else `CannotDereference`. New
  `Stmt::DerefAssignment` checker: requires `pointer: &mut T`, else `CannotAssignThroughRef`
  (immutable ref) / `CannotDereference` (non-ref); the stored value is checked against the
  referent and move-recorded. New errors `CannotBorrowMutably` / `CannotDereference` /
  `CannotAssignThroughRef`. Unit tests in `types.rs` and `moves.rs`. Flow-sensitive aliasing
  exclusivity is deferred to lifetime inference.
- 2026-06-18: String concatenation `+`. The arithmetic arm of `check_expr` now peels each
  operand with `Type::peel_string_ref` before the numeric path: when both are `string` and the op
  is `+`, the result is `Type::String` (a new owned string). Any other arithmetic op on a string,
  or mixing a string with a non-string, is `InvalidBinaryOperator`. Operands are not consuming
  positions, so `+` borrows-to-read and never moves (matching `==`). Unit tests in
  `tests/expr_tests.rs`.
- 2026-06-09: `&string` slice equality. `&string` is a borrowed string slice; the
  `Equal`/`NotEqual` arm of `check_expr` now compares operands through `Type::peel_string_ref`,
  which normalizes `&string` → `string` (one layer, string only) so an owned `string` and a
  `&string` slice are equality-compatible in any combination. Other `&T` are left intact, so
  `&i32 == i32` and `i32 == &string` stay type errors. Comparison operands are not consuming
  positions, so borrowing for `==` never moves. Unit test in `types.rs`.
- 2026-06-08: Immutable borrows `&T`. New `Type::Reference(Box<Type>)` (Display `&T`; compatible
  iff referents are; `referent()` peels one layer). `resolve_type` maps `ast::Type::Reference`.
  `Expr::Reference` arm in `check_expr` requires a place (`is_place_expr`: identifier or
  parenthesised identifier; else `CannotBorrowValue`) and yields `&T` **without** moving the operand —
  borrowing never consumes. References are always `Copy` and never move-tracked (`is_type_copy` true,
  `is_type_move_tracked` false via its `_` arm). Method-call and field-access resolution auto-deref via
  `obj_ty.referent()`, so `r.len()` / `r.field` / `r.method()` work for `r: &string` / `r: &Struct`.
- 2026-06-07: `Copy` trait + `@derive(Copy, Clone)`. `copy_structs`/`clone_structs`
  (`HashSet<String>`) populated from `StructDef.attributes` in `register_struct`
  (`record_derive_intent`); pass 1b `validate_copy_derive` checks every field of a Copy struct is Copy
  (`CopyDeriveNonCopyField`). `Type::is_move_tracked` replaced by context-aware
  `is_type_move_tracked`/`is_type_copy` (a struct is move-tracked unless it derives Copy). Struct
  `.clone()` resolves in the method-call arm when Clone/Copy-derived and no user `clone` exists. Copy
  implies Clone; unknown derive args ignored.
- 2026-06-07: Move semantics by default. New `moves.rs` (`record_move`); `SymbolInfo.moved_at` +
  `mark_moved`/`clear_moved`/`snapshot_moves`/`restore_moves`; `UseOfMovedValue`. Consuming positions
  in `statements.rs` (VarDecl/Assignment/Return/FieldAssignment) and `expressions.rs` (call-arg loops)
  record moves; the `Expr::Identifier` read arm reports use-after-move; conditional regions
  snapshot/restore. Tracked types limited to `string` initially.
- 2026-06-05: Struct functional-update in `Expr::StructLiteral`. With `base` present, the base is
  checked against `Type::Struct(name)` (mismatch → `Mismatch`) and the missing-field scan skipped.
  Shorthand needs no change (parser lowered it to `Expr::Identifier`).
- 2026-06-05: `string.clone() -> string` — `(Type::String,"clone")` arm in
  `resolve_builtin_method` (nullary; args → `ArgumentCountMismatch`). Mirrored in `llvm-backend`.
- 2026-06-04: Panic runtime — `resolve_panic_builtin` recognizes `panic`/`assert`/`unreachable`
  in `check_plain_call` before ordinary resolution (user funcs shadow); each returns `Type::Unknown`;
  wrong arity/type reuse `ArgumentCountMismatch`/`Mismatch`. No new variants.
- 2026-06-04: `Expr::Unsafe` type-checking (1C) — identical to `Expr::Block` (pushes a scope,
  yields the trailing expr's type). Inert.
- 2026-05-31: Integer primitive methods — `resolve_builtin_method` resolves
  `wrapping`/`saturating`/`.shr(n)` on integer receivers; `check_unary_int_intrinsic_arg` enforces
  arity 1 + compatible arg type.
- 2026-05-31: Builtin method dispatch on primitive/string — `resolve_builtin_method` in
  `expressions.rs`; the `Call`→`FieldAccess` arm consults it before `MethodNotFound`. First:
  `string.len() -> u64`.
- 2026-05-27: Comparison chain rejection — `check_expr` emits `ComparisonChain` when a
  comparison's LHS is itself a comparison (all six ops). Uses `BinaryOp::is_comparison()` (ast-types).
- 2026-05-25: Float literal suffixes — `infer_suffixed_float_type` maps `FloatSuffix` →
  `F32`/`F64`; mismatched annotations surface via the assignment type-check path.
- 2026-05-20: Lint infra — `Warning`/`WarningCode` (`warnings.rs`); `run_lints` final pass; first
  lint `prefer-loop-over-while-true` (`while true`, suppressed by `@allow(...)`; parenthesised
  `while (true)` deliberately not matched). Public signature now `Result<Vec<Warning>, Vec<TypeError>>`.
- 2026-05-13: IEEE-754 native float comparison — inequalities (`<`,`>`,`<=`,`>=`)
  restricted to `is_numeric()`, rejecting struct/string/bool (prevents codegen panics). NaN handled
  natively via LLVM `fcmp`.
- 2026-04-18: Integer literal suffixes — `infer_suffixed_integer_type` via `suffix_to_type` +
  range check (`IntegerLiteralOutOfRange`). Unsuffixed literals over i32 range now error rather than
  silently promoting to i64.
- 2026-04-18: Bitwise type checking — `BitAnd/BitOr/BitXor/Shl` require integer operands, return
  the operand type; `BitNot` requires integer. Floats/bools → `InvalidBinaryOperator`/`InvalidOperator`.
- 2026-04-16: Const declarations — `constants` map, `register_const_item`, `check_const_item`,
  `is_const_expr`, `Stmt::Const` arm, identifier fallback. New: `ConstAlreadyDefined`, `InvalidConstExpr`.
- 2026-04-04: `Stmt::ForRange` `inclusive` flag destructured; no integer validation change.
- 2026-07-10: Const generics, `where` clauses & turbofish. `const_scope` holds const params
  (name → int type); `enter/exit_generic_scope` sets both scopes. `Type::Array.size` is now `ArrayLen`
  (`Fixed`/`Param`); a `Type::ConstValue` marker carries a const argument through monomorphization.
  `check_generic_call` seeds turbofish args, infers const params from array-argument lengths
  (`unify_array_len`), enforces every param is bound, and checks `where` predicates
  (`eval_const_predicate`). `GenericFnSig` gains `const_types` + `where_predicates`; generic-struct
  instantiation infers const params from field values and checks its predicates. New errors:
  `UnknownArrayLength`, `ConstPredicateViolated`, `TurbofishCountMismatch`, `TurbofishKindMismatch`,
  `ConstParamNotInteger`; `GenericParamNotInferable` now fires at the call site (turbofish exists).
