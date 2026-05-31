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
type and records an arity diagnostic when the argument count is wrong. The first
intrinsic is `string.len() -> u64` (§2.7).

`&mut self` and consuming `self` methods are rejected at registration time with
`UnsupportedSelfParam` until ownership semantics land (Phase 1.5).

Constant declarations (`const NAME: Type = expr`): the `constants: HashMap<String, Type>` field
in `TypeChecker` holds both module-level and function-body consts. `is_const_expr` validates
that a RHS is a constant expression (literals, arithmetic on literals, casts, and identifiers
that refer to other known consts). Function-body `Stmt::Const` nodes are validated in
`check_stmt`. `Expr::Identifier` resolution falls back to `constants` after the symbol table,
so const names are usable in any expression context.

## Recent Updates
- 2026-05-31: Builtin method dispatch on primitive & string types §2. New private
  `resolve_builtin_method(recv, method, args, span)` in `type_checkers/expressions.rs`;
  the `Call`→`FieldAccess` arm consults it for non-struct receivers before emitting
  `MethodNotFound`. First intrinsic: `string.len() -> u64`. Wrong arity yields
  `ArgumentCountMismatch`. Integer `wrapping_*`/`saturating_*`/`.shr(n)` remain unblocked
  follow-ups.
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
