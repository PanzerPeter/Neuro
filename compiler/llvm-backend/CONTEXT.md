# llvm-backend

## Purpose
Emit native object code from the typed Neuro HIR via LLVM IR generation.

## Entry Point
- Type: Library function
- Input: `program: &neuro_hir::HirProgram, optimization: OptimizationLevelSetting, source: &str,
  source_path: &str`
- Output: `Result<Vec<u8>, CodegenError>`

The backend consumes the typed HIR produced by `hir-lowering`: every HIR node carries its
resolved type (`HirExpr::ty`), so codegen reads types inline rather than re-deriving them —
**there is no backend type-collection pass**. A single `type_env` (binding name → resolved type),
populated as bindings are lowered, exists only so the place statements `obj.field = …` and
`arr[i] = …` can recover a binding's nominal struct/array type.

`source` / `source_path` are the original module text and path, wrapped in a
`source_location::SourceFile` solely to render `file:line:col` in panic-family runtime
diagnostics. They affect nothing else.

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- neuro-hir — the typed HIR lowered from (`HirProgram` / `HirExpr` / `HirType`)
- ast-types — the `BinaryOp` / `UnaryOp` enums (reused unchanged by the HIR)
- shared-types — type system primitives, `FormatSpec` for interpolation
- diagnostics — error type infrastructure
- source-location — `SourceFile` byte-offset → line/column mapping for panic diagnostics

inkwell 0.9.0 (feature `llvm20-1`) is a third-party crate, not Shared Kernel. Requires LLVM 20;
set `LLVM_SYS_201_PREFIX` (e.g. `/usr/lib/llvm20`) before building. `semantic-analysis` is not a
production dependency — neurc orders type-check then HIR lowering before codegen.
`syntax-parsing` and `hir-lowering` appear only in `[dev-dependencies]` (tests and benches lower
source to HIR before compiling).

`resolve_builtin_method` / `is_panic_builtin` / `is_io_builtin` are duplicated from
`semantic-analysis` to keep the backend independent of the type-checker slice.

## Module Emission Order
`compile` splits into `build_module` (generate + verify) and `emit_object_code`, so codegen tests
can assert on IR text that object emission erases. Inside `build_module` the order is fixed and
load-bearing:

1. **Signature pre-declaration** over every function, method, and closure before any body —
   `declare_function` / `declare_method` / `declare_impl` / `declare_closure` add the LLVM
   signature and register it in `functions`; the `codegen_*` counterparts fetch that declaration
   rather than adding one. Monomorphization means the call graph is no longer definition-ordered
   (an instance may be called by, or call, items emitted before it), so a call must resolve
   regardless of order.
2. **Vtables** (`emit_vtables`) — after all signatures are declared, before any body, so item
   order never matters.
3. **Bodies.**
4. **Soft-float builtins** linked in when the module uses `half`/`bfloat`, after codegen and
   before `verify`.

## Stack Slot Placement
`CodegenContext::entry_alloca` positions the builder before the entry block's first instruction,
allocates, and restores. **Every** local binding, result slot, induction variable, scratch temp,
drop flag, and closure environment goes through it. Allocating at the current builder position
meant a slot inside a loop body was re-allocated per iteration, so a long enough loop segfaulted —
at every `-O` level, since `mem2reg` only promotes entry-block allocas. The initializing store
stays where it was; sharing one slot across iterations is sound because each is written before it
is read, and a fresh frame per call keeps recursion correct. Parameter and `self` allocas in
`functions.rs` are already in the entry block by construction.

## String ABI
`string` = anonymous LLVM struct `{ ptr, i64 }`:
- field 0 (`ptr`): pointer to null-terminated UTF-8 bytes in `.rodata`
- field 1 (`i64`): byte count **excluding** the null terminator

Literals are emitted to `.rodata`, never heap-allocated; the appended NUL
(`STRING_NULL_TERMINATOR` in `literals.rs`) exists only for C-string FFI validity. `len` is
authoritative — interior NULs are legal counted content, so consumers must not treat the data as
NUL-terminated.

Passed and returned by value. On x86-64 SysV this fits two registers, so no `sret` indirection.
The semantic `Type::String` is unchanged — the fat-pointer layout is a backend-only detail.

### `&string`
An **immutable** `&string` is the `{ ptr, i64 }` fat pointer itself, held by value — it is not a
pointer to one. `string` is immutable, so the referent's address carries nothing the fat pointer
does not, and demanding one forces every computed slice (`s.slice(a..b)`, which has no home) into
a stack slot whose address then outlives the frame it was taken in (BUG-008). By value, a slice is
returned like any other aggregate, `.len()` is an `extractvalue`, and no slot exists to dangle.

`&mut string` is the exception: a store through it has to reach the referent, so it stays the
referent's address (an opaque `ptr`), exactly like every other `&mut T`. `&&string` is likewise a
pointer — the outer reference borrows a reference, not a string. The backend `Type::Reference`
therefore carries `mutable`, which is the only thing distinguishing the two lowerings;
`TypeMapper::map_type` matches one reference level, never `referent()`. `codegen_reference` reads
the place instead of taking its address when the borrow's own type is `&string`, and
`codegen_deref` is the identity on one. `mangle()` still ignores `mutable`: it distinguishes no
two monomorphizations today, and honouring it would rename every existing symbol.

### String operators
`==` / `!=` lower to a length check plus libc `memcmp` (universally available). The length check
uses `select` to pass `n=0` to `memcmp` when lengths differ (safe, no extra blocks).
`codegen_binary` handles string `Equal`/`NotEqual` *before* the numeric coercion: each operand
goes through `load_string_fatptr` (an owned `string` and a `&string` are already the struct; only
a `&mut string` is loaded through) and then `codegen_string_eq`. Detection keys off
`left_ty.referent() == String`, covering owned, borrowed, and mixed operands.

`+` is concatenation, routed to `codegen_string_concat` before the numeric coercion: both operands
are normalized with `load_string_fatptr`, a `len1 + len2` buffer is `malloc`'d, each operand's
bytes are `memcpy`'d in (the second at a `gep i8` offset of `len1`), and a fresh `{ ptr, len }` is
returned. The result is a new owned, immutable string with **no** NUL terminator (consistent with
the `len` contract). The frontend types the result as owned `String` even when an operand is
`&string`, so the value is never a reference. **The heap buffer is not freed** — an anonymous heap
string is owned by no tracked binding, so `+`, string interpolation, and `String::to_string` all
leak until heap-string ownership lands.

## Struct ABI
User structs lower to anonymous LLVM structs `{ T0, T1, ... }` in declaration order (no padding —
LLVM handles alignment). `TypeMapper` holds the layout table (`set_struct_fields`, fed by
`CodegenContext::set_struct_defs`) beside `enum_words`, so `map_type` builds a named struct's
aggregate: a struct works as a free function's **parameter and return type** and as a field of
another struct. That ABI is by value and direct — no `sret` — matching what methods already did
for `&self`. `get_struct_llvm_type` delegates to `TypeMapper::struct_type`, so one definition of
the layout serves both paths; recursion is bounded by `MAX_STRUCT_DEPTH`, which is insurance
rather than a live case (a cycle is impossible today — a field type must be declared before use).

Values live on the stack via `alloca`, initialised field-by-field with `insertvalue`; reads are
`getelementptr`+`load`, writes `getelementptr`+`store`. A functional update
(`Point { x: 1.0, ..p }`) seeds the aggregate from the base struct value rather than `get_undef()`
and `insertvalue`s the explicit fields over it.

`codegen_field_access` reads a field of a **non-place** object (a chain `o.inner.v`, a call result)
with `extractvalue`, keeping the GEP-and-load path for a named binding. `get_struct_ptr_and_type`
still requires a place, so a `&mut self` method reached through a chain remains unsupported.

## Method ABI
`impl` methods lower to LLVM free functions mangled `StructName__methodName` (double underscore).
`codegen_method_call` recovers the receiver struct by splitting the symbol on `__`, so the
separator must appear **exactly once**. Two rules hold that: semantic analysis rejects a declared
name containing `__` (`TypeError::ReservedNameSeparator`), and every monomorphized instance name
uses a single-underscore `_g_` marker (`identity_g_i32`, `Pair_g_i32_f64`).

- `&self` (and owned `self`, which reaches codegen only on a `Copy` receiver) take the struct
  **by value** as `param[0]`, named `self` in the alloca map. Callers load their stack var and
  pass the value.
- `&mut self` takes the struct **by pointer**: `codegen_method` emits `param[0]` as `ptr` and
  binds `self` directly to it (no copy) with the recorded type still the struct, so `self.field`
  reads and writes go through to the caller's storage. It also seeds `type_env["self"]` so a
  `self.field = …` write resolves the struct.
- Associated functions (no `self_param`) lower identically without the implicit first param;
  `TypeName::func(args)` becomes `codegen_call("StructName__funcName", args)`.

A method call is recognised when a `Call`'s callee is a `FieldAccess`; the receiver's struct name
comes from the callee node's HIR type. The call site detects a by-pointer callee from its first
LLVM param being a pointer and passes the receiver place's address (via
`get_struct_ptr_and_type`, which auto-loads a `&mut Struct` receiver) rather than the loaded value.

An overloaded operator needs no codegen of its own: `hir-lowering` desugars it to an ordinary
method call, so the backend emits a plain `StructName__op` call.

## Builtin Method ABI
Intrinsics on non-struct receivers resolve in `resolve_builtin_method` (`context.rs`), which maps
a receiver `Type` plus method name to a `BuiltinMethod` tag **only** — the call's result type comes
from the HIR callee node, because `checked_*` yields a monomorphized `Option<T>` instance whose
mangled name only the frontend can produce. The method-call arm of `codegen_call_expr` passes both
the receiver type (from `object.ty`) and that result type into `codegen_builtin_method`
(`expressions/methods.rs`).

- `string.len()` → `extractvalue` field 1 (O(1) stored byte length, `u64`, no conversion).
- `string.clone()` → the receiver's own fat-pointer value: strings are immutable and
  `.rodata`-backed, so a `{ ptr, len }` copy is observationally deep. Must duplicate the buffer
  once heap strings land.
- `string.slice(a..b)` / `.slice(a..=b)` → `codegen_string_slice`, computing a
  `(ptr+start, end-start)` fat pointer (`end` = `b+1` for `..=`). Runtime bounds
  (`0 <= start <= end <= len`) and UTF-8 codepoint-boundary checks at both endpoints route through
  `codegen_guard_or_panic` (`panic.rs`) — abort, no unwinding, in every build. The result is the
  computed fat pointer itself, returned by value with no stack slot, so a slice returned across a
  call boundary stays valid. The `Range` argument is consumed here; reaching it through general
  `codegen_expr` is an internal error.
- `struct.clone()` → handled in the struct method-call arm rather than `resolve_builtin_method`
  (which is keyed by `Type`): when the receiver is a struct, the field is `clone`, and no
  `StructName__clone` exists, it passes `BuiltinMethod::StructClone`. Semantic analysis already
  verified the `Clone` derive. Lowers to the receiver's aggregate value — faithful while
  stack-allocated, must recurse into heap-owning fields later.
- Integer intrinsics — `wrapping_{add,sub,mul}`, `saturating_{add,sub,mul}`, `.shr(n)` — resolve
  on any integer receiver to its own type and lower in `codegen_int_intrinsic`. Both operands are
  coerced to the receiver int via `coerce_if_needed` (an argument literal may arrive widened to
  i32). Wrapping → plain `add`/`sub`/`mul`, no `nsw`/`nuw`, never trapping. `.shr` → `ashr`
  (signed) / `lshr` (unsigned). `saturating_add`/`sub` → `llvm.{s,u}{add,sub}.sat`;
  `saturating_mul` has no direct intrinsic and becomes `{s,u}mul.with.overflow` + `select`
  (unsigned → MAX; signed → MIN on differing operand signs, else MAX).
- `.is_nan()` → `codegen_is_nan`: `fcmp uno x, x` on the receiver value, yielding the `i1` a
  `bool` lowers to. The self-comparison IS the test — NaN is the only value unordered with
  itself — and it is why the check cannot be spelled in source, where `x != x` uses the ordered
  predicate. Resolved for `F32`/`F64` spelled out rather than via this slice's `Type::is_float`,
  which also admits `f16`/`bf16`.
- `checked_{add,sub,mul}` → `codegen_checked_int_intrinsic`: `llvm.{s,u}{add,sub,mul}.with.overflow`
  via the shared `emit_with_overflow`, then `build_option_value` (`collections/mod.rs`) selects
  `Some(result)` / `None` on the negated overflow bit. Branchless — both variants are materialized
  and `select`ed. The `Option<T>` instance, its variant tags, and its payload layout all come from
  the call's result type; nothing about `Option` is assumed here.

## Literals and Constants ABI
`codegen_literal` takes the literal's **resolved type** and emits the constant at it. An unsuffixed
literal has no width of its own and nothing coerces a call argument or a return value, so the
suffix default (`i32` / `f64`) used to reach the verifier; the suffix rule survives only as the
fallback for a non-numeric resolved type.

Module-level consts emit as `@NAME = internal constant TYPE VALUE` globals before any function
definitions, and their LLVM value is also stored in `CodegenContext.const_values` so body
references resolve without loading from the global. Body-level consts fold in Rust and store the
`BasicValueEnum` in `const_values` for the function scope — no `alloca`, purely compile-time.

Folding uses a pure-Rust `FoldedConst { Int(i64), Float(f64), Bool(bool), Str(String) }` rather
than inkwell's const-arithmetic API (inconsistent across versions): all arithmetic happens in Rust
(wrapping ints, IEEE-754 floats, and a `(Bool, Bool)` arm for `&&`/`||`/`==`/`!=`), and a single
`const_int` / `const_float` / `const_struct` builds the final LLVM value. The `FoldedConst` path
always wraps, regardless of `overflow_checks`.

## Enum ABI
`compile` builds an `enum_words` table (each enum's widest-variant field count) and hands it to
the `TypeMapper`, which maps an enum to the tagged union `{ i32 tag, [W x i64] payload }` — usable
as a parameter, return, or field via `map_type`. `codegen_enum_construct`
(`expressions/enums.rs`) packs the discriminant tag plus each scalar payload field into its own
64-bit slot (floats bitcast to int width, then zero-extended), a lossless round-trip for `match`.
Payloads are scalar Copy primitives only, enforced by semantic analysis. `codegen_enum_value` is
the split-out half that builds an enum from already-evaluated values, and the context's
`enum_variants` table (name → declaration order) resolves `Some` / `None` tags **by name** rather
than assuming the prelude's declaration order.

## Aggregate ABIs
- **Tuples** — `map_type` → anonymous LLVM struct `{ T1, T2, ... }`. `codegen_tuple_literal`
  builds it with `insert_value` (with per-element `coerce_if_needed` for default-typed literals);
  `codegen_tuple_index` reads element N with `extract_value`, auto-loading through a `&tuple`
  borrow pointer first. Tuples flow through parameters and returns.
- **Arrays** — `map_type` → LLVM `[N x T]`. `expressions/arrays.rs` lowers array literals, index
  read/write (with a debug-only bounds guard through `codegen_guard_or_panic`), and
  `for x in arr` / `for x in &arr`. `BuiltinMethod::ArrayLen` is a compile-time `u64`.
  `coerce_if_needed` has an element-wise array arm for typed `[i64; N] = [..]` literals.
- **Array rest** — `codegen_array_rest` builds a fresh `[T; N - start]` aggregate by loading
  elements `start..N` of the source (via `array_place_ptr`) and `insert_value`-ing them. A
  zero-length remainder (the rest-less arity-assert form) yields an undef `[T; 0]`, discarded in
  statement position.
- **Newtypes** — transparent at runtime: `Type::from_hir` erases `HirType::Newtype { inner, .. }`
  to `from_hir(inner)`, so codegen never sees a newtype. `NewtypeConstruct` and `NewtypeAccess`
  both codegen their inner expression unchanged. No backend `Type` variant, type mapping, or item
  handling.

## Reference and Primitive Lowering
`map_type` lowers a reference to an opaque `ptr`, with two exceptions: an immutable `&string`
(the fat pointer itself, above) and `Reference(DynObject)` (the two-word `dyn_ref_type()` struct).
A bare `DynObject` is rejected as unsized.

`codegen_reference` returns the borrowed place's storage pointer — mutability is compile-time
only. `codegen_deref` loads the referent; `codegen_deref_assignment` stores at the pointer.
**Auto-deref is value-driven**: a borrowed receiver lowers to a `PointerValue`, so
`string_receiver_struct`, `StructClone`, `codegen_method_call`, and `get_struct_ptr_and_type` load
through the pointer when they see one; an owned receiver is already a value. There is no context
state for ref-ness — it is read from `variable_types` (a `&Struct` alloca holds a `ptr`) and from
the lowered value kind.

Unit-returning calls are valid in statement position: `codegen_call` / `codegen_method_call`
return an `Option` (`None` = void), and the shared `codegen_call_dispatch` is wrapped with a
void-error in value position.

- **`char`** lowers to LLVM `i32`. Casts use `is_int_like` / `is_unsigned_like` so char↔integer
  (and char→char) reuse the int-to-int path — char zero-extends, code points being non-negative —
  and comparisons hit the signed-int branch, correct since valid code points are < 2²¹.
- **`f16` / `bf16`** lower to LLVM `half` / `bfloat`. Backend `is_float()` **includes** the halves,
  so equality (`fcmp`) and `as`-casts route through the float instructions. The float→float cast
  and `coerce_if_needed` pick `fpext` / `fptrunc` by **bit width**, not a fixed F32/F64 pair; an
  f16↔bf16 cast (equal width, different format) routes through f32.

## if / else-if / else Lowering
`codegen_if` lowers an `if / else if+ / else?` chain as a binary tree: each call creates
`then`/`else`/`ifcont` blocks, and the `else` block hosts the final `else` body or recursively
calls `codegen_if` with the first remaining `else_if` arm (`split_first` recursion), so every arm
is mutually exclusive and the final `else` is reached only when all conditions are false.

A value-producing `if`/`else` in expression position goes to `codegen_if_expr`: a result `alloca`
written per arm, loaded at the merge block. A trailing `if` acting as a block's or a body's value
arrives as a `HirStmt::Expr` holding an if-expression — **hir-lowering owns that promotion**, so
the backend needs no rule of its own and `codegen_body` handles only `HirStmt::Expr` tails.

`codegen_block_expr` reads a trailing `HirStmt::Expr` as the block's value only when its type is
not `HirType::Void`: a block ending in a call to a unit function has no value, and asking for one
failed with "function call returned void when value expected". A `void` tail is emitted through
`codegen_stmt` like any other non-expression tail — which is also the shape the named-argument
hoisting rewrite produces for a unit call.

An `unsafe` block lowers through `codegen_block_expr` exactly like a bare block, emitting
identical IR.

## Match Lowering
`codegen_match` (`expressions/matches.rs`) evaluates the scrutinee **once** into an alloca, then
builds a per-arm test-block chain: each arm ORs its `HirMatchTest`s (tag compare / scalar `==` /
range `lo<=x<=hi`, signed vs unsigned by scrutinee type) and branches to the arm body or the next
test. An arm body materializes its bindings — the whole scrutinee, or an enum payload slot decoded
by `decode_enum_payload_field`, the inverse of the payload pack — evaluates the guard (branching
to the next arm on failure), then evaluates the body into a shared result slot. Bindings are saved
and restored in the name maps per arm, and the fall-through block is `unreachable`, because
exhaustiveness is a frontend guarantee.

`codegen_single_test`, `SavedBinding`, `bind_arm`, and `restore_bindings` are `pub(crate)` so
`val_else.rs` can share them.

## val-else Lowering
`codegen/val_else.rs`. The scrutinee is stored once into an alloca, `codegen_single_test` picks
the branch, and the else block runs in its own drop scope with its binding saved and restored. The
success block's bindings are materialized by `bind_arm` and deliberately **not** restored — they
belong to the enclosing block, which is the whole difference from a match arm. The else block is
terminated with `unreachable` if it still falls through; the frontend has already rejected that
case, so this only keeps the emitted function verifier-clean.

## Logical Operator Lowering
`&&` / `||` short-circuit. `codegen_binary` intercepts them before eager operand evaluation and
delegates to `codegen_short_circuit`: evaluate the LHS in the current block, conditionally branch
to a `logic.rhs` block (taken only on the deciding edge — true for `&&`, false for `||`), and merge
the RHS value with the short-circuit constant (`false`/`true`) via a phi in `logic.merge`. Both phi
predecessors are captured *after* their side is emitted (`get_insert_block`), so an RHS that
appends blocks (a nested if-expression) works, and an RHS that terminates its block is dropped from
the phi. Operands are guaranteed `i1` by semantics; the eager `And | Or` arm is an unreachable ICE
guard.

`codegen_binary` also checks that both coerced operands are integer or float values **before** the
operator match — every arm calls `into_int_value` / `into_float_value`, which *panic* on a struct,
array, or pointer rather than returning an error — and answers one that is not with
`CodegenError::InvalidOperandType`. Semantic analysis and HIR lowering both reject such an operand,
so this is the backstop rather than the diagnostic.

`BinaryOp::NullCoalesce` reaching `codegen_binary` or `fold_const` is an `InternalError`: `??` is
desugared to a `match` by hir-lowering, so a binary node still carrying it means the HIR did not
come from that pass. `??` in a const expression is rejected outright.

## Loop Lowering
`codegen_loop` mirrors `codegen_while` without a condition block: it branches unconditionally into
`loop.body` and back to its top, so the only exit is a `break` (`LoopTargets { continue_bb: body,
break_bb: exit }`, so `continue` re-enters the top). A `break`-less `loop` leaves `loop.exit`
without predecessors, and the function epilogue supplies its terminator.

`LoopTargets` carries `break_slot: Option<PointerValue>` for the value form: `codegen_loop`
allocates a result slot when the loop's HIR type is non-`Void` and returns the loaded value, and a
value `break v` stores into the resolved loop's slot before branching.

It also carries `label: Option<String>`. `break` / `continue` resolve via `resolve_loop_target`: a
labeled one scans `loop_targets` from innermost out for the matching label, an unlabeled one takes
the top. Label validity is guaranteed by semantic analysis, so an unresolved label is an
`InternalError`.

## Integer Overflow ABI
Integer `+` / `-` / `*` honor the overflow rule, keyed off `OptimizationLevelSetting`:
- `-O0` → `overflow_checks = true`. `codegen_int_arith` emits
  `llvm.{s,u}{add,sub,mul}.with.overflow`, extracts `{result, overflow_bit}`, conditionally
  branches to a per-op `arith.overflow` block (`llvm.trap` + `unreachable`), and continues in
  `arith.cont` with the result.
- `-O1..-O3` → `overflow_checks = false`. `emit_wrapping_int_arith` emits plain
  `build_int_add/sub/mul` (two's-complement wrap).

Signedness picks the `s`/`u` variant via `TypeMapper::is_unsigned_int`. Division, modulo, bitwise
ops (`build_and`/`or`/`xor`/`left_shift`, `build_not` for `BitNot`), and floats are unaffected.

## Panic Runtime ABI
Panic-family builtins `panic(msg: string)`, `assert(cond: bool)`, `unreachable()` lower in
`panic.rs`. Contract: **abort, no unwinding** — no landing pads, so the happy path is zero-cost and
`Drop` fires only on normal scope exit. The `Call`→`Identifier` arm intercepts these names via
`CodegenContext::is_panic_builtin` before `codegen_call`, but only when no user function of the
same name is registered (user functions shadow, matching the semantic resolver).

Each builtin writes its diagnostic to stderr (fd 2) via external POSIX `write`
(`get_or_declare_write`), then calls libc `abort` (`get_or_declare_abort`, `noreturn cold`) plus an
`unreachable` terminator:
- `panic` → write `"panic: "`, the msg fat-ptr, `" at file:line:col\n"`, abort.
- `unreachable` → write `"internal error: entered unreachable code at file:line:col\n"`, abort.
- `assert` → true falls through to `assert.cont`; false enters `assert.fail` (write
  `"assertion failed at file:line:col\n"`, abort).

That sequence is **not** emitted inline — see Error-Path Outlining. The `file:line:col` suffix
comes from the `Call` span start via the `SourceFile` (empty when no source is supplied).
`write` + `abort` are POSIX/libc (Linux, macOS; MSVC CRT on Windows).

Because `panic` / `unreachable` terminate the block with `unreachable`, following statements are
dead code: `codegen_stmt` early-returns when the block is already terminated, and `codegen_return`
and `codegen_body`'s tail path skip the `ret` when evaluating the returned expression terminated
the block (`func f() -> i32 { panic("x") }`). This keeps LLVM from seeing instructions after a
terminator.

## Error-Path Outlining
`outlining.rs` emits every panic-family failure path into a module-private cold function and leaves
one call at the failure site, so the diagnostic machinery never sits inline in the function that
can fail. It covers `panic` / `assert` / `unreachable` and every `codegen_guard_or_panic` caller —
array and `Vec` bounds, string-slice bounds, UTF-8 codepoint boundary.

- Thunks are named `neuro.cold.panic.N`, `Linkage::Private`, with attributes
  `cold noreturn noinline minsize`; the call site repeats `cold noreturn` so the information
  survives inlining of the *enclosing* function. `noinline` is load-bearing: without it the inliner
  folds a single-call-site function straight back in.
- `cold_thunks: HashMap<(bool, String), FunctionValue>` dedups by (takes a runtime message,
  constant diagnostic text). Monomorphization's copies of one generic body render identical text
  from the same span and therefore share one thunk.
- The runtime-message form is a `(ptr, i64)` thunk: only the constant fragments are baked in, the
  fat pointer travels as two arguments. `emit_write_cstr` / `emit_write` / `emit_abort_unreachable`
  (`panic.rs`) are `pub(crate)` for it, and `build_thunk_body` saves and restores the builder
  position since thunks are created lazily mid-function.
- `mark_cold_branch(branch, cold_edge_is_true)` attaches `!prof` `branch_weights` (`2000 : 1`) to
  every guard branch and to the `-O0` overflow check. The overflow trap is weighted but **not**
  outlined — its block is a single `llvm.trap`, so a call would trade one instruction for another.

## Standard-Output ABI
`print(text: string)` / `println(text: string)` lower in `io.rs`. The `Call`→`Identifier` arm
intercepts them via `CodegenContext::is_io_builtin` after the panic family and under the same
user-function-shadows rule. Both return unit, so the dispatch yields `Ok(None)`: a statement
discards it, and value position reports the ordinary void-where-a-value-was-expected error. No
`CodegenContext` state is added — the helper and the global are found by name via
`module.get_function` / `get_global`.

The argument is already the `{ ptr, i64 }` fat pointer (interpolation renders every hole before the
call is reached), so lowering is an `extractvalue` pair and a call. `println` follows the text with
a second call over `neuro.print.newline`, a one-byte `.rodata` global emitted once per module. That
byte is `\n`; on Windows the CRT's text-mode fd 1 turns it into `\r\n`, which is why the
`print_builtins` and `examples` tests compare stdout with line endings normalized.

There is **no buffering**: output reaches fd 1 through the same external POSIX `write` the panic
runtime declares, unflushed. A buffered stdout is later work.

Both go through `neuro.print.write_all(ptr, i64)`, one module-private `Linkage::Private` helper
emitted on first use, holding the short-write retry loop: `write` may consume less than it was
offered (a pipe with a full buffer does exactly that), and a bare call per site would silently
truncate the language's primary result channel. The loop stops on a non-positive return so a
closed or failing descriptor cannot spin. `get_or_build_write_all` saves and restores the builder
position, since the helper is built lazily mid-function, and `split_printable` reports a
non-aggregate operand as an internal error rather than asking it for a struct variant it has not
got.

## String Interpolation ABI
`codegen/expressions/interp.rs` renders each part to a `{ ptr, len }` fat pointer and concatenates
them into one fresh `malloc`'d buffer (which leaks, exactly as `+` concatenation does). The
rendering helpers live in `format_helpers.rs` (`snprintf`-backed integer and float conversion,
hand-written binary digits), `format_layout.rs` (sign-aware field padding, debug quoting, UTF-8
encoding of a `char`), and `format_float.rs` (restoring the point `%g` drops, normalizing C's
`e+00` exponent). Each is emitted once per module with internal linkage rather than inlined at
every hole. `snprintf` is the one external declaration this adds.

## Closure ABI
`codegen/closures.rs`. A closure value is a `{ fn_ptr, env_ptr }` fat pointer, and `map_type`
lowers `Type::Function` to that two-pointer struct. `declare_closure` / `codegen_closure` emit each
`HirItem::Closure` as a function `(env_ptr, params...) -> ret` whose prologue GEP/loads the
captures out of the environment struct into locals. `codegen_closure_value` allocates that struct
in the **defining frame**, snapshots each Copy capture, and pairs the closure function pointer with
it. `codegen_call_dispatch` routes a call whose callee is a local variable to
`codegen_indirect_call`, which extracts both pointers and issues an indirect call with the
environment as the hidden first argument.

The closure environment is frame-local, so a closure that escapes its defining scope is out of
scope this phase.

## Dynamic Dispatch ABI
`codegen/dispatch.rs`. `emit_vtables` walks every `impl Trait for Type` whose trait is
user-declared and emits a private constant global `[N x ptr]` per `(trait, type)`, in the trait's
declaration order, filled with per-method **thunks**. A thunk is needed because a `&self` method
takes its struct by value while a trait object holds only a pointer, so the thunk loads the
receiver and forwards; a `&mut self` method is already pointer-passed and forwards directly.

`codegen_dyn_coerce` builds the `{ data, vtable }` fat pointer for a `HirExprKind::DynCoerce`, and
`codegen_dyn_method_call` extracts both words, GEPs the method's fixed slot, and issues an indirect
call. `CodegenContext` carries `trait_methods` (vtable slot order, via `set_trait_methods`) and
`vtables`.

Static dispatch needs nothing here — `impl Trait` is monomorphized away before the HIR arrives.

## Drop ABI (deterministic destruction)
`drops.rs` inserts a `{struct}__drop(&mut self)` call at each lexical scope exit for an owned
binding of a `Drop` type. `drop_types: HashSet<String>` (filled by `compile` from `impl Drop for T`
blocks) gates everything: when it is empty the scope stack stays empty and zero IR is emitted, so
non-Drop programs are unaffected. `drop_scopes: Vec<Vec<DropEntry>>` is a stack of lexical scopes;
each `DropEntry` records the binding name, storage `alloca`, an `i1` drop flag, and a `DropTarget`
(`UserDrop(struct)` | `Collection`).

`codegen_function` / `codegen_method` open the body scope and register by-value `Drop` or
collection parameters for destruction at function exit; `codegen_var_decl` registers a local and
allocates its flag (initialised `true`). Branch, loop, and block bodies (`codegen_if`,
`codegen_while`/`loop`/`for_range`, `codegen_arm_into_alloca`, `codegen_block_expr`) push and pop
their own scope and emit that scope's drops in reverse declaration order at normal fall-through.
`return` runs every open scope (`emit_drops_through(0)`); `break`/`continue` run down to the loop
body scope recorded in `LoopTargets.drop_scope_depth`. A panic aborts without running drops (no
landing pads).

Each drop is flag-guarded (`if flag { drop(); flag = false }`), and `mark_moved_for_drop` clears a
binding's flag at every move site (bind / assign / return / break value / call arg / struct-field
store), so a moved value is dropped exactly once.

**Known limits**: reassigning a `Drop` binding does not drop its prior value, and a struct's `Drop`
fields are not auto-dropped (no recursive glue).

## Collections ABI
`Vec<T>`, `HashMap<K, V>`, `BTreeMap<K, V>`, and `String` share one by-value header —
`{ ptr buffer, i64 len, i64 cap, i64 used }` (`TypeMapper::collection_header_type`) — held in the
owner's alloca, with all elements in a single heap buffer. `len` counts live elements/entries,
`cap` the allocated slots, and `used` the *occupied* slots (live + tombstoned) that the hash map's
load factor is measured against; the other kinds leave `used` zero.

Buffer layouts, per kind:
- **`Vec<T>`** — a plain `[T]` run. Growth doubles `cap` (minimum 8) through one shared byte-sized
  `__neuro_vec_reserve(header, elem_size)` helper, so every `Vec<T>` in a module reuses it.
- **`HashMap<K, V>`** — `{ i8 state, K key, V value }` slots, power-of-two `cap`, so the bucket is
  `hash & (cap - 1)`. Linear probing; `state` is `0` EMPTY / `1` FULL / `2` TOMBSTONE. A lookup
  stops at the first EMPTY and skips tombstones; an insert takes the first non-FULL slot, so a
  tombstoned run is reused. Rehashing at a 3/4 load factor reclaims tombstones, which is what keeps
  a churned table's probe runs bounded.
- **`BTreeMap<K, V>`** — `{ K key, V value }` slots kept sorted by key: binary search to look up,
  `memmove` the tail to insert or erase. That gives the ordered iteration the type promises; a
  multi-way tree would change only the insert/erase constant, not this ABI.
- **`String`** — a byte run; `len` and `cap` are byte counts and `used` stays zero. It carries no
  type arguments, so one instantiation serves every program. `push_str` reserves through
  `__neuro_string_reserve(header, extra)` — capacity becomes `max(cap * 2, len + extra, 8)`, so one
  large append is a single `realloc` rather than a chain of doublings — then `memcpy`s the
  argument's bytes at `buffer + len`. `to_string` `malloc`s (at least one byte, so an empty result
  is never null) and copies out a `{ ptr, i64 }` `string`; a borrowed view into the buffer would
  dangle after the next `push_str`, which the borrow checker does not yet track.

Loop-shaped operations are emitted once per instantiation as private helpers named
`__neuro_{hmap,bmap}_{find,insert,keys}_<key>_<value>` (plus `__neuro_vec_reserve`,
`__neuro_string_reserve`, and `__neuro_hash_string`), created through `get_or_build_helper`, which
saves and restores the caller's insertion point and `current_function`. A lookup returns the slot
index, or `-1`. `codegen/collections/` is split `mod` / `vectors` / `keys` / `maps`, with `maps/`
further split into `lookup`, `insertion`, `iteration`, `probing`, and `growth`.

Key equality, order, and hash are compiler-supplied for int-like and `string` keys (FNV-1a for
strings, a SplitMix64 finalizer for integers, `memcmp` for string order) and routed to
`{Struct}__{eq,lt,hash}` for struct keys, adapting each argument to whatever parameter shape the
impl declared. Semantic analysis has already required those impls and rejects raw float keys.

`v[i]` is bounds-checked in **every** build, unlike `[T; N]`: a `Vec`'s length is not a
compile-time constant the optimizer can fold away. `pop` / `get` build their `Option<T>` with
`codegen_enum_value`.

A collection binding is registered in the drop scope with `DropTarget::Collection`, so scope exit
`free`s field 0 under the same runtime drop flag that user `Drop` types use — a moved-out
collection is not freed twice. An unnamed collection *temporary* (`for k in m.keys()`) is
registered the same way under a synthetic `__`-containing name that no source binding can collide
with; without that, the only route to map iteration would leak. This is also what frees a `String`
builder's buffer, since it is registered as an ordinary collection. **A `string` inside a
collection is not freed**, and neither is the heap `string` that `+`, interpolation, or
`String::to_string` produces; both ride with the heap-string work.

New libc declarations these need: `free`, `realloc`, `memmove`, `memset` (alongside the existing
`malloc`, `memcpy`, `memcmp`, `write`, `abort`, `snprintf`), each declared on first use in
`context.rs`.

## Soft-Float ABI
On generic x86-64, LLVM lowers `fpext` / `fptrunc` on `half` / `bfloat` — and f16/bf16
comparisons, which widen to f32 first — to runtime calls: `__extendhfsf2`, `__truncsfhf2`,
`__truncdfhf2`, `__truncsfbf2`, `__truncdfbf2`. Linux and macOS get these from libgcc/compiler-rt
(linked by the `cc` driver), but the Windows linkers (clang → lld-link → MSVC) link no such
runtime, so the symbols are undefined and linking fails. `src/softfloat/` provides our own
definitions and `compile` links them in behind a `module_uses_half_precision` gate. They are
`weak_odr`, so a platform runtime may still override, and integer-only, so they never recursively
re-emit these libcalls.

`builtins.ll` is generated from `reference.c` (`clang -O2 -emit-llvm`, then stripped of
target-specific datalayout/triple/attributes and marked `weak_odr`) and was exhaustively verified
against clang's native `_Float16` / `__bf16`. Regenerate via that command if LLVM's IR syntax
changes.

## Future: MLIR Integration
When tensor ops land, `melior` (Rust MLIR bindings, same LLVM 20 / MLIR 20 install) joins inkwell.
Lowering: AST → HIR → MLIR dialects (linalg/tensor/func/arith) → Enzyme MLIR AD pass → GPU dialects
(nvgpu/rocdl) or the `llvm` dialect → inkwell for final LLVM IR. inkwell stays the terminal
emission layer in all paths.
