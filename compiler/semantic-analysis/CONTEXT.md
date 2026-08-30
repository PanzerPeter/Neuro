# semantic-analysis

## Purpose
Validate the type correctness and scope rules of a parsed Neuro program before code generation.

## Entry Point
- Type: Library function
- Input: `items: &[Item]`
- Output: `Result<Vec<Warning>, Vec<TypeError>>` — `Ok` carries non-fatal lint warnings, `Err`
  fatal type errors. Warnings are dropped when errors are present.

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- ast-types — read-only traversal of `Item` / `Expr` / `Stmt` nodes
- shared-types — `Span` embedded in every `TypeError`, `FormatSpec` for interpolation holes
- diagnostics — error type infrastructure

`syntax-parsing` is `[dev-dependencies]` only (integration tests), never production.

## Notes
Fail-slow: every type error is collected in one pass, so the developer sees the complete set per
compilation.

`type_checkers/expressions/` holds the `check_expr` dispatch (`mod.rs`) plus `calls`,
`enum_exprs`, `struct_exprs`, `operators`, `blocks`, `places`, `sequences`, `builtins`,
`const_predicates`, `interpolation`, and `try_expr`. `type_checkers/declarations/` holds the
reserved-name pass, generic scope, and generic unification/substitution in `mod.rs`, with one
module per declaration kind beside it. `tests/` is split by subject.

### Pass order
`check_program` is multi-pass, with lettered sub-passes slotted between the numbered ones
(0z/0a/0/1b/1c/1d/2b) as later requirements landed. The full ordering and its rationale live in
`docs/compiler/components/semantic-analysis.md`; the load-bearing points are:

- **0z — `check_reserved_names`** runs *first, before anything mangles*. It rejects any declared
  name (function, param, struct, field, enum, variant, trait, method, const, newtype) containing
  `__` with `ReservedNameSeparator`. `__` is the receiver/method separator in the flat function
  table and the backend splits method symbols on it, so a user name carrying its own `__` could
  forge another item's symbol.
- **1 — structs** pre-registered into `struct_defs`, with `@derive` Copy/Clone intent into
  `copy_structs` / `clone_structs`. **1b — `validate_copy_derive`** runs per struct once all are
  registered, so a Copy field that is another struct resolves regardless of declaration order.
- **1d — `register_trait`** runs before impl registration.
- **2 — impl method signatures** into `functions` (mangled `StructName__methodName`) and
  `impl_methods` (struct → method → mangled key).
- **2b — `check_operator_supertraits`** enforces `Comparable: PartialEq` order-independently
  (`MissingSupertraitImpl`).
- **3 — consts** into `constants`, giving forward references and cross-function visibility with
  no ordering constraint. **3b — every function signature** via `register_function_signature`
  (parameter and return types resolved in the function's generic scope), run over *all* functions
  before any body is checked, so a call resolves regardless of source order and mutually recursive
  functions can name each other. `check_function` reads the signature back via
  `lookup_registered_signature` rather than resolving it twice.
- **4 — full check**: `check_function` / `check_impl` / `check_const_item`.
- **5 — lints**: `run_lints` walks bodies collecting non-fatal `Warning`s
  (`prefer-loop-over-while-true` today, silenced by `@allow(prefer_loop_over_while_true)`;
  parenthesised `while (true)` deliberately not matched). Lints run independently of type errors.

### Expressions are checked exactly once
This has to be arranged deliberately for the trailing bare expression of a non-void body: it is
skipped in `check_function`'s statement loop and checked afterwards with the declared return type
as its expected type. Checking it in both places re-ran its effects — a by-value argument was
recorded as moved twice, and the second read then reported a use of the value the expression had
moved itself — and duplicated any diagnostic the tail produced. The method loop in
`declarations/impls.rs` follows the same rule.

### The value-position rules for `if` and `match`
Four rules interlock here, each fixing a shape that silently mis-typed:

1. **A trailing `if`/`else` is a value at every depth.** An `if` in statement position parses to
   `Stmt::If`, never `Stmt::Expr(Expr::If)`, so `check_block_expr_type` matches a trailing
   `Stmt::If` carrying an `else` and routes it through `check_if_expr`. Without that, an `if`
   written as the last thing inside an if-branch or a bare block typed as `void`, which made
   `val r = if a { x } else { if b { y } else { z } }` a spurious mismatch.
2. **An `if`/`else` in value position carries its context into its arms**, mirroring `check_match`
   exactly: the arm-type hint is the caller's expected type when there is one, else the first
   arm's type once known. Without it, an arm naming no type of its own — a bare `None`, an untyped
   integer literal — resolved against nothing even when the `val` it initialized was annotated,
   and `if`/`else` disagreed with the `match` spelling of the same computation.
   `check_bare_block_expr`, `check_unsafe_block_expr`, and `check_block_expr_type` thread the same
   expected type down to the tail.
3. **An arm that LEAVES the scope contributes no type.** `check_if_expr` routes every arm through
   `arm_value_type`, and `check_arm` consults `expr_diverges`: a block ending in `return` /
   `break` / `continue` reports `Type::Unknown` instead of the `Void` its trailing statement gives
   it, so it neither supplies the expression's type nor has to match it. Without the rule
   `if n > 0 { return 1 } else { 2 }` was "expected void, found i32" — naming the diverging arm as
   the EXPECTED type — while the same shape written with `panic` compiled, because the panic
   family was already `Unknown`.
4. **A divergent arm does not decide the type.** `check_if_expr` and `check_match` take the result
   from the first arm that is not `Type::Unknown` and compare every arm against it. A `panic` /
   `unreachable` arm is `Unknown` — compatible with everything — so taking it made the whole
   expression untyped and its binding vanish, purely because of the order the arms were written.

`expr_diverges` / `stmt_diverges` are owned by `val_else.rs` (which needs them for its
`else`-must-diverge rule) and are `pub(crate)` for these callers.

### Return paths
A non-void function or method must produce a value on every path, reported as
`TypeError::MissingReturn`. Two helpers in `declarations/functions.rs` state the rule once:
`tail_is_implicit_return` recognises the implicit return — a trailing bare expression, or a
trailing `if`/`else`, which the parser always shapes as `Stmt::If` and which was therefore never
checked against the declared return type at all — and `check_implicit_return` checks it. An `if`
whose every arm leaves the function carries no value and is a statement, so the divergence check
covers it instead. Without the rule the backend left the exit block without a return, LLVM
terminated it with `unreachable` (a legal terminator, so the verifier stayed silent), and the
program ran off the end of the function at runtime.

Relatedly, **a parameter whose type failed to resolve is still bound, at `Type::Unknown`.**
Skipping it turned every use in the body into a second "undefined variable" report chasing an
error already given; `Unknown` is compatible with everything, so binding it is what actually stops
the cascade.

### Primitive and reference type contracts
- Struct types are **nominal** — two `Type::Struct` are compatible iff their names match. The same
  holds for `Type::Enum`, `Type::Newtype` (which is NOT compatible with its inner type),
  `Type::Generic` (a type-parameter placeholder compatible only with itself), and
  `Type::DynObject`.
- `Type::Reference { inner, mutable }` (Display `&T` / `&mut T`) is compatible only when
  **mutability and referent both match** — there is no `&mut T` → `&T` coercion. References are
  always `Copy` and never move-tracked. Method-call and field-access resolution auto-deref via
  `referent()`, so `r.len()` / `r.field` / `r.method()` work through a borrow.
- `Type::peel_string_ref` normalizes `&string` → `string`, one layer, string only. It is what
  makes an owned `string` and a `&string` slice interchangeable for `==`, `!=`, and `+`, while
  `&i32 == i32` and `i32 == &string` stay type errors.
- `+` on two strings yields a new owned `Type::String`. Any other arithmetic op on a string, or
  mixing a string with a non-string, is `InvalidBinaryOperator`. Comparison and `+` operands are
  **not consuming positions**, so they borrow to read and never move.
- `char` is Copy; `is_valid_cast` permits char↔integer and char→char only (no float, no bool);
  ordering comparisons accept it alongside numerics on its built-in total order.
- `f16` / `bf16` have a deliberately narrow contract: Copy, `==`/`!=` via the compatible-type
  path, `as`-casts to and from any numeric type and to and from each other — but **no
  arithmetic**. `+ - * / %` on a half operand is `TypeError::HalfFloatArithmetic` ("compute in
  f32"), and `is_float()` deliberately still excludes them so arithmetic and inference paths skip
  them.
- Ordering comparisons (`< > <= >=`) are restricted to `is_numeric()` plus `char`, rejecting
  struct/string/bool operands.
- A comparison whose LHS is itself a comparison is `ComparisonChain` (all six operators).
- Arrays (`Type::Array { element, size }`) and tuples (`Type::Tuple`) are compatible on equal
  shape with matching elements, and are `Copy` exactly when every element is. A non-Copy element
  is `NonCopyArrayElement` / `NonCopyTupleElement`.
- Unsuffixed integer literals over the `i32` range error (`IntegerLiteralOutOfRange`) rather than
  silently promoting to `i64`; suffixed literals infer through `infer_suffixed_integer_type` /
  `infer_suffixed_float_type`.
- Bitwise `BitAnd`/`BitOr`/`BitXor`/`Shl` require integer operands and return the operand type;
  `BitNot` requires an integer.

### Methods, impls, and dispatch
`check_impl` binds `self` as a var of the struct type — **mutable for `&mut self`**, immutable for
`&self` — then the remaining params, before checking the body. A `&mut self` body may therefore
assign to `self.field`.

**Method calls** (`instance.method(args)`) are recognised when a `Call`'s `func` is a
`FieldAccess`; the object's struct type drives an `impl_methods` lookup for the mangled name, then
arity and argument types are validated (skipping param[0] = `self`). When the resolved method is
in `mut_self_methods`, `check_mut_self_receiver` enforces the exclusive borrow: the receiver must
be a `mut` place (or reached through `&mut T`) and must not already be borrowed — the same
coexistence rule as a `&mut place` borrow — registering a transient exclusive borrow that clears
at statement end. A `&T` receiver or a non-`mut` binding is `CannotBorrowMutably`; a live borrow is
`CannotMutablyBorrowWhileBorrowed`.

**Associated calls** (`TypeName::func(args)`) are recognised when `func` is an `Expr::Path`; the
mangled `TypeName__funcName` is looked up directly in `functions`.

**Consuming `self`** is rejected at registration with `UnsupportedSelfParam` unless the receiver
is `Copy` (where it is ABI-identical to `&self`); a by-value non-`Copy` struct ABI does not exist
yet.

**Builtin method dispatch.** For a non-struct receiver, `resolve_builtin_method` checks a fixed
compiler-known set before `MethodNotFound`, returning the result type (and an arity diagnostic on
a wrong count):
- `string.len() -> u64`, `string.clone() -> string` (nullary);
- `string.slice(a..b) -> &string` via `check_string_slice` — one `Expr::Range` argument with
  integer bounds, else `SliceExpectsRange`. A bare `Expr::Range` anywhere else is
  `RangeNotAllowed`;
- on any integer receiver, `wrapping_{add,sub,mul}`, `saturating_{add,sub,mul}`, and `.shr(n)`,
  each taking one same-typed argument (`check_unary_int_intrinsic_arg`) and returning the receiver
  type;
- `checked_{add,sub,mul}` take the same argument but return `Option<T>` over the receiver,
  instantiated through the shared `option_of` (`collections.rs`) so the overflow-reporting
  intrinsics and the fallible collection readers materialize the same prelude enum instance. A
  program with no `Option` in scope gets `UnknownTypeName`.
- A struct receiver's `.clone()` is a nullary builtin when the struct derives `Clone`/`Copy` and
  no user `clone` method exists (a user method shadows); it returns the struct type.

**Panic-family builtins.** `check_plain_call` consults `resolve_panic_builtin` before ordinary
resolution, and only when no user function of the same name is registered. `panic(msg: string)`,
`assert(cond: bool)`, `unreachable()` each validate arity and type
(`ArgumentCountMismatch` / `Mismatch`) and return `Type::Unknown` — **not** `Void`, because the
call *diverges* and must satisfy any context (unit statement, non-`void` tail return, value
binding) until a dedicated `!` type lands.

**Standard-output builtins.** `resolve_io_builtin` (`expressions/builtins.rs`) is consulted after
the panic family under the same shadowing rule. `print(text: string)` and `println(text: string)`
both return `Type::Void` — they **return**, unlike the panic family, so the result is the real
unit type and cannot stand in for a value. The argument is an owned `string` or an immutable
`&string` (the same fat pointer `.slice(range)` yields); a `&mut string` is a pointer to the fat
pointer and is a `Mismatch`. No move is recorded: the text is read, not consumed.

### Loops
`loop_stack: Vec<LoopContext>` (innermost last) carries each active loop's label,
`is_value_loop`, accumulated `break_value_ty`, and `has_break`. `check_loop_body` pushes a context
for `while` / `for` / `loop` and returns `LoopExit { value_ty, has_break }`; only `loop` is a value
loop.

- **Labels.** `check_loop_control_label` validates `break` / `continue`: an unlabeled one needs a
  non-empty stack (else `BreakOutsideLoop` / `ContinueOutsideLoop`), a labeled one needs a matching
  active label (else `UndefinedLabel`).
- **Value breaks.** `record_break_value` rejects a value targeting a `while` / `for`
  (`BreakValueInUnitLoop`), sets the loop's type on the first value-break, and reports a `Mismatch`
  on a disagreeing later one.
- **`Expr::Loop`'s type** is its agreed value-break type; unit when only plain `break`s target it;
  and **the expected type when no `break` targets it at all**. Such a loop never reaches its exit,
  so it satisfies any context — the same divergent contract the panic-family builtins carry. That
  is what keeps `func f() -> i32 { loop { ... return x } }` valid now that a trailing `loop` is
  checked as the implicit return.

The `prefer-loop-over-while-true` lint walker descends through `Stmt::Expr(Expr::Loop)`, since
there is no `Stmt::Loop`.

### Ownership, borrows, and lifetimes
**Move by default** (`type_checkers/moves.rs`). A non-`Copy` value is moved out of its source
binding when placed into a new owner — a `val`/`mut` initializer, an assignment RHS, a `return`, a
struct-field assignment value, or a by-value call argument. `record_move` marks the source moved,
but only when the consumed expression is a bare place identifier of a move-tracked type
(`is_type_move_tracked` is true for `Type::String`, every collection, and any `Type::Struct` not
deriving `Copy`). Reading a moved binding is `UseOfMovedValue`, carrying the original move span;
`SymbolInfo.moved_at` holds the per-binding state and reassigning a `mut` clears it. `.clone()`
borrows rather than moving — the canonical opt-out.

The analysis is deliberately conservative: `if`/`while`/`for` bodies and if-expression arms
snapshot and restore move state, so a conditional move never leaks onto a non-executing path. It
may miss some moves (a second-iteration loop move, say) but never rejects a valid program.

**`Copy` and `@derive(Copy, Clone)`.** `copy_structs` / `clone_structs` are populated from
`StructDef.attributes` in `register_struct`; pass 1b checks every field of a Copy struct is itself
Copy (`CopyDeriveNonCopyField`). Copy implies Clone; unknown derive arguments are ignored.

**Places, borrows, and derefs.** The `Expr::Reference` arm requires a *place*
(`is_place_expr`: an identifier or a parenthesised identifier, else `CannotBorrowValue`) and
yields `&T` **without** moving the operand — borrowing never consumes. `&mut` of a non-`mut`
binding is `CannotBorrowMutably`. `Expr::Deref` types `*r` to the referent, else
`CannotDereference`. `Stmt::DerefAssignment` requires `pointer: &mut T` — an immutable reference is
`CannotAssignThroughRef` and a non-reference is `CannotDereference` — and the stored value is
checked against the referent and move-recorded. Flow-sensitive aliasing exclusivity is deferred to
lifetime inference.

**Borrow exclusivity** (`symbol_table.rs` plus the `Expr::Reference` arm). Each binding tracks
borrows taken against its place — persistent counts (a borrow held by a reference binding via
`val r = &x`) plus transient counts (a borrow passed to a call, used in a condition, or returned).
At a `&place` site a `&mut` is rejected while any borrow is live
(`CannotMutablyBorrowWhileBorrowed`) and a `&` while a `&mut` is live
(`CannotBorrowWhileMutablyBorrowed`); any number of shared borrows may coexist. A direct
`&place` / `&mut place` initializer is promoted to a persistent borrow held by the new binding
(`attach_borrow`), released when that binding leaves scope; reassigning a `mut` reference releases
its old borrow first. Transient borrows are dropped at the end of every statement
(`clear_transient_borrows`), so a borrow never outlives the statement that took it.

This is **lexical, not NLL**: only direct-borrow initializers create tracked persistent borrows,
so the analysis never rejects a valid program, but it may miss borrows escaping through compound
expressions. Read/move-while-borrowed is not yet checked — it awaits full lifetime inference.

**Returned-reference outlives** (lifetime elision; `declarations/` + `statements.rs`). A
function or method whose declared return type is a `Type::Reference` must not return a reference
borrowing a place that dies with the call. `current_fn_outliving` holds the names that outlive the
call — reference-typed parameters (single-input elision applies the input lifetime to outputs)
plus `self` for an instance method. It is rebuilt per function/method and cleared on exit. At each
`return` and trailing implicit return whose type is a reference, `check_returned_reference` walks
the returned expression: a `&place` whose root place is local emits `ReturnsReferenceToLocal`; a
returned reference *binding* is flagged when its `borrow_provenance` is local; `if`/`else` arms,
bare and `unsafe` blocks, and `match` arm bodies are followed into their tails.
`is_local_to_function` treats an absent name (a constant, an out-of-scope place) as non-local, so
a valid program is never rejected.

Explicit lifetime annotations are validated and then **erased**: `lifetime_scope` is populated by
`enter_generic_scope` from each definition's `lifetimes`, an unknown name in a `Type::Reference` is
`UndeclaredLifetime`, and `&'a T` and `&T` remain the same semantic type. No outlives logic rides
on them — the elision rule above already accepts returning a borrowed parameter, which is exactly
the `longest<'a>` case.

### Generics
**Functions.** A generic `FunctionDef` is registered in `generic_funcs` (not `functions`) with a
signature carrying `Type::Generic` placeholders plus the ordered parameter names; `generic_scope`
puts its parameters in scope so `resolve_type` maps their names to `Generic`. Generic bodies are
checked **once, abstractly**, so only type-agnostic operations type-check there — an instantiation
that needs more is `hir-lowering`'s to refuse. `check_generic_call` infers each type argument by
unifying declared parameter types against argument types (`unify_generic`), validates arity and
the `Copy`-argument restriction, checks trait bounds (`check_trait_bounds` /
`TraitBoundNotSatisfied`, keyed off `GenericFnSig.bounds`), and returns the substituted return
type. Errors: `GenericParamShadowsBuiltin`, `GenericParamNotInferable` (fires at the call site,
since turbofish exists), `GenericArgumentNotCopy`.

**Structs and impls.** A generic `StructDef` goes to `generic_structs`, with its
placeholder-typed fields also kept in `struct_defs` under the base name so generic-`impl` method
bodies check abstractly; the bare name is `GenericStructNeedsArgs`. A generic `impl` goes to
`generic_impls` and its method signatures register under the base.
`instantiate_generic_struct` — called from `resolve_type` for a `Type::Generic` annotation and
from `check_generic_struct_literal` after inferring the arguments from field values — materializes
a distinct nominal `Type::Struct("Base<args>")` with concrete fields (`substitute_generic`) and
per-instance methods (`remap_method_type`) registered on demand, so downstream field access and
method dispatch reuse the ordinary struct machinery. Type arguments are `Copy`-restricted. Errors:
`GenericArgCountMismatch`, `NotAGenericType`, `NestedGenericTypeArg` (a generic instantiated with
an enclosing type parameter is deferred).

**Enums.** `generic_enums` (base → template) and `enum_instances` (instance → base + arguments).
Pass 0 routes an `EnumDef` with generics to `register_generic_enum`, which resolves the template's
variants with the parameters in scope (a `Type::Generic` payload placeholder is exempt from the
scalar-payload rule) and keeps them in `enum_defs` under the base name so construction sites can
infer the arguments. `instantiate_generic_enum` monomorphizes per argument set, re-checking the
scalar restriction per instance (`Option<string>` is rejected) and registering the instance under
the mangled nominal name `Base<Arg, ...>`. `resolve_type` instantiates a `Type::Generic`
application naming a generic enum and rejects the bare name (`GenericEnumNeedsArgs`).

The three construction checkers (`check_enum_unit_path`, `check_enum_tuple_call`,
`check_enum_struct_literal`) take the expected type: the instance comes from the expected type
when there is one, else the payload is unified against the template and any parameter still
unbound is taken from the enclosing function's return instance (`enum_return_type_args` — the only
context a tail `if` branch has), else `GenericEnumNotInferable`. An enum pattern written with the
base name matches the scrutinee's instance and binds payloads at the instance's concrete types.
`Option` / `Result` are **not** special-cased anywhere here — `neurc` injects their declarations.

**Const generics, `where`, turbofish.** `const_scope` holds const params (name → int type) and
`enter/exit_generic_scope` sets both scopes. `Type::Array.size` is an `ArrayLen`
(`Fixed` / `Param`) and a `Type::ConstValue` marker carries a const argument through
monomorphization. `check_generic_call` seeds turbofish arguments, infers const params from
array-argument lengths (`unify_array_len`), enforces that every param is bound, and checks `where`
predicates (`eval_const_predicate`); generic-struct instantiation does the same from field values.
Errors: `UnknownArrayLength`, `ConstPredicateViolated`, `TurbofishCountMismatch`,
`TurbofishKindMismatch`, `ConstParamNotInteger`.

### Traits
`traits` (name → `TraitInfo` of resolved method signatures), `trait_impls` (the `(trait, type)`
pairs with an impl), and `generic_bounds` (type-parameter → bound trait names, live inside a
generic definition) carry the trait system. `register_impl` calls `check_trait_conformance` for any
non-lang-item trait impl: every required method present (`MissingTraitMethod`), each impl method a
trait member (`NotATraitMethod`) with a matching signature (`TraitMethodSignatureMismatch`), or
`UnknownTrait`. Method dispatch resolves `obj.m()` on a bounded type parameter via
`resolve_generic_trait_method`. Traits are otherwise fully erased — the parser injects default
methods into impls, so they check as ordinary methods.

**Lang items** are compiler-known traits the user only ever writes an `impl` for:
- `Drop` — `register_drop_impl` requires exactly the destructor `drop(&mut self)` (no params, no
  return, else `InvalidDropImpl`) and `T` must not be `Copy` (`DropTypeCannotBeCopy`). No Drop
  state is kept on the checker; the backend recomputes the Drop-type set from the AST.
- `Hashable` — `register_hashable_impl` enforces the single `hash(&self) -> u64`
  (`InvalidHashableImpl`).
- The **operator traits** (`Add`, `Sub`, `Mul`, `Div`, `Rem`, `Neg`, `Not`, `BitAnd`, `BitOr`,
  `BitXor`, `Shl`, `PartialEq`, `Comparable`), defined in `type_checkers/operator_traits.rs`.
  `register_operator_impl` requires a `Copy` receiver (`OperatorTraitRequiresCopy`) and a declared
  `type Output` equal to the method return (`AssociatedTypeMismatch`), and wires each operator's
  result type into `operator_binary_impls` (`(struct, BinaryOp)` → `OperatorDispatch { rhs,
  result }`) or `operator_unary_impls`. In `check_expr` a binary or unary operator whose peeled
  left/operand type is a struct with a matching entry takes the impl's result type **before** the
  built-in numeric and comparison paths. Not yet: the dedicated in-place `*Assign` traits
  (compound assignment goes through the parse-time desugar to the by-value operator), `MatMul`/`@`,
  and auto-derived trait default methods — each operator needs its own impl method.

**Dynamic dispatch.** `resolve_type` delegates to a private `resolve_type_ctx(ty, behind_ref)`
whose flag is set only by the `Reference` arm, so a bare `dyn Trait` is
`DynTraitNotBehindReference` while `&dyn Trait` resolves after `trait_object_safety` checks every
method takes `&self`/`&mut self` (`TraitNotObjectSafe`). `assignable(found, expected)` is ordinary
compatibility **plus** the single implicit `&T` → `&dyn Trait` unsizing coercion, and backs the
call-argument, return, and annotated-binding checks. A method call on a `DynObject` receiver types
against the trait's declared signature. Return-position `impl Trait` resolves transparently in
`check_function` via `resolve_impl_return`, which reads the concrete type structurally from the
body's result expression (`shallow_result_type`: struct literal, enum value, newtype
construction, or a block/`if` tail) and verifies it implements the trait — so callers see the
concrete type at zero cost. Errors: `ImplTraitNotAllowedHere`, `ImplReturnNotInferable`,
`ImplReturnDoesNotImplement`.

### Closures
`type_checkers/closures.rs`. `check_closure` types an `Expr::Closure` as
`Type::Function { params, ret }`: parameters require an annotation (`ClosureParamNeedsType`), a
block body requires an explicit return type and is checked like a function body
(`ClosureBlockNeedsReturnType`), and a single-expression body infers its return type. Capture
analysis (a free-variable walk) rejects capturing a non-Copy enclosing local
(`ClosureCapturesNonCopy`) or assigning to a captured variable (`ClosureAssignsCapture`); module
constants and functions are referenced directly, not captured. The body is checked with
`current_function_return_type` redirected to the closure's return type, so an early `return` binds
to the closure. `check_plain_call` dispatches a call on a local binding of function type.

### Fallible types
`fallible_kind` (`expressions/operators.rs`) is the shared resolver, so `?` and `??` accept exactly
the same set of types. It resolves a type to an `Option` / `Result` instance through
`enum_instance_base` — a shadowing non-generic declaration is its own base.

- **`??`** is routed to `check_null_coalesce` **before** the shared operand check, because the
  operator is not operand-symmetric: the right side is typed by the left's *payload*, not by the
  left. `fallible_payload` returns the `Some`/`Ok` slot-0 type; anything else is
  `NullCoalesceOnNonFallible`. The `Result` error payload is deliberately unconstrained — `??`
  discards it. A mistyped fallback is an ordinary `Mismatch`.
- **`?`** (`expressions/try_expr.rs`) types `Expr::Try` as the operand's success payload after two
  checks. The operand must be fallible (else `TryOnNonFallible`), and
  `current_function_return_type` must be an instance of the SAME fallible enum, since that is
  where the failure goes (else `TryOutsideFallibleFunction`, which also covers propagating an
  `Option` out of a `Result` function — the two do not convert). For `Result`, the operand's `Err`
  payload must already equal the function's, reported as an ordinary `Mismatch`: the spec forwards
  the error with no implicit `.into()`, so `.map_err(...)` is the explicit conversion path.
  Success payloads are unconstrained; only the error types must agree.
- **`val-else`** (`val_else.rs`). `check_val_else` checks the scrutinee, runs the pattern through
  `check_pattern`, checks the `else` branch in its own scope, and only THEN defines the pattern's
  bindings in the enclosing scope — so the branch cannot see bindings its own failure means were
  never produced. `else_binding_type` resolves the scrutinee through `enum_instance_base`: a
  `Result` binds the `Err` payload, an `Option` is `ValElseBindingOnOption` (its failure variant is
  empty; `|_|` and the omitted form are filtered out before the check), any other type binds the
  scrutinee itself. A local `stmts_diverge` walk enforces `ValElseMustDiverge`.

### Pattern matching
`type_checkers/matches.rs`. `check_match` types the scrutinee (restricted to enum / integer /
`char` / `bool`), checks each arm's patterns against it, introduces pattern bindings into a
per-arm scope for the guard and body, unifies arm-body types (the first arm drives literal
inference), and verifies exhaustiveness — enum variant coverage, both `bool` values, or a `_`
catch-all, with guarded arms never counting. Payload sub-patterns are restricted to bindings and
`_` this phase, and or-patterns cannot bind. Errors: `NonExhaustiveMatch`,
`UnsupportedMatchScrutinee`, `PatternTypeMismatch`, `MatchArmTypeMismatch`, `InvalidRangePattern`,
`VariantPatternFormMismatch`, `OrPatternBinding`, `RefutablePayloadPattern`.

### Enums, newtypes, arrays, tuples
- **Enums.** `enum_defs` (name → variants with `VariantForm` and resolved fields) is registered in
  a pre-pass before structs. `register_enum` rejects duplicates and non-scalar payloads
  (`UnsupportedEnumPayload` — payloads are limited to scalar Copy primitives this phase).
  Construction: `E::V` (Path) → unit, `E::V(..)` (Call→Path) → tuple, `E::V { .. }`
  (`EnumStructLiteral`) → struct, with arity/field/form diagnostics.
- **Newtypes.** `predeclare_newtype` reserves each name (rejecting builtin/struct/enum/newtype
  collisions via `NewtypeAlreadyDefined`), then `resolve_newtype_inners` resolves inner types once
  all nominal names are known, rejecting cyclic (`CyclicNewtype`) and non-Copy
  (`NewtypeInnerNotCopy`) inners — the inner is restricted to Copy types this phase, so a newtype
  forwards Copy. Construction `Name(value)` is handled in `check_plain_call`; `.0` yields the
  inner type in the `TupleIndex` check.
- **Arrays.** `resolve_type` resolves `[T; N]`; `check_expr` handles array literals (homogeneous,
  length vs annotation) and indexing (`NotIndexable` / `IndexNotInteger`); `array.len()` is `u64`;
  `Stmt::ForEach` binds the element type; `Stmt::IndexAssignment` requires a mutable target.
  `Expr::ArrayRest { array, start, exact }` requires an array source and yields the
  `[T; N - start]` remainder, with `exact` demanding `N == start`
  (`ArrayPatternLengthMismatch`). Other errors: `ArrayLengthMismatch`, `CannotInferEmptyArray`.
- **Tuples.** Each element is checked against the expected tuple's element type when annotated;
  `t.N` is `NotATuple` on a non-tuple and `TupleIndexOutOfBounds` past the arity. Struct, tuple,
  and array *destructuring* is parser-desugared and reaches this slice as ordinary field-access
  and index bindings.

### Collections
`Type::Collection { kind, args }` with `CollectionKind::{Vec, HashMap, BTreeMap, String}` is a
compiler-known nominal type, never `Copy` and always move-tracked.
`type_checkers/collections.rs` owns the rules:

- `resolve_collection` resolves the generic application from `resolve_type`, validating storable
  elements (`Copy` or `string`) and map keys; a program declaring its own generic type of that
  name shadows the builtin.
- `check_collection_new` types `Vec::new()` from the expected type, else
  `CollectionTypeNotInferable`.
- `resolve_collection_method` types the method surface, requiring a mutable receiver for the
  mutating half and taking ownership only of *stored* arguments — a lookup key is read, like a
  `==` operand. Fallible readers instantiate the prelude `Option<T>`.
- Raw float keys are rejected toward `OrderedF32` / `OrderedF64` (IEEE-754 `<` is a partial
  order); a struct key requires `impl PartialEq` plus `impl Hashable` (hashed) or `impl
  Comparable` (ordered).
- Indexing, index assignment, and `for`-in accept a `Vec` alongside an array.

`String` is a fourth, **nullary** kind (`arity() == 0`), so `Collection { kind: String, args: [] }`
reuses every existing collection rule with no new `Type` variant. The bare name resolves as a
complete type in `resolution.rs` — the "collection needs type arguments" arm applies only to
`arity() > 0`, so a user-declared `struct String` still shadows it — and `check_collection_new`
returns the type directly rather than demanding an annotation. `ParamSlot::Text` (accepting
`string` or an immutable `&string`, and not moving it — the latitude `+` gives its operands) and
`ResultShape::OwnedString` back `push_str` and `to_string`; `len` / `clear` fall out of the
existing kind-agnostic entries. `Type`'s `Display` omits `<>` for a nullary collection.

Errors: `CollectionTypeNotInferable`, `InvalidCollectionElement`, `InvalidCollectionKey`,
`InvalidHashableImpl`.

### String interpolation
`type_checkers/expressions/interpolation.rs`. Each hole's expression is checked, its type
auto-dereferenced through a borrow, and its written spec validated against that type — radix kinds
need an integer, fixed-point and scientific need a float, `+` needs a signed integer or float,
zero fill cannot combine with `<`/`^`, and width and precision are bounded. The literal always
types as `string`, so a rejected hole does not cascade. Errors: `UnformattableType`,
`FormatSpecMismatch`, `FormatWidthTooLarge`, `FormatPrecisionTooLarge`.

### Visibility
A struct field is private to its declaring module unless it carries `export`, and **this slice is
where that is enforced** — the rule needs the receiver's type, so module-resolution (which runs
first) cannot state it. `register_struct` / `register_generic_struct` record each struct's `module`
and its private field names, and `instantiate_generic_struct` copies both onto every monomorphized
instance. `current_module` is set from the item being checked in pass 4, and `reject_private_field`
compares the two at the four places a field is reached: a read (`check_field_access_expr`, which
also covers struct destructuring, since the parser desugars it into field reads), a write
(`Stmt::FieldAssignment`), and a literal's listed fields (plain and generic).
`reject_private_update` covers `..base`, which supplies every *unlisted* field and would otherwise
copy private ones out. New error: `PrivateField`. Nothing else reads `current_module`, and a
single-file program is one module, so the rule is inert there.

### Modules
Nothing about imports reaches this slice — module-resolution consumes every `Item::Import` and
rewrites every name it bound. The one exception is `Pattern::UnqualifiedEnum`, which the resolver
rejects when no import accounts for it; reaching the checker means the resolver did not run,
reported as `UnimportedVariantPattern` and contributing no exhaustiveness coverage.

### Constants
`constants: HashMap<String, Type>` holds both module-level and body consts. `is_const_expr`
validates the RHS (literals, arithmetic on literals, casts, identifiers referring to other known
consts); a body `Stmt::Const` is validated in `check_stmt`. `Expr::Identifier` falls back to
`constants` after the symbol table, so const names work in any expression context. Errors:
`ConstAlreadyDefined`, `InvalidConstExpr`.

### Three rules that exist because the backend cannot answer them
Each closed a path where a program type-checked and then aborted codegen with an internal error:

- **`VoidBinding`** (BUG-016). A binding whose initializer has type `void` is rejected in the
  `Stmt::VarDecl` arm, beside the existing `Type::Unknown` guard and mirroring it — the error is
  recorded and the name left undefined. Testing the binding's TYPE rather than its initializer's
  shape is what makes one check cover every spelling: a `void` call is only two of them, the
  others being an `if`, a `match`, a bare block, a `loop { break }`, and an explicit `: void`
  annotation. Statement position is untouched, so `println("hi")` on its own line still compiles.
- **`MissingPartialEqImpl`** (BUG-015). `check_binary_expr`'s equality arm asks
  `has_builtin_equality` (`expressions/operators.rs`) whether the operand type has equality
  without an impl: the scalars (half-precision included), `string` after `peel_string_ref`, and a
  newtype forwarding one of those. A `Type::Generic` answers yes — a generic body is checked once
  as a template, so the instantiation is `hir-lowering`'s to refuse. A struct operand is reported
  as the missing trait; every other operand (array, tuple, enum, collection, non-string reference)
  reuses `InvalidBinaryOperator`, which is what the ordering comparisons already gave. The
  operator-trait dispatch above the arm is untouched, so an explicit `impl PartialEq` compiles as
  before.
- **`FunctionUsedAsValue`** (BUG-013). `Expr::Identifier` resolution consults `functions` and
  `generic_funcs` before falling through to `UndefinedVariable`, so a function name in value
  position is told apart from a name that does not exist. No coercion was added — a function is
  still not a value.
