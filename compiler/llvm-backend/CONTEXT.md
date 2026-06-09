# llvm-backend

## Purpose
Emit native object code from a type-checked Neuro AST via LLVM IR generation.

## Entry Point
- Type: Library function
- Input: `items: &[Item], optimization: OptimizationLevelSetting, source: &str, source_path: &str`
- Output: `Result<Vec<u8>, CodegenError>`

`source` / `source_path` are the original module text and path, wrapped in a
`source_location::SourceFile` solely to render `file:line:col` in panic-family runtime
diagnostics (§1.2). They do not affect type-checking or codegen elsewhere.

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- ast-types — read-only traversal of the type-checked AST
- shared-types — type system primitives
- diagnostics — error type infrastructure
- source-location — `SourceFile` byte-offset → line/column mapping for panic diagnostics (§1.2)

## Notes
inkwell 0.9.0 (feature `llvm20-1`, LLVM 20 bindings) is a third-party crate, not Shared Kernel.
Requires LLVM 20 with MLIR enabled; set `LLVM_SYS_201_PREFIX` (e.g. `/usr/lib/llvm20`) before building.
`semantic-analysis` is not a production dependency — neurc orders type-check before codegen.
`syntax-parsing` appears only in `[dev-dependencies]` (integration tests).

## String ABI
`string` = anonymous LLVM struct `{ ptr, i64 }`:
- field 0 (`ptr`): pointer to null-terminated UTF-8 bytes in `.rodata`
- field 1 (`i64`): byte count **excluding** the null terminator

Literals are emitted to `.rodata`, never heap-allocated; the appended NUL
(`STRING_NULL_TERMINATOR` in `literals.rs`) exists only for C-string FFI validity. `len` is
authoritative — interior NULs are legal counted content, so consumers must not treat data as
NUL-terminated. Phase 1.7 heap strings share this exact ABI (indistinguishable to consumers).

Passed/returned by value. On x86-64 SysV this fits two registers, so no sret indirection.
The semantic `Type::String` is unchanged — the fat-pointer layout is a backend-only detail.

`==`/`!=` on strings lower to a length check + libc `memcmp` (universally available). The length
check uses `select` to pass `n=0` to `memcmp` when lengths differ (safe, no extra blocks). A
`&string` slice (§2.7) is just a pointer to the fat pointer, so `codegen_binary` handles string
`Equal`/`NotEqual` *before* the numeric coercion: each operand is run through `load_string_fatptr`
(load through the pointer for a borrow, pass through an owned struct value) and then `codegen_string_eq`.
Detection keys off `left_ty.referent() == String`, covering owned, borrowed, and mixed operands.

## Struct ABI
User structs lower to anonymous LLVM structs `{ T0, T1, ... }` in declaration order (no padding —
LLVM handles alignment). Values live on the stack via `alloca`, initialised field-by-field with
`insertvalue`; reads = `getelementptr`+`load`, writes = `getelementptr`+`store`. Not yet usable as
function params/returns (Phase 2+; type mapper errors there). Layout in `CodegenContext.struct_defs`;
`get_struct_llvm_type` rebuilds the type on demand (LLVM dedups by structure).

## Method ABI
`impl` methods lower to LLVM free functions mangled `StructName__methodName` (double underscore;
the identifier grammar forbids `__` in user names).

`&self` methods take the struct **by value** as `param[0]` (named `self` in the alloca map) —
correct for read-only access; callers load their stack var and pass the value. `&mut self` /
consuming `self` are rejected by semantic analysis before codegen.

Associated functions (no `self_param`) lower identically without the implicit first param; callers
invoke `TypeName::func(args)` → `codegen_call("StructName__funcName", args)`.

Method calls (`instance.method(args)`) are recognised in `codegen_expr` when the `Call`'s `func` is
a `FieldAccess`. `fa_struct_names` (keyed by `Call` span start) carries the struct name so
`codegen_method_call` reconstructs the mangled name without re-querying the AST.

## Builtin Method ABI
Intrinsics on non-struct (primitive/string) receivers resolve in `resolve_builtin_method`
(`context.rs`) during the type-collection pass, which tags the `Call`'s full span `(start, end)` in
`builtin_methods` with the result type. `codegen_expr` checks `builtin_methods` before the struct
path and lowers via `codegen_builtin_method`. Keyed by full span (not `span.start`) because chained
calls (`s.clone().len()`) nest two `Call` nodes sharing `span.start` — same workaround as
`binary_left_types`.

- `string.len()` → `extractvalue` field 1 (O(1) stored byte length, u64, no conversion).
- `string.clone()` (§2.7) → the receiver's own fat-pointer value: strings are immutable /
  `.rodata`-backed, so a `{ ptr, len }` copy is observationally deep. Must duplicate the buffer
  once heap strings land.
- `struct.clone()` (§2.3) → handled in the type-pass struct method-call arm (not
  `resolve_builtin_method`, which is keyed by `Type`): when receiver is a struct, field is `clone`,
  and no `StructName__clone` exists, it tags `BuiltinMethod::StructClone`. Semantic analysis already
  verified the `Clone` derive. Lowers to the receiver's aggregate value (faithful while
  stack-allocated; must recurse into heap-owning fields later).
- Integer intrinsics — `wrapping_{add,sub,mul}`, `saturating_{add,sub,mul}`, `.shr(n)` — resolve on
  any integer receiver to its own type, lower in `codegen_int_intrinsic` (`expressions.rs`). Both
  operands coerced to the receiver int via `coerce_if_needed` (arg literal may arrive widened to
  i32). Wrapping → plain `add`/`sub`/`mul` (no `nsw`/`nuw`, never traps). `.shr` → `ashr` (signed) /
  `lshr` (unsigned). `saturating_add`/`sub` → `llvm.{s,u}{add,sub}.sat`. `saturating_mul` (no direct
  intrinsic) → `{s,u}mul.with.overflow` + `select` (unsigned→MAX; signed→MIN on differing operand
  signs, else MAX).

`resolve_builtin_method` / `is_panic_builtin` are duplicated from `semantic-analysis` to keep the
backend independent of the type-checker slice.

## if / else-if / else Lowering
`codegen_if` lowers an `if/else if+/else?` chain as a binary tree: each call creates `then`/`else`/
`ifcont` blocks; the `else` block hosts the final `else` body or recursively calls `codegen_if` with
the first remaining `else_if` arm (`split_first` recursion), so every arm is mutually exclusive and
the final `else` is reached only when all conditions are false.

A value-producing `if`/`else` in expression position → `codegen_if_expr`: a result `alloca` written
per arm, loaded at the merge block. A statement-position `if` (`Stmt::If`) that is the *tail* of a
function/method body is routed through `codegen_if_expr` by `codegen_body` (else the non-void
function emits `unreachable` at merge and runs off its end). `record_tail_if_type` (type pass)
records the tail `if`'s result type so the slot can be allocated (mirrors the `Expr::If` arm).

## Logical Operator Lowering
`&&`/`||` short-circuit (§1.4). `codegen_binary` intercepts them before eager operand evaluation and
delegates to `codegen_short_circuit`: evaluate LHS in the current block, conditionally branch to a
`logic.rhs` block (taken only on the deciding edge — true for `&&`, false for `||`), and merge the
RHS value with the short-circuit constant (`false`/`true`) via a phi in `logic.merge`. Both phi
predecessors are captured *after* their side is emitted (`get_insert_block`), so an RHS that appends
blocks (nested if-expr) works; an RHS that terminates its block is dropped from the phi. Operands are
guaranteed `i1` by semantics. The eager `And | Or` arm is now an unreachable ICE guard.

## Integer Overflow ABI
Integer `+`/`-`/`*` honor the §1.2 rule, keyed off `OptimizationLevelSetting`:
- `-O0` → `overflow_checks = true`. `codegen_int_arith` emits `llvm.{s,u}{add,sub,mul}.with.overflow`,
  extracts `{result, overflow_bit}`, conditionally branches to a per-op `arith.overflow` block
  (`llvm.trap` + `unreachable`); execution continues in `arith.cont` with the result.
- `-O1..-O3` → `overflow_checks = false`. `emit_wrapping_int_arith` emits plain
  `build_int_add/sub/mul` (two's-complement wrap).

Signedness picks the `s`/`u` variant via `TypeMapper::is_unsigned_int`. Division, modulo, bitwise,
floats unaffected. The `FoldedConst` compile-time path always wraps.

## Panic Runtime ABI
Panic-family builtins `panic(msg: string)`, `assert(cond: bool)`, `unreachable()` (§1.2) lower in
`panic.rs`. Contract: **abort, no unwinding** — no landing pads, so the happy path is zero-cost and
`Drop`/`defer` (future) fire only on normal scope exit. The `Call`→`Identifier` arm
(`expressions.rs`) intercepts these names via `CodegenContext::is_panic_builtin` before
`codegen_call`, but only when no user function of the same name is registered (user functions shadow,
matching the semantic resolver).

Each builtin writes its diagnostic to stderr (fd 2) via external POSIX `write`
(`get_or_declare_write`), then calls libc `abort` (`get_or_declare_abort`, `noreturn`) + an
`unreachable` terminator:
- `panic` → write `"panic: "`, the msg fat-ptr, `" at file:line:col\n"`, abort.
- `unreachable` → write `"internal error: entered unreachable code at file:line:col\n"`, abort.
- `assert` → true falls through to `assert.cont`; false enters `assert.fail` (write
  `"assertion failed at file:line:col\n"`, abort).

The dynamic `panic` message is a runtime `string` fat ptr; fields read via `extractvalue` and passed
straight to `write`. The `file:line:col` suffix comes from the `Call` span start via the `SourceFile`
(empty when no source supplied). `write`+`abort` are POSIX/libc (Linux, macOS; MSVC CRT on Windows).

Because `panic`/`unreachable` terminate the block with `unreachable`, following statements are dead
code: `codegen_stmt` early-returns when the block is already terminated, and `codegen_return` /
`codegen_body`'s tail path skip the `ret` when evaluating the returned expr terminated the block
(`func f() -> i32 { panic("x") }`). Keeps LLVM from seeing instructions after a terminator.

## Constant Declarations ABI
Module-level consts emit as `@NAME = internal constant TYPE VALUE` globals before any function defs;
their LLVM value is also stored in `CodegenContext.const_values` so body references resolve without
loading from the global. Body `Stmt::Const` nodes fold in Rust (`FoldedConst`) and store the
`BasicValueEnum` in `const_values` for the function scope — no `alloca`, purely compile-time.

Folding uses a pure-Rust `FoldedConst { Int(i64), Float(f64), Bool(bool), Str(String) }` rather than
inkwell's const-arithmetic API (inconsistent across versions): all arithmetic in Rust (wrapping ints,
IEEE-754 floats); a single `const_int`/`const_float`/`const_struct` builds the final LLVM value.
`global_const_types: HashMap<String, Type>` carries module-level const types, re-seeded into
`type_env` after each `type_env.clear()` (`visit_function_for_types` / `visit_method_for_types`) so
type inference resolves const identifiers in bodies.

## Future: MLIR Integration (Phase 3+)
When tensor ops land, `melior` (Rust MLIR bindings, same LLVM 20 / MLIR 20 install) joins inkwell.
Lowering: AST → Neuro High-Level IR → MLIR dialects (linalg/tensor/func/arith) → Enzyme MLIR AD pass
→ GPU dialects (nvgpu/rocdl) or `llvm` dialect → inkwell for final LLVM IR. inkwell stays the terminal
emission layer in all paths.

## Recent Updates
- 2026-06-09: `loop { ... }` statement (§3.7). `codegen_loop` mirrors `codegen_while` without a
  condition block: it branches unconditionally into `loop.body` and back to its top, so the only
  exit is a `break` (pushes `LoopTargets { continue_bb: body, break_bb: exit }` — `continue`
  re-enters the top). `type_pass` visits the body for type recording. A `break`-less `loop` leaves
  `loop.exit` without predecessors; the function epilogue supplies its terminator.
- 2026-06-09: Mutable borrows `&mut T` + deref `*` (§2.5). `&mut place` lowers like `&place` —
  `codegen_reference` returns the place's storage pointer (mutability is compile-time only; the
  backend `Type::Reference` is unchanged). New `Expr::Deref` (`codegen_deref`): loads the referent
  through the pointer, typed by the referent recorded in `type_pass`. New `Stmt::DerefAssignment`
  (`codegen_deref_assignment`): stores the value at the pointer. `type_pass` records the referent
  for a `Deref` and visits a `DerefAssignment`. Unit-returning calls are now valid in statement
  position: `codegen_call` / `codegen_method_call` return `Option` (None = void); the shared
  `codegen_call_dispatch` is wrapped with a void-error in value position and discarded by the
  `Stmt::Expr` call path.
- 2026-06-08: Immutable borrows `&T` §2.4. New backend `Type::Reference(Box<Type>)` (+ `from_ast`,
  `referent()`); `map_type` lowers any reference to an opaque `ptr`. `Expr::Reference` lowers to the
  storage pointer of the borrowed place (`codegen_reference` returns the alloca pointer — no load).
  Auto-deref is value-driven: a borrowed receiver lowers to a `PointerValue`, so `string_receiver_struct`
  (string `len`/`clone`), `StructClone`, `codegen_method_call` (self), and `get_struct_ptr_and_type`
  (field access) load through the pointer when they see one; an owned receiver is already a value.
  `resolve_builtin_method` auto-derefs `&string` for string methods but keeps integer intrinsics
  value-only. `type_pass` records `Reference(inner)` for borrows and peels `referent()` when computing
  struct names/mangling. No new context state — ref-ness is read from `variable_types` (a `&Struct`
  alloca holds a `ptr`) and from the lowered value kind.
- 2026-06-07: `struct.clone()` §2.3 — `BuiltinMethod::StructClone` (`context.rs`); type-pass struct
  method-call arm tags it when no `StructName__clone` exists; lowers to the receiver's aggregate value.
  Move/Copy is semantic-only — no other codegen change. See "Builtin Method ABI".
- 2026-06-05: Struct functional-update (§3.3). `codegen_struct_literal` (`codegen/structs.rs`) takes
  `base: Option<&Expr>`; with a base it seeds the aggregate from the base struct value (vs
  `get_undef()`), then explicit fields `insert_value` over it. `type_pass.rs` visits `base`. Shorthand
  needs no change (already an `Expr::Identifier`).
- 2026-06-05: `string.clone()` §2.7 — `BuiltinMethod::StringClone` + `(Type::String,"clone")` arm
  (`context.rs`); lowers to the receiver fat-ptr (value copy = deep while `.rodata`-backed). Re-keyed
  `builtin_methods` from `span.start` to `(start,end)` for chained calls. See "Builtin Method ABI".
- 2026-06-04: Panic runtime §1.2 — new `panic.rs`; `compile` gained `source`/`source_path`;
  `get_or_declare_write`/`abort` (`context.rs`); terminated-block guards in `codegen_stmt`/
  `codegen_return`/`codegen_body` tail. See "Panic Runtime ABI".
- 2026-06-04: `Expr::Unsafe` lowering (Phase 1.7) via `codegen_block_expr` like `Expr::Block` (shared
  `Expr::Block | Expr::Unsafe` type-pass arm). Inert — identical IR to a bare block.
- 2026-06-04: Fixed binary-operand type-map collision (§1.4). A binary node and its leftmost
  descendant share `span.start`, so a parent (`&&`, left `Bool`) clobbered its child comparison's left
  type, truncating i32 operands to i1 (`c >= 48 && c <= 57` wrong). Left types now in dedicated
  `binary_left_types` keyed by `(start,end)`. Regression: `neurc/tests/short_circuit_runtime.rs`.
- 2026-06-02: `&&`/`||` short-circuit (§1.4) via `codegen_short_circuit` (phi over `logic.rhs`/
  `logic.merge`); previously both operands ran (`build_and`/`build_or`). See "Logical Operator Lowering".
- 2026-06-02: `fold_const` (`literals.rs`) gained a `(Bool, Bool)` binary arm (`&&`/`||`/`==`/`!=`);
  `const FLAG: bool = true && false` previously ICE'd on the catch-all arm.
- 2026-06-02: Fixed tail-position `if`/`else` implicit-return miscompile. Unified
  `codegen_function`/`codegen_method` into `codegen_body`, which treats a trailing `Stmt::If { else:
  Some, .. }` as a value-producing if-expr; `record_tail_if_type` records its type. Previously emitted
  `unreachable` → fall-through segfault. See "if / else-if / else Lowering".
- 2026-06-02: Formalized string literal/runtime distinction §2.7 (`STRING_NULL_TERMINATOR` named).
  Behaviour unchanged. See "String ABI".
- 2026-05-31: Integer primitive methods §1.2/§1.4 — `Wrapping`/`Saturating`/`Shr` in `BuiltinMethod`
  + `resolve_builtin_method`; lowered in `codegen_int_intrinsic`. See "Builtin Method ABI".
- 2026-05-31: Builtin method dispatch on primitive/string §2 — `BuiltinMethod` enum +
  `resolve_builtin_method` + `builtin_methods` map (`context.rs`); first intrinsic `string.len()`.
- 2026-05-30: Integer overflow §1.2 — `overflow_checks` (from `-O0`) gates `codegen_int_arith`
  (`with.overflow` + `llvm.trap` for debug, wrapping for release). See "Integer Overflow ABI".
- 2026-05-25: Float literal suffixes §1.2/§1.4 — `codegen_literal` dispatches `F32`/`F64`;
  `type_pass.rs` records the matching type; `FoldedConst::from_literal` discards the suffix (type
  carried by context).
- 2026-05-18: Exhaustive `BinaryOp::NullCoalesce` arms in `codegen_binary`/`fold_const` return
  `InternalError` (semantic-analysis gates `??`, so reaching codegen is a pipeline bug → ICE).
- 2026-04-18: Bitwise codegen §1.4 — `BitAnd/BitOr/BitXor/Shl` → `build_and/or/xor/left_shift`;
  `BitNot` → `build_not`; type-pass maps to left-operand type; `fold_const` handles all on `Int`.
- 2026-04-16: Const declarations §1.3 end-to-end — `codegen_global_const`, `codegen_const_expr`,
  `FoldedConst`, `const_values`/`global_const_types`, `Stmt::Const` codegen, type-pass support.
- 2026-04-04: `codegen_for_range` accepts `inclusive: bool` → `<=` (`ULE`/`SLE`) vs `<`.
