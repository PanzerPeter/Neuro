# llvm-backend

## Purpose
Emit native object code, or the textual LLVM module behind it, from the typed Neuro HIR.

## Entry Point
- Type: Library function
- Input: `program: &neuro_hir::HirProgram, optimization: OptimizationLevelSetting, source: &str,
  source_path: &str`
- Output: `Result<Vec<u8>, CodegenError>` from `compile`, or `Result<String, CodegenError>`
  from `compile_to_ir`, which prints the module instead of selecting instructions

`CodegenError` implements `From<inkwell::builder::BuilderError>`, so the several hundred
builder calls inside codegen use `?` directly. A builder failure is always an internal
invariant break, never a fault in the program being compiled, and it surfaces as
`CodegenError::LlvmError`.

The backend consumes the typed HIR produced by `hir-lowering`: every HIR node carries its
resolved type (`HirExpr::ty`), so codegen reads types inline rather than re-deriving them.
**There is no backend type-collection pass**. A single `type_env` (binding name → resolved type),
populated as bindings are lowered, exists only so the place statements `obj.field = …` and
`arr[i] = …` can recover a binding's nominal struct/array type.

`source` / `source_path` are the original module text and path, kept solely to render
`file:line:col` in panic-family runtime diagnostics. The column counts characters, as
`neurc`'s compile-time diagnostics do. They affect nothing else.

`optimization` selects two independent things. It picks the `TargetMachine`'s level, which
governs instruction selection and register allocation; and it picks the LLVM IR pass
pipeline (`default<O1>` / `default<O2>` / `default<O3>`) run over the verified module before
instruction selection. Both are needed: the `TargetMachine` level runs no IR passes at all,
so without the pipeline nothing promotes an `alloca` to an SSA value, inlines a call, or
hoists loop-invariant work. `-O0` runs no pipeline, which is what keeps its checked
arithmetic and bounds guards where codegen emitted them.

The module is stamped with the target triple and data layout before the pipeline runs, so
the optimizer reasons about the real size, alignment, and pointer width of the types it
transforms.

## Shared Kernel
- neuro-hir: the typed HIR lowered from (`HirProgram` / `HirExpr` / `HirType`)
- ast-types: the `BinaryOp` / `UnaryOp` enums (reused unchanged by the HIR)
- shared-types: type system primitives, `FormatSpec` for interpolation

inkwell 0.10.0 (feature `llvm20-1`) is a third-party crate, not Shared Kernel. Requires LLVM 20;
set `LLVM_SYS_201_PREFIX` (e.g. `/usr/lib/llvm20`) before building. `semantic-analysis` is not a
production dependency: neurc orders type-check then HIR lowering before codegen.
`syntax-parsing` and `hir-lowering` appear only in `[dev-dependencies]` (tests and benches lower
source to HIR before compiling).

`resolve_builtin_method` / `is_panic_builtin` / `is_io_builtin` are duplicated from
`semantic-analysis` to keep the backend independent of the type-checker slice.

## Module Emission Order
`compile` splits into `build_module` (generate + verify) and `emit_object_code`, so codegen tests
can assert on IR text that object emission erases. `compile_to_ir` is the third caller of that
split: `build_module`, then `optimize_module` for the data layout, triple and pass pipeline, then
`print_to_string`. Inside `build_module` the order is fixed and
load-bearing:

1. **Signature pre-declaration** over every function, method, and closure before any body:
   `declare_function` / `declare_method` / `declare_impl` / `declare_closure` add the LLVM
   signature and register it in `functions`; the `codegen_*` counterparts fetch that declaration
   rather than adding one. Monomorphization means the call graph is no longer definition-ordered
   (an instance may be called by, or call, items emitted before it), so a call must resolve
   regardless of order.
2. **Vtables** (`emit_vtables`): after all signatures are declared, before any body, so item
   order never matters.
3. **Bodies.**
4. **Standard-output drain** (`finalize_stdout_buffer`): inserted after all bodies, because only
   a finished module knows whether it prints at all and because the exit paths it edits are all
   emitted by then. See Standard-Output ABI.
5. **Soft-float builtins** linked in when the module uses `half`/`bfloat`, after codegen and
   before `verify`.

## Stack Slot Placement
`CodegenContext::entry_alloca` positions the builder before the entry block's first instruction,
allocates, and restores. **Every** local binding, result slot, induction variable, scratch temp,
drop flag, and closure environment goes through it. Allocating at the current builder position
meant a slot inside a loop body was re-allocated per iteration, so a long enough loop segfaulted,
at every `-O` level, since `mem2reg` only promotes entry-block allocas. The initializing store
stays where it was; sharing one slot across iterations is sound because each is written before it
is read, and a fresh frame per call keeps recursion correct. Parameter and `self` allocas in
`functions.rs` are already in the entry block by construction.

## Name Scoping
`variables`, `variable_types`, and `type_env` are flat maps per function, so lexical scoping
is imposed on top of them by `name_scopes`, a stack of frames pushed and popped by
`push_drop_scope` / `pop_drop_scope`. Every binding form registers its name through
`bind_name`, which returns whatever the name meant before; a declaration records that in the
innermost frame, and leaving the scope replays the frame through `restore_bindings`.

The two stacks ride together because they delimit the same `{ }`: a binding whose owner the
drop scope releases on the way out has to stop being resolvable on the way out too. While the
maps were unscoped, a block-local binding stayed resolvable after its block and every later
mention of that name reached the inner slot, reading a stale value or writing through a freed
buffer. Neither the type checker (which scopes correctly, and rejects the name after the
block) nor the LLVM verifier could see it: at equal types the IR is well formed.

Match-arm and `val`-else pattern bindings use the same `bind_name` / `restore_bindings` pair
directly rather than through a frame, because their scope is an arm rather than a block, and
`val`-else deliberately leaves its ok-branch bindings registered for the enclosing block.

## String ABI
`string` = anonymous LLVM struct `{ ptr, i64 }`:
- field 0 (`ptr`): pointer to null-terminated UTF-8 bytes in `.rodata`
- field 1 (`i64`): byte count **excluding** the null terminator

Literals are emitted to `.rodata`, never heap-allocated; the appended NUL
(`STRING_NULL_TERMINATOR` in `literals.rs`) exists only for C-string FFI validity. `len` is
authoritative: interior NULs are legal counted content, so consumers must not treat the data as
NUL-terminated.

Passed and returned by value. On x86-64 SysV this fits two registers, so no `sret` indirection.
The semantic `Type::String` is unchanged: the fat-pointer layout is a backend-only detail.

### `&string`
An **immutable** `&string` is the `{ ptr, i64 }` fat pointer itself, held by value. It is not a
pointer to one. `string` is immutable, so the referent's address carries nothing the fat pointer
does not, and demanding one forces every computed slice (`s.slice(a..b)`, which has no home) into
a stack slot whose address then outlives the frame it was taken in (BUG-008). By value, a slice is
returned like any other aggregate, `.len()` is an `extractvalue`, and no slot exists to dangle.

`&mut string` is the exception: a store through it has to reach the referent, so it stays the
referent's address (an opaque `ptr`), exactly like every other `&mut T`. `&&string` is likewise a
pointer: the outer reference borrows a reference, not a string. The backend `Type::Reference`
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

`@derive(PartialEq)` equality is expanded here too, in `codegen/expressions/struct_eq.rs`, and for
the same reason: before the numeric coercion, which would ask an aggregate value for its integer
variant. `codegen_derived_struct_eq` walks the struct's fields from `struct_defs`, comparing each
with `icmp` / `fcmp` / `codegen_string_eq` and recursing into a nested struct, then `and`s the
results: none of the comparisons can have a side effect, so an `and` chain is cheaper than the
branching a short-circuit would need. A `&mut S` operand is loaded through first. `!=` negates.
Recursion is bounded by `MAX_DERIVE_DEPTH`, insurance against a future self-referential layout.
A hand-written `impl PartialEq` never arrives here: lowering turned it into a method call.

`+` is concatenation, routed to `codegen_string_concat` before the numeric coercion: both operands
are normalized with `load_string_fatptr`, a `len1 + len2` buffer is `malloc`'d, each operand's
bytes are `memcpy`'d in (the second at a `gep i8` offset of `len1`), and a fresh `{ ptr, len }` is
returned. The result is a new owned, immutable string with **no** NUL terminator (consistent with
the `len` contract). The frontend types the result as owned `String` even when an operand is
`&string`, so the value is never a reference.

### Heap-string ownership
The fat pointer describes a `.rodata` literal and a `malloc`'d buffer identically, so ownership
cannot be read off a value at runtime. It is decided at compile time instead, by
`produces_owned_string` (`drops.rs`): an expression owns its buffer only if it is an
`InterpString`, a `+` yielding `string`, `String::to_string`, a `string.clone()`, or a call to a function
`codegen/string_ownership.rs` proved allocates on every return path. Everything else (a literal,
a variable, a `slice`) answers `false` and is never freed. The asymmetry is deliberate: a missed
`true` leaks a buffer, a wrong `true` hands `.rodata` to `free`.

Consumers act on that answer in one of two ways. A consumer that keeps the value registers an
owner for it: `codegen_var_decl` registers a `string` binding whose initializer owns its buffer as
`DropTarget::HeapString`, so the scope-exit machinery releases it exactly as it releases a
collection's storage, flag-guarded against a move. A consumer that copies the bytes out and keeps
none of them calls `release_string_temporary` instead, which frees the buffer on the spot when the
operand produced one. This is what makes an *anonymous* heap string, one no binding ever names,
reachable by a release at all. Those consumers are `print` / `println`'s argument, an interpolation
hole, both `+` operands, both `==` / `!=` operands, a `.len()` receiver, a `push_str` argument, a
map key (an `insert`, and a lookup: `get` / `contains_key` / `remove`), and a statement whose value
nothing reads. The release goes through `__neuro_release`, so a temporary a
`pool` block allocated from the arena is left to the arena's own sweep.

Reassigning a registered binding releases the buffer it displaces and then re-derives ownership from
the assigned expression, so `s = s + "!"` frees the old buffer and keeps the new one while
`s = "literal"` frees the old buffer and leaves the binding owning nothing.

A bare binding name proves nothing about ownership, but the binding it names does, at run time:
`load_owned_string_flag` reads its flag before the move clears it, and `val u = t` / `s = t`
store that value into the target's flag, so the buffer changes owner rather than losing one. A
binding that may ever own a buffer carries a `HeapString` entry: one whose initializer owns, one
moved from a binding that may own, and every `mut` one (its flag starts clear for a literal, so
`mut s = "x"` then `s = a + b` has a flag for the assignment to arm). An immutable binding holding
a literal can never own and carries none. A holder moved whole does the same per position:
`load_held_string_flags` reads every `string` position's flag off the source and
`store_held_string_flags` writes them into the target's matching positions. A holder built from a
literal collects the same thing per position while the declaration or assignment evaluates it
(`literal_string_moves`): `codegen_literal_position` reads the flag of a `string` binding moved
into a struct, tuple or array literal position (`Entry { label: t }`, nested literals included,
each extending the path), and a functional update contributes its base's flags for the positions
it takes. The collection is suspended inside any position that is not itself a literal, since a
literal there (a call argument, a branch) builds some other value.

A third kind of consumer STORES the fat pointer somewhere that outlives the expression. Ownership
then belongs to the storage, and is tracked one of three ways:

- **A position inside a holder** (a struct field, an array or tuple element, one of those nested).
  `plan_held_drops` plans a `DropTarget::HeapString` held entry for every `string` position, with
  its flag DISARMED — the type proves nothing. `arm_stored_string_positions` arms the positions the
  initializer or field assignment stored a provable allocation into, walking the aggregate literal
  in step with the held paths. A holder's move or its field's move clears those flags exactly as it
  clears any other. An enum payload gets no such entry: its slot is destroyed under a tag switch
  with no flag to guard it.
- **A function's return value.** `codegen/string_ownership.rs` reads every body once, before any is
  generated, and collects the functions whose every exit (each `return`, plus an expression tail)
  allocates. `impl` methods and associated functions are collected too, keyed by the
  `Type__method` their call sites mangle, and a producer called with arguments is a producer. The set is a fixpoint, because one producer can be another's only return path; it
  starts empty and grows, so a recursive cycle never enters it. A name a local binding shadows is
  excluded, since `codegen_call_dispatch` may send that call through the indirect path. A tail
  `if` (with an `else`), `match` or block exits through each branch's own tail (`tail_exits`). A
  `return` written inside an expression (a `loop` body, an `if` or `match` used as a value, a
  block) is an exit `collect_returns` does not enumerate, so a body holding one is never a
  producer: missing that exit once let a function that returned a literal through it be read as
  allocating, and its caller freed the literal.
- **A by-value argument.** The same pass records the `string` parameters whose callee provably only
  READS them, by a whitelist of positions that copy the bytes out (a `print`/`println` argument, an
  interpolation hole, a binary operand, a `.len()` or `.clone()` receiver, a `push_str` argument,
  anything under an `as` cast, and an argument to another declared function whose parameter in
  that position is itself read only). Every other occurrence is a retention. The last position
  makes this summary a fixpoint too, growing from empty exactly like the return summary, and it
  trusts no callee name a local binding shadows. `.clone()` qualifies only because a `string`
  clone copies its bytes into a buffer of its own (`copy_string_bytes`) and is an owned producer;
  it once handed back the receiver's fat pointer, which made every clone an alias. Which side then releases depends on where the argument came from. An
  argument that ALLOCATED in place owns no flag, so `release_owned_arguments` frees it right after
  the call, where the callee's frame is already gone. An argument that names a PLACE keeps the flag
  it already had and is released by that place's own scope: the argument loop skips the move's
  disarm for a read-only parameter, because the callee retains nothing past the call and a place
  disarmed there would reach no release at all.

- **A collection slot** (a `Vec` element, a map key or value). A slot's fat pointer says no more
  about ownership than any other, so the boundary decides it instead: every `string` that enters a
  slot is copied into a buffer the collection owns (`value_for_collection_slot`, which adopts
  rather than copies when `produces_owned_string` proves the operand was built for this store),
  and every `string` read out of one is copied back out (`value_from_collection_slot`). The
  collection then releases its live slots before freeing its buffer, through one
  per-instantiation `__neuro_<kind>_drop_elems_<args>` helper that `emit_collection_free` and
  `clear()` both call; `remove` and an overwriting `insert` or `v[i] =` release just the slot they
  give up. `pop` is the one read that transfers instead of copying, because the slot is gone by
  the time the reader sees it. `collections/elements.rs` owns the value helpers and the `Vec`
  walk, `collections/maps/release.rs` the two map walks.

  Reads then re-enter the machinery above: `produces_owned_string` answers `true` for a collection
  index, so `val s = v[0]` is a `DropTarget::HeapString` binding and `println(v[0])` is released
  at the consumer. A `for`-in binding is registered inside the loop body's own scope, so each
  pass releases its copy. A payload the fallible readers hand out inside an `Option` is registered
  by the `match` arm that binds it (`produces_owned_option_payload`), which is the only arm shape
  where a `string` payload can be proven owned.

**Known limits**: among the covered positions, three shapes stay unproven
and leak: a holder a call built (the call proves nothing about its positions), a function with one
literal-returning path, and a parameter the callee may store. A collection read carried into a
view-producing method (`v[0].slice(...)`, `.chars()`) leaks its copy, as any other anonymous
string does there.

## Struct ABI
User structs lower to anonymous LLVM structs `{ T0, T1, ... }` in declaration order (no padding:
LLVM handles alignment). `TypeMapper` holds the layout table (`set_struct_fields`, fed by
`CodegenContext::set_struct_defs`) beside `enum_payloads`, and `struct_written_names` (fed by
`set_struct_written_names`) maps each key to the name the programmer wrote: they differ only for
a monomorphized generic instance, and only the derived debug rendering reads it. `map_type` builds a named struct's
aggregate: a struct works as a free function's **parameter and return type** and as a field of
another struct. That ABI is by value and direct (no `sret`), matching what methods already did
for `&self`. `get_struct_llvm_type` delegates to `TypeMapper::struct_type`, so one definition of
the layout serves both paths; recursion is bounded by `MAX_STRUCT_DEPTH`, which is insurance
rather than a live case (a cycle is impossible today: a field type must be declared before use).

Values live on the stack via `alloca`, initialised field-by-field with `insertvalue`; reads are
`getelementptr`+`load`, writes `getelementptr`+`store`. A functional update
(`Point { x: 1.0, ..p }`) seeds the aggregate from the base struct value rather than `get_undef()`
and `insertvalue`s the explicit fields over it. Every field it does not write is moved out of the
base, so those positions are disarmed there; the overridden ones stay the base's to release.

`codegen_field_access` reads a field of a **non-place** object (a chain `o.inner.v`, a call result)
with `extractvalue`, keeping the GEP-and-load path for a named binding.

`get_struct_ptr_and_type` addresses a receiver place. It resolves a named binding (auto-loading
through a `&Struct` / `&mut Struct` binding, whose alloca holds a pointer rather than the
aggregate) and, recursively, a **field of a place**: the parent's pointer, then a GEP to the
field's slot. That second arm is what makes `self.inner.next()` work: an adapter's `&mut self`
method driving the iterator it wraps writes back into the field's own storage, where reaching
the field as a value would discard the advance. Anything else (a call result, a literal) has no
address and is refused: a `&mut self` method needs storage to write through.

## Method ABI
`impl` methods lower to LLVM free functions mangled `StructName__methodName` (double underscore).
`codegen_method_call` recovers the receiver struct by splitting the symbol on `__`, so the
separator must appear **exactly once**. Two rules hold that: semantic analysis rejects a declared
name containing `__` (`TypeError::ReservedNameSeparator`), and every monomorphized instance name
uses a single-underscore `_g_` marker (`identity_g_i32`, `Pair_g_i32_f64`).

- `&self` and owned `self` take the struct **by value** as `param[0]`, named `self` in the
  alloca map. Callers load their stack var and pass the value. The two differ only in ownership:
  a mangled name in `consuming_self_methods` has its receiver `mark_moved_for_drop`-ed at the
  call site and registered in the callee's own drop scope, so a consumed `Drop` receiver is
  destroyed once, by the callee.
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
a receiver `Type` plus method name to a `BuiltinMethod` tag **only**: the call's result type comes
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
  `codegen_guard_or_panic` (`panic.rs`): abort, no unwinding, in every build. The result is the
  computed fat pointer itself, returned by value with no stack slot, so a slice returned across a
  call boundary stays valid. The `Range` argument is consumed here; reaching it through general
  `codegen_expr` is an internal error.
- `string.char_slice(a..b)` / `.char_slice(a..=b)` → `codegen_char_slice`
  (`expressions/char_slice.rs`), the same borrowed `&string` with its range counting **code
  points** instead of bytes. Each endpoint is resolved to a byte offset by the module-private
  `neuro.string.char_offset(ptr, len, index)` helper: a byte walk that skips continuation bytes
  (`0b10xxxxxx`) and answers `-1` for an index the string does not reach. The resulting byte
  pair goes through the same `string_fat_slice` tail `.slice` uses. Only the bounds guard survives
  (`start` found, and `start <= end` on the resolved offsets, which catches a reversed range and an
  unreachable end together); the UTF-8 boundary checks do not exist here, because a code point
  index cannot name a position inside a code point. `char_offset(s, n)` for an `n`-character string
  is `len`, which is what makes the end of the string a legal upper bound.
- `string.__char_at(offset)` → `BuiltinMethod::StringCharAt` → `codegen_char_at`
  (`expressions/char_at.rs`), the Unicode scalar whose UTF-8 encoding begins at that byte, as an
  `i32`. The module-private `neuro.string.char_at(ptr, len, offset)` helper takes the lead byte's
  payload (selected from its own value, since the width follows from it) and folds in every
  following byte matching `0b10xxxxxx`, so a scalar's own bytes bound the loop and no width is
  computed up front. An offset at or past `len` answers `0`. This is the only byte-indexed read of
  a string in the backend, and its one caller is the prelude's codepoint iterator: the frontend
  refuses the method to every other module. There is no `.chars()` tag here: that call is already
  the iterator's struct literal by the time the HIR arrives.
- `seq.slice(a..b)` / `.slice(a..=b)` on an array, `Vec`, or slice → `BuiltinMethod::SequenceSlice`
  → `codegen_sequence_slice` (`expressions/slices.rs`), the slice ABI below. It resolves **ahead
  of** the collection method surface in `codegen_call_expr`, because a `Vec` receiver's `.slice`
  borrows the buffer rather than acting on the header.
- `slice.len()` → `BuiltinMethod::SliceLen` → `extractvalue` field 1 of the fat pointer (`u64`).
- `struct.clone()` → handled in the struct method-call arm rather than `resolve_builtin_method`
  (which is keyed by `Type`): when the receiver is a struct, the field is `clone`, and no
  `StructName__clone` exists, it passes `BuiltinMethod::StructClone`. Semantic analysis already
  verified the `Clone` derive. Lowers to the receiver's aggregate value: faithful while
  stack-allocated, must recurse into heap-owning fields later.
- `tensor.clone()` → `BuiltinMethod::TensorClone` → `codegen_tensor_clone`
  (`expressions/tensors.rs`). A tensor value is a DLPack handle, so the clone allocates a second
  handle and a second buffer and `memcpy`s the elements across: the copy is independent and both
  `data` addresses stay stable. An owned
  receiver lowers to the tensor pointer; a `&Tensor` receiver lowers to the *address of* that
  pointer. Both are `ptr` in LLVM, so the distinction comes from `recv_ty`, not from the value:
  the one auto-deref site the value-driven rule below cannot decide.
- `tensor.to(device)` → `BuiltinMethod::TensorTo` → `codegen_tensor_to` (same file). The device
  argument is the prelude `Device` enum; its tag (`extractvalue` field 0) is compared against
  `enum_variant_tag("Device", "CPU")` and routed through `codegen_guard_or_panic`, so a transfer
  to any other device aborts with a diagnostic rather than silently leaving the buffer on the
  host. A host transfer is the move itself and emits no copy: the receiver's value is the result.
  `resolve_builtin_method` matches `.to` on the receiver type rather than its referent, so a
  `&Tensor` resolves to nothing: a borrow cannot be consumed. Because the result *is* the
  receiver's buffer pointer, `codegen_tensor_to` calls `mark_moved_for_drop` on the receiver;
  without it the one buffer would be freed by both the source binding and the transfer's result.
- Integer intrinsics (`wrapping_{add,sub,mul}`, `saturating_{add,sub,mul}`, `.shr(n)`) resolve
  on any integer receiver to its own type and lower in `codegen_int_intrinsic`. Both operands are
  coerced to the receiver int via `coerce_if_needed` (an argument literal may arrive widened to
  i32). Wrapping → plain `add`/`sub`/`mul`, no `nsw`/`nuw`, never trapping. `.shr` → `ashr`
  (signed) / `lshr` (unsigned). `saturating_add`/`sub` → `llvm.{s,u}{add,sub}.sat`;
  `saturating_mul` has no direct intrinsic and becomes `{s,u}mul.with.overflow` + `select`
  (unsigned → MAX; signed → MIN on differing operand signs, else MAX).
- `.is_nan()` → `codegen_is_nan`: `fcmp uno x, x` on the receiver value, yielding the `i1` a
  `bool` lowers to. The self-comparison IS the test (NaN is the only value unordered with
  itself), and it is why the check cannot be spelled in source, where `x != x` uses the ordered
  predicate. Resolved for `F32`/`F64` spelled out rather than via this slice's `Type::is_float`,
  which also admits `f16`/`bf16`.
- `checked_{add,sub,mul}` → `codegen_checked_int_intrinsic`: `llvm.{s,u}{add,sub,mul}.with.overflow`
  via the shared `emit_with_overflow`, then `build_option_value` (`collections/mod.rs`) selects
  `Some(result)` / `None` on the negated overflow bit. Branchless: both variants are materialized
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
`BasicValueEnum` in `const_values` for the function scope (no `alloca`, purely compile-time).

Folding uses a pure-Rust `FoldedConst { Int(i64), Float(f64), Bool(bool), Str(String) }` rather
than inkwell's const-arithmetic API (inconsistent across versions): all arithmetic happens in Rust
(wrapping ints, IEEE-754 floats, and a `(Bool, Bool)` arm for `&&`/`||`/`==`/`!=`), and a single
`const_int` / `const_float` / `const_struct` builds the final LLVM value. The `FoldedConst` path
always wraps, regardless of `overflow_checks`.

## Enum ABI
`compile` builds an `enum_payloads` table (each enum's variant payload types) and hands it to the
`TypeMapper`. `enum_payload_shape` derives the layout from it as `(slots, words)`: `slots` is the
widest variant's field count, `words` the widest single payload field rounded up to whole 64-bit
words (`llvm_words`, a deliberate over-estimate — every field is rounded up before it is summed,
which cannot under-count a layout whose maximum alignment is 8). An enum is therefore the tagged
union `{ i32 tag, [W x [K x i64]] payload }`, usable as a parameter, return, or field via
`map_type`.

`codegen_enum_construct` (`expressions/enums.rs`) writes each payload field into its slot through
`enum_payload_cell`: a zeroed `[K x i64]` stack slot the field's own type is stored into and the
slot type loaded back out of. Going through memory is what makes a slot type-agnostic — a
`string` fat pointer, a struct, an array or a tuple round-trips bit-exactly, exactly as a scalar
does — and zeroing it is what keeps a field narrower than the slot from leaving poison in the
words the load reads anyway. The cell is an `entry_alloca`, not a local one: a cell built at the
builder's position inside a loop body grows the stack by one slot per iteration. A payload may be
any sized type; semantic analysis rejects the unsized ones. `codegen_enum_value` is
the split-out half that builds an enum from already-evaluated values, and the context's
`enum_variants` table (name → declaration order) resolves `Some` / `None` tags **by name** rather
than assuming the prelude's declaration order.

## Aggregate ABIs
- **Tuples**: `map_type` → anonymous LLVM struct `{ T1, T2, ... }`. `codegen_tuple_literal`
  builds it with `insert_value` (with per-element `coerce_if_needed` for default-typed literals);
  `codegen_tuple_index` reads element N with `extract_value`, auto-loading through a `&tuple`
  borrow pointer first. Tuples flow through parameters and returns.
- **Arrays**: `map_type` → LLVM `[N x T]`. `expressions/arrays.rs` lowers array literals, index
  read/write (with a bounds guard through `codegen_guard_or_panic`, in every build), and
  `for x in arr` / `for x in &arr`. `BuiltinMethod::ArrayLen` is a compile-time `u64`.
  `coerce_if_needed` has an element-wise array arm for typed `[i64; N] = [..]` literals.
- **Array rest**: `codegen_array_rest` builds a fresh `[T; N - start]` aggregate by loading
  elements `start..N` of the source (via `array_place_ptr`) and `insert_value`-ing them. A
  zero-length remainder (the rest-less arity-assert form) yields an undef `[T; 0]`, discarded in
  statement position.
- **Slices**: `map_type` lowers `&[T]` *and* `&mut [T]` to `slice_ref_type()`, the
  `{ ptr buffer, i64 len }` fat pointer held by value; a bare `[T]` is rejected as unsized. The
  mutable form is by value too (unlike `&mut string`): a write through a slice goes to the buffer
  the pointer names, not to the pair. `expressions/slices.rs` owns every operation.
  `slice_source` reduces the three receivers to one `(buffer, element type, length)` triple:
  an array via `array_place_ptr` with its static `N`, a `Vec` via `collection_place_ptr` plus its
  `FIELD_LEN`/`FIELD_BUFFER`, a slice by `extractvalue`. `codegen_slice_coerce` (the
  `SliceCoerce` node) pairs that triple back into a fat pointer. `codegen_sequence_slice` computes
  `(base + start, end - start)` behind a bounds guard that runs in **every** build, not only debug
  ones: an out-of-range range hands back a *view* that outlives the check, so there is no later
  point at which a release build could still notice. Index reads/writes and `for x in xs` keep the
  ordinary debug-only element guard, against the runtime length instead of a constant.
- **Newtypes**. Transparent at runtime: `Type::from_hir` erases `HirType::Newtype { inner, .. }`
  to `from_hir(inner)`, so codegen never sees a newtype. `NewtypeConstruct` and `NewtypeAccess`
  both codegen their inner expression unchanged. No backend `Type` variant, type mapping, or item
  handling.

## Reference and Primitive Lowering
`map_type` lowers a reference to an opaque `ptr`, with three exceptions: an immutable `&string`
(the fat pointer itself, above), `Reference(DynObject)` (the two-word `dyn_ref_type()` struct),
and `Reference(Slice)` (the two-word `slice_ref_type()` struct, mutable or not). A bare
`DynObject` or `Slice` is rejected as unsized. `Type::Tensor { .. }` maps to an opaque `ptr`: the
value is a **DLPack handle** (a pointer to the `DLManagedTensorVersioned` that
`dlpack_managed_tensor_type` lays out), and its `data` field addresses the element buffer, whose
own layout `tensor_buffer_type` gives as a flat, row-major `[d0*d1*... x T]` array. The rank-0
tensor's buffer is `[1 x T]` (the empty product), not a zero-length array. Because the value is
just the handle, a tensor with a dynamic `?` axis maps, moves and releases like any other;
`types::static_extents` guards the sites that do need a number — the buffer layout, its byte
size, an index's strides — and reports `UnsupportedType` rather than sizing an allocation from a
guess. Host memory only:
`.to(device)` guards on the requested device rather than moving anything, and the handle reports
`kDLCPU` until a device backend flips that field.

The buffer is out of line because the language has a tensor *own* it and promises that buffer a
stable address across an in-place update: neither is expressible for an SSA value, which has no
address.
It is also what makes a large tensor compilable: an aggregate copy is a whole-buffer `load`/`store`
pair that only `-O1`'s SROA turns into a `memcpy`, and SelectionDAG crashed legalizing one above
~50k elements at `-O0`. There is no size cap any more, at any optimization level.

`codegen/dlpack.rs` owns the handle: it allocates both blocks, fills every field, emits the
per-tensor-type `shape` and `strides` constants, and defines the one shared `deleter`. See the
DLPack Representation section below.

`expressions/tensors.rs` owns the construction nodes. `alloc_tensor` is the single place the
allocator is chosen (the hook 2D's arena replaces), and every construction node routes through it,
taking back both the handle it returns and the `data` pointer it writes elements through.
`zeros()` / `ones()` / `identity()` and a literal whose elements are all constants emit the buffer
once as a private `.rodata` global and `memcpy` it in, so a fill of any size costs one call rather
than an instruction per element; a literal mentioning a runtime value is written slot by slot;
`random_normal` writes its counted loop straight into the heap buffer.

`expressions/tensor_rng.rs` holds the xorshift64 generator `random_normal` draws from, and the
float intrinsics its Box-Muller transform calls. Nothing else in the backend draws a random
number.

A half-precision tensor element is widened to `f32` for each operation and rounded back once
(`widen_half` / `narrow_float` in `tensor_arith.rs`, used by `tensor_element_arith`, so by the
elementwise operators, compound assignment and `@`), and a reduction over one folds in an `f32`
accumulator and narrows the finished value (BUG-073). LLVM's own `half` / `bfloat` arithmetic is
not relied on, for the reason `elementwise_math.rs` gives.

`expressions/tensor_arith.rs` owns the binary operators, the broadcast machinery, the `@`
contraction and the in-place compound assignment: everything that READS buffers that already
exist, as against `tensors.rs`, which builds and re-describes tensor values. The two share the
allocation helpers (`tensor_layout`, `alloc_tensor`, `tensor_slot`) and nothing else.

`codegen_tensor_compound_assign` there is the one tensor node that allocates
**nothing**: `HirStmt::TensorCompoundAssign` loads the target's own handle out of its variable
slot and runs a counted loop writing each updated element back into the buffer that handle
already addresses, so the handle and its `data` pointer are the same values after the statement
as before. The right-hand side is evaluated before the target is touched, and an owned operand
is released through `build_dlpack_release` after the loop (`mark_moved_for_drop` first, so the
buffer is not freed twice) while a borrowed one is only read. Element arithmetic goes through
`tensor_element_arith`, which reuses the scalar `codegen_int_arith` / `codegen_int_div_rem`
guards: an overflowing element panics on the debug tier and a zero divisor panics in every
build, exactly as the scalar operator does.

`codegen_tensor_binary` (same file) is the by-value operator, dispatched from the `Binary` arm
of `codegen_expr` on the **result** type rather than the left operand's, since either side may
be the scalar being broadcast. It allocates, which is the whole difference from the compound
form: a fresh handle and buffer, both operands read, and each owned operand released afterwards.
Both nodes share `emit_elementwise_loop`. Each operand resolves to an `ElementSource`: a
`Scalar` value, or a `Buffer` plus one flat stride per axis of the result, computed by
`broadcast_strides`. A stretched axis carries stride **0**, which is the whole of broadcasting;
an operand of the result's own shape is marked `contiguous` and walked slot for slot, so the
common case pays no coordinate arithmetic. The destination is always contiguous, so the loop
counter is its slot index, and the result coordinates are decomposed once per iteration and
shared by both sources.

`@` branches out of `codegen_tensor_binary` into `codegen_tensor_matmul` before any of that: a
matrix product contracts an axis instead of walking the result element for element, so it is a
different loop shape rather than a different body. `emit_contraction_loop` walks the destination
flat, one iteration per output element, recovering the row and column from the counter by
division and remainder on `N`, and reduces over K in an inner loop. The accumulator is an entry
`alloca` rather than a `phi`, because `tensor_element_arith` may split the body around an
overflow guard and a `phi` would then have to chase whichever block came out of it. Both
operands resolve through `codegen_tensor_operand`, the same handle/buffer/owned triple
`codegen_operand_source` builds an `ElementSource` from, and an owned one is released by
`release_consumed_buffer` once the product is built. Overflow and divide-by-zero guards are the
element's, exactly as for the element-wise family, so an overflowing accumulation panics where
an overflowing scalar `+` would.

`codegen_tensor_shape_cast` (same file) lowers `HirExprKind::TensorShapeCast`: `.t()`,
`.reshape(...)`, `.permute(...)` and `.flatten(...)`. It is not a `BuiltinMethod` — the method
name alone would not say how the axes move, so lowering resolved that into the node's
`permutation` and the backend never sees the four spellings. Both halves consume the receiver
(`mark_moved_for_drop`), leaving exactly one buffer alive. With no permutation the receiver's own
handle is returned after `build_dlpack_redescribe` rewrites its `ndim`, `shape`, and `strides`
to the result type's globals: the elements are already in the result's order, so a reshape of any
size costs three stores, allocates nothing, and does not move `data`. With one, a fresh handle and
buffer are allocated and `emit_permuted_copy` fills them, then the receiver's handle is released
through `build_dlpack_release` (the deleter, not a private free). That copy is ONE flat loop over
the result's linear index rather than a nest of `rank` loops: both stride vectors are compile-time
constants, so a result index decomposes into coordinates with constant `udiv`/`urem` and
recomposes into a source offset with constant `mul`, and the IR is the same size at every rank.

`expressions/tensor_reduce.rs` owns `HirExprKind::TensorReduce`: `.sum()`, `.mean()`,
`.max()` and `.min()`. Reducing along axis `k` splits the flat run into three constant
factors — `outer` elements above the axis, `mid` along it, `inner` below — so result slot
`r` gathers `(r / inner) * mid * inner + j * inner + (r % inner)` for `j` in `0..mid`, and a
whole-tensor reduction is that same walk with `outer` and `inner` both 1. Two counted loops,
never a nest of `rank` of them, for the reason the permuted copy gives. The accumulator
starts at the run's FIRST element rather than at an identity, which is what gives `.max()` and
`.min()` a starting value without a per-dtype sentinel (the checker has already refused an
empty run). A sum reuses `codegen_int_arith`, so an overflowing reduction panics exactly where
an overflowing `+` would; `.mean()` divides the float accumulator by the run length. Nothing
is moved here, and a receiver a binding owns is left to that binding's own drop. What IS
released, once the fold has read everything, is a receiver that no binding owns:
`release_receiver_temporary` frees the buffer of a receiver built for the call — an operator
result, a call's return, a tensor constructor, another reduction — which otherwise has nothing
to release it. The predicate is a whitelist of shapes that provably allocate their own buffer,
not "anything that is not a place": an `if`, a `match` or a block yields whatever its branch
yields, which may be a buffer a binding still owns.

`expressions/tensor_sort.rs` owns `HirExprKind::TensorSort`: `.sort()`, `.argsort()` and
`.topk()`. It walks the same `outer`/`mid`/`inner` split the reduction does, and builds, per
run, a permutation of `0..mid` in one stack scratch array; the three methods then differ only
in what the writer at the end reads out of it — the elements in that order, the permutation
truncated to `i32`, or the leading `k` of both into a two-tensor tuple. The permutation is
seeded with the identity and carried by a stable insertion sort, so equal elements never
cross and an argsort of a tensor with ties is reproducible. The float comparator spells out
only the two `NaN` tests: an ordered `<` / `>` is already false on a `NaN` operand, so
"`a` is real AND (`a` beats `b` OR `b` is `NaN`)" is exactly the specification's rule that
`NaN` sorts to the end whatever the direction. Nothing is moved here and every result is a
fresh handle, so a receiver a binding owns stays that binding's; a receiver no binding owns is
released once the selection has copied what it needs, through the same
`release_receiver_temporary` the reduction uses.

`expressions/tensor_apply.rs` owns `HirExprKind::TensorApply`, the functional traversals
`.map` / `.zip` / `.reduce`. One counted loop over the flat buffer whatever the receiver's
rank: the traversals are elementwise, so the element count is a single compile-time product
and there is no axis arithmetic at all — the simplest of the tensor walks. The function value
is lowered ONCE, before the loop, and `split_function_value` keeps its `{ fn_ptr, env_ptr }`
halves so `call_function_value` can dispatch per element without rebuilding them; that split
is what `codegen_indirect_call` in `closures.rs` now also calls, so an ordinary `f(x)` and a
traversal's per-element call go through one path. `t.map(make_rule())` must not rebuild its
rule per element, the same rule an adapter chain in a `for` head follows. `.map` and `.zip`
write each answer into a freshly allocated buffer at the index they read from, and `.reduce`
carries its answer in an `alloca` seeded from `init` and loads it out at the end, which is
also why a fold produces a scalar rather than a handle. Nothing is moved here; the receiver
and a `.zip`'s operand are each freed through the same `release_receiver_temporary` the
reduction uses, so a chained `t.map(..).map(..)` releases its intermediate.

`expressions/elementwise_math.rs` owns `HirExprKind::Math`. A scalar is one application; a
tensor is the `.map` loop with the function inlined, reusing `tensor_apply.rs`'s walk helpers
(`walk_tensor`, `element_count`, `load_walked`, `buffer_slot`), and releases a temporary
receiver the same way. Each function is its LLVM intrinsic (`llvm.exp`, `llvm.log`,
`llvm.sqrt`, `llvm.tanh`, `llvm.fabs`, `llvm.pow`), resolved to libm by the `-lm` the driver
links; `Sign` is two ordered compares and two selects, so NaN passes through. A half-precision
element is widened to `f32` around the function and narrowed once after it, because the
intrinsics' `half` / `bfloat` overloads are not ones every target lowers. `Math` is one of
`builds_its_own_buffer`'s shapes.

`expressions/tensor_einsum.rs` owns `HirExprKind::TensorEinsum`, the Einstein-notation
contraction. The notation is gone by this point: HIR supplies one extent per subscript letter
and, per operand, which letter each of its axes carries, which reduces the whole construct to
flat index arithmetic over row-major buffers with every factor a compile-time constant. Two
counted loops for the reason the reduction gives — the outer walks the result's elements, the
inner the contracted letters' product — so the IR is the same size whatever the ranks are. A
letter's index is recovered from a counter by dividing out the letters below it and taking the
remainder (`decode_counter`), and an operand's offset is that index times a per-letter
COEFFICIENT: the row-major strides of every axis the letter sits on, ADDED together
(`coefficients`). Summing them is what makes a letter repeated within one operand walk its
diagonal, which is the whole of `"ii->"`. The accumulator starts at the additive identity
rather than at a first element, unlike the reduction's: the loop sums products, so there is no
element to seed it with and an empty contraction is genuinely zero. Both the product and the
accumulation reuse `codegen_int_arith`, so an overflowing contraction panics exactly where an
overflowing `*` or `+` would, which is also why the inner counter is reloaded before its
increment — a checked operation may have split the body around its guard. Nothing is moved
here; each operand that no binding owns is freed through the same
`release_receiver_temporary` the reduction uses, once every read is behind the loops.

`expressions/tensor_index.rs` owns `HirExprKind::TensorIndex`. Every stride is a compile-time
constant (every extent is part of the type), so the index is arithmetic on the flat row-major
run behind `data`: each `Position` axis contributes `position * stride[k]` and each `Range` axis
contributes `start * stride[k]`. Reading an element is that offset, one `getelementptr`, and one
`load`. A slice ALLOCATES a fresh handle through `alloc_dlpack_tensor` and copies into it — a
tensor owns its buffer and releases it through its own deleter, so a view sharing one would be a
double free, and a copy is also what keeps the DLPack contract's contiguous `strides` and
zero `byte_offset` true of every value. The copy loop walks the RESULT, whose linear index is its own buffer index,
and recovers each source coordinate as `(i / result_stride) % extent`; both divisors are
constants. A run-time position is guarded by `guard_tensor_position` in every build, as an array
index is: `overflow_checks` gates integer overflow only, because wrapping gives an overflow a defined
result and an index past the storage has none. A slice's bounds were settled at compile time, so
nothing about them is checked here.

## DLPack Representation
A tensor value *is* the exchange structure DLPack 1.1 defines, so the pointer a Neuro
program passes around is the pointer a foreign consumer takes: there is no wrap step at an FFI
boundary. `type_mapping.rs` holds the layout (`dlpack_managed_tensor_type`), the dtype table
(`dlpack_dtype`, covering every integer, float, `bf16`, and `bool` element), and the buffer sizing
(`tensor_buffer_bytes`); `codegen/dlpack.rs` holds the emission.

Fields are filled at construction: `version` `{1, 1}`, `manager_ctx` the control block below,
`deleter` the shared
`__neuro_dlpack_deleter`, `flags` 0 (the buffer is writable), `device` `{kDLCPU, 0}`, `ndim` the
rank, `dtype` from the table with `lanes` 1, `byte_offset` 0, and `shape` / `strides` pointing at
private constants named `__neuro_dlpack_shape_<mangle>` / `__neuro_dlpack_strides_<mangle>` and
shared by every value of that tensor type. Strides count **elements, not bytes**. Rank 0 has no
axis, so both pointers are null: DLPack's own spelling for a scalar. The globals are pointer
fields, so dynamic shapes can later supply a per-value vector without changing the layout.

`manager_ctx` carries the compiler's own per-tensor control block
(`dlpack_control_block_type`), which trails the exchange structure inside the SAME `malloc`
(`dlpack_tensor_storage_type`): a struct's first field sits at offset 0, so the allocation's
address is already the `DLManagedTensorVersioned*` a foreign consumer takes, and `deleter`'s one
`free(self)` releases both. Reserving the field therefore costs a store, not an allocation. Its
fields are `data_bytes`, the unpadded element-buffer length, and the two derivative slots
(`DerivativeSlot`): `grad`, and `hessian`, which only a `@grad(order: 2)` `.backward()` fills.
Each is null, or a handle the slot OWNS. The block is reached at its fixed offset in the storage
block (`dlpack_derivative_slot`), never by loading `manager_ctx`, because a handle built by a
foreign producer carries that producer's context in the field. The deleter reads both slots and
releases each filled one through the derivative's own deleter before the buffer
(`release_derivatives`), which is
sound because `__neuro_dlpack_deleter` only ever runs on a handle whose `deleter` field names it,
and only this compiler writes that name; `a_tensor_is_released_through_its_own_deleter` pins the
order and that `manager_ctx` is never read.

**The derivative slots' operations** (`codegen/expressions/tensor_grad.rs`), dispatched ahead of
the builtin table because most are unit: `.grad()` / `.hessian()` check their slot and panic on
an empty one, then yield the slot's own ADDRESS as the `&Tensor` (a `&Tensor` is the address of a
cell holding the handle, and the slot is one, so no copy); `.zero_grad()` is
`release_derivatives`, emptying both; `__set_grad(g)`, the private method a `.backward()` lowers
to, releases BOTH slots and stores `g` (marked moved), so a first-order `.backward()` never leaves
a Hessian of an earlier point behind; `__set_hessian(h)`, which the lowering emits after it under
`order: 2`, releases and fills the Hessian slot alone. `.backward()` itself never reaches the
backend. An order-preserving shape cast keeps the handle, so it releases both slots: its result
starts without the consumed receiver's derivatives, exactly as the permuting path's fresh handle
does. The slots assume a handle this compiler built, which every handle is while nothing imports
a foreign tensor.

Two allocations for the tensor, not one: the structure and its control block come from `malloc`,
the elements from the over-aligned allocator at 64 bytes, because DLPack requires a 64-byte-aligned `data` and `malloc`
guarantees only `max_align_t`. Fusing them would need the structure's size rounded up to 64 as an
IR constant expression, and LLVM 20 has been withdrawing constant-expression arithmetic; the
element buffer's size is computable in Rust (`tensor_buffer_bytes`), the structure's is not. The
allocation size is rounded up to the alignment, but only the unpadded element run is ever copied
(`dlpack_copy_length`).

**The over-aligned allocator is spelled per-platform** (`ALIGNED_ALLOC_FN` / `ALIGNED_FREE_FN` in
`codegen/context.rs`, the single place both names are chosen). C11's `aligned_alloc(alignment,
size)`, released by ordinary `free`, is not in Microsoft's UCRT: their `free` cannot release an
over-aligned block, so MSVC offers `_aligned_malloc(size, alignment)`, taking the same pair the
other way round, paired with `_aligned_free`. `codegen/dlpack.rs` is the only call site and orders
the arguments; the deleter frees the buffer through `ALIGNED_FREE_FN` and the structure through
plain `free`, since on Windows the two blocks come from different allocators and crossing them
corrupts the heap rather than leaking. Codegen targets the host
(`TargetMachine::get_default_triple`), so the host `cfg` is the target's.

Release goes through the handle's own `deleter` field (`build_dlpack_release`), never through a
direct free, so the release a scope exit performs is provably the one a foreign owner performs.
`__neuro_dlpack_deleter` releases the gradient and the Hessian, then `data`, then the structure, in that order: reading `data` out
of a block it had already freed would be a use-after-free.

`codegen_reference` returns the borrowed place's storage pointer: mutability is compile-time
only. `codegen_deref` loads the referent; `codegen_deref_assignment` stores at the pointer, and
is what `HirPlace::Deref` lowers to.
**Auto-deref is value-driven**: a borrowed receiver lowers to a `PointerValue`, so
`string_receiver_struct`, `StructClone`, `codegen_method_call`, and `get_struct_ptr_and_type` load
through the pointer when they see one; an owned receiver is already a value. There is no context
state for ref-ness: it is read from `variable_types` (a `&Struct` alloca holds a `ptr`) and from
the lowered value kind.

Unit-returning calls are valid in statement position: `codegen_call` / `codegen_method_call`
return an `Option` (`None` = void), and the shared `codegen_call_dispatch` is wrapped with a
void-error in value position.

- **`char`** lowers to LLVM `i32`. Casts use `is_int_like` / `is_unsigned_like` so char↔integer
  (and char→char) reuse the int-to-int path (char zero-extends, code points being non-negative),
  and comparisons hit the signed-int branch, correct since valid code points are < 2²¹.
- **Float-to-integer** casts lower to `llvm.fptosi.sat` / `llvm.fptoui.sat`, not the plain
  `fptosi` / `fptoui`. The plain instructions are defined only when the truncated value fits the
  target and yield `poison` otherwise, which made an out-of-range cast's result depend on the
  optimization level rather than on the source. The saturating form is total: in-range values
  still truncate toward zero, out-of-range values clamp to the target's bound, and NaN maps to
  zero. `FoldedConst::cast_to` computes the same function in Rust, so a folded cast and a
  run-time one agree.
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
arrives as a `HirStmt::Expr` holding an if-expression: **hir-lowering owns that promotion**, so
the backend needs no rule of its own and `codegen_body` handles only `HirStmt::Expr` tails.

`codegen_body` applies the same test to a function body's tail: a `HirStmt::Expr` counts as the
implicit return only when its own type is not `HirType::Void`. A trailing `if` with a `return` in
every arm is typed `void`, having no value to give; reading it as the implicit return returned the
`i32` placeholder that stands in for a void position, so a function returning anything else failed
the verifier with `ret i32 0`. Such a body takes the statement path, whose dead merge block is
already closed with `unreachable`.

`codegen_block_expr` reads a trailing `HirStmt::Expr` as the block's value only when its type is
not `HirType::Void`: a block ending in a call to a unit function has no value, and asking for one
failed with "function call returned void when value expected". A `void` tail is emitted through
`codegen_stmt` like any other non-expression tail, which is also the shape the named-argument
hoisting rewrite produces for a unit call.

An `unsafe` block lowers through `codegen_block_expr` exactly like a bare block, emitting
identical IR.

## Match Lowering
`codegen_match` (`expressions/matches.rs`) evaluates the scrutinee **once** into an alloca, then
builds a per-arm test-block chain: each arm ORs its `HirMatchTest`s (tag compare / scalar `==` /
range `lo<=x<=hi`, signed vs unsigned by scrutinee type) and branches to the arm body or the next
test. An arm body materializes its bindings (the whole scrutinee, or an enum payload slot decoded
by `decode_enum_payload_field`, the inverse of the construction encoding through the same cell),
evaluates the guard (branching
to the next arm on failure), then evaluates the body into a shared result slot. Bindings are saved
and restored in the name maps per arm, and the fall-through block is `unreachable`, because
exhaustiveness is a frontend guarantee.

Ownership of an enum payload crosses at the arm. The match disowns every held drop flag of the
scrutinee as soon as ANY arm binds — which arm ran is a runtime fact and the flags are static —
so the arm's binding has to be what releases what it took. Each arm body therefore runs in a drop
scope of its own, and `bind_arm` registers an owning payload binding in it (`owns_payload`); an
arm that MOVES the payload out disarms the flag first, through the ordinary
`mark_moved_for_drop` on the arm body, and the scope then releases nothing. The disowning hands
back each flag with the value it held, and an arm that binds NOTHING (`B(_)`, `_`) stores them
back at its entry: it took nothing, so the scrutinee still owns the whole value and its scope
releases it. What still leaks is the part of a variant an arm that binds did not bind
(`A(x, _)` over two owning fields): that is the safe direction, since which part left is known
only per pattern.

`codegen_single_test`, `SavedBinding`, `bind_arm`, and `restore_bindings` are `pub(crate)` so
`val_else.rs` can share them. `val_else` passes `owns_payload: false`: its binding belongs to the
enclosing block, which registers it, rather than to the pattern.

## val-else Lowering
`codegen/val_else.rs`. The scrutinee is stored once into an alloca, `codegen_single_test` picks
the branch, and the else block runs in its own drop scope with its binding saved and restored. The
success block's bindings are materialized by `bind_arm` and deliberately **not** restored: they
belong to the enclosing block, which is the whole difference from a match arm. For the same reason
their ownership is `ArmOwnership::EnclosingString`: a `string` payload a collection's fallible
reader copied out (`produces_owned_option_payload`) is registered in the ENCLOSING drop scope,
where the binding lives, and every other payload is left unregistered, as the else binding's is.
The else block is
terminated with `unreachable` if it still falls through; the frontend has already rejected that
case, so this only keeps the emitted function verifier-clean.

## Logical Operator Lowering
`&&` / `||` short-circuit. `codegen_binary` intercepts them before eager operand evaluation and
delegates to `codegen_short_circuit`: evaluate the LHS in the current block, conditionally branch
to a `logic.rhs` block (taken only on the deciding edge: true for `&&`, false for `||`), and merge
the RHS value with the short-circuit constant (`false`/`true`) via a phi in `logic.merge`. Both phi
predecessors are captured *after* their side is emitted (`get_insert_block`), so an RHS that
appends blocks (a nested if-expression) works, and an RHS that terminates its block is dropped from
the phi. Operands are guaranteed `i1` by semantics; the eager `And | Or` arm is an unreachable ICE
guard.

`codegen_binary` also checks that both coerced operands are integer or float values **before** the
operator match (every arm calls `into_int_value` / `into_float_value`, which *panic* on a struct,
array, or pointer rather than returning an error), and answers one that is not with
`CodegenError::InvalidOperandType`. Semantic analysis and HIR lowering both reject such an operand,
so this is the backstop rather than the diagnostic.

`BinaryOp::NullCoalesce` reaching `codegen_binary` or `fold_const` is an `InternalError`: `??` is
desugared to a `match` by hir-lowering, so a binary node still carrying it means the HIR did not
come from that pass. `??` in a const expression is rejected outright.

`fold_const` computes in `i128` and range-checks every arithmetic result against the node's own
resolved type, answering `CodegenError::ConstOverflow` when it does not fit. `+`, `-`, `*`, `/`,
`%` and unary `-` all go through that check, and `MIN / -1` and `MIN % -1` are recognised against
the operand type's minimum because neither is caught by the range alone. A `const` never reaches
the run-time tier that would choose between panicking and wrapping, so an initializer that
overflows has no defined value under both rules at once and is rejected instead of folded.
Operators with no run-time overflow rule keep their run-time meaning: `&`, `|`, `^`, `~` and `<<`
truncate to the node's type, and an `as` cast still narrows explicitly.

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

The three counted loops: `codegen_for_range` (`statements.rs`), `codegen_for_each`
(`expressions/arrays.rs`), and `codegen_vec_for_each` (`collections/vectors.rs`). Each takes an
`index: Option<&str>`, the `u64` position binding of `for (i, x) in xs.enumerate()`. `loop_index.rs`
owns its scope bookkeeping: `bind_loop_index` opens a slot and shadows the name across
`variables` / `variable_types` / `type_env`, `store_loop_index` refreshes it at the top of the
body, and `unbind_loop_index` restores the outer meaning at the exit block. The array and `Vec`
loops publish their own induction variable; the range loop steps a separate zero-based counter in
its step block, because its induction variable carries the range's bounds and element type rather
than a position. The slot is never the induction variable itself even where the values agree:
`mem2reg` erases the copy, and aliasing a slot the loop steps would make a future edit silently
wrong.

`codegen_for_range` takes a `reversed` flag for `.rev()` and keeps its ASCENDING induction
variable, mirroring it onto the user's binding at the top of the body (`start + last - k`).
Counting the binding down instead would have to step below `start` to terminate, and on an
unsigned range starting at zero that step wraps to the top of the type: the loop would never
exit. Both the mirror's sum and its subtraction wrap, which is correct: every value yielded
lies inside the element type, so the arithmetic is exact modulo its width. A reversed slice
axis is the same reflection one level down: `copy_tensor_slice` maps result coordinate `c` to
source `extent - 1 - c`, about the range's own extent, since the base offset already carries
its start.

## Integer Overflow ABI
Integer `+` / `-` / `*` and unary `-` honor the overflow rule, keyed off
`OptimizationLevelSetting`:
- `-O0` → `overflow_checks = true`. `codegen_int_arith` emits
  `llvm.{s,u}{add,sub,mul}.with.overflow`, extracts `{result, overflow_bit}`, and hands the
  negated overflow bit to `codegen_guard_or_panic`, so an overflow prints
  `panic: integer overflow at file:line:col` and aborts through the same outlined thunk every
  other guard uses. `codegen_binary` therefore takes the expression's source offset.
- `-O1..-O3` → `overflow_checks = false`. `emit_wrapping_int_arith` emits plain
  `build_int_add/sub/mul` (two's-complement wrap).

Signedness picks the `s`/`u` variant via `TypeMapper::is_unsigned_int`. Bitwise ops
(`build_and`/`or`/`xor`/`left_shift`, `build_not` for `BitNot`) and floats are unaffected.

Unary `-` on an integer is `0 - x` and overflows exactly where that subtraction does (at a
signed type's `MIN`, and at every nonzero value of an unsigned type), so `codegen_unary` builds
a zero and hands the pair to the same `codegen_int_arith` (`pub(super)` for that caller),
taking the expression's source offset like `codegen_binary`. Emitted separately as
`build_int_neg` it wrapped silently on the debug tier while `0 - x` panicked, so the two
spellings of one quantity disagreed.

A negation whose operand is an integer **literal** short-circuits that path and is materialized
through `codegen_literal` with the magnitude already negated. The checker range-checks such a
negation against the value it denotes, so it is in range for its type and there is nothing to
guard; computing it would be wrong as well as redundant, because the most negative value of a
signed type is written as a magnitude one past that type's maximum and narrows to `MIN`'s own
bit pattern, on which `0 - MIN` overflows.

## Integer Division ABI
Integer `/` and `%` go through `codegen_int_div_rem`, which guards the two operand pairs LLVM's
`sdiv` / `udiv` / `srem` / `urem` leave undefined. Left unguarded these are not quietly wrong:
`-O0` dies of `SIGFPE` with nothing printed, and `-O1` and above fold the surrounding code around
a poison value.
- **Zero divisor**: guarded in *every* build, panicking `division by zero` / `remainder by zero`.
  It is not an overflow and has no two's-complement answer to wrap to, so there is no defined
  release behaviour the check could be dropped in favour of. The guard folds away wherever the
  divisor is a constant or its range is known.
- **`MIN / -1`** (signed only), an integer overflow, so it follows the rule above: with
  `overflow_checks` it panics `integer overflow`; without, the divisor is replaced by `1` through a
  `select`, since `MIN / 1` is `MIN` and `MIN % 1` is `0` (the two's-complement wraps), and `-1`
  never reaches the instruction. `MIN` is `1 << (width - 1)`, built from the operand's own width
  because `const_int` truncates rather than sign-extends.

Unsigned operands skip the second guard: no unsigned quotient is unrepresentable.

## Panic Runtime ABI
Panic-family builtins `panic(msg: string)`, `assert(cond: bool)`, `unreachable()` lower in
`panic.rs`. Contract: **abort, no unwinding**, no landing pads, so the happy path is zero-cost and
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

That sequence is **not** emitted inline. See Error-Path Outlining. The `file:line:col` suffix
comes from the `Call` span start, resolved against the module text (empty when no source is
supplied).
`write` + `abort` are POSIX/libc (Linux, macOS; MSVC CRT on Windows). `abort` runs no exit hook, so
`build_thunk_body` records each panic thunk's FIRST instruction for the standard-output drain,
ahead of the diagnostic's first `write`. Recording the `abort` call instead (until BUG-067) put the
drain after the diagnostic, so on a shared pipe the panic printed before the output leading up to
it. See Exit-path draining.

Because `panic` / `unreachable` terminate the block with `unreachable`, following statements are
dead code: `codegen_stmt` early-returns when the block is already terminated, and `codegen_return`
and `codegen_body`'s tail path skip the `ret` when evaluating the returned expression terminated
the block (`func f() -> i32 { panic("x") }`). This keeps LLVM from seeing instructions after a
terminator.

## Error-Path Outlining
`outlining.rs` emits every panic-family failure path into a module-private cold function and leaves
one call at the failure site, so the diagnostic machinery never sits inline in the function that
can fail. It covers `panic` / `assert` / `unreachable` and every `codegen_guard_or_panic` caller:
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
- `mark_cold_branch(branch)` attaches `!prof` `branch_weights` (`2000 : 1`) to every guard
  branch. Every guard in the language has one shape (continuation on true, failure on false), so
  the cold edge is always the false one and the helper takes no side argument.

## Standard-Output ABI
`print(text: string)` / `println(text: string)` lower in `io.rs`. The `Call`→`Identifier` arm
intercepts them via `CodegenContext::is_io_builtin` after the panic family and under the same
user-function-shadows rule. Both return unit, so the dispatch yields `Ok(None)`: a statement
discards it, and value position reports the ordinary void-where-a-value-was-expected error.

The argument is already the `{ ptr, i64 }` fat pointer (interpolation renders every hole before the
call is reached), so lowering is an `extractvalue` pair and a call. `println` follows the text with
a second call over `neuro.print.newline`, a one-byte `.rodata` global emitted once per module. That
byte is `\n`; on Windows the CRT's text-mode fd 1 turns it into `\r\n`, which is why the
`print_builtins` and `examples` tests compare stdout with line endings normalized.

Output is **buffered**. `PRINT_BUFFER_BYTES` (4096) bytes of `.bss` (`neuro.print.buffer`) plus an
`i64` fill counter (`neuro.print.used`) and an `i8` mode cache (`neuro.print.mode`), all
`Linkage::Private`. Four module-private helpers, each emitted on first use, each saving and
restoring the builder position because they are built lazily mid-function:

- `neuro.print.emit(ptr, i64)`: the only thing a builtin calls with bytes. Copies into the buffer
  when they fit; drains first when they do not; and when they are larger than the buffer could ever
  hold, hands them to `write_all` directly after that drain, so one enormous string stays one
  syscall instead of being chopped into pages. `emit` has consumed the bytes by the time it
  returns on every path, which is what lets `codegen_io_builtin` free an owned argument right
  after the call.
- `neuro.print.flush()`: drains and zeroes the counter. A no-op when nothing is buffered, so it
  is cheap enough to call unconditionally from an exit path.
- `neuro.print.line_end()`: emitted after `println`'s newline. Resolves `isatty(1)` once into
  `neuro.print.mode` and drains only when fd 1 is a terminal, so interactive output stays
  line-by-line while a pipe or file gets the full buffer. The compiler knows where the line
  boundary is, so the runtime never scans bytes for `\n`. `print` writes no terminator and so
  calls this not at all: the same rule C's line-buffered stdio follows. `isatty` is declared as
  `_isatty` on Windows, chosen with `cfg!(windows)` since `neurc` compiles for the host.
- `neuro.print.write_all(ptr, i64)`: the drain primitive, holding the short-write retry loop:
  `write` may consume less than it was offered (a pipe with a full buffer does exactly that), and
  a bare call per site would silently truncate the language's primary result channel. The loop
  stops on a non-positive return so a closed or failing descriptor cannot spin.

`split_printable` reports a non-aggregate operand as an internal error rather than asking it for a
struct variant it has not got.

### Exit-path draining
Buffering is only sound if the buffer always reaches fd 1 before the process stops, and the
language stops in exactly two ways: `main` returns, or the panic runtime calls `abort`. `abort`
runs no exit hook, and the Windows fallback linkers are invoked with `/ENTRY:main`, so `atexit`
and `llvm.global_dtors` are not available to lean on either.

`finalize_stdout_buffer` (io.rs) inserts the drain instead, called from `build_module` after every
body is generated and before soft-float linking and `verify`. It returns immediately unless
`neuro.print.buffer` exists, so a program that never prints reserves no buffer, declares no
`isatty`, and keeps its exit paths untouched. Otherwise it inserts `call void @neuro.print.flush()`
before every recorded process-exit instruction and before every `ret` in `@main`: `main` is
emitted under its own name with no wrapper, so that is the C entry point itself.

The exit instructions are recorded as they are emitted, into `CodegenContext::process_exit_points`,
by `record_process_exit` (context.rs): `emit_abort_unreachable` (panic.rs) calls it right after
building its call, and it takes the block's last instruction
because inkwell's `CallSiteValue` does not convert to an `InstructionValue`. Draining in front of
the panic path is not only about not losing bytes: it is what keeps the stderr diagnostic behind
the stdout output that led up to it.

## String Interpolation ABI
`codegen/expressions/interp.rs` renders each part to a `{ ptr, len }` fat pointer and concatenates
them into one fresh `malloc`'d buffer. Each part carries a `PieceOwner` saying whether the
rendering allocated its bytes or borrowed them, and the scratch buffers are released once the
concatenation has copied them out. `__neuro_pad`, `__neuro_point`, and `__neuro_exp` return their
input untouched when the text already has the requested shape, so a result that may alias its
source is released under a pointer comparison (`OwnedUnlessSameAs`) rather than outright. The
rendering helpers live in `format_helpers.rs` (integer and float conversion, hand-written binary
digits), `format_layout.rs` (sign-aware field padding, debug quoting, UTF-8 encoding of a `char`),
and `format_float.rs` (restoring the point `%g` drops, normalizing C's `e+00` exponent). Each is
emitted once per module with internal linkage rather than inlined at every hole. `snprintf` is the
one external declaration this adds.

Integer holes do **not** go through `snprintf`. `get_or_define_fmt_int(radix, upper)` emits one
`__neuro_fmt_int_<radix>` per radix actually used: a backwards digit loop over a 24-byte scratch
buffer (22 octal digits is the widest any radix produces, plus a sign), then one `malloc` and one
`memcpy` of exactly the bytes written. The radix is a definition-time constant, so instruction
selection turns the `udiv`/`urem` into a multiply-and-shift. The helper takes a magnitude and an
ASCII sign byte rather than a printf conversion: the checker rejects `+` on unsigned values and on
every radix conversion, so only signed decimal can carry a sign, and `render_integer` computes the
magnitude with a wrapping negation (`0 - i64::MIN` is `i64::MIN`'s own bit pattern, which read as
unsigned is the magnitude wanted).

Float holes still call the C library, but once. `build_snprintf_alloc` renders into a
`SCRATCH_TEXT_BYTES` stack buffer and uses `snprintf`'s return value as the length, replacing the
`(NULL, 0)` probe call that used to precede every render. The buffer is sized for the widest
conversion the format mini-language admits (`%.Nf` on a full-magnitude `f64`, with `N` capped by
`MAX_FORMAT_PRECISION`), and a render that does not fit falls back to allocating what `snprintf`
asked for and rendering again, so correctness does not rest on that size being right. It takes the
helper's own `FunctionValue`, because it runs inside a helper body the builder was moved into and
`current_function` still names the caller.

A `@derive(Debug)` struct hole renders through `render_struct_debug`, which frames the fields from
`struct_defs` as `Name { field: value, ... }` and renders each one under the same `FormatKind::
Debug`, which is what quotes a nested `string` or `char` and recurses into a nested struct. The
name comes from `struct_written_names` (`HirStruct::written_name`, set by `set_struct_written_names`
beside `set_struct_defs`), so a monomorphized instance prints `Wrapper`, not `Wrapper_g_i32`. A
field-less struct renders as its bare name. The pieces reuse the same `PieceOwner` bookkeeping the
enclosing literal uses, so a nested rendering's scratch buffers are freed once copied out.

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

Static dispatch needs nothing here: `impl Trait` is monomorphized away before the HIR arrives.

## Drop ABI (deterministic destruction)
`drops.rs` inserts a `{struct}__drop(&mut self)` call at each lexical scope exit for an owned
binding of a `Drop` type. `drop_types: HashSet<String>` (filled by `compile` from `impl Drop for T`
blocks) gates everything: when it is empty the scope stack stays empty and zero IR is emitted, so
non-Drop programs are unaffected. `drop_scopes: Vec<Vec<DropEntry>>` is a stack of lexical scopes;
each `DropEntry` records the binding name, storage `alloca`, an `i1` drop flag, and a `DropTarget`
(`UserDrop(struct)` | `Collection` | `HeapString` | `TensorBuffer` | `PoolRegistered`).
`PoolRegistered` is the one target `emit_drops_through` skips: its release belongs to the pool
sweep below, and the entry exists only so the flag tracks moves like any other.

A holder owns what it holds, so `DropEntry` also carries `held: Vec<HeldDrop>`, one entry per
owning position reachable from the binding by a statically known field or element path.
`register_owned_binding` plans them at the binding site: `holds_owner` decides whether a type
owns anything transitively, `held_positions` enumerates a struct's fields, a tuple's elements or
an array's elements, and each position gets its own `i1` flag and a GEP taken once at the
declaration, which dominates every later drop site. `Aggregate` is the target of a holder that
releases nothing itself; `EnumPayload(enum)` is the target of an enum, whose live positions
depend on the tag, so it switches on the tag and destroys the active variant's owning payload
slots through `emit_value_destructor` rather than through a static path.

Per-position flags are what make a partial move sound. `mark_moved_for_drop` resolves the place
an expression names (`moved_place`) into a binding plus a path, and clears that path's flag and
every flag beneath it, leaving the siblings armed; a bare binding, or a place through an index
the compiler cannot evaluate, clears everything. The index case clears nothing when the read moves
nothing out (`read_moves_nothing`): a `Copy` value, or a collection's `string` element, whose read
copies. Disarming the holder there left every owner in it unreleased. That is also what makes destructuring work, since
the parser desugars it to a temporary plus one projection per leaf. A store into a field or an
array element releases the displaced position and re-arms it (`displace_held_position`, at any
depth; `displace_array_element` at a run-time index, which compares the written address against
each tracked element). Until BUG-075 only a field of a named binding did, so a nested field or an
element lost its old value without a destructor. A place reached through a borrow has no flags
here and is not released (BUG-077). A reassignment re-arms the whole plan
(`rearm_held_drop_flags`), which is unconditional because a held position exists only where its
own type proves ownership.

A fresh value that holds a user `Drop` type and that no binding ever owns is destroyed where it
is last read (`drop_unbound_temporary`, BUG-047): a call, struct literal or enum construction whose
statement value is discarded, one a field is read from (unless the field itself owns something and
is moved out), and one passed as a `&self` receiver. It is stored to a slot, registered in a
throwaway scope, and that scope's drops are emitted at once. Inside a `pool` it does nothing.

Every lookup of a binding's drop entry goes through `live_drop_entry`, which matches the entry's
storage against the alloca the name resolves to now. A binding that owns nothing registers no
entry, so a lookup by name alone fell through a `&mut` shadowing an owner to the owner itself
(BUG-076): a field store through the borrow released the outer value, which its scope exit then
released again.

Every aggregate literal disowns the places written into it: a struct literal per field, and a
tuple or array literal per element (`codegen_tuple_literal`, `codegen_array_literal`). The tuple
and array halves were missing until BUG-052, so `(weights, counts)` left both bindings armed and
the value was released by its old binding and again by the holder. An enum construction disowns
a payload whose type `holds_owner`, the payloads its drop releases under the tag switch; that half
was missing until BUG-060, so `Some(xs)` released a `Vec` twice. A `string` payload is not
released by the enum and keeps its source's ownership.

Two conservative edges keep it sound rather than complete. A `match` whose arms bind disowns the
scrutinee's entire plan (`mark_held_moved_for_drop`), because which payload left depends on a
runtime tag, so a part a binding arm left unbound leaks instead of being released twice (an arm
that binds nothing restores the plan). And
`collection_place_ptr` still copies a collection read out of something that is NOT a place (a
`m.keys()` result, say) into a temporary; that copy aliases the holder's buffer, so
`reads_a_held_place` keeps it from being registered as an owner in its own right.

`codegen_function` / `codegen_method` open the body scope and register by-value `Drop`,
collection, or tensor parameters for destruction at function exit; `codegen_var_decl` registers a local and
allocates its flag (initialised `true`). Branch, loop, and block bodies (`codegen_if`,
`codegen_while`/`loop`/`for_range`, `codegen_arm_into_alloca`, `codegen_block_expr`) push and pop
their own scope and emit that scope's drops in reverse declaration order at normal fall-through.
`return` runs every open scope (`emit_drops_through(0)`); `break`/`continue` run down to the loop
body scope recorded in `LoopTargets.drop_scope_depth`. A panic aborts without running drops (no
landing pads).

Each drop is flag-guarded (`if flag { drop(); flag = false }`), and `mark_moved_for_drop` clears a
binding's flag at every move site (bind / assign / return / break value / call arg / struct-field
store), so a moved value is dropped exactly once.

Reassignment is the second drop site. `codegen_assignment` evaluates the new value, calls
`drop_reassigned_value` to run the same flag-guarded release against the binding's storage, stores,
and then calls `rearm_drop_flag` for the incoming value. The order is load-bearing: a reassignment
may read the value it displaces (`s = s + "!"`), so releasing before the new value is built would
hand the producer freed memory. Two bindings are exempt. A pool-registered one keeps the arena's
LIFO sweep as its only release, because a per-assignment free would return a pointer the arena
still holds. A self-assignment (`p = p`) skips both the release and the move-marking, since the
storage keeps the value it already had.

**Known limits**: an anonymous heap `string` reaches no drop site, because it belongs to no
binding; it is released at its consumer instead (see Heap-string ownership above), and one that
escapes into a position able to store it is released by nobody.

### Places: resolving the holder, addressing per form
`codegen_place_store` has one arm per `HirPlace` form rather than one address computation,
because the indexable types do not share one: a `Vec` slot sits behind a header, a slice slot
behind a fat pointer, a tensor element behind a DLPack handle, an array element behind a
bounds-guarded GEP.

What IS shared is resolving the HOLDER, and that is `held_place_ptr` (`codegen/structs.rs`): a
binding, a struct field at any depth, an array element, a tuple element, a dereference, or an
aggregate element of a `Vec` or a borrowed slice (`vec_element_place` / `slice_element_place`,
bounds-checked like the element read), returning `None` for anything that is a temporary rather
than storage. A `string` slot is never a holder: its read copies the bytes out, and handing its
address on would let a consumer release the collection's own copy. It loads through a
binding's slot when that slot holds an address, which covers a borrow of an aggregate and the
`self` of a `&mut self` method; a borrowed slice is excluded because its slot holds the fat
pointer by value, which is why the LLVM representation and not the semantic type alone decides.

**The trap this closed, and why it is worth remembering.** `array_place_ptr` and
`collection_place_ptr` both fall back to materializing a temporary for a receiver they cannot
resolve. That is the correct answer for a READ and a silently lost write: `grid[0][1] = 9`
compiled and changed nothing, and the same fallback under a method receiver was
`docs/BUGS.md` BUG-036 (`registry.open.push(1)` mutated a copy). Both now route through
`held_place_ptr` first, and `codegen_index_assignment` refuses the fallback outright with a
diagnostic naming why. A resolver shared between reads and writes has to be told which it is.
The checker now refuses a place rooted at a temporary itself, with a span, so the backend
refusal is a guard that no checked program reaches.

**Every element and field store evaluates its value before it takes an address.** The value may
grow the `Vec` the element lives in, or reassign the tensor whose buffer the element is in, and
an address taken first then points into memory the value just freed: `v[0] = { v.push(..); 42 }`
wrote into the old buffer and lost the write. This is the assignment rule "the new value is evaluated first",
applied to the address as much as to the displaced value.

## Pool Arena ABI
`arena.rs` carries the allocator behind `pool { }`: two internal globals,
`__neuro_arena_base` (the chunk, `malloc`ed lazily on the first `pool` a run reaches) and
`__neuro_arena_offset` (the first free byte in it). `codegen_pool_expr` calls
`__neuro_arena_mark` to read the offset, emits the body, and calls `__neuro_arena_release` with
that mark, which is the whole bulk free. Nesting needs nothing more: an inner block's mark is a
larger offset. A body that cannot fall through (a panic) gets no release, since the process is
leaving anyway.

**Which allocations the arena captures is decided at compile time, by `pool_depth`.** Every
allocation site asks `alloc_fn` / `aligned_alloc_fn` for its allocator, and those hand back the
arena's bump functions only while codegen is emitting the inside of a pool body. An allocation a
CALLEE makes is emitted while that callee's own body is being generated, with `pool_depth` back
at zero, so it stays on the heap: the arena holds what the block writes, which is what makes the
rule sound without an ownership analysis. It costs the arena's speed on such a path, never its
safety.

**A store whose place outlives the block is emitted at depth zero too**, which is the
language's routing rule: the mark restore would leave that buffer dangling, so it comes from libc instead.
`HirStmt::Assign` goes through `store_outside_pool`, which consults `route_store_off_arena` and,
when it answers yes, swaps `pool_depth` to zero for the whole statement before restoring it. The
whole statement rather than just the right-hand side: an address computation that allocates is
reachable from the same place, and a `PoolAware` value stored this way must take its own `Drop`
rather than the block's sweep, which `pool_registered_type` decides from the same counter.

Which places outlive is read off `pool_locals`, one `HashSet<String>` per open `pool` region,
pushed beside `pool_marks` in `codegen_pool_expr` and filled by `note_pool_local` at each
`codegen_var_decl`. A store is kept on the bump path only when `Self::place_root` (in `drops.rs`,
over `moved_place`) names a binding in the INNERMOST frame. Everything else routes: a `*p = v`
write, whose referent belongs to whoever handed the reference over; a binding of an enclosing
pool, whose own arena outlives this block's release; and any binding the set happens to miss,
which costs the arena's speed on that store and never its safety. `semantic-analysis` rejects
the stores routing cannot save — a value that already holds arena memory when the statement
starts — so the two sides meet at the same line.

**Every release goes through a wrapper**, `__neuro_release` and `__neuro_aligned_release`
(`release_fn` / `aligned_release_fn`), which return without calling libc when the pointer lies
inside the chunk. Arena memory is reclaimed by the mark restore, and handing it to `free` would
corrupt the heap. The wrapper is unconditional rather than pool-dependent because a buffer
allocated inside a pool can be released anywhere, including in a function emitted earlier; only
the pointer says which allocator owns it. In a program with no `pool` nothing ever stores a
non-null base, so the check folds away.

The two libc pairs stay distinct through the wrappers, since `free` cannot release an
over-aligned block on Windows. `realloc` is NOT wrapped and needs no arena path: the only
buffers it grows (a `Vec`'s and a `String` builder's) are produced by `realloc` from a null
pointer, so they never come from the arena at all. A map's table and the `Vec` that `keys()`
returns take libc `malloc` directly for the same pair of reasons: the table belongs to the map,
which may outlive the block that grows it (BUG-045), and the key `Vec` will be grown by `realloc`,
which must never see an arena pointer (BUG-046). The growth path frees the old table through the
wrapper.

### `PoolAware` registration and the LIFO sweep
`pool_aware_types: HashSet<String>` (filled by `compile` from `impl PoolAware for T` blocks) gates
this half exactly as `drop_types` gates the Drop ABI: while it is empty no registry global, no
helper, and no sweep call is emitted, and a pool's exit stays the single store above.

Inside a pool body, `codegen_var_decl` sends a binding of a `PoolAware` struct to
`register_pool_aware` INSTEAD of `register_local_drop`, so the type's `Drop` does not also run:
the sweep replaces it. That emits the type's own `{T}__register_with_pool(self, handle)`
(a `&self` receiver, so by value) and then `__neuro_pool_register`. `PoolHandle` is a stack slot
holding the innermost `pool_marks` entry, the active arena region's identity. An initializer that
merely MOVES an already-registered binding takes `transfer_pool_registration` instead, which pushes
the list entry without re-running the constructor hook.

The list is a single `__neuro_pool_head` global of `{ next, instance, bulk_release, flag }` cells
bump-allocated from the arena itself, pushed at the head. `codegen_pool_expr` reads the head on
entry and hands it to `__neuro_pool_sweep` as a stop marker, so a nested block releases its own
registrations and leaves the enclosing block's alone. The sweep walks the list forward, which IS
reverse registration order, unlinks each cell before calling its `bulk_release(&mut self)` thunk
indirectly, honours the cell's flag so a moved-out value is passed over, and runs before
`__neuro_arena_release` reclaims the memory the instances and the cells live in.

**Known limits**: the chunk is reserved once and never released, an allocation that does not fit
falls back to the heap (correct, not fast), and `Vec` / `String` buffers and map tables stay off
the arena for the reasons above. Registration follows bindings, so a `PoolAware` temporary is never
registered, and the sweep issues one call per instance rather than batching per device.

## Collections ABI
`Vec<T>`, `HashMap<K, V>`, `BTreeMap<K, V>`, and `String` share one by-value header:
`{ ptr buffer, i64 len, i64 cap, i64 used }` (`TypeMapper::collection_header_type`), held in the
owner's alloca, with all elements in a single heap buffer. `len` counts live elements/entries,
`cap` the allocated slots, and `used` the *occupied* slots (live + tombstoned) that the hash map's
load factor is measured against; the other kinds leave `used` zero.

Buffer layouts, per kind:
- **`Vec<T>`**: a plain `[T]` run. Growth doubles `cap` (minimum 8) through one shared byte-sized
  `__neuro_vec_reserve(header, elem_size)` helper, so every `Vec<T>` in a module reuses it.
- **`HashMap<K, V>`**: `{ i8 state, K key, V value }` slots, power-of-two `cap`, so the bucket is
  `hash & (cap - 1)`. Linear probing; `state` is `0` EMPTY / `1` FULL / `2` TOMBSTONE. A lookup
  stops at the first EMPTY and skips tombstones; an insert takes the first non-FULL slot, so a
  tombstoned run is reused. Rehashing at a 3/4 load factor reclaims tombstones, which is what keeps
  a churned table's probe runs bounded.
- **`BTreeMap<K, V>`**: `{ K key, V value }` slots kept sorted by key: binary search to look up,
  `memmove` the tail to insert or erase. That gives the ordered iteration the type promises; a
  multi-way tree would change only the insert/erase constant, not this ABI.
- **`String`**: a byte run; `len` and `cap` are byte counts and `used` stays zero. It carries no
  type arguments, so one instantiation serves every program. `push_str` reserves through
  `__neuro_string_reserve(header, extra)`: capacity becomes `max(cap * 2, len + extra, 8)`, so one
  large append is a single `realloc` rather than a chain of doublings, then `memcpy`s the
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
`free`s field 0 under the same runtime drop flag that user `Drop` types use: a moved-out
collection is not freed twice. An unnamed collection *temporary* (`for k in m.keys()`) is
registered the same way under a synthetic `__`-containing name that no source binding can collide
with; without that, the only route to map iteration would leak. A collection read out of a place
another binding holds (`b.items.len()`) is not such a temporary: the copy aliases the holder's
buffer, which the holder's own drop releases, so `reads_a_held_place` suppresses the registration. This is also what frees a `String`
builder's buffer, since it is registered as an ordinary collection. **A `string` inside a
collection is not freed**, and neither is the heap `string` that `+`, interpolation, or
`String::to_string` produces; both ride with the heap-string work.

A tensor binding is registered the same way, with `DropTarget::TensorBuffer`: the binding's storage
holds the DLPack handle, so scope exit loads it and calls the handle's own `deleter` under the same
flag, which releases the element buffer and the structure together. Unlike
`HeapString` the ownership comes from the type alone: every tensor construction allocates, and
there is no borrowed value of tensor type to confuse it with. **A tensor held in a struct field is
not freed**, exactly as a collection field is not; recursing into a struct's fields is one gap, not
one per element type.

New libc declarations these need: `free`, `realloc`, `memmove`, `memset`, `aligned_alloc`
(alongside the existing `malloc`, `memcpy`, `memcmp`, `write`, `abort`, `snprintf`), each declared
on first use in `context.rs`.

## Soft-Float ABI
On generic x86-64, LLVM lowers `fpext` / `fptrunc` on `half` / `bfloat` (and f16/bf16
comparisons, which widen to f32 first) to runtime calls: `__extendhfsf2`, `__truncsfhf2`,
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
