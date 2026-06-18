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

Five-pass `check_program`:
1. Pre-register all `Item::Struct` into `struct_defs` (and `@derive` Copy/Clone intent into
   `copy_structs`/`clone_structs`). Pass 1b runs `validate_copy_derive` per struct once all are
   registered (so a Copy field that is another struct resolves regardless of order).
2. Pre-register all `Item::Impl` method signatures into `functions` (mangled `StructName__methodName`)
   and `impl_methods` (struct → method → mangled key).
3. Pre-register all `Item::Const` names/types into `constants` (forward refs + cross-function
   visibility, no ordering constraint).
4. Full-check: `check_function` / `check_impl` / `check_const_item`.
5. Lint pass: `run_lints` walks bodies collecting non-fatal `Warning`s. Currently
   `prefer-loop-over-while-true` (§3.7), silenced by `@allow(prefer_loop_over_while_true)`. Lints run
   independently of type errors (warnings still collected for tests inspecting the checker, but
   dropped from the final `Err`).

Struct types are nominal — two `Type::Struct` are compatible iff names match.

`check_impl` binds `self` as a var of the struct type — **mutable for `&mut self`**, immutable for
`&self` — then the remaining params, before checking the body. A `&mut self` body may therefore
assign to `self.field` (§2.5).

Method calls (`instance.method(args)`) — recognised in `check_expr` when the `Call`'s `func` is a
`FieldAccess`; the object's struct type drives an `impl_methods` lookup for the mangled name, then
arity/arg types are validated (skipping param[0] = `self`). When the resolved method is in
`mut_self_methods` (a `&mut self` receiver), `check_mut_self_receiver` enforces the §2.5 borrow:
the receiver must be a `mut` place (or reached through `&mut T`) and must not already be borrowed —
the same coexistence rule as a `&mut place` borrow — registering a transient exclusive borrow that
clears at statement end. A `&T` receiver or a non-`mut` binding is `CannotBorrowMutably`; a live
borrow is `CannotMutablyBorrowWhileBorrowed`.

Associated calls (`TypeName::func(args)`) — recognised when `func` is an `Expr::Path`; mangled name
`TypeName__funcName` looked up directly in `functions`.

Builtin method dispatch: for a non-struct (primitive/string) receiver, `resolve_builtin_method`
checks a fixed compiler-known set before `MethodNotFound`, returning the result type (and an arity
diagnostic on wrong count). Intrinsics: `string.len() -> u64`, `string.clone() -> string` (§2.7,
nullary); and on any integer receiver `wrapping_{add,sub,mul}`, `saturating_{add,sub,mul}`, `.shr(n)`
(§1.2/§1.4) — each one same-typed arg (`check_unary_int_intrinsic_arg`), returns the receiver type.
A struct receiver's `.clone()` (§2.3) is a nullary builtin when the struct derives `Clone`/`Copy` and
no user `clone` method exists (user method shadows); returns the struct type.

Panic-family builtins (§1.2): `check_plain_call` consults `resolve_panic_builtin` before ordinary
resolution, only when no user function of the same name is registered (user `func panic(...)`
shadows). Builtins: `panic(msg: string)`, `assert(cond: bool)`, `unreachable()`; each validates
arity/type (`ArgumentCountMismatch`/`Mismatch`) and returns `Type::Unknown` — not `Void`, because the
call **diverges** (aborts) and must satisfy any context (unit stmt, non-`void` tail return, value
binding) until a dedicated `!`/never type lands. Lowering lives in `llvm-backend`.

Consuming `self` methods are rejected at registration with `UnsupportedSelfParam` (they need the
by-value struct ABI). `&mut self` is supported (§2.5): `register_impl` records its mangled key in
`mut_self_methods` for the call-site borrow check above.

Move-by-default ownership (§2.2, `type_checkers/moves.rs`): a non-`Copy` value is *moved* out of its
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

Borrow exclusivity (§2.4, §2.5, `symbol_table.rs` + the `Expr::Reference` arm): each binding tracks
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

Returned-reference outlives (§2.6, lifetime elision; `declarations.rs` + `statements.rs`): a function
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
- 2026-06-17: Returned-reference outlives / lifetime elision §2.6. New `current_fn_outliving:
  HashSet<String>` on `TypeChecker` (reference params + `self`), rebuilt in `check_function` /
  `check_impl` and cleared on exit. New `SymbolTable::borrow_provenance`. New
  `check_returned_reference` (+ free fns `tail_expr` / `root_place_name`, method
  `is_local_to_function`) invoked from `Stmt::Return` and both trailing implicit-return sites when the
  return type is a `Type::Reference`. New error `ReturnsReferenceToLocal`. Elision-only — no annotation
  surface; explicit `<'a>` awaits generics. Tests in `type_checkers/tests/mod.rs`.
- 2026-06-16: Borrow exclusivity §2.4/§2.5. `SymbolInfo` gained persistent/transient borrow counters
  plus a `borrows` provenance; new `SymbolTable` methods `borrow_counts` / `add_transient_borrow` /
  `attach_borrow` / `release_borrow_of` / `clear_transient_borrows`, and `pop_scope` now releases a
  dying reference binding's borrow. The `Expr::Reference` arm checks coexistence and registers the
  borrow as transient; `check_stmt` wraps `check_stmt_inner` to clear transient borrows at statement
  end; `VarDecl` / `Assignment` promote a direct `&place` initializer to a persistent borrow. New
  errors `CannotMutablyBorrowWhileBorrowed` / `CannotBorrowWhileMutablyBorrowed`. Tests in
  `type_checkers/tests/mod.rs`.
- 2026-06-16: `f16`/`bf16` half-precision primitives (§1.2). New `Type::F16`/`Type::BF16`: `"f16"`/`"bf16"`
  resolve in `resolve_type`; the `FloatSuffix::F16`/`BF16` literal suffixes infer to them; `is_half_float()`
  added. Narrow contract — Copy (not move-tracked), `==`/`!=` via the compatible-type path, `as`-cast
  to/from any numeric type and to/from each other (`is_valid_cast` half arms), but **no arithmetic**:
  `+ - * / %` on a half operand emits `TypeError::HalfFloatArithmetic` ("compute in f32"). `is_float()`
  deliberately still excludes halves so arithmetic/inference paths skip them.
- 2026-06-15: `char` primitive type (§1.2). New `Type::Char`: `"char"` resolves in `resolve_type`;
  `Literal::Char` infers to `Type::Char`; `is_valid_cast` permits char↔integer and char→char only
  (no float/bool); ordering comparisons (`< > <= >=`) accept `char` (built-in total order) alongside
  numerics; char is Copy (not move-tracked). Equality reuses the existing compatible-type path.
- 2026-06-15: `loop` as a value expression (§3.7). `loop_labels` became `loop_stack:
  Vec<LoopContext>` (label + `is_value_loop` + accumulated `break_value_ty`). `check_loop_body`
  pushes a context for while/loop/for; only `loop` (and `Expr::Loop`) is a value loop. A value
  `break v` calls `record_break_value`: it rejects a value targeting a `while`/`for`
  (`TypeError::BreakValueInUnitLoop`), sets the loop's type on first value-break, and reports
  `Mismatch` on a disagreeing later one. `Expr::Loop` returns the agreed type (or unit).
- 2026-06-15: Loop labels (§3.7). The `loop_depth: u32` counter is replaced by `loop_labels:
  Vec<Option<String>>` (innermost last). Each loop pushes its label (or `None`) for the duration of
  its body; `check_loop_control_label` validates `break`/`continue`: an unlabeled one needs a
  non-empty stack (else `BreakOutsideLoop` / `ContinueOutsideLoop`), a labeled one needs a matching
  active label (else the new `TypeError::UndefinedLabel`).
- 2026-06-09: `loop { ... }` statement (§3.7). `check_stmt` handles `Stmt::Loop` like `while`'s body
  (increments `loop_depth` so `break`/`continue` inside are in-loop; snapshot/restore moves around
  the body per §2.2), minus the condition. The `prefer-loop-over-while-true` lint walker recurses
  into `loop` bodies. No new error code — the construct is unconditionally valid.
- 2026-06-09: Mutable borrows `&mut T` + deref `*` (§2.5). `Type::Reference` is now
  `{ inner, mutable }` (Display `&mut T`; compatible only when mutability **and** referents
  match — no `&mut T`→`&T` coercion). `resolve_type` carries `mutable` through. The
  `Expr::Reference` arm rejects `&mut` of a non-`mut` binding (`CannotBorrowMutably`). New
  `Expr::Deref` arm: types `*r` to the referent, else `CannotDereference`. New
  `Stmt::DerefAssignment` checker: requires `pointer: &mut T`, else `CannotAssignThroughRef`
  (immutable ref) / `CannotDereference` (non-ref); the stored value is checked against the
  referent and move-recorded. New errors `CannotBorrowMutably` / `CannotDereference` /
  `CannotAssignThroughRef`. Unit tests in `types.rs` and `moves.rs`. Flow-sensitive aliasing
  exclusivity is deferred to lifetime inference.
- 2026-06-18: String concatenation `+` §2.7. The arithmetic arm of `check_expr` now peels each
  operand with `Type::peel_string_ref` before the numeric path: when both are `string` and the op
  is `+`, the result is `Type::String` (a new owned string). Any other arithmetic op on a string,
  or mixing a string with a non-string, is `InvalidBinaryOperator`. Operands are not consuming
  positions, so `+` borrows-to-read and never moves (matching `==`). Unit tests in
  `tests/expr_tests.rs`.
- 2026-06-09: `&string` slice equality §2.7. `&string` is a borrowed string slice; the
  `Equal`/`NotEqual` arm of `check_expr` now compares operands through `Type::peel_string_ref`,
  which normalizes `&string` → `string` (one layer, string only) so an owned `string` and a
  `&string` slice are equality-compatible in any combination. Other `&T` are left intact, so
  `&i32 == i32` and `i32 == &string` stay type errors. Comparison operands are not consuming
  positions, so borrowing for `==` never moves. Unit test in `types.rs`.
- 2026-06-08: Immutable borrows `&T` §2.4. New `Type::Reference(Box<Type>)` (Display `&T`; compatible
  iff referents are; `referent()` peels one layer). `resolve_type` maps `ast::Type::Reference`.
  `Expr::Reference` arm in `check_expr` requires a place (`is_place_expr`: identifier or
  parenthesised identifier; else `CannotBorrowValue`) and yields `&T` **without** moving the operand —
  borrowing never consumes. References are always `Copy` and never move-tracked (`is_type_copy` true,
  `is_type_move_tracked` false via its `_` arm). Method-call and field-access resolution auto-deref via
  `obj_ty.referent()`, so `r.len()` / `r.field` / `r.method()` work for `r: &string` / `r: &Struct`.
- 2026-06-07: `Copy` trait + `@derive(Copy, Clone)` §2.3. `copy_structs`/`clone_structs`
  (`HashSet<String>`) populated from `StructDef.attributes` in `register_struct`
  (`record_derive_intent`); pass 1b `validate_copy_derive` checks every field of a Copy struct is Copy
  (`CopyDeriveNonCopyField`). `Type::is_move_tracked` replaced by context-aware
  `is_type_move_tracked`/`is_type_copy` (a struct is move-tracked unless it derives Copy). Struct
  `.clone()` resolves in the method-call arm when Clone/Copy-derived and no user `clone` exists. Copy
  implies Clone; unknown derive args ignored.
- 2026-06-07: Move semantics by default §2.2. New `moves.rs` (`record_move`); `SymbolInfo.moved_at` +
  `mark_moved`/`clear_moved`/`snapshot_moves`/`restore_moves`; `UseOfMovedValue`. Consuming positions
  in `statements.rs` (VarDecl/Assignment/Return/FieldAssignment) and `expressions.rs` (call-arg loops)
  record moves; the `Expr::Identifier` read arm reports use-after-move; conditional regions
  snapshot/restore. Tracked types limited to `string` initially.
- 2026-06-05: Struct functional-update §3.3 in `Expr::StructLiteral`. With `base` present, the base is
  checked against `Type::Struct(name)` (mismatch → `Mismatch`) and the missing-field scan skipped.
  Shorthand needs no change (parser lowered it to `Expr::Identifier`).
- 2026-06-05: `string.clone() -> string` §2.7 — `(Type::String,"clone")` arm in
  `resolve_builtin_method` (nullary; args → `ArgumentCountMismatch`). Mirrored in `llvm-backend`.
- 2026-06-04: Panic runtime §1.2 — `resolve_panic_builtin` recognizes `panic`/`assert`/`unreachable`
  in `check_plain_call` before ordinary resolution (user funcs shadow); each returns `Type::Unknown`;
  wrong arity/type reuse `ArgumentCountMismatch`/`Mismatch`. No new variants.
- 2026-06-04: `Expr::Unsafe` type-checking (Phase 1.7) — identical to `Expr::Block` (pushes a scope,
  yields the trailing expr's type). Inert.
- 2026-05-31: Integer primitive methods §1.2/§1.4 — `resolve_builtin_method` resolves
  `wrapping`/`saturating`/`.shr(n)` on integer receivers; `check_unary_int_intrinsic_arg` enforces
  arity 1 + compatible arg type.
- 2026-05-31: Builtin method dispatch on primitive/string §2 — `resolve_builtin_method` in
  `expressions.rs`; the `Call`→`FieldAccess` arm consults it before `MethodNotFound`. First:
  `string.len() -> u64`.
- 2026-05-27: Comparison chain rejection §1.4 — `check_expr` emits `ComparisonChain` when a
  comparison's LHS is itself a comparison (all six ops). Uses `BinaryOp::is_comparison()` (ast-types).
- 2026-05-25: Float literal suffixes §1.2/§1.4 — `infer_suffixed_float_type` maps `FloatSuffix` →
  `F32`/`F64`; mismatched annotations surface via the assignment type-check path.
- 2026-05-20: Lint infra §3.7 — `Warning`/`WarningCode` (`warnings.rs`); `run_lints` final pass; first
  lint `prefer-loop-over-while-true` (`while true`, suppressed by `@allow(...)`; parenthesised
  `while (true)` deliberately not matched). Public signature now `Result<Vec<Warning>, Vec<TypeError>>`.
- 2026-05-18: `BinaryOp::NullCoalesce` rejection — `OperatorNotYetSupported { op, hint, span }`
  (hint: "requires Option<T>/Result<T,E> — Phase 2"), returns `Unknown` for recovery. `??` is parsed
  only to lock in R-to-L associativity ahead of Phase 2.
- 2026-05-13: IEEE-754 native float comparison §1.2/§3.10 — inequalities (`<`,`>`,`<=`,`>=`)
  restricted to `is_numeric()`, rejecting struct/string/bool (prevents codegen panics). NaN handled
  natively via LLVM `fcmp`.
- 2026-04-18: Integer literal suffixes §1.4 — `infer_suffixed_integer_type` via `suffix_to_type` +
  range check (`IntegerLiteralOutOfRange`). Unsuffixed literals over i32 range now error rather than
  silently promoting to i64.
- 2026-04-18: Bitwise type checking §1.4 — `BitAnd/BitOr/BitXor/Shl` require integer operands, return
  the operand type; `BitNot` requires integer. Floats/bools → `InvalidBinaryOperator`/`InvalidOperator`.
- 2026-04-16: Const declarations §1.3 — `constants` map, `register_const_item`, `check_const_item`,
  `is_const_expr`, `Stmt::Const` arm, identifier fallback. New: `ConstAlreadyDefined`, `InvalidConstExpr`.
- 2026-04-04: `Stmt::ForRange` `inclusive` flag destructured; no integer validation change.
