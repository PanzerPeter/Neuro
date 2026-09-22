# semantic-analysis

## Purpose
Validate the type correctness and scope rules of a parsed Neuro program before code generation.

## Entry Point
- Type: Library function
- Input: `items: &[Item]`
- Output: `Result<Vec<Warning>, Vec<TypeError>>`: `Ok` carries non-fatal lint warnings, `Err`
  fatal type errors. Warnings are dropped when errors are present.

## Shared Kernel
- ast-types: read-only traversal of `Item` / `Expr` / `Stmt` nodes
- shared-types: `Span` embedded in every `TypeError`, `FormatSpec` for interpolation holes

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
(0z/0a/0/1b/1c/1d/2b/3c) as later requirements landed. The full ordering and its rationale live in
`docs/compiler/components/semantic-analysis.md`; the load-bearing points are:

- **0z. `check_reserved_names`** runs *first, before anything mangles*. It rejects any declared
  name (function, param, struct, field, enum, variant, trait, method, const, newtype) containing
  `__` with `ReservedNameSeparator`. `__` is the receiver/method separator in the flat function
  table and the backend splits method symbols on it, so a user name carrying its own `__` could
  forge another item's symbol.
- **1. structs** pre-registered into `struct_defs`, with `@derive` intent into `copy_structs` /
  `clone_structs` / `debug_structs` / `partial_eq_structs`, and every derive argument validated
  (`UnknownDerive`, `UnimplementedDerive`, `DuplicateDerive`). **1b. `validate_copy_derive`** and
  **`validate_field_derives`** run per struct once all are registered, so a field that is another
  struct resolves regardless of declaration order.
- **1d. `register_trait`** runs before impl registration.
- **2. impl method signatures** into `functions` (mangled `StructName__methodName`) and
  `impl_methods` (struct → method → mangled key).
- **2b. `check_operator_supertraits`** enforces `Comparable: PartialEq` order-independently
  (`MissingSupertraitImpl`). **2c. `check_derive_impl_conflicts`** rejects a struct that both
  derives a trait and declares an `impl` of it (`DeriveConflictsWithImpl`).
- **3. consts** into `constants`, giving forward references and cross-function visibility with
  no ordering constraint. **3b. every function signature** via `register_function_signature`
  (parameter and return types resolved in the function's generic scope), run over *all* functions
  before any body is checked, so a call resolves regardless of source order and mutually recursive
  functions can name each other. `check_function` reads the signature back via
  `lookup_registered_signature` rather than resolving it twice.
- **3c. `check_grad_attributes`** (`type_checkers/grad.rs`) holds a `@grad` function's
  signature to what its derivative needs: a rank-0 `Tensor<f32, []>` return, every tensor
  parameter `&mut` over a float element with literal extents (`GradSignature`), a free
  non-generic function with a bare attribute (`GradFormUnsupported`), and no declared struct
  named `GradsOf_<f>` (`GradGeneratedNameTaken`; `__<f>__rev` cannot clash, pass 0z reserves
  `__`). The bundle prefix is duplicated from `hir-lowering`, which emits it. Which constructs
  a `@grad` BODY may use is deliberately not checked here: the transform owns that rule set
  and reports it with a span itself.
- **4. full check**: `check_function` / `check_impl` / `check_const_item`.
- **5. lints**: `run_lints` walks bodies collecting non-fatal `Warning`s
  (`prefer-loop-over-while-true` today, silenced by `@allow(prefer_loop_over_while_true)`;
  parenthesised `while (true)` deliberately not matched). Lints run independently of type errors.

### Expressions are checked exactly once
This has to be arranged deliberately for the trailing bare expression of a non-void body: it is
skipped in `check_function`'s statement loop and checked afterwards with the declared return type
as its expected type. Checking it in both places re-ran its effects: a by-value argument was
recorded as moved twice, and the second read then reported a use of the value the expression had
moved itself. It also duplicated any diagnostic the tail produced. The method loop in
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
   arm's type once known. Without it, an arm naming no type of its own (a bare `None`, an untyped
   integer literal) resolved against nothing even when the `val` it initialized was annotated,
   and `if`/`else` disagreed with the `match` spelling of the same computation.
   `check_bare_block_expr`, `check_unsafe_block_expr`, and `check_block_expr_type` thread the same
   expected type down to the tail.
3. **An arm that LEAVES the scope contributes no type.** `check_if_expr` routes every arm through
   `arm_value_type`, and `check_arm` consults `expr_diverges`: a block ending in `return` /
   `break` / `continue` reports `Type::Unknown` instead of the `Void` its trailing statement gives
   it, so it neither supplies the expression's type nor has to match it. Without the rule
   `if n > 0 { return 1 } else { 2 }` was "expected void, found i32" (naming the diverging arm as
   the EXPECTED type), while the same shape written with `panic` compiled, because the panic
   family was already `Unknown`.
4. **A divergent arm does not decide the type.** `check_if_expr` and `check_match` take the result
   from the first arm that is not `Type::Unknown` and compare every arm against it. A `panic` /
   `unreachable` arm is `Unknown` (compatible with everything), so taking it made the whole
   expression untyped and its binding vanish, purely because of the order the arms were written.

`expr_diverges` / `stmt_diverges` are owned by `val_else.rs` (which needs them for its
`else`-must-diverge rule) and are `pub(crate)` for these callers.

### Return paths
A non-void function or method must produce a value on every path, reported as
`TypeError::MissingReturn`. Two helpers in `declarations/functions.rs` state the rule once:
`tail_is_implicit_return` recognises the implicit return: a trailing bare expression, or a
trailing `if`/`else`, which the parser always shapes as `Stmt::If` and which was therefore never
checked against the declared return type at all. `check_implicit_return` then checks it. An `if`
whose every arm leaves the function carries no value and is a statement, so the divergence check
covers it instead. Without the rule the backend left the exit block without a return, LLVM
terminated it with `unreachable` (a legal terminator, so the verifier stayed silent), and the
program ran off the end of the function at runtime.

Relatedly, **a name whose type failed to resolve is still bound, at `Type::Unknown`.** This
holds in both places one can appear: a parameter whose type did not resolve, and a `Stmt::VarDecl`
whose initializer did not type-check. Skipping either turned every later use of the name into a
second "undefined variable" report chasing an error already given; `Unknown` is compatible with
everything, so binding it is what actually stops the cascade. It applies only where the
initializer was actually reported: `Type::Unknown` also comes back from a DIVERGING
initializer (`panic`, `unreachable`), which produces no value to bind and is routed to
`VoidBinding` below instead. The `Stmt::VarDecl` arm tells the two apart by whether the
error list grew while the initializer was checked.

### Primitive and reference type contracts
- Struct types are **nominal**: two `Type::Struct` are compatible iff their names match. The same
  holds for `Type::Enum`, `Type::Newtype` (which is NOT compatible with its inner type),
  `Type::Generic` (a type-parameter placeholder compatible only with itself), and
  `Type::DynObject`.
- `Type::Reference { inner, mutable }` (Display `&T` / `&mut T`) is compatible only when
  **mutability and referent both match**: there is no `&mut T` → `&T` coercion. References are
  always `Copy` and never move-tracked. Method-call and field-access resolution auto-deref via
  `referent()`, so `r.len()` / `r.field` / `r.method()` work through a borrow.
- `Type::Slice(element)` and `Type::DynObject` are the two **unsized** types: valid only as a
  reference referent, and carrying the language's only two implicit conversions (see Slices,
  below, and Traits).
- `Type::peel_string_ref` normalizes `&string` → `string`, one layer, string only. It is what
  makes an owned `string` and a `&string` slice interchangeable for `==`, `!=`, and `+`, while
  `&i32 == i32` and `i32 == &string` stay type errors.
- `+` on two strings yields a new owned `Type::String`. Any other arithmetic op on a string, or
  mixing a string with a non-string, is `InvalidBinaryOperator`. Comparison and `+` operands are
  **not consuming positions**, so they borrow to read and never move.
- `char` is Copy; `is_valid_cast` permits char↔integer and char→char only (no float, no bool);
  ordering comparisons accept it alongside numerics on its built-in total order.
- `f16` / `bf16` have a deliberately narrow contract: Copy, `==`/`!=` via the compatible-type
  path, `as`-casts to and from any numeric type and to and from each other, but **no
  arithmetic**. `+ - * / %` on a half operand is `TypeError::HalfFloatArithmetic` ("compute in
  f32"), and `is_float()` deliberately still excludes them so arithmetic and inference paths skip
  them.
- Ordering comparisons (`< > <= >=`) are restricted to `is_numeric()` plus `char`, rejecting
  struct/string/bool operands.
- A comparison whose LHS is itself a comparison is `ComparisonChain` (all six operators).
- Arrays (`Type::Array { element, size }`) and tuples (`Type::Tuple`) are compatible on equal
  shape with matching elements, and are `Copy` exactly when every element is. An element that is
  not `Copy` makes the aggregate move-tracked instead of rejecting it: see Sub-place moves.
- Unsuffixed integer literals over the `i32` range error (`IntegerLiteralOutOfRange`) rather than
  silently promoting to `i64`; suffixed literals infer through `infer_suffixed_integer_type` /
  `infer_suffixed_float_type`. `check_integer_range` takes an `i128` and compares against the
  target type's own bounds, so both ends of `i64` and `u64` are expressible.
- A negation directly over an integer literal is range-checked as the **negated** value, in
  `check_unary_expr` ahead of the general operand walk. Checking the magnitude alone rejects the
  most negative value of every signed type, whose magnitude is one past that type's maximum
  (`-2147483648` for `i32`), and accepts `val x: u8 = -1`, whose magnitude fits while the value
  it denotes does not. Every integer target takes this path. An out-of-range negation over an
  **unsigned** target is `NegativeLiteralForUnsignedType`, which names the wrapping spelling
  (`0u8.wrapping_sub(1u8)`) rather than only the range; the split is keyed on the target's
  signedness through `is_signed_integer`, not on the value's sign, so `val x: i8 = -200` stays
  an ordinary `IntegerLiteralOutOfRange`. This is the invariant the backend relies on to
  materialize a negated literal as a constant instead of guarding it at run time.
- Bitwise `BitAnd`/`BitOr`/`BitXor`/`Shl` require integer operands and return the operand type;
  `BitNot` requires an integer.

### Methods, impls, and dispatch
`check_impl` binds `self` as a var of the struct type (**mutable for `&mut self`**, immutable for
`&self`), then the remaining params, before checking the body. A `&mut self` body may therefore
assign to `self.field`.

**Method calls** (`instance.method(args)`) are recognised when a `Call`'s `func` is a
`FieldAccess`; the object's struct type drives an `impl_methods` lookup for the mangled name, then
arity and argument types are validated (skipping param[0] = `self`). When the resolved method is
in `mut_self_methods`, `check_mut_self_receiver` enforces the exclusive borrow: the receiver must
be a `mut` place (or reached through `&mut T`) and must not already be borrowed (the same
coexistence rule as a `&mut place` borrow), registering a transient exclusive borrow that clears
at statement end. A `&T` receiver or a non-`mut` binding is `CannotBorrowMutably`; a live borrow is
`CannotMutablyBorrowWhileBorrowed`.

**Associated calls** (`TypeName::func(args)`) are recognised when `func` is an `Expr::Path`; the
mangled `TypeName__funcName` is looked up directly in `functions`.

**Consuming `self`** on a move-tracked receiver is recorded in `consuming_self_methods` at
registration, keyed by the mangled name (a `Copy` receiver is duplicated by value, so calling one
consumes nothing and it is not recorded). At the call site `record_consumed_receiver` moves the
receiver, so a later use is `UseOfMovedValue`; a `&T` / `&mut T` receiver, and the `self` of a
borrowing method, own nothing to give away and are `CannotMoveOutOfBorrow` instead. The receiver
is also left out of `current_fn_outliving`, since it is destroyed when the method returns.

**Builtin method dispatch.** For a non-struct receiver, `resolve_builtin_method` checks a fixed
compiler-known set before `MethodNotFound`, returning the result type (and an arity diagnostic on
a wrong count):
- `string.len() -> u64`, `string.clone() -> string` (nullary);
- `string.slice(a..b) -> &string` and `string.char_slice(a..b) -> &string`, both via
  `check_string_slice`: one `Expr::Range` argument with integer bounds, else
  `SliceExpectsRange`. A bare `Expr::Range` anywhere else is `RangeNotAllowed`. The two share a
  check because they share a contract: they differ only in whether the indices count bytes or
  code points, which is a backend concern with no type-level consequence;
- `string.chars() -> Chars` (nullary), the prelude's codepoint iterator. Like `.slice` it
  registers a transient borrow of the receiver, because the iterator holds a view into the
  receiver's bytes rather than owning them. `Chars` is a prelude declaration, so a program
  compiled with `@no_prelude` (or one shadowing it) gets `UnknownTypeName` instead;
- `string.__char_at(offset) -> char`, gated on `in_prelude()` (the declaration being checked
  carries `ast_types::PRELUDE_MODULE`). It is the decode step `Chars::next` is written against;
  the language specifies no byte-indexed read of a string, so every other module sees
  `MethodNotFound`;
- on any integer receiver, `wrapping_{add,sub,mul}`, `saturating_{add,sub,mul}`, and `.shr(n)`,
  each taking one same-typed argument (`check_unary_int_intrinsic_arg`) and returning the receiver
  type;
- `f32`/`f64`.`is_nan()` (nullary) returns `bool`. Gated on `Type::is_float`, which admits the
  full-precision floats only: `f16`/`bf16` fall through to `MethodNotFound`, having no scalar
  arithmetic that could produce a NaN. Like the integer intrinsics it matches on `recv` rather
  than the referent, so a `&f64` receiver needs an explicit deref;
- `checked_{add,sub,mul}` take the same argument but return `Option<T>` over the receiver,
  instantiated through the shared `option_of` (`collections.rs`) so the overflow-reporting
  intrinsics and the fallible collection readers materialize the same prelude enum instance. A
  program with no `Option` in scope gets `UnknownTypeName`.
- A struct receiver's `.clone()` is a nullary builtin when the struct derives `Clone`/`Copy` and
  no user `clone` method exists (a user method shadows); it returns the struct type.

**Panic-family builtins.** `check_plain_call` consults `resolve_panic_builtin` before ordinary
resolution, and only when no user function of the same name is registered. `panic(msg: string)`,
`assert(cond: bool)`, `unreachable()` each validate arity and type
(`ArgumentCountMismatch` / `Mismatch`) and return `Type::Unknown`, **not** `Void`, because the
call *diverges* and must satisfy any context (unit statement, non-`void` tail return, value
binding) until a dedicated `!` type lands.

**Standard-output builtins.** `resolve_io_builtin` (`expressions/builtins.rs`) is consulted after
the panic family under the same shadowing rule. `print(text: string)` and `println(text: string)`
both return `Type::Void`: they **return**, unlike the panic family, so the result is the real
unit type and cannot stand in for a value. The argument is an owned `string` or an immutable
`&string` (the same fat pointer `.slice(range)` yields); a `&mut string` is a pointer to the fat
pointer and is a `Mismatch`. No move is recorded: the text is read, not consumed.

### Loops
`loop_stack: Vec<LoopContext>` (innermost last) carries each active loop's label,
`is_value_loop`, accumulated `break_value_ty`, the `expected_ty` the loop expression is
checked against, and `has_break`. `check_loop_body` pushes a context
for `while` / `for` / `loop` and returns `LoopExit { value_ty, has_break }`; only `loop` is a value
loop.

- **Labels.** `check_loop_control_label` validates `break` / `continue`: an unlabeled one needs a
  non-empty stack (else `BreakOutsideLoop` / `ContinueOutsideLoop`), a labeled one needs a matching
  active label (else `UndefinedLabel`).
- **Value breaks.** `record_break_value` rejects a value targeting a `while` / `for`
  (`BreakValueInUnitLoop`), sets the loop's type on the first value-break, and reports a `Mismatch`
  on a disagreeing later one.
- **Expected type.** `check_loop_body` carries the loop expression's expected type into its
  `LoopContext`, and the `Stmt::Break` arm reads it back through `break_target_expected` — by
  label, so a `break outer v` adopts the annotation of the loop it actually leaves. Without it a
  literal in `break [10, 20]` was typed on its own and then failed against an annotation the
  `if`-arm, `match`-arm and block-tail spellings of the same program all satisfied.
- **`Expr::Loop`'s type** is its agreed value-break type; unit when only plain `break`s target it;
  and **the expected type when no `break` targets it at all**. Such a loop never reaches its exit,
  so it satisfies any context: the same divergent contract the panic-family builtins carry. That
  is what keeps `func f() -> i32 { loop { ... return x } }` valid now that a trailing `loop` is
  checked as the implicit return.

- **Enumerated heads.** `for (i, x) in xs.enumerate()` reaches the checker as `Stmt::ForRange` /
  `Stmt::ForEach` with `index: Some(_)`: the parser resolves the adapter, so nothing here checks a
  method call. `define_loop_index` binds it as `u64` (matching `.len()` and the index expression,
  so `xs[i]` needs no cast) in the same scope as the element binding, which is what makes
  `for (i, i) in ...` a `VariableAlreadyDefined` rather than a shadow.

- **Reversed heads.** `Stmt::ForRange` and `TensorIndexArg::Range` carry a `reversed` flag
  for `.rev()`, and the checker reads neither. A reversal changes the order values arrive in,
  not the bounds, the element type, or a slice's surviving extent, so there is no rule here to
  state and no diagnostic to raise: the parser has already rejected the only ill-formed case
  (a receiver that is not a range).

- **Adapted heads.** `type_checkers/loop_adapters.rs`. `check_loop_adapters` folds the head's
  `adapters` over the element type the base head produced: `.map(f)` replaces it with `f`'s
  return type, `.filter(p)` leaves it alone. Each function is checked in the scope *enclosing*
  the loop (it is evaluated once, before the loop, and cannot name the binding it feeds), and is
  READ rather than moved, so a closure binding used by a head stays usable afterwards. Four
  rejections: a non-function or wrong-arity argument (`LoopAdapterNotCallable`), a parameter that
  does not accept the element (`LoopAdapterInput`), a `.filter` predicate that does not answer
  `bool`, and a `.map` returning `void` (both `LoopAdapterOutput`): the last because the loop
  binding would otherwise have no type the backend can represent.

### The iteration protocol
`type_checkers/iteration.rs`. A `Stmt::ForEach` head that is neither a collection, an array,
nor a slice falls through to `iteration_item`, which answers what one step binds:

- `IntoIterator` is consulted first (`trait_impls`), and the iterator is the declared return
  type of that impl's `into_iter`. Otherwise the head is taken to be its own iterator: the
  blanket `impl<I: Iterator> IntoIterator for I` stated as a rule, since a blanket impl has no
  syntax yet.
- The `Item` comes from the ITERATOR's own `impl Iterator` entry in `impl_assoc`, not from the
  container's. That is also what enforces `IntoIterator::Iter: Iterator`: a bound on an
  associated-type declaration has no syntax, so the requirement is checked on the type
  `into_iter` actually returned.
- The head is a consuming position: the loop takes the value as its iterator, so it is
  `record_move`d like any other by-value placement.
- A head implementing neither trait is `TypeError::NotIterable`, which replaced the
  `NotIndexable` this arm used to reuse: iterating and indexing are no longer the same question.
- A BORROW of a protocol type is `TypeError::BorrowedIterableHead`. The referent peel that
  auto-derefs `&T` receivers makes `&c` / `&mut c` look resolvable against the impl on the owned
  type, and lowering has no path for that head; the arm rejects it here so the diagnostic keeps
  its span instead of surfacing as a lowering error. Arrays, `Vec`s and slices are unaffected:
  they are matched off the referent before this arm is reached.

`char_indices_receiver` (same module) recognises the `text.char_indices()` head form ahead of
the ordinary `check_expr` on the iterable, and `check_char_indices_head` types it as the same
`Chars` iterator `.chars()` yields: the position it binds is a byte offset the lowering reads
off that iterator rather than a payload it yields. A non-`string` receiver is `MethodNotFound`,
matching what the call gets anywhere but a `for` head.

`instantiate_impls_for` copies each generic impl's `trait_impls` entry and its `impl_assoc`
bindings onto every monomorphized instance, substituted. Without that a generic iterator
adapter would satisfy `Iterator` under its base name only, and no `for` head over an instance
of it could find its `Item`.

The `prefer-loop-over-while-true` lint walker descends through `Stmt::Expr(Expr::Loop)`, since
there is no `Stmt::Loop`.

### Ownership, borrows, and lifetimes
**Move by default** (`type_checkers/moves.rs`). A non-`Copy` value is moved out of its source
binding when placed into a new owner: a `val`/`mut` initializer, an assignment RHS, a `return`, a
struct-literal or struct-field assignment value, or a by-value call argument. `record_move` marks
the source moved when the consumed expression is a place of a move-tracked type
(`is_type_move_tracked` is true for `Type::String`, every collection, every tensor, any
`Type::Struct` not deriving `Copy`, every `Type::Generic`, and an array, tuple or newtype holding
any of those). Reading a moved binding is `UseOfMovedValue`, carrying the original move span;
`SymbolInfo.moves` holds the per-binding state and reassigning a `mut` clears it. `.clone()`
borrows rather than moving: the canonical opt-out.

A consuming position checked *after* another one in the same expression has to see the
earlier move. `place_moved_at` answers where a place's root binding was moved, and the by-value
tensor operator uses it: both operands are type-checked before either move is recorded, so
`a + a` read the right operand while the binding still looked owned, compiled, and gave two
owners to one buffer. It reports `UseOfMovedValue` only when *this* expression is what
invalidated the operand; a move that predates it is already reported where the operand is read.

A place is more than a bare identifier. `place_origin` resolves a field path (`l.w`,
`o.inner.w`), an array index (`a[0]`) and a tuple element (`t.1`) to the type it denotes and to
whether reaching it crossed a reference. A place reached
through a borrow owns nothing to give away and is `CannotMoveOutOfBorrow` instead of a move;
that covers a dereferenced borrow (`val x = *r`) and `self.field` in a **borrowing** method.
`self` is bound as the struct type so field access reads normally, which leaves its ownership out
of its type; `self_is_owned`, set per method body from the `SelfParam`, carries it instead, and a
consuming receiver's fields are the callee's to move out. `..base` moves its base when it supplies any non-`Copy` field
(`record_update_base_move`), and moves nothing when every unlisted field is `Copy`.

**Sub-place moves.** A move out of a sub-place is recorded against the PATH below its root
binding — `"label"`, `"0"`, `"1.name"` — not against the root alone. `MoveState` on `SymbolInfo`
holds the whole-binding span plus a map of moved paths, and `MoveState::conflict` rejects a read
whose path either contains, or is contained by, a moved one. That is what makes
`val (a, b) = pair` work: the destructure desugar binds `tmp.0` then `tmp.1`, which under a
collapse-to-root rule was a use of a moved value on the second leaf, for every struct, tuple and
array alike. Reading the aggregate as a WHOLE still conflicts with any outstanding part, so a
partially moved value cannot be passed on.

Three consequences shape the code. `place_path` returns `None` for an index the compiler cannot
name statically (a runtime `a[i]`), and the move is then recorded against the whole binding,
because it cannot say which element left. `TypeChecker::in_sub_place` marks the inner links of a
place chain so `t` in `t.a` is not judged as a whole-binding read; `with_sub_place_read` is the
same suppression for `Expr::ArrayRest`, whose `exact` node is an arity assertion over elements
the leading projections have already taken. And a loop body's `moves_since` compares part maps,
not just the whole span, so a partial move a second iteration would repeat is still reported.

The analysis is deliberately conservative: `if`/`while`/`for` bodies and if-expression arms
snapshot and restore move state, so a conditional move never leaks onto a non-executing path. It
may miss some moves, but it never rejects a valid program.

A loop body is the exception to the plain restore. `report_loop_body_moves` runs after the body
and before the restore, and reports `MovedInLoopBody` for every binding that was intact at the
snapshot and is still moved-out when the body ends: the next iteration would move it again, which
is a double free rather than a diagnostic. A body that re-establishes the binding (a `mut`
reassignment clears `moved_at`) and a binding declared inside the body (it leaves with the body's
scope) are both untouched, and a body that always leaves the loop via `break` or `return`
(`stmts_exit_loop`) is exempt, because its move runs once. The restore after it is unchanged: the
loop may run zero times, so the binding still owns its value on the path past the loop.

**`@derive(...)`.** `copy_structs` / `clone_structs` / `debug_structs` / `partial_eq_structs` are
populated from `StructDef.attributes` in `record_derive_intent`, which also validates the argument
list: `IMPLEMENTED_DERIVES` (`Copy`, `Clone`, `Debug`, `PartialEq`) are acted upon,
`PENDING_DERIVES` (`Hashable`) reports `UnimplementedDerive`, anything else `UnknownDerive`, and a
repeat is `DuplicateDerive`. Nothing is silently ignored. Pass 1b checks every field of a Copy
struct is itself Copy (`CopyDeriveNonCopyField`) and every field of a `Debug` / `PartialEq` struct
is renderable / comparable by the derived rules (`DeriveFieldUnsupported`,
`is_debug_renderable` / `is_derived_comparable`: a scalar, `string`, or another struct carrying the
same derive, since the generated code reaches inside a field no other way). A generic template's
fields are type parameters, which no derive rule can judge, so the check runs again per
monomorphized instance in `instantiate_generic_struct`. Copy implies Clone.

**Derived `Debug` and `PartialEq`.** Neither routes through a method: that is what separates a
derive from a hand-written `impl PartialEq`, which lands in `operator_binary_impls` and dispatches.
`==` / `!=` on a `partial_eq_structs` struct is accepted by `has_derived_equality` beside the
built-in scalar equality, and lowering leaves it a binary node for the backend to expand
field-wise. A `debug_structs` struct is the one type whose renderability depends on the specifier:
it has no display form, so `{x:?}` renders it and `{x}` does not (`UnrenderableStruct`).

**Places, borrows, and derefs.** The `Expr::Reference` arm requires a *place*
(`is_place_expr`: an identifier or a parenthesised identifier, else `CannotBorrowValue`) and
yields `&T` **without** moving the operand: borrowing never consumes. `&mut` of a non-`mut`
binding is `CannotBorrowMutably`. `Expr::Deref` types `*r` to the referent, else
`CannotDereference`. `Place::Deref` requires `pointer: &mut T` (an immutable reference is
`CannotAssignThroughRef` and a non-reference is `CannotDereference`), and the stored value is
checked against the referent and move-recorded. Flow-sensitive aliasing exclusivity is deferred to
lifetime inference.

Note the asymmetry, which is `docs/BUGS.md` BUG-033: an assignment TARGET is a full `Place`
(a field, an element, a tensor coordinate), while the operand of `&` is still only a bare
binding. The two notions of place are not yet the same one.

**Assignment targets** (`check_assign` / `resolve_place` in `type_checkers/statements.rs`).
`resolve_place` returns the type of the LOCATION and reports the place's own errors; a single
store path then checks the value against it and records the move. The forms keep per-form rules
rather than collapsing into one, because they genuinely differ: a slice takes its write
permission from the borrow rather than the binding (`&mut [T]` is writable through an immutable
binding, and a `mut` `&[T]` binding is not), a private field is rejected, and a tensor index that
leaves an axis standing is `AssignToTensorSlice` because a slice is a fresh tensor and not
storage. Everything else inherits the mutability of `Place::root()`, the binding the place bottoms
out at.

**Borrow exclusivity** (`symbol_table.rs` plus the `Expr::Reference` arm). Each binding tracks
borrows taken against its place: persistent counts (a borrow held by a reference binding via
`val r = &x`) plus transient counts (a borrow passed to a call, used in a condition, or returned).
At a `&place` site a `&mut` is rejected while any borrow is live
(`CannotMutablyBorrowWhileBorrowed`) and a `&` while a `&mut` is live
(`CannotBorrowWhileMutablyBorrowed`); any number of shared borrows may coexist. A direct
`&place` / `&mut place` initializer is promoted to a persistent borrow held by the new binding
(`attach_borrow`), released when that binding leaves scope; reassigning a `mut` reference releases
its old borrow first. Transient borrows are dropped at the end of every statement
(`clear_transient_borrows`), so a borrow never outlives the statement that took it.

**Borrowee access** (`check_borrowee_read` in `expressions/places.rs`, `reject_move_of_borrowee`
in `moves.rs`, the target check in `check_binding_store`). The rules above govern borrows against
each other; these three govern the borrowed place itself. A read of a binding is
`CannotUseWhileMutablyBorrowed` while an exclusive borrow is held by a live binding; a move out
of it (or out of a field of it) is `CannotMoveWhileBorrowed` while ANY borrow is live; assigning
to it is `CannotAssignWhileBorrowed` on the same ground, checked before the RHS so `r = &mut x`
does not conflict with the borrow it installs.

The read rule reads the **persistent** counts only, the other two read persistent plus transient.
A `&mut` handed to a call is over when the call returns, but the transient counter survives to
the end of the statement, so counting it there would reject `combine(bump(&mut y), y)`. There is
no equivalent sound program on the move side. `in_borrow_operand` suppresses the read rule while
the operand of `&` / `&mut` is typed: naming a place in order to borrow it is not an access to it,
and the borrow site has its own diagnostic. The move rule stands down when a persistent exclusive
borrow is live, because the read rule has already reported that name.

This is **lexical, not NLL**: a borrow held by a binding freezes its borrowee for that binding's
whole scope, so code reads back through the borrow or confines it to a block. Only direct-borrow
initializers create tracked persistent borrows, so borrows escaping through compound expressions
are still missed.

**Returned-reference outlives** (lifetime elision; `declarations/` + `statements.rs`). A
function or method whose declared return type is a `Type::Reference` must not return a reference
borrowing a place that dies with the call. `current_fn_outliving` holds the names that outlive the
call: reference-typed parameters (single-input elision applies the input lifetime to outputs)
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
on them: the elision rule above already accepts returning a borrowed parameter, which is exactly
the `longest<'a>` case.

### Generics
**Functions.** A generic `FunctionDef` is registered in `generic_funcs` (not `functions`) with a
signature carrying `Type::Generic` placeholders plus the ordered parameter names; `generic_scope`
puts its parameters in scope so `resolve_type` maps their names to `Generic`. Generic bodies are
checked **once, abstractly**, so only type-agnostic operations type-check there: an instantiation
that needs more is `hir-lowering`'s to refuse. `check_generic_call` infers each type argument by
unifying declared parameter types against argument types (`unify_generic`), validates arity,
checks trait bounds (`check_trait_bounds` / `TraitBoundNotSatisfied`, keyed off
`GenericFnSig.bounds`), and returns the substituted return type. A **type argument carries no
`Copy` requirement**: the abstract body was already checked against a conservatively non-`Copy`
`T`, so it holds for every instantiation. Errors: `GenericParamShadowsBuiltin`,
`GenericParamNotInferable` (fires at the call site, since turbofish exists).

**`Type::Generic` answers `false` to `is_type_copy` and `true` to `is_type_move_tracked`**, which
is what makes checking the body once sound: a second read of a `T`-typed binding is
`UseOfMovedValue`, a closure may not capture one, and `[v, v]` / `(v, v)` are rejected by the
aggregate element rule. Three positions are re-validated once per instantiation and therefore
defer on `Type::mentions_generic` instead of asking `is_type_copy`: an array and a tuple
annotation in `resolve_type`, and `validate_copy_derive`'s field scan. That keeps
`func first<T>(a: [T; 3])` and `@derive(Copy) struct Buffer<T, const CAP> { data: [T; CAP] }`
working, since the caller's own annotation for the concrete argument is checked where it is
written.

**Structs and impls.** A generic `StructDef` goes to `generic_structs`, with its
placeholder-typed fields also kept in `struct_defs` under the base name so generic-`impl` method
bodies check abstractly; the bare name is `GenericStructNeedsArgs`. A generic `impl` goes to
`generic_impls` and its method signatures register under the base.
`instantiate_generic_struct` (called from `resolve_type` for a `Type::Generic` annotation and
from `check_generic_struct_literal` after inferring the arguments from field values) materializes
a distinct nominal `Type::Struct("Base<args>")` with concrete fields (`substitute_generic`) and
per-instance methods (`remap_method_type`) registered on demand, so downstream field access and
method dispatch reuse the ordinary struct machinery. A type argument carries no `Copy`
requirement: the instance holds the value and is move-tracked when what it holds is. Errors:
`GenericArgCountMismatch`, `NotAGenericType`, `NestedGenericTypeArg`
(a generic instantiated with an enclosing type parameter is deferred).

**Enums.** `generic_enums` (base → template) and `enum_instances` (instance → base + arguments).
Pass 0 predeclares every enum NAME; pass 1a resolves the variants, with a generic template's
parameters in scope, and keeps them in `enum_defs` under the base name so construction sites can
infer the arguments. `instantiate_generic_enum` monomorphizes per argument set, re-checking the
sizedness rule per instance and registering the instance under the mangled nominal name
`Base<Arg, ...>`. It resolves the template's own variants first, because a struct field naming
an instance (`Option<i32>`) is resolved in pass 1, before pass 1a has run. `resolve_type` instantiates a `Type::Generic`
application naming a generic enum and rejects the bare name (`GenericEnumNeedsArgs`).

The three construction checkers (`check_enum_unit_path`, `check_enum_tuple_call`,
`check_enum_struct_literal`) take the expected type: the instance comes from the expected type
when there is one, else the payload is unified against the template and any parameter still
unbound is taken from the enclosing function's return instance (`enum_return_type_args`, the only
context a tail `if` branch has), else `GenericEnumNotInferable`. An enum pattern written with the
base name matches the scrutinee's instance and binds payloads at the instance's concrete types.
`Option` / `Result` are **not** special-cased anywhere here: `neurc` injects their declarations.

**Const generics, `where`, turbofish.** `const_scope` holds const params (name → int type) and
`enter/exit_generic_scope` sets both scopes. `Type::Array.size` and the `extent` of every
`TensorAxis` in `Type::Tensor.shape` are an `ArrayLen`
(`Fixed` / `Param` / `Dynamic`, the last reachable only from a tensor's `?`) and a
`Type::ConstValue` marker carries a const argument through
monomorphization. `check_generic_call` seeds turbofish arguments, infers const params from
array-argument lengths and tensor-argument extents (`unify_array_len`, `unify_tensor_shape`),
enforces that every param is bound, and checks `where`
predicates (`eval_const_predicate`); generic-struct instantiation does the same from field values.
Errors: `UnknownArrayLength`, `ConstPredicateViolated`, `TurbofishCountMismatch`,
`TurbofishKindMismatch`, `ConstParamNotInteger`.

**Shape generics.** A tensor extent written as a name is a const parameter the parser has
already re-kinded, so nothing here treats it specially except where a symbolic extent has no
number to check against. `resolve_type` maps a `TensorExtent::Param` to `ArrayLen::Param` when
`const_scope` knows the name and reports `UnknownTensorDimension` otherwise.
`unify_tensor_shape` binds every axis even after one has failed, so a parameter the rest of the
shape does bind is not also reported as uninferable, and `conflicting_shape_param` turns the
failure into `TensorShapeParamConflict`, which names the parameter and both extents rather than
printing an expected type that was itself inferred. A tensor literal against a symbolic extent
is `TensorLiteralSymbolicExtent`: one literal serves every instantiation, so there is no length
to check it against. Indexing a symbolic axis keeps its rank check and drops only the
compile-time bounds check, which is the debug-tier guard's job at that point.

**Dynamic shapes.** `resolve_type` maps a `TensorExtent::Dynamic` to `ArrayLen::Dynamic`, and
`ArrayLen::satisfies` is the whole rule: an EXPECTED `?` accepts any extent, and the
reverse is refused, since a `?` found where a literal is expected would let the consumer index at
strides the run-time shape may not have. A `?` binds no shape parameter (`unify_array_len` has no
case for it), so a shape-generic call over a dynamic argument is an uninferable parameter rather
than a wrong extent. `reject_dynamic_extent` (in `tensors.rs`) is the single gate every
extent-consuming operation passes through — construction, literal coercion, indexing, the four
shape casts, `.clone()`, `.to(device)`, and compound assignment — reporting
`TensorDynamicExtent` with the operation and the type. What remains legal on a `?`-shaped tensor
is what needs no extent: binding, passing, returning, moving and dropping.

### Traits
`traits` (name → `TraitInfo` of resolved method signatures), `trait_impls` (the `(trait, type)`
pairs with an impl), `impl_assoc` (`(trait, type)` → what that impl bound each associated type
to), and `generic_bounds` (type-parameter → `BoundInfo`s, live inside a generic definition)
carry the trait system. `register_impl` calls `check_trait_conformance` for any
non-lang-item trait impl: every required method present (`MissingTraitMethod`), each impl method a
trait member (`NotATraitMethod`) with a matching signature (`TraitMethodSignatureMismatch`), or
`UnknownTrait`. Method dispatch resolves `obj.m()` on a bounded type parameter via
`resolve_generic_trait_method`. Traits are otherwise fully erased: the parser injects default
methods into impls, so they check as ordinary methods.

**Associated types.** `TraitInfo.assoc_types` lists what a trait declares (`type Item`);
`self_assoc` holds what the impl under check bound each one to, installed by `enter_impl_assoc`
around both `register_impl` and `check_impl` and consulted by `resolve_type` for a
`Type::Named` spelled `Self::Item`. A declared position is *not* a type at the declaration:
`register_trait` leaves it `Unknown` and keeps the signature as written in `TraitMethodSig.decl`,
which `trait_signature_mismatch` then resolves per impl, so the trait's `Self::Item` is compared
as the type that impl chose. Conformance also requires every declared name bound
(`MissingAssociatedType`) and nothing else bound (`UnknownAssociatedType`); an unbound path
anywhere else is `UnboundAssociatedType`. A trait declaring an associated type is **not
object-safe** (`TraitNotObjectSafe`): a trait object erases the implementor, and nothing else
says what the member is.

**Constrained bounds.** `T: Trait<Assoc = U>` is resolved by `resolve_bounds` (in
`declarations/mod.rs`) into `BoundInfo { trait_name, assoc }`, in a second pass over the
parameter list so a constraint may name another parameter of the same list; a binding the trait
never declared is `UnknownAssociatedType`. `resolve_generic_trait_method` then types a call
whose signature names an associated type by installing the bound's constraints as `self_assoc`
and re-resolving `TraitMethodSig.decl`: the same per-impl re-resolution conformance does. A
bare bound leaves the position open and still reports `UnconstrainedAssociatedType`. At the call
site `check_assoc_bindings` compares the constraint against `impl_assoc` for the concrete type
argument (or, for a type parameter passed through from an enclosing generic, against that
parameter's own bound), reporting `AssociatedTypeBoundMismatch`; `resolve_impl_return` runs the
same check for return-position `impl Trait<Assoc = U>`.

**Lang items** are compiler-known traits the user only ever writes an `impl` for:
- `Drop`: `register_drop_impl` requires exactly the destructor `drop(&mut self)` (no params, no
  return, else `InvalidDropImpl`) and `T` must not be `Copy` (`DropTypeCannotBeCopy`). No Drop
  state is kept on the checker; the backend recomputes the Drop-type set from the AST.
- `Hashable`: `register_hashable_impl` enforces the single `hash(&self) -> u64`
  (`InvalidHashableImpl`).
- The **operator traits** (`Add`, `Sub`, `Mul`, `Div`, `Rem`, `Neg`, `Not`, `BitAnd`, `BitOr`,
  `BitXor`, `Shl`, `PartialEq`, `Comparable`), defined in `type_checkers/operator_traits.rs`.
  `register_operator_impl` requires a `Copy` receiver (`OperatorTraitRequiresCopy`) and a declared
  `type Output` equal to the method return (`AssociatedTypeMismatch`), and wires each operator's
  result type into `operator_binary_impls` (`(struct, BinaryOp)` → `OperatorDispatch { rhs,
  result }`) or `operator_unary_impls`. In `check_expr` a binary or unary operator whose peeled
  left/operand type is a struct with a matching entry takes the impl's result type **before** the
  built-in numeric and comparison paths. Not yet: user-declarable `*Assign` traits and
  auto-derived trait default methods: each operator needs its own impl method.
- **By-value tensor operators** (`type_checkers/tensor_broadcast.rs`) run **before** the
  operator-trait and built-in numeric paths, on either operand being a tensor or a borrow of
  one. `broadcast_shapes` joins the two shapes: aligned at the trailing axis, an extent of 1
  stretched across a wider one, a lower-rank operand supplying the innermost axes. A shape
  parameter's extent is never the axis that stretches (whether it is 1 is unknown until the
  instantiation) but is stretched into, since a literal 1 on the other side is known. A pair
  that does not join is `TensorBroadcastMismatch`; only the five arithmetic operators and `@`
  are defined (`InvalidBinaryOperator` otherwise); the element must have arithmetic
  (`TensorElementNotArithmetic`); a `?` extent has no element count for the fresh result
  (`TensorDynamicExtent`); and an owned operand is moved once the operands are known to
  combine, so a rejected operator does not also report a use-after-move.
  A scalar operand carries the element's own type: `tensor_element_expectation` types the
  right operand by the left tensor's element, and `scalar_broadcast_expectation` does the same
  in reverse for a literal written to the left of a tensor BINDING, which is the one place a
  syntactic lookahead is used (a speculative `check_expr` would record the discarded attempt's
  diagnostics). A scalar beside anything else still needs its suffix.
- **`@` is the one tensor operator that is not element-wise.** `matmul_shape` (same file) is
  reached instead of the broadcast join and contracts rather than stretches: two rank-2 operands
  whose inner axes agree give `[M, K] @ [K, N]` -> `[M, N]`, taking the left operand's row axis
  and the right operand's column axis with their names. Anything else — a rank other than 2, a
  scalar operand, a disagreeing inner extent, disagreeing axis NAMES on the contracted axis — is
  `TensorMatMulMismatch`. `ArrayLen` equality is what compares the inner axes, so `matmul<M, N, K>`
  checks its repeated `K` once at the declaration rather than per instantiation. A `?` is rejected
  on BOTH operand shapes rather than only the result's (`TensorDynamicExtent`), because the
  contracted axis bounds the loop even though it appears in neither operand's result. Everything
  after the join — the element-arithmetic check, the move recording — is shared with the
  element-wise operators. A user type reaches `@` through the `MatMul` operator trait instead.
- **Compound assignment** (`Stmt::Assign` with `op: Some(_)`) implements the operator-trait
  dispatch rule in `type_checkers/statements.rs`. A tensor place routes to
  `check_tensor_compound_assign` (`type_checkers/tensors.rs`), the compiler-known `*Assign`
  implementation; every other place re-forms the `Expr::Binary` desugar over `Place::to_expr()`
  and checks it as an ordinary store, which is what keeps a user operator-trait impl reachable
  through `+=`. The
  tensor path checks the operand **before** the target's mutability, which is the evaluation
  order the language specifies; requires the element to have arithmetic
  (`TensorElementNotArithmetic` rejects `bool` and the half-precision types, matching their
  scalar contract); and moves an owned operand, so a right-hand side that moved the target
  itself (`w += w`) is `UseOfMovedValue`. The operand is accepted by
  `compound_assign_operand_fits` (`tensor_broadcast.rs`), which takes the by-value operator's
  broadcast rule with one asymmetry the in-place write forces: the join has to come back as the
  TARGET's shape, so an operand may be stretched up to it but never past it. A scalar of the
  element type is accepted the same way, which is what makes `w *= 2.0` the scalar broadcast.

**Dynamic dispatch.** `resolve_type` delegates to a private `resolve_type_ctx(ty, behind_ref)`
whose flag is set only by the `Reference` arm, so a bare `dyn Trait` is
`DynTraitNotBehindReference` while `&dyn Trait` resolves after `trait_object_safety` checks every
method takes `&self`/`&mut self` (`TraitNotObjectSafe`). `assignable(found, expected)` is ordinary
compatibility **plus** the single implicit `&T` → `&dyn Trait` unsizing coercion, and backs the
call-argument, return, and annotated-binding checks. A method call on a `DynObject` receiver types
against the trait's declared signature. Return-position `impl Trait` resolves transparently in
`check_function` via `resolve_impl_return`, which reads the concrete type structurally from the
body's result expression (`shallow_result_type`: struct literal, enum value, newtype
construction, or a block/`if` tail) and verifies it implements the trait, so callers see the
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
to the closure, and with `loop_stack` emptied for the same reason: an enclosing loop is not a
`break` target from inside a closure, so one written there is `BreakOutsideLoop`. Both are restored
afterwards. `check_plain_call` dispatches a call on a local binding of function type.

### Composition
`expressions/compose.rs`. `check_compose` types an `Expr::Compose` as
`Type::Function { params: [first stage's parameter], ret: last stage's return }`, after resolving
every name in the chain and checking that each stage's result is assignable to the next stage's
parameter (`ComposeStageMismatch`). A stage must be a non-generic free function of exactly one
parameter: `ComposeGenericFunction`, `ComposeArity`, `ComposeUndefined`, and
`ComposeNotANamedFunction` for a name that resolves to a *binding* instead. That last one is the
capture rule reaching a new surface rather than a rule of its own: the composed closure calls each
stage directly, and a stage held in a binding would have to be captured, which the Copy-only
capture model forbids. Every stage is resolved before any mismatch is reported, so one chain
reports each bad name it holds.

### Fallible types
`fallible_kind` (`expressions/operators.rs`) is the shared resolver, so `?` and `??` accept exactly
the same set of types. It resolves a type to an `Option` / `Result` instance through
`enum_instance_base`: a shadowing non-generic declaration is its own base.

- **`??`** is routed to `check_null_coalesce` **before** the shared operand check, because the
  operator is not operand-symmetric: the right side is typed by the left's *payload*, not by the
  left. `fallible_payload` returns the `Some`/`Ok` slot-0 type; anything else is
  `NullCoalesceOnNonFallible`. The `Result` error payload is deliberately unconstrained: `??`
  discards it. A mistyped fallback is an ordinary `Mismatch`.
- **`?`** (`expressions/try_expr.rs`) types `Expr::Try` as the operand's success payload after two
  checks. The operand must be fallible (else `TryOnNonFallible`), and
  `current_function_return_type` must be an instance of the SAME fallible enum, since that is
  where the failure goes (else `TryOutsideFallibleFunction`, which also covers propagating an
  `Option` out of a `Result` function, since the two do not convert). For `Result`, the operand's `Err`
  payload must already equal the function's, reported as an ordinary `Mismatch`: the spec forwards
  the error with no implicit `.into()`, so `.map_err(...)` is the explicit conversion path.
  Success payloads are unconstrained; only the error types must agree.
- **`val-else`** (`val_else.rs`). `check_val_else` checks the scrutinee, runs the pattern through
  `check_pattern`, checks the `else` branch in its own scope, and only THEN defines the pattern's
  bindings in the enclosing scope, so the branch cannot see bindings its own failure means were
  never produced. `else_binding_type` resolves the scrutinee through `enum_instance_base`: a
  `Result` binds the `Err` payload, an `Option` is `ValElseBindingOnOption` (its failure variant is
  empty; `|_|` and the omitted form are filtered out before the check), any other type binds the
  scrutinee itself. A local `stmts_diverge` walk enforces `ValElseMustDiverge`.

### Pattern matching
`type_checkers/matches.rs`. `check_match` types the scrutinee (restricted to enum / integer /
`char` / `bool`), checks each arm's patterns against it, introduces pattern bindings into a
per-arm scope for the guard and body, unifies arm-body types (the first arm drives literal
inference), and verifies exhaustiveness: enum variant coverage, both `bool` values, or a `_`
catch-all, with guarded arms never counting. Payload sub-patterns are restricted to bindings and
`_` this phase, and or-patterns cannot bind. Errors: `NonExhaustiveMatch`,
`UnsupportedMatchScrutinee`, `PatternTypeMismatch`, `MatchArmTypeMismatch`, `InvalidRangePattern`,
`VariantPatternFormMismatch`, `OrPatternBinding`, `RefutablePayloadPattern`.

### Enums, newtypes, arrays, tuples
- **Enums.** `enum_defs` (name → variants with `VariantForm` and resolved fields) is filled in
  two passes: `predeclare_enum` (pass 0) reserves the name and rejects duplicates,
  `resolve_enum_variants` (pass 1a) resolves the payloads. They are split because a payload may
  name a struct and a struct field may name the enum, so neither table can be complete before the
  other's names exist. A payload may be any SIZED type, `Copy` or not; `void` and the unsized
  types are `UnsupportedEnumPayload`. Construction: `E::V` (Path) → unit, `E::V(..)` (Call→Path)
  → tuple, `E::V { .. }` (`EnumStructLiteral`) → struct, with arity/field/form diagnostics.
- **Newtypes.** `predeclare_newtype` reserves each name (rejecting builtin/struct/enum/newtype
  collisions via `NewtypeAlreadyDefined`), then `resolve_newtype_inners` resolves inner types once
  all nominal names are known and rejects cycles (`CyclicNewtype`), which is what makes every
  predicate that recurses through an inner type terminate. A newtype forwards both `Copy` and
  move-tracking from its inner type. Construction `Name(value)` is handled in `check_plain_call`;
  `.0` yields the inner type in the `TupleIndex` check.
- **Arrays.** `resolve_type` resolves `[T; N]`; `check_expr` handles array literals (homogeneous,
  length vs annotation) and indexing (`NotIndexable` / `IndexNotInteger`); `array.len()` is `u64`;
  `Stmt::ForEach` binds the element type, and a BY-VALUE head over a move-tracked element type
  records a move of the iterable: the loop takes the elements over, which is what codegen already
  disowns the array for, so the array owns nothing after the loop. `for x in &arr` borrows and
  moves nothing. `Stmt::IndexAssignment` requires a mutable target.
  `Expr::ArrayRest { array, start, exact }` requires an array source and yields the
  `[T; N - start]` remainder, with `exact` demanding `N == start`
  (`ArrayPatternLengthMismatch`). Other errors: `ArrayLengthMismatch`, `CannotInferEmptyArray`.
- **Slices.** `Type::Slice(element)` is `[T]`, the unsized run behind `&[T]` / `&mut [T]`.
  `resolve_type_ctx` accepts it only `behind_ref`, exactly as it does `dyn Trait`, and reports
  `SliceNotBehindReference` otherwise. Two slice types are compatible when their elements are:
  a length is not part of the type. `assignable` carries the unsizing coercion
  `&[T; N]` / `&Vec<T>` / `&[T]` → `&[T]` (`unsizes_to_slice`), with mutability matching exactly,
  and every argument, return, and annotated-binding site routes through it. `.slice(range)` on an
  array, a `Vec`, or a slice yields `&[T]`; `slice.len()` is `u64`; indexing, `for x in xs`, and
  `Stmt::IndexAssignment` all accept a slice, the last taking its write permission from the
  *reference* (`&mut [T]`) rather than from the binding's own `mut`.
  A `.slice` call registers a shared borrow of the place its receiver roots at
  (`slice_borrow_root` sees through a chain of slice calls), so a live view blocks a `&mut` of
  the source and `borrow_target_of` promotes it to a persistent borrow when it initializes a
  binding, which is what makes returning a view of a local a `ReturnsReferenceToLocal`.
- **Tensors.** `Type::Tensor { element, shape }` is the statically shaped `Tensor<T, [d0, ...]>`.
  `shape` is a `Vec<TensorAxis>`, one `{ name, extent }` per dimension. Rank and every extent are
  part of the type, so two tensors are compatible only when their elements match and their shapes
  agree axis for axis (`shapes_agree` / `TensorAxis::agrees_with`); an empty `shape` is the rank-0
  scalar tensor. An extent is an `ArrayLen`: `Fixed` everywhere concrete, `Param` inside a
  shape-generic definition, where monomorphization makes it concrete (see **Shape generics**).
- **Named dimensions.** `TensorAxis.name` is the optional `batch:` of a named shape. It is
  part of the type but not of type identity: two axes agree when their extents agree and their
  names agree *where both carry one*, so a named shape and an unnamed one with the same extents
  are interchangeable while `[height: H, width: W]` and `[width: W, height: H]` are not. That
  rule lives in one place, `TensorAxis::agrees_with`, and both `is_compatible_with` and
  `unify_tensor_shape` route through it — derived `PartialEq` on the axis is structural and
  therefore stricter, so a shape comparison never uses `==`. A repeated name in one shape is
  `DuplicateTensorAxisName` (raised in `resolve_type`, where the name's span is still to hand),
  and a disagreement is `TensorAxisNameMismatch`, which names the axis and both names because two
  transposed shapes print near-identically. `record_type_mismatch` (`type_checkers/mod.rs`) is the
  one entry point that chooses between that error and a plain `Mismatch`, so every argument,
  binding, and generic-call path tells the same story. An axis that survives an index keeps its
  name at its new extent (`TensorAxis::with_extent`), which is what lets a row of
  `[height: 2, width: 3]` annotate as `[width: 3]`. A name is also *read back* by
  `.permute` / `.flatten`, the only places one is resolved as an identifier rather than
  compared (see **Tensor shape manipulation**); HIR lowering carries the names for that one
  consumer and every backend still reads `TensorDim.extent` alone. `resolve_type` restricts the element to a fixed-width scalar (integers,
  `f16`/`bf16`/`f32`/`f64`, `bool`) and reports `NonScalarTensorElement` otherwise. A tensor owns
  its buffer, so `is_type_copy` is false and `is_type_move_tracked` is true: it moves on
  assignment and on being passed. `Tensor` is a prelude name a module may shadow, so the
  shape-less spelling `Tensor<f32>` reaches the `Type::Generic` arm, where an unshadowed
  `Tensor` reports `TensorShapeRequired` rather than `NotAGenericType`.
- **Tensor ownership, in `type_checkers/expressions/builtins.rs`.** Two intrinsic methods sit on
  a tensor receiver. `.clone()` is nullary, auto-derefs `&Tensor<T, S>` (the result is the
  referent, so cloning through a borrow yields an owned tensor), and moves nothing: it is the
  opt-out `UseOfMovedValue`'s own text points at. `.to(device)` takes one argument of the prelude
  enum `Device` (`DEVICE_TYPE_NAME` in `type_checkers/tensors.rs`), returns the receiver's type,
  and calls `record_move` on the receiver: it consumes the tensor. It matches on `recv` rather
  than the referent, so a `&Tensor` receiver falls through to `MethodNotFound`: a borrow cannot
  be consumed.
- **Tensor values, in `type_checkers/tensors.rs`.** Two ways in, one checker.
  An array literal reaching `check_array_literal_expr` with a `Type::Tensor` expectation is a
  *tensor* literal: `check_tensor_literal` walks the annotation's shape and the nesting
  together, so each leaf is checked at the annotation's element type (a literal is typed *by*
  it, a value that already has a type is not converted) and every axis must be rectangular.
  Wrong extent is `TensorExtentMismatch`, wrong nesting depth is `TensorRankMismatch`, and a
  rank-0 annotation is `TensorScalarNeedsConstructor`: a rank-0 tensor has no array form.
  Nothing changes when no tensor annotation is in scope, which is what keeps `[1.0, 2.0]` an
  `[f64; 2]`.
  A call whose callee is `Path { Tensor, ctor }` routes to `check_tensor_construction`, guarded
  by `tensor_name_is_free` so a program declaring its own `Tensor` keeps it. The tensor type
  comes from the turbofish's single `type_args` entry when the parser built one, and from the
  expectation otherwise; neither is `TensorTypeNotInferable`. The six helpers are `zeros`,
  `ones`, `identity`, `random_normal`, `scalar`, and `from` (`UnknownTensorConstructor`
  otherwise), each with its own applicability rule reported as
  `TensorConstructorNotApplicable`: `identity` is square and rank 2, `random_normal` draws only
  into `f32`/`f64`, `scalar` is rank 0, and `from` takes the same nested literal the annotated
  form coerces.
- **Tensor shape manipulation, in `type_checkers/tensor_shape.rs`.** `.t()`,
  `.reshape([...])`, `.permute([...])`, and `.flatten()` / `.flatten(dims: [...])` reach
  `check_tensor_shape_method` from the builtin arm, matched on `recv` (not the referent) like
  `.to(device)`: each CONSUMES its receiver via `record_move`, so a borrow falls through to
  `MethodNotFound`. **Their arguments are never handed to `check_expr`**, because a `.permute`
  entry may be a dimension NAME, which the specification resolves against the receiver's own shape and
  which no value scope declares; `const_integer` folds a position or extent out of the syntax
  instead. `.t()` is rank-2 only (`TensorTransposeRank`); `.reshape` takes at most one `-1`
  (`TensorReshapeRepeatedInference`) and may not change the element count
  (`TensorReshapeElementCount`, `TensorReshapeIndivisible`); `.permute` names every axis
  exactly once (`TensorPermuteRank`, `TensorAxisRepeated`); `.flatten` merges an ADJACENT run
  (`TensorFlattenNotAdjacent`) and merges everything when given no argument. An unknown
  dimension name is `UnknownTensorAxisName`, which lists the names the shape does declare.
  `.t()` and `.permute` carry each axis's name along; `.reshape` and a merged `.flatten` axis
  are unnamed, a new extent not being the axis the old name documented. A `Param` extent has
  no element count, so `.reshape` / `.flatten` report `TensorShapeCastSymbolicExtent` inside a
  shape-generic definition while the two reordering methods still work. The label in
  `.flatten(dims: ...)` is bound by the `argument-binding` slice, whose seeded builtin
  signature is the one method entry it carries.
- **Tensor reductions, in `type_checkers/tensor_reduce.rs`.** `.sum()`, `.mean()`,
  `.max()` and `.min()` reach `check_tensor_reduce` from the builtin arm, matched on the
  REFERENT rather than on `recv`: a reduction reads the buffer it summarises, records no
  move, and so accepts `&Tensor<T, S>` where the consuming shape casts do not. With no
  argument the result is the element type; with an `axis:` argument it is the tensor of the
  remaining axes, each keeping its name. The argument is read as syntax for the same reason
  `.permute`'s is — a dimension NAME resolves against the receiver's shape and no value
  scope declares it — and a negative index counts from the end, which is how the
  specification spells the last axis. The element must be an integer or `f32`/`f64`
  (`TensorReduceElementType`); `.mean` narrows that to `f32`/`f64`, an integer mean having
  no rounding rule in the specification (`TensorReduceMeanNotFloat`); a reduced run of zero
  elements has no value to produce, `.max()` least of all, so the whole family reports
  `TensorReduceEmpty` rather than each inventing an identity. Every extent must be a number
  here (`TensorShapeCastSymbolicExtent`, `TensorDynamicExtent`): the run length decides
  whether the reduction has a value, and the backend walks the buffer at strides the shape
  supplies. The `axis:` label is bound by the `argument-binding` slice's seeded builtin
  signatures.
- **Tensor order-based selections, in `type_checkers/tensor_sort.rs`.** `.sort()`,
  `.argsort()` and `.topk()` reach `check_tensor_sort` from the builtin arm, matched on the
  REFERENT for the reason the reductions are: each allocates its own result, records no move,
  and accepts `&Tensor<T, S>`. `.sort` answers the receiver's own type, `.argsort` the same
  shape at `i32`, and `.topk` a `(values, indices)` tuple whose selected axis is `k` long and
  unnamed, a truncated axis no longer being the thing its name documented. The arguments
  arrive complete and in declaration order because `argument-binding` fills an omitted one
  from its default, and all of them are read as syntax rather than as values: an axis may be
  a dimension NAME, and `k:` and `descending:` decide the result's shape and the comparator
  before any element exists (`TensorSortArgNotConstant`). The element must be an integer or
  `f32`/`f64` (`TensorSortElementType`); a rank-0 receiver has no axis to order
  (`TensorSortRankZero`); an empty axis has no ordering (`TensorSortEmpty`); and `k` must lie
  in `1..=extent` (`TensorTopKOutOfRange`). Every extent must be a number here
  (`TensorShapeCastSymbolicExtent`, `TensorDynamicExtent`), because the result's shape and
  the backend's strides are both built from it.
- **Functional traversals, in `type_checkers/tensor_apply.rs`.** `.map(f)`,
  `.zip(other, f)`, and `.reduce(init, f)` reach `check_tensor_apply` from
  `resolve_builtin_method`, matched on the REFERENT for the reason the reductions are: the
  first two allocate their own result and the third allocates nothing, so none records a
  move and `&Tensor<T, S>` is an acceptable receiver. Every argument here IS a value,
  unlike a reduction's `axis:`, so each is checked in the ordinary way. The function's
  parameters are checked against what it will be handed — the element type for `.map`, both
  element types for `.zip`, and the SEED FIRST then the element for `.reduce`, which is the
  order `|acc, x|` is written in (`TensorApplyNotCallable`, `TensorApplyArity`,
  `TensorApplyParamType`). `.map` and `.zip` answer the receiver's shape over the function's
  RETURN type, which is the one place a tensor's element type changes, so that type has to
  be a number a buffer can hold (`TensorApplyResultElement`); `.reduce` answers the seed's
  own type and so requires the function to answer it too (`TensorReduceAccumulator`). A
  `.zip`'s operand is a tensor of the receiver's extents (`TensorZipOperandNotTensor`,
  `TensorZipShapeMismatch`), because one index walks both buffers. Every extent must be a
  number here (`TensorShapeCastSymbolicExtent`), the result buffer being built from it.
  There is deliberately no `.filter`: its output length depends on the values in the buffer,
  so its result would have no shape to name.
- **Einstein notation, in `type_checkers/tensor_einsum.rs`.** `einsum("bij,bjk->bik", a, b)`
  reaches `resolve_einsum_builtin` from the free-function arm of `check_plain_call`, beside
  `resolve_panic_builtin` and `resolve_io_builtin` and inside the same
  `!self.functions.contains_key` guard, so a program's own `einsum` shadows it. It is the one
  variadic call in the language, and only because the subscript literal fixes its arity: the
  comma-separated pieces left of `->` say how many operands there are and what rank each one
  has. The subscripts are read as SYNTAX (`parse_subscripts`), never as a value — a string a
  program computes cannot decide a result shape the rest of checking depends on
  (`EinsumSubscriptNotLiteral`), and anything that is not ASCII letters separated by `,`
  around one `->` is `EinsumMalformedSubscripts`. Each operand is matched on the REFERENT for
  the reason the reductions are: the call allocates its own result, records no move, and
  accepts `&Tensor<T, S>`. One pass binds every letter to an extent and fixes the element
  type, reporting `EinsumOperandCount`, `EinsumOperandNotTensor`, `EinsumOperandRank`,
  `EinsumElementType`, `EinsumElementMismatch`, `EinsumSymbolicExtent` and
  `EinsumExtentConflict`; the output letters then have to be distinct
  (`EinsumOutputLetterRepeated`) and each bound by some input
  (`EinsumOutputLetterUnbound`). The three letter diagnostics name the letter, as the
  language requires: the letter is the only thing in the call that says which axes were meant to
  agree. An empty output subscript yields the ELEMENT type rather than `Tensor<T, []>`,
  matching the whole-tensor `.sum()`.
- **Tensor slicing and indexing, in `type_checkers/tensor_index.rs`.** `check_tensor_index`
  takes one argument per axis and answers one of two types: an axis given a `Position` is
  DROPPED and one given a `Range` (a `..` full axis is the range over the whole extent)
  SURVIVES at its new extent, so an index naming every axis reads the element type and any
  other builds `Tensor<T, [survivors]>`. An argument count other than the rank is
  `TensorIndexRankMismatch`. A position is any integer expression — `IndexNotInteger`
  otherwise — and only a *constant* one is bounds-checked here (`TensorIndexOutOfBounds`); a
  run-time position is left to the backend's debug-tier guard, the tier an array index sits
  on. Both bounds of a range must fold through `eval_literal_int`
  (`expressions/const_predicates.rs`) or it is `TensorSliceBoundNotConstant`: the extent is
  part of the result's TYPE, so it cannot wait for a value. A reversed or over-long range is
  `TensorSliceOutOfRange`. Two spellings reach this: `Expr::TensorIndex` through
  `check_tensor_index_expr`, and the one-argument `Expr::Index` whose object is a tensor,
  routed from `check_index_expr` (`expressions/places.rs`) ahead of the sequence rules so a
  rank-1 tensor takes the ordinary bracket. Indexing READS its receiver — nothing is moved,
  and `referent()` sees through a borrow — because a slice is a fresh owned copy rather than
  a view, which is what keeps one buffer to one owner. A range index on a non-tensor is
  `TensorIndexOnNonTensor`, whose text names `.slice(a..b)`.
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
  name shadows the builtin. `string` is storable because the collection takes a copy of the bytes
  rather than the operand's fat pointer: the slot owns what it holds, an element read copies back
  out, and the collection releases its slots when it is destroyed.
- `check_collection_new` types `Vec::new()` from the expected type, else
  `CollectionTypeNotInferable`.
- `resolve_collection_method` types the method surface, requiring a mutable receiver for the
  mutating half. No collection method moves its arguments: every one of them is read, like a
  `==` operand, because an insertion copies the bytes rather than taking the operand's buffer.
  Fallible readers instantiate the prelude `Option<T>`.
- Raw float keys are rejected toward `OrderedF32` / `OrderedF64` (IEEE-754 `<` is a partial
  order); a struct key requires `impl PartialEq` plus `impl Hashable` (hashed) or `impl
  Comparable` (ordered).
- Indexing, index assignment, and `for`-in accept a `Vec` alongside an array.

`String` is a fourth, **nullary** kind (`arity() == 0`), so `Collection { kind: String, args: [] }`
reuses every existing collection rule with no new `Type` variant. The bare name resolves as a
complete type in `resolution.rs`: the "collection needs type arguments" arm applies only to
`arity() > 0`, so a user-declared `struct String` still shadows it, and `check_collection_new`
returns the type directly rather than demanding an annotation. `ParamSlot::Text` (accepting
`string` or an immutable `&string`, and not moving it: the latitude `+` gives its operands) and
`ResultShape::OwnedString` back `push_str` and `to_string`; `len` / `clear` fall out of the
existing kind-agnostic entries. `Type`'s `Display` omits `<>` for a nullary collection.

Errors: `CollectionTypeNotInferable`, `InvalidCollectionElement`, `InvalidCollectionKey`,
`InvalidHashableImpl`.

### String interpolation
`type_checkers/expressions/interpolation.rs`. Each hole's expression is checked, its type
auto-dereferenced through a borrow, and its written spec validated against that type: radix kinds
need an integer, fixed-point and scientific need a float, `+` needs a signed integer or float,
zero fill cannot combine with `<`/`^`, and width and precision are bounded. The literal always
types as `string`, so a rejected hole does not cascade. A struct hole answers through
`struct_render_obstacle`, which distinguishes a missing `@derive(Debug)` from a missing `:?`.
Errors: `UnformattableType`, `UnrenderableStruct`, `FormatSpecMismatch`, `FormatWidthTooLarge`,
`FormatPrecisionTooLarge`.

### Visibility
A struct field is private to its declaring module unless it carries `export`, and **this slice is
where that is enforced**: the rule needs the receiver's type, so module-resolution (which runs
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
Nothing about imports reaches this slice: module-resolution consumes every `Item::Import` and
rewrites every name it bound. The one exception is `Pattern::UnqualifiedEnum`, which the resolver
rejects when no import accounts for it; reaching the checker means the resolver did not run,
reported as `UnimportedVariantPattern` and contributing no exhaustiveness coverage.

### Constants
`constants: HashMap<String, Type>` holds both module-level and body consts. `is_const_expr`
validates the RHS (literals, arithmetic on literals, casts, identifiers referring to other known
consts); a body `Stmt::Const` is validated in `check_stmt`. `Expr::Identifier` falls back to
`constants` after the symbol table, so const names work in any expression context. Errors:
`ConstAlreadyDefined`, `InvalidConstExpr`.

### Pool blocks
`type_checkers/pools.rs` carries the escape rules `pool { }` needs. A pool's arena is
released at the block's closing brace, so anything that carries arena memory past it, or jumps
over it, is rejected there and nowhere else. `pool_stack` holds one `PoolContext` per open block,
recording two floors taken at the opening brace: the symbol-table scope index, which tells the
block's own bindings from the ones it inherits, and the `loop_stack` depth, which tells a jump
that stays inside the block from one that leaves it.

The checks hang off the statement arms that already know the types: every assignment form calls
`check_pool_store` with the PLACE's type and the VALUE expression, `Stmt::DerefAssignment` calls
`check_pool_ref_store` with the referent type and the value (the place behind a reference is not
resolved here, so the referent type stands in for it), and `Return` / `Break` / `Continue` call the
control-flow pair. A sixth, `check_pool_retention`, hangs off the `Expr::Call` arm instead,
because what it watches is not a store the block writes at all. All six are inert when
`pool_stack` is empty, which is every program that writes no `pool`.

One rule there is not an escape rule. `check_pool_construction` refuses a value of a `Drop`-only
struct that the block would OWN: the arena is released in a single store and cannot run an
arbitrary destructor per object. It reads THROUGH aggregates rather than at the annotation alone
(`drop_only_within`): an array, a tuple, a newtype, an enum payload or another struct's field that
reaches a `Drop`-only type costs the arena the same per-object destructor as a bare one, and the
diagnostic names the inner type it found. The walk stops at a `PoolAware` type, which answers for
everything it holds, and carries a visited set so a nominal cycle cannot recurse forever. It hangs off the two `check_expr` arms that hand the block a
fresh owned value, `Expr::StructLiteral` and `Expr::Call`, and the call form names the callee in
the diagnostic (`PoolDropOnlyValue`). `drop_structs` is filled by `register_drop_impl` during the
declaration pass, so it is complete before any body is checked. The opt-out is the prelude's
`PoolAware` trait: an `impl PoolAware for T` lands in `trait_impls` through the ordinary
conformance path, and a pair present there is accepted. `PoolAware` is therefore NOT a lang-item
here the way `Drop` and `Hashable` are. Only its name is known, and its shape is checked by
`check_trait_conformance` against the prelude declaration like any user trait.

What may cross the boundary is decided by the place's TYPE and by the value's PROVENANCE, in that
order. A type carrying no pointer at all (the scalars, `void`, and an enum, a newtype, an array or
a tuple built only out of those) crosses unconditionally, which is why `total = total + 1` compiles inside a pool.
`pool_safe` reads an enum's payloads and a newtype's inner type out of `enum_defs` and
`newtype_defs` rather than trusting the name: a payload may be non-`Copy`, so `Option<string>`
holds a pointer exactly as a `string` does (BUG-041).
Everything else has to pass `carries_no_arena`, which returns true only where the source PROVES
the value holds no arena memory:

- a literal, including a `string` one, whose bytes live in `.rodata` rather than an allocation;
- a binding of pointerless type, or one declared before the OUTERMOST open pool (the outermost,
  not the innermost: an enclosing block's arena outlives a nested block's release too);
- a name or a path that is not a binding at all: a unit variant (`None`, `Msg::Empty`), a
  constant or a function item;
- `&e`, `*e`, `(e)`, `e as T` and a unary operator over a value that passes;
- a call whose provenance is provable and whose receiver and every argument also pass. A
  function this program DECLARES — a free function found in `functions`, or an associated
  function / method found through `impl_methods` — is always provable: its body is emitted with
  the backend's pool depth back at zero, so what it allocates comes from libc. The operand walk
  is what rules out its handing back arena memory it was given.

Everything else is arena memory by assumption. The walk takes an `Emission` because provenance
depends on where the backend puts the value, and the two call sites want different answers.

`Emission::InPlace` is the reading for an argument handed to a callee (`check_pool_retention`):
the value is emitted where it was written, inside the pool, so anything inlined there takes the
bump path. A builtin or collection method is therefore not provable — its body is not a function
at all but instructions emitted at the call site.

`Emission::Routed` is the reading for a store (`check_pool_store`, `check_pool_ref_store`),
because the language routes an allocation whose owner outlives the block to the heap and the backend
does exactly that: `store_outside_pool` in `llvm-backend` emits the whole statement with
`pool_depth` at zero. A builtin's inlined allocation then lands on libc too, so it becomes
provable, and so do `a + b` and `"row {n}"`, which allocate one fresh buffer at the point they
are written. What routing cannot do is move a buffer allocated EARLIER, which is why the operand
walk still runs: `out = local` and `out = local + "c"` stay refused where `local` is the block's.

Dynamic dispatch is the one exclusion neither emission rescues, and the language rule names it: behind a
vtable the implementation is not known until runtime and neither is what it allocates.
`dispatches_dynamically` reads the receiver's type for a `Type::DynObject` referent, and a `dyn`
call fails the routed reading as well as the in-place one.

The store rules above see only places the block's own text writes. A store a CALLEE performs
is written in the callee, so `check_pool_retention` covers it separately: a call inside a pool
is refused when an argument fails the in-place reading AND the call gives the callee write access to a
place declared before the outermost open pool. Write access is read from the SIGNATURE, never
from the callee's body — a `&mut self` receiver (found in `mut_self_methods`) and a `&mut T`
parameter are the complete set of channels a callee has back into its caller, and whether the
body actually stores through one is not asked. That over-approximates in the same direction
`carries_no_arena` does: unproven means refused. Two things narrow it without weakening it. An
argument whose declared parameter type is pointerless hands over no address, whatever
expression computed it (BUG-043). And `outliving_root` skips a root binding of pointerless type,
which has nowhere to keep one, so `put(&mut n, i + 1)` with `n: i32` is accepted through a
generic `&mut T`. A generic free function is not in `functions`, so `declared_params` reads its
template signature from `generic_funcs` instead, with the type parameters left abstract (BUG-044).

A collection's `push` and `insert` keep their argument the way a `&mut self` method does, but a
builtin has no signature to read. `resolve_collection_method` therefore calls
`check_pool_collection_store` itself for any method with a `ParamSlot::Value` position, passing
the element and key types the surface resolved, and the same diagnostic
(`PoolValueRetainedByCallee`) names the method (BUG-042).

Both rules resolve their callee through the shared `callee_key`, which returns the key into
`functions` for a free function, a `Type::method` path, or a method on a struct receiver, and
`None` otherwise. `receiver_struct` resolves the receiver through a field chain
(`outer.inner.stash(..)`) one `struct_defs` step at a time, so a nested receiver is checked
like a direct one (BUG-051). `callee_provenance_is_provable` is `callee_key(..).is_some()`,
plus the routed relaxation above. Note the two rules want OPPOSITE conservatism from it — an unnameable callee is assumed to
allocate arena memory (safe) but cannot be shown to retain any (unsafe). That is why every
callee the retention rule can name has to be resolvable: a generic template and a nested
receiver were once unnameable, and each let a callee keep arena memory unchecked.

One asymmetry the parameter walk has to handle: an instance method's signature in `functions`
carries the implicit `self` as `params[0]`, as the bare struct type rather than a reference, so
a receiver's mutability is only ever in `mut_self_methods`. `Expr::Call`'s `args` exclude the
receiver, so the walk skips `params[0]` exactly when the callee is a field-access expression —
the same test `callee_operand` uses.

### Three rules that exist because the backend cannot answer them
Each closed a path where a program type-checked and then aborted codegen with an internal error:

- **`VoidBinding`** (BUG-016). A binding whose initializer has type `void` is rejected in the
  `Stmt::VarDecl` arm, beside the `Type::Unknown` guard: the error is recorded and, unlike the
  reported-`Unknown` case above, the name is left undefined. The arm also covers a DIVERGING
  initializer, whose `Type::Unknown` reaches it unreported: `val x = panic("boom")` used to
  type-check and then abort codegen with the same internal error this rule was written to
  close. Testing the binding's TYPE rather than its initializer's
  shape is what makes one check cover every spelling: a `void` call is only two of them, the
  others being an `if`, a `match`, a bare block, a `loop { break }`, and an explicit `: void`
  annotation. Statement position is untouched, so `println("hi")` on its own line still compiles.
- **`MissingPartialEqImpl`** (BUG-015). `check_binary_expr`'s equality arm asks
  `has_builtin_equality` (`expressions/operators.rs`) whether the operand type has equality
  without an impl: the scalars (half-precision included), `string` after `peel_string_ref`, and a
  newtype forwarding one of those. A `Type::Generic` answers yes: a generic body is checked once
  as a template, so the instantiation is `hir-lowering`'s to refuse. A struct operand is reported
  as the missing trait; every other operand (array, tuple, enum, collection, non-string reference)
  reuses `InvalidBinaryOperator`, which is what the ordering comparisons already gave. The
  operator-trait dispatch above the arm is untouched, so an explicit `impl PartialEq` compiles as
  before.
- **`FunctionUsedAsValue`** (BUG-013). `Expr::Identifier` resolution consults `functions` and
  `generic_funcs` before falling through to `UndefinedVariable`, so a function name in value
  position is told apart from a name that does not exist. No coercion was added: a function is
  still not a value.
