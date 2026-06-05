# llvm-backend

## Purpose
Emit native object code from a type-checked Neuro AST via LLVM IR generation.

## Entry Point
- Type: Library function
- Input: `items: &[Item], optimization: OptimizationLevelSetting, source: &str, source_path: &str`
- Output: `Result<Vec<u8>, CodegenError>`

`source` / `source_path` are the original module text and its path; they are wrapped in a
`source_location::SourceFile` and used solely to render `file:line:col` in panic-family
runtime diagnostics (§1.2). They do not affect type-checking or codegen of any other construct.

## Data Ownership
- Tables: none
- Events Published: none
- Events Consumed: none
- Public Read Model: none

## Shared Kernel
- ast-types — read-only traversal of the type-checked AST
- shared-types — type system primitives
- diagnostics — error type infrastructure
- source-location — `SourceFile` byte-offset → line/column mapping for panic diagnostics (§1.2)

## Notes
inkwell 0.9.0 with feature `llvm20-1` (LLVM 20 bindings) is a third-party crate, not Neuro-owned Shared Kernel.
Requires LLVM 20 installed with MLIR enabled; set `LLVM_SYS_201_PREFIX` to the LLVM 20
prefix (e.g. `/usr/lib/llvm20`) before building.
`semantic-analysis` has no production dependency here; neurc orchestrates ordering so
that type checking always precedes code generation. `syntax-parsing` appears only in
`[dev-dependencies]` for integration tests.

## String ABI
`string` values are represented as an anonymous LLVM struct `{ ptr, i64 }`:
- field 0 (`ptr`): pointer to null-terminated UTF-8 bytes in read-only memory (`.rodata`)
- field 1 (`i64`): byte count of the string **excluding** the null terminator

String **literals** are emitted to `.rodata` and are never heap-allocated; the appended NUL
(`STRING_NULL_TERMINATOR` in `literals.rs`) exists only so the pointer is a valid C string for
FFI. The `len` field is authoritative — interior NUL bytes are legal content and are counted,
so consumers must not treat the data as NUL-terminated. Runtime (heap) strings will land in
Phase 1.7 and share this exact ABI, so the two are indistinguishable to a consumer.

The struct is passed and returned by value. On x86-64 SysV this fits in two registers
(rax/rdx or equivalent), so no sret indirection is needed for typical string functions.
The semantic type `Type::String` in `semantic-analysis` is unchanged; the fat pointer
layout is a backend implementation detail invisible to other slices.

`==` and `!=` on strings are lowered to a length check followed by a `memcmp` call
declared as an external libc symbol. `memcmp` is universally available on all supported
platforms (Linux, macOS). The length check uses `select` to pass `n=0` to `memcmp` when
lengths differ, keeping it safe without requiring additional basic blocks.

## Struct ABI
User-defined struct types are lowered to anonymous LLVM struct types `{ T0, T1, ... }`
with fields in declaration order (no padding insertion — LLVM handles natural alignment).
Struct values are stored on the stack via `alloca` and initialised field-by-field with
`insertvalue`. Field reads use `getelementptr` + `load`; field writes use
`getelementptr` + `store`. Struct types are not yet supported as function parameters
or return types (Phase 2+ limitation; the type mapper returns an error for those cases).
Field layout is held in `CodegenContext.struct_defs`; `get_struct_llvm_type` rebuilds
the anonymous LLVM struct type on demand (LLVM deduplicates by structure).

## Method ABI
`impl` methods are lowered to LLVM free functions under a mangled name
`StructName__methodName` (double-underscore separator). Users cannot define names
containing `__` because the identifier grammar forbids it.

For `&self` instance methods the struct is passed **by value** as the first LLVM
parameter (`param[0]`, named `self` in the alloca map). This is semantically correct
for read-only access; callers load their stack variable and pass the value directly.
`&mut self` and consuming `self` are rejected by semantic analysis before codegen runs.

Associated functions (no `self_param`) are lowered identically but have no implicit
first parameter; callers invoke them as `TypeName::func(args)` which the codegen maps
to `codegen_call("StructName__funcName", args)`.

Method calls (`instance.method(args)`) in `codegen_expr` are recognised when the `Call`
node's `func` is a `FieldAccess`. `fa_struct_names` (keyed by the `Call` span start)
carries the struct name so `codegen_method_call` can reconstruct the mangled name without
re-querying the AST.

## Builtin Method ABI
Intrinsic methods on non-struct (primitive / string) receivers are resolved by
`resolve_builtin_method` (in `context.rs`) during the type-collection pass, which tags the
`Call`'s full span `(start, end)` in `builtin_methods` and records the result type.
`codegen_expr` checks `builtin_methods` before the struct path and lowers via
`codegen_builtin_method`. The map is keyed by the full span rather than `span.start` because a
chained builtin call (`s.clone().len()`) nests two `Call` nodes that share the same
`span.start`; `(start, end)` is unique per node (same workaround as `binary_left_types`).
`string.len()` lowers to a single `extractvalue` of field 1 from the string fat pointer
`{ ptr, i64 }` — the stored byte length (O(1), no scan); the i64 value is the `u64` length with
no conversion. `string.clone()` (§2.7) lowers to the receiver's own fat-pointer value: strings
are immutable and `.rodata`-backed (no heap string type yet), so a `{ ptr, len }` value copy is
observationally a deep copy — this must duplicate the underlying buffer once runtime heap
strings land. `resolve_builtin_method` is duplicated from the `semantic-analysis` resolver so
the backend stays independent of the type-checker slice.

Integer intrinsics — `wrapping_{add,sub,mul}`, `saturating_{add,sub,mul}`, and `.shr(n)` —
resolve on any integer receiver to the receiver's own type and lower in `codegen_int_intrinsic`
(`expressions.rs`). Both operands are coerced to the receiver's int type via `coerce_if_needed`
(the arg literal can arrive widened to i32). Wrapping ops emit plain `add`/`sub`/`mul` (no
`nsw`/`nuw`, so they wrap and never trap regardless of the build's overflow-check flag). `.shr`
emits `ashr` for signed and `lshr` for unsigned, selected from the receiver's signedness.
`saturating_add`/`saturating_sub` use the overloaded `llvm.{s,u}{add,sub}.sat` intrinsics, which
return the clamped result directly. `saturating_mul` has no direct intrinsic: it uses
`{s,u}mul.with.overflow` and `select`s the bound on overflow — unsigned → MAX; signed → MIN when
the operand signs differ (product negative), MAX otherwise.

## if / else-if / else Lowering

`codegen_if` lowers an `if/else if+/else?` chain by treating it as a binary tree:
- Each call creates three basic blocks — `then`, `else`, `ifcont` (merge).
- The `else` block either hosts the final `else` body directly or recursively calls
  `codegen_if` with the first remaining `else_if` arm, passing the rest of the arms
  and the original `else_block` to that recursive call.
- This `split_first` recursion ensures every arm is mutually exclusive with all
  subsequent arms; the final `else` body is only reachable when all preceding
  conditions are false.

A value-producing `if`/`else` in expression position is lowered by `codegen_if_expr`
into a result `alloca` written by each arm and loaded at the merge block. A statement
-position `if` parses to `Stmt::If`, so when such an `if` (with an `else`) is the *tail*
of a function or method body, `codegen_body` routes it through `codegen_if_expr` and
returns the loaded value — otherwise the non-void function would emit `unreachable` at
the merge block (no instruction at `-O0`) and run off its own end. The type pass records
the tail `if`'s result type at its span (`record_tail_if_type`) so the result slot can be
allocated, mirroring the `Expr::If` arm.

## Logical Operator Lowering
`&&` and `||` short-circuit (§1.4). `codegen_binary` intercepts them before its eager
operand evaluation and delegates to `codegen_short_circuit`, which evaluates the LHS in
the current block, conditionally branches to a `logic.rhs` block (taken only on the
deciding edge — true for `&&`, false for `||`), and merges the RHS value with the
short-circuit constant (`false` for `&&`, `true` for `||`) via a phi in `logic.merge`.
Both phi predecessors are captured *after* their side is emitted (`get_insert_block`),
so a RHS that appends its own blocks (nested `if`-expression) is handled; a RHS that
terminates its block is dropped from the phi. Operands are guaranteed `i1` by semantic
analysis, so no coercion is performed. The `BinaryOp::And | BinaryOp::Or` arm of the
eager match is now an unreachable ICE guard.

## Integer Overflow ABI
Integer `+`, `-`, and `*` honor the §1.2 overflow rule, keyed off the
`OptimizationLevelSetting` passed to `compile`:
- `-O0` (debug) → `CodegenContext.overflow_checks = true`. `codegen_int_arith`
  emits the matching `llvm.{s,u}{add,sub,mul}.with.overflow` intrinsic, extracts
  the `{result, overflow_bit}` aggregate, and conditionally branches to a per-op
  `arith.overflow` block that calls `llvm.trap` + `unreachable`. Execution
  continues in the `arith.cont` block carrying the result.
- `-O1..-O3` (release) → `overflow_checks = false`. `emit_wrapping_int_arith`
  emits the plain `build_int_add/sub/mul` (two's-complement wrap).

Signedness selects the `s`/`u` intrinsic variant via `TypeMapper::is_unsigned_int`.
Division, modulo, bitwise ops, and floats are unaffected. The `FoldedConst`
compile-time path is independent and always wraps.

## Panic Runtime ABI
The panic-family builtins `panic(msg: string)`, `assert(cond: bool)`, and `unreachable()`
(§1.2) lower in `panic.rs`. The contract is **abort, no unwinding**: no landing pads are
emitted, so the happy path is zero-cost and `Drop`/`defer` (future) fire only on normal
scope exit. The `Call`→`Identifier` arm in `expressions.rs` intercepts these names via
`CodegenContext::is_panic_builtin` *before* `codegen_call`, but only when no user function of
the same name is registered (user functions shadow the builtin, matching the semantic
resolver). `is_panic_builtin` is duplicated from `semantic-analysis` to keep the backend
independent of the type-checker slice.

Each builtin writes its diagnostic to stderr (fd 2) with the external POSIX `write` call
(`get_or_declare_write`), then calls libc `abort` (`get_or_declare_abort`, marked `noreturn`)
followed by an `unreachable` terminator:
- `panic` → `write "panic: "`, `write <msg fat-ptr>`, `write " at file:line:col\n"`, abort.
- `unreachable` → `write "internal error: entered unreachable code at file:line:col\n"`, abort.
- `assert` → conditional branch: a true condition falls through to `assert.cont`; a false one
  enters `assert.fail`, which writes `"assertion failed at file:line:col\n"` and aborts.

The dynamic `panic` message is a runtime `string` fat pointer `{ ptr, i64 }`; its fields are
read with `extractvalue` and passed straight to `write`. The `file:line:col` suffix is derived
from the `Call` span start via the `SourceFile` supplied to `compile`; it is empty when no
source was provided. `write`+`abort` are POSIX/libc symbols available on Linux and macOS, and
via the MSVC CRT compatibility layer on Windows.

Because `panic`/`unreachable` terminate the block with `unreachable`, statements that follow a
divergent call are dead code. `codegen_stmt` early-returns when the current block is already
terminated, and `codegen_return` / `codegen_body`'s tail path skip the `ret` when evaluating
the returned expression terminated the block (`func f() -> i32 { panic("x") }`). This keeps
LLVM from seeing instructions after a terminator.

## Future: MLIR Integration (Phase 3+)
When tensor operations are introduced, `melior` (Rust MLIR bindings, targeting the same
LLVM 20 / MLIR 20 installation) will be added alongside inkwell. The lowering strategy
will be: AST → Neuro High-Level IR → MLIR dialects (linalg/tensor/func/arith) →
Enzyme MLIR AD pass → GPU dialects (nvgpu/rocdl) or `llvm` dialect → inkwell for final
LLVM IR emission. inkwell remains the terminal code-emission layer in all paths.

## Constant Declarations ABI
Module-level consts are emitted as `@NAME = internal constant TYPE VALUE` LLVM globals before
any function definitions. Their LLVM value is also stored in `CodegenContext.const_values` so
that identifier references inside function bodies resolve without loading from the global.

Function-body `Stmt::Const` nodes fold the constant expression in Rust (via `FoldedConst`) and
store the resulting `BasicValueEnum` in `const_values` for the duration of the function scope.
No `alloca` is emitted; consts are purely compile-time values.

Constant folding uses a pure Rust `FoldedConst { Int(i64), Float(f64), Bool(bool), Str(String) }`
enum rather than inkwell's const-arithmetic API (which has inconsistent availability across
inkwell versions). All arithmetic is performed in Rust with wrapping semantics for integers and
IEEE-754 for floats; a single `const_int`/`const_float`/`const_struct` call creates the final
LLVM value.

`global_const_types: HashMap<String, Type>` in `CodegenContext` carries module-level const
types and is re-seeded into `type_env` after each `type_env.clear()` in
`visit_function_for_types` and `visit_method_for_types`, so the type-inference pass can resolve
const identifiers inside function bodies.

## Recent Updates
- 2026-06-05: Struct functional-update lowering (§3.3). `codegen_struct_literal` (`codegen/structs.rs`)
  takes `base: Option<&Expr>`: with a base it seeds the aggregate from the base struct value
  (`codegen_expr(base).into_struct_value()`) instead of `get_undef()`, then each explicit field
  `insert_value` overwrites its slot — so unlisted fields keep the base's values. `type_pass.rs`
  visits `base` for type collection. Field-init shorthand needs no codegen change (it is an
  ordinary `Expr::Identifier` by the time codegen runs).
- 2026-06-05: `string.clone() -> string` builtin §2.7 (Phase 1.7). New `BuiltinMethod::StringClone`
  + `(Type::String, "clone")` arm in `resolve_builtin_method` (`context.rs`); `codegen_builtin_method`
  (`expressions/methods.rs`) lowers it to the receiver's fat-pointer value (a value copy — a deep copy
  while strings are immutable / `.rodata`-backed; must duplicate the heap buffer once heap strings land).
  Re-keyed `builtin_methods` from `span.start` to the full span `(start, end)` so chained builtin
  calls (`s.clone().len()`, two `Call` nodes sharing `span.start`) dispatch correctly — same class of
  `span.start` collision the `binary_left_types` map already works around.
- 2026-06-04: Panic runtime §1.2 (Phase 1.7). New `panic.rs` lowers `panic`/`assert`/`unreachable`
  to a stderr diagnostic (`write` to fd 2) + libc `abort` + `unreachable` — abort, no unwinding.
  `compile` gained `source`/`source_path` params (wrapped in `SourceFile`) so diagnostics carry
  `file:line:col`. `get_or_declare_write`/`get_or_declare_abort` added (`context.rs`); the
  `Call`→`Identifier` arm intercepts the builtins via `is_panic_builtin` before `codegen_call`
  (user functions shadow). Added terminated-block guards in `codegen_stmt`, `codegen_return`, and
  `codegen_body`'s tail path so dead code after a divergent call does not break verification.
  See "Panic Runtime ABI".
- 2026-06-04: Added `Expr::Unsafe` lowering (Phase 1.7 groundwork). Lowered via `codegen_block_expr`
  exactly like `Expr::Block`; the type pass collects its result type through the shared
  `Expr::Block | Expr::Unsafe` arm. `unsafe` is inert — identical IR to a bare block.
- 2026-06-02: Logical `&&`/`||` now short-circuit (§1.4). `codegen_binary` intercepts them
  before eager operand evaluation and routes through the new `codegen_short_circuit`
  (phi-merge over a `logic.rhs`/`logic.merge` block pair). Previously both operands were
  evaluated and combined with `build_and`/`build_or`, so the RHS always ran. See
  "Logical Operator Lowering".
- 2026-06-02: `fold_const` (`literals.rs`) gained a `(Bool, Bool)` binary arm (`&&`, `||`,
  `==`, `!=`). A `bool`-typed const with a binary initializer (`const FLAG: bool = true && false`)
  previously hit the catch-all `_` arm and aborted with an ICE despite passing semantic analysis.
- 2026-06-02: Fixed miscompilation of a tail-position `if`/`else` used as an implicit
  return. Unified `codegen_function`/`codegen_method` body lowering into `codegen_body`,
  which now treats a trailing `Stmt::If { else_block: Some(..), .. }` as a value-producing
  if-expression; `record_tail_if_type` (type pass) records its result type at the `if` span.
  Previously the statement path emitted `unreachable` for the non-void return → fall-through
  segfault. See "if / else-if / else Lowering".
- 2026-06-02: Formalized the string literal/runtime distinction §2.7. Named the literal
  terminator byte `STRING_NULL_TERMINATOR` (`literals.rs`) with a WHY comment; both literal
  lowering paths now state that `len` excludes it and is authoritative (interior NULs counted).
  Behaviour unchanged — codegen already computed `len` this way. See "String ABI".
- 2026-05-31: Integer primitive methods §1.2, §1.4. Extended `BuiltinMethod` +
  `resolve_builtin_method` (`context.rs`) with `Wrapping{Add,Sub,Mul}`, `Saturating{Add,Sub,Mul}`,
  `Shr` resolving on any integer receiver to its own type. `codegen_int_intrinsic`
  (`expressions.rs`) lowers them; wrapping = plain non-trapping arithmetic, `.shr` = `ashr`/`lshr`
  by signedness, saturating add/sub = `llvm.{s,u}{add,sub}.sat`, saturating mul =
  `{s,u}mul.with.overflow` + `select`. See "Builtin Method ABI".
- 2026-05-31: Builtin method dispatch on primitive & string types §2. New `BuiltinMethod`
  enum + `resolve_builtin_method` + `builtin_methods` map in `context.rs`; the `type_pass`
  method-call arm tags non-struct receivers, and `codegen_builtin_method` in `expressions.rs`
  lowers them. First intrinsic `string.len()` emits `extractvalue ..., 1`. See "Builtin Method ABI".
- 2026-05-30: Implemented integer overflow semantics §1.2. `CodegenContext.overflow_checks` (set from `-O0`) gates `codegen_int_arith`, which emits `llvm.{s,u}{add,sub,mul}.with.overflow` + `llvm.trap` for debug builds and plain wrapping arithmetic for release. Routed the `Add`/`Subtract`/`Multiply` integer arms of `codegen_binary` through it. See "Integer Overflow ABI".
- 2026-05-18: Added exhaustive `BinaryOp::NullCoalesce` arms in `codegen_binary` and `fold_const` (Int path); both return `CodegenError::InternalError`. Semantic-analysis gates this operator (Phase 2 feature), so reaching codegen indicates a pipeline bug — surfaced as an ICE rather than a panic so the float-fallthrough arm stays well-behaved.
- 2026-04-04: Updated `codegen_for_range` to accept `inclusive: bool` from `Stmt::ForRange` and generate `<=` (`ULE`/`SLE`) instead of `<` (`ULT`/`SLT`) comparison instructions when true.
- 2026-04-16: Implemented §1.3 const declarations end-to-end: `codegen_global_const`,
  `codegen_const_expr`, `FoldedConst` folder, `const_values`/`global_const_types` maps,
  `Stmt::Const` codegen, and type-pass support.
- 2026-04-18: Implemented bitwise operator codegen §1.4. `BinaryOp::BitAnd/BitOr/BitXor/Shl` lower
  to `build_and`/`build_or`/`build_xor`/`build_left_shift`. `UnaryOp::BitNot` lowers to `build_not`.
  `type_pass.rs` maps bitwise binary ops to the left-operand type (same as arithmetic). `fold_const`
  handles all four binary bitwise ops on `FoldedConst::Int` and `BitNot` on `FoldedConst::Int`.
- 2026-05-25: Float literal type suffixes §1.2/§1.4. `codegen_literal` dispatches `Literal::Float(val, Some(F32))` → `f32_type().const_float(val)` and `None | Some(F64)` → `f64_type().const_float(val)`. `type_pass.rs` records the matching `Type::F32` / `Type::F64` for downstream type lookups. `FoldedConst::from_literal` discards the suffix because folded float arithmetic carries the type via the consuming `Type` context.
- 2026-06-04: Fixed a binary-operand type-map collision (§1.4). The type pass had stored a binary node's left-operand type at `expr_types[span.start + 1]`, but a binary node and its leftmost descendant share the same `span.start`, so a parent (e.g. `&&`, left type `Bool`) clobbered its left child comparison's left type (e.g. `i32`). The leftmost comparison of an `&&`/`||` was then codegen'd as `Bool`, truncating its i32 operands to i1 and producing wrong results (`c >= 48 && c <= 57` with `c = 51` → false). Left-operand types now live in a dedicated `binary_left_types` map keyed by the full span `(start, end)`, which is unique per node. Regression coverage: `compiler/neurc/tests/short_circuit_runtime.rs`.
