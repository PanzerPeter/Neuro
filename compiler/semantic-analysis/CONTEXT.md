# semantic-analysis

## Purpose
Validate type correctness and scope rules of a parsed Neuro program before code generation is attempted.

## Entry Point
- Type: Library function
- Input: `items: &[Item]`
- Output: `Result<Vec<Warning>, Vec<TypeError>>` — `Ok` carries non-fatal lint warnings; `Err` carries fatal type errors. Warnings are dropped when errors are present.

## Data Ownership
- Tables: none
- Events Published: none
- Events Consumed: none
- Public Read Model: none

## Shared Kernel
- ast-types — read-only traversal of `Item`, `Expr`, `Stmt` nodes
- shared-types — `Span` embedded in every `TypeError` for diagnostic location
- diagnostics — error type infrastructure

## Notes
Fail-slow strategy: all type errors are collected in a single pass so the developer
sees the complete error set in one compilation. `syntax-parsing` appears only in
`[dev-dependencies]` (integration tests); it is not a production dependency.

Five-pass strategy within `check_program`:
1. Pre-register all `Item::Struct` definitions into `struct_defs`.
2. Pre-register all `Item::Impl` method signatures into `functions` (mangled as
   `StructName__methodName`) and into `impl_methods` (struct → method → mangled key).
3. Pre-register all `Item::Const` names and types into `constants` (enables forward
   references and cross-function visibility without ordering constraints).
4. Full-check pass: `check_function` for each `Item::Function`, `check_impl` for each
   `Item::Impl`, `check_const_item` for each `Item::Const`.
5. Lint pass: walk function and method bodies via `run_lints` collecting non-fatal
   `Warning`s. Currently implements `prefer-loop-over-while-true` (§3.7); silenced by
   `@allow(prefer_loop_over_while_true)` on the enclosing function/method. Lints run
   independently of type errors so style guidance reaches the developer even when the
   program also has type errors (warnings are dropped from the final `Err` return
   value, but they are still collected for tests that inspect the checker directly).

Struct types use nominal typing — two `Type::Struct` values are compatible iff their
names are equal.

`impl` method scoping: `check_impl` binds `self` as an immutable variable of the
struct type, then binds the remaining parameters, before checking the method body.

Method calls (`instance.method(args)`) are recognised in `check_expr` when the `Call`
node's `func` is a `FieldAccess`. The object's struct type drives a lookup into
`impl_methods` to find the mangled function name, then arity and argument types are
validated against the registered signature (skipping param[0] which is `self`).

Associated function calls (`TypeName::func(args)`) are recognised in `check_expr`
when the `Call` node's `func` is an `Expr::Path`. The mangled name is reconstructed
as `TypeName__funcName` and looked up directly in `functions`.

Builtin method dispatch: when a method-call receiver is a non-struct (primitive or
string) type, `resolve_builtin_method` checks it against a fixed, compiler-known set of
intrinsics before falling through to `MethodNotFound`. It returns the method's result
type and records an arity diagnostic when the argument count is wrong. Intrinsics:
`string.len() -> u64` and `string.clone() -> string` (§2.7, both nullary); and on any
integer receiver `wrapping_{add,sub,mul}`, `saturating_{add,sub,mul}`, and `.shr(n)`
(§1.2, §1.4) — each takes one same-typed argument (validated by
`check_unary_int_intrinsic_arg`) and returns the receiver type.

Panic-family builtins (§1.2): `check_plain_call` consults `resolve_panic_builtin` before
ordinary function resolution, but only when no user function of the same name is registered
(a user `func panic(...)` shadows the builtin). The builtins are `panic(msg: string)`,
`assert(cond: bool)`, and `unreachable()`; each validates arity and argument type
(`ArgumentCountMismatch` / `Mismatch`) and returns `Type::Unknown`. The result is `Unknown`
(the "compatible with everything" type) rather than `Void` because the call **diverges**
(aborts), so it must satisfy any context — a unit statement, a non-`void` tail return
(`func f() -> i32 { panic(..) }`), or a value binding — until a dedicated `!`/never type lands.
Lowering lives entirely in `llvm-backend`.

`&mut self` and consuming `self` methods are rejected at registration time with
`UnsupportedSelfParam` until ownership semantics land (Phase 1.7).

Move-by-default ownership (§2.2, `type_checkers/moves.rs`): a non-`Copy` value is
*moved* out of its source binding when placed into a new owner — a `val`/`mut`
initializer, an assignment RHS, a `return` value, a struct-field assignment value,
or a by-value call argument. `record_move` marks the source binding moved (only when
the consumed expression is a bare place identifier of a move-tracked type;
`Type::is_move_tracked()` returns `true` for `Type::String` only — structs become
tracked when `Copy`/`@derive(Copy)` lands). Reading a moved binding emits
`UseOfMovedValue` (carrying the original move span) from the `Expr::Identifier` arm.
`SymbolInfo.moved_at: Option<Span>` holds the per-binding state; reassigning a `mut`
clears it. `.clone()` borrows its receiver, so it does not move — the canonical
opt-out. The analysis is conservative: `if`/`while`/`for` bodies and `if`-expression
arms snapshot/restore move state (`SymbolTable::snapshot_moves` / `restore_moves`) so
a conditional move never leaks onto a path that did not execute it. It may miss some
moves (e.g. second-iteration loop moves) but never rejects a valid program.

Constant declarations (`const NAME: Type = expr`): the `constants: HashMap<String, Type>` field
in `TypeChecker` holds both module-level and function-body consts. `is_const_expr` validates
that a RHS is a constant expression (literals, arithmetic on literals, casts, and identifiers
that refer to other known consts). Function-body `Stmt::Const` nodes are validated in
`check_stmt`. `Expr::Identifier` resolution falls back to `constants` after the symbol table,
so const names are usable in any expression context.

## Recent Updates
- 2026-06-07: Move semantics by default §2.2 (Phase 1.7). New `type_checkers/moves.rs`
  with `record_move`; `SymbolInfo.moved_at` + `mark_moved`/`clear_moved`/`snapshot_moves`/
  `restore_moves` on `SymbolTable`; `Type::is_move_tracked()`; new `TypeError::UseOfMovedValue`.
  Consuming positions in `statements.rs` (VarDecl/Assignment/Return/FieldAssignment) and
  `expressions.rs` (all three call-argument loops) record moves; the `Expr::Identifier` read
  arm reports use-after-move. Conditional regions snapshot/restore so moves do not leak across
  branches. Tracked types limited to `string` (only non-`Copy` type today).
- 2026-06-05: Struct functional-update type-checking (§3.3) in `Expr::StructLiteral`
  (`type_checkers/expressions.rs`). When `base` is present, the base expression is checked
  against `Type::Struct(name)` (mismatch → `TypeError::Mismatch`) and the missing-field scan is
  skipped — `..base` supplies every unlisted field. Without a base the existing
  `MissingStructField` check is unchanged. Field-init shorthand needs no semantic change: the
  parser already lowered it to an `Expr::Identifier`, so an undefined name surfaces as the
  ordinary undefined-variable error.
- 2026-06-05: `string.clone() -> string` builtin §2.7 (Phase 1.7). New `(Type::String, "clone")`
  arm in `resolve_builtin_method` (`type_checkers/expressions.rs`): nullary, returns `Type::String`,
  records `ArgumentCountMismatch` when given arguments. Non-`string` receivers still fall through to
  `MethodNotFound`. Mirrored independently in `llvm-backend` (`BuiltinMethod::StringClone`).
- 2026-06-04: Panic runtime §1.2 (Phase 1.7). New `resolve_panic_builtin` in
  `type_checkers/expressions.rs` recognizes `panic(string)`, `assert(bool)`, `unreachable()`
  before ordinary resolution in `check_plain_call`; consulted only when no user function of the
  same name exists (user functions shadow). Each returns `Type::Unknown` (divergent — satisfies
  any return/binding context); wrong arity/type reuse `ArgumentCountMismatch` / `Mismatch`. No
  new error variants.
- 2026-06-04: Added `Expr::Unsafe` type-checking (Phase 1.7 groundwork). Treated identically to
  `Expr::Block`: pushes a scope and yields the trailing expression's type. `unsafe` is inert —
  no special semantics, no new diagnostics.
- 2026-05-31: Integer primitive methods §1.2, §1.4. `resolve_builtin_method` now resolves
  `wrapping_{add,sub,mul}`, `saturating_{add,sub,mul}`, and `.shr(n)` on integer receivers,
  returning the receiver type. New `check_unary_int_intrinsic_arg` enforces arity 1 and an
  argument type compatible with the receiver (`ArgumentCountMismatch` / `Mismatch`).
- 2026-05-31: Builtin method dispatch on primitive & string types §2. New private
  `resolve_builtin_method(recv, method, args, span)` in `type_checkers/expressions.rs`;
  the `Call`→`FieldAccess` arm consults it for non-struct receivers before emitting
  `MethodNotFound`. First intrinsic: `string.len() -> u64`. Wrong arity yields
  `ArgumentCountMismatch`.
- 2026-05-20: Added lint infrastructure (§3.7). New `Warning` / `WarningCode` types in `warnings.rs`; `TypeChecker` accumulates a `warnings: Vec<Warning>` collected by `run_lints` in a final pre-return pass. First implemented lint: `prefer-loop-over-while-true` — fires on any `Stmt::While { condition: Expr::Literal(Boolean(true), _), .. }`, suppressed by `@allow(prefer_loop_over_while_true)` on the enclosing `FunctionDef` / `MethodDef`. Parenthesised `while (true)` is deliberately not matched (acts as an explicit escape hatch). `type_check`'s public signature is now `Result<Vec<Warning>, Vec<TypeError>>`.
- 2026-05-18: Added `BinaryOp::NullCoalesce` rejection arm. Emits new `TypeError::OperatorNotYetSupported { op, hint, span }` with the hint "requires Option<T> / Result<T, E> — available in Phase 2", returns `Type::Unknown` so error recovery continues. Codegen never sees `??` while semantic-analysis is in the pipeline; the variant is parsed solely to lock in the R-to-L associativity from Appendix B row 14 ahead of the Phase 2 implementation.
- 2026-04-18: Integer literal type suffixes §1.4. `Literal::Integer(value, Some(suffix))` short-circuits `infer_integer_type`; `infer_suffixed_integer_type` maps the suffix to a `Type` via `suffix_to_type` and range-checks the value (reusing `check_integer_range` + `IntegerLiteralOutOfRange`). Unsuffixed literals that exceed the i32 range now emit an `IntegerLiteralOutOfRange` rather than silently promoting to `i64`.
- 2026-05-25: Float literal type suffixes §1.2/§1.4. `Literal::Float(value, Some(suffix))` short-circuits `infer_float_type`; `infer_suffixed_float_type` maps the `FloatSuffix` to `Type::F32` / `Type::F64` via `float_suffix_to_type`. Mismatched annotations (e.g., `val x: f32 = 1.5f64`) surface through the existing assignment type-check path.

- 2026-04-04: Updated `type_checker` to correctly destructure the new `inclusive` flag on `Stmt::ForRange`. No integer validation rules changed as bounds checking works the same for inclusive and exclusive endpoints.
- 2026-04-16: Implemented §1.3 const declarations: four-pass `check_program`, `constants` map,
  `register_const_item`, `check_const_item`, `is_const_expr`, `Stmt::Const` arm, identifier
  fallback to const map. New error variants: `ConstAlreadyDefined`, `InvalidConstExpr`.
- 2026-04-18: Implemented bitwise operator type checking §1.4. `BinaryOp::BitAnd/BitOr/BitXor/Shl`
  require both operands to be integer types (`is_integer()` — not float, not bool) and return the
  operand type. `UnaryOp::BitNot` requires an integer operand and returns the same type. Floats and
  bools produce `InvalidBinaryOperator` / `InvalidOperator` errors respectively.
- 2026-05-13: Implemented IEEE-754 native float comparison §1.2, §3.10 and strict inequality type bounds. Inequality operators (`<, >, <=, >=`) are restricted to `is_numeric()` types (ints and floats), correctly rejecting struct, string, and bool types that lack natural ordering, preventing codegen panics. IEEE-754 NaN handling works natively via LLVM `fcmp` generation.
- 2026-05-27: Comparison chain rejection §1.4. In `check_expr`, before type-checking comparison operands, the checker inspects whether the LHS of a comparison `Binary` node is itself a comparison `Binary`. If so, emits `TypeError::ComparisonChain { span }` and returns `Type::Unknown`. Covers all six comparison operators. Uses `BinaryOp::is_comparison()` helper added to `ast-types`.
