# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

## BUG-169: a float literal in a sized array field of an inferred generic struct is an `f64`

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (`check_generic_struct_literal` in
  `type_checkers/expressions/struct_exprs.rs`)
- **Severity**: minor. Valid source is refused; nothing miscompiles

**Minimal repro**

```neuro
struct History<CAP> { seen: [f32; CAP] }

func main() -> i32 {
    val h = History { seen: [0.0, 0.5] }
    return 0
}
```

Expected: `h` is a `History<2>`. Observed: `type mismatch: expected [f32; 2], found [f64; 2]`.

**Root cause**: a field whose declared type names a parameter gets no expected type unless the
context names the instance, so the literal falls back to `f64`. `[f32; CAP]` names only a size,
and its element type is already concrete.

**Workaround**: annotate the binding (`val h: History<2> = ...`) or suffix the literal
(`0.0f32`).

**Fix sketch**: give such a field the declared type with its length left to inference, so the
array literal takes its element type from it and still binds `CAP` from its length.

## BUG-168: a `match` on a diverging scrutinee reports a template parameter

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (`check_enum_pattern` in `type_checkers/matches.rs`)
- **Severity**: minor. Only dead code reaches it, and the error names a type the program never
  wrote

**Minimal repro**

```neuro
func main() -> i32 {
    match panic("x") {
        Some(a) => a + 1,
        None => 0,
    }
}
```

Expected: the program compiles, since no arm can run, or one error saying the scrutinee never
produces a value. Observed: `cannot apply binary operator + to types T and i32`.

**Root cause**: the scrutinee's type is the diverging one, so `Some(a)` binds `a` at the generic
enum's own parameter `T` instead of at a concrete type. The recovery that binds such a payload at
the unknown type is limited to programs already refused, so that a clean program never lowers an
unknown binding.

**Workaround**: none needed; the `match` is dead code.

**Fix sketch**: give a diverging scrutinee its own rule: accept the `match` with every arm
unreachable, or refuse it with one error that names the divergence.

## BUG-167: a block statement whose value nothing reads leaks it

- **Status**: open, confirmed
- **Area**: `llvm-backend` (`is_fresh_value` in `codegen/drops/mod.rs`)
- **Severity**: minor. A leak, and only for a block written as a statement with a value at its end

**Minimal repro**

```neuro
func some() -> Option<string> { Option::Some("x".clone() + "yzw") }

func main() -> i32 {
    { some() }
    0
}
```

Expected: the `Option<string>` the block yields is released at the end of its statement.
Observed: AddressSanitizer's leak check reports its string.

**Root cause**: the block is typed by its tail, so it yields the value and marks it moved. The
statement that discards it releases only a value `is_fresh_value` accepts, and that list has no
`Block` or `Unsafe` (`is_fresh_string` already has them, which is why a bare `string` does not leak).

**Workaround**: drop the braces (a bare `some()` statement releases its result), or bind it.

**Fix sketch**: accept `Block` and `Unsafe` in `is_fresh_value`, since their tail is moved out to
the reader, with a leak-checked test beside `a_value_ending_a_for_body_over_an_iterator_is_released`.

## BUG-166: a tensor expression with more than 4095 checks is an internal error

- **Status**: open, confirmed
- **Area**: `mlir-backend` (the `MAX_SITES` cap in `tensor_arithmetic.rs`, `tensor_compound.rs` and
  `kernel/mod.rs`)
- **Severity**: minor. Only a pathological expression reaches it, and only at `-O 0`

**Minimal repro**: at `-O 0`, one statement summing 4200 `i32` tensor terms
(`val r = &a + &a + ...` with `a: Tensor<i32, [2]>`).

Expected: the program compiles. Observed: `internal error: no MLIR body computes the tensor
operation '__tensor_op_0'`.

**Root cause**: a check site is numbered in 12 bits, so a body that would need more than 4095 sites
returns `None`. That used to leave the operation to the inkwell tensor lowering, which 5A deleted, so
nothing computes it now.

**Workaround**: split the expression into `val` bindings, or build at `-O 1` or above.

**Fix sketch**: have `hir-lowering` split an outlined expression whose check count would pass the
cap, or give every site past it one shared key that reports the statement without a column.

## BUG-165: a chain of checked integer tensor operations carries a check per pair of operations

- **Status**: open, confirmed
- **Area**: `mlir-backend` (`linalg-fuse-elementwise-ops` in the CPU and GPU pipelines of
  `bridge.rs`)
- **Severity**: minor. Debug builds only; compile time, memory and run time grow with the square
  of the chain's length

**Minimal repro**: at `-O 0`, `val r = &a + &a + ...` with `a: Tensor<i32, [2]>`. `--emit llvm-ir`
holds 345 `atomicrmw` check stores for 25 terms and 1270 for 50, where about one per term is
expected. 250 terms take about 14 s to compile (0.2 s at `-O 2`), and 500 terms run LLVM out of
memory under a 3 GB cap.

**Root cause**: each checked operation's `linalg.generic` writes its status in an `scf.if`, a side
effect, so fusion copies a producer into its consumer without erasing it. Every later operation
re-fuses the whole chain before it: n operations carry n(n+1)/2 checks.

**Workaround**: split long integer tensor expressions into `val` bindings, or build at `-O 1` or
above.

**Fix sketch**: build a straight element-wise chain as one `linalg.generic` in
`build_expression`, so fusion has nothing to do; or leave `linalg-fuse-elementwise-ops` out of the
pipeline for checked bodies, which costs a buffer per operation but stays linear.

## BUG-164: a function named like a C library function takes its place

- **Status**: open, confirmed
- **Area**: `llvm-backend` (`declare_function`, `declare_method` in `codegen/functions.rs`)
- **Severity**: critical. A float method silently returns the wrong value; other names fail
  LLVM verification or panic the compiler

**Minimal repro**

```neuro
func exp(x: f64) -> f64 { 100.0 }

func main() -> i32 {
    val y: f64 = 1.0
    (y.exp() * 1000.0) as i32
}
```

Expected: exit code 158 (2718 modulo 256). Observed: exit code 160 (100000 modulo 256), because
`y.exp()` ran the program's `exp`. `func pow(x: f64, y: f64) -> f64 { 0.0 }` makes `2.0.pow(3.0)`
return 0. `func free(x: i32) -> i32` or `func write(a: i32, b: i32, c: i32) -> i32` beside a
string concatenation or a `println` fails with `module verification failed: "Call parameter type
does not match function signature!"`. `func malloc(n: u64) -> u64` panics `neurc` in
`format_helpers.rs` (`expected PointerValue variant`). `func abort() -> i32` turns an index panic
into a segmentation fault.

**Root cause**: confirmed in the code. A function, method or closure of the root module is
emitted under its bare name (`@exp`, `@free`; a module's items are `@"geo::make"`). The backend
finds the C functions it calls (`malloc`, `free`, `write`, `abort`) by name with `get_function`
and gets the program's function instead, and the linker binds the math library calls LLVM emits
for `.exp()` and `.pow()` to the program's definitions.

**Workaround**: rename the function.

**Fix sketch**: emit every program function except the root `main` under a symbol no C name can
take, such as a `neuro.fn.` prefix (a Neuro name holds no `.`). Calls already go through the
backend's name-keyed function table, so only the declarations, the `neurc test` entry's lookup
of each test, and the runtime pruning's removal from that table change. The open question is
`--emit obj`: `docs/guides/cli-usage.md` and both Python differential harnesses load root
functions from the object by their bare names, so a library build needs an exported alias per
root function, or the backend needs to know when it builds an executable.

## BUG-163: a `()` pattern does not parse

- **Status**: open, confirmed
- **Area**: `syntax-parsing` (patterns)
- **Severity**: minor. Valid source is refused, and `_` is a workaround

**Minimal repro**

```neuro
func run() -> Result<(), i32> { Ok(()) }

func main() -> i32 {
    match run() {
        Ok(()) => 0,
        Err(code) => code
    }
}
```

Expected: exit code 0. Observed: `unexpected token LeftParen, expected a literal or variant
pattern` at the inner `(`. The unit type is the zero-element tuple and `()` is its value, so the
zero-element tuple pattern should match it as `(a, b)` matches a pair.

**Workaround**: write `Ok(_)`.

**Fix sketch**: accept `()` wherever a tuple pattern is accepted, matching only the unit value,
and let exhaustiveness count it as covering `()`.

## BUG-162: `??` refuses a fallback that returns nothing when the payload is `()`

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (`??` typing, the unit type)
- **Severity**: minor. Valid source is refused with a message naming a type the language does not have

**Minimal repro**

```neuro
func noop() {}

func main() -> i32 {
    val r: Result<(), i32> = Ok(())
    r ?? noop()
    0
}
```

Expected: exit code 0. Observed: `type mismatch: expected (), found void`, also for a callee
declared `-> ()` and for `?? println("...")`. A function with no return type returns `()`, and
`-> ()` says the same, but the checker keeps two unit types: a written `()` is the empty
tuple, a call to a function returning nothing is `void`, and `is_compatible_with` never equates
them. The `??` fallback check is where `Result<(), E>` from `std::fs` meets a call.

**Workaround**: a diverging fallback (`?? panic("...")`), or `if r.is_err() { ... }`.

**Fix sketch**: make the empty tuple and `void` one type in the checker, or compatible in both
directions, then check that lowering gives a `??` over a `()` payload no value to merge.

## BUG-161: a struct field cannot name a struct declared after it

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (struct registration, pass 1)
- **Severity**: major. Valid source is refused, in a program and in a library module alike

**Minimal repro**

```neuro
struct Outer { inner: Pair }

struct Pair { y: i32 }

func main() -> i32 {
    val o = Outer { inner: Pair { y: 3 } }
    o.inner.y
}
```

Expected: exit code 3. Observed: `unknown type name 'Pair'`, then `struct 'Outer' has no field
'inner'`. Pass 1 resolves each struct's field types while it registers the structs, in source
order, so a field naming a later struct finds nothing. Enum payloads and newtype inners resolve in
later passes and do not have the problem.

**Workaround**: declare the field's struct first.

The same ordering is the only thing that refuses a struct holding itself: `struct A { x: i32, a: A }`
and the pair `struct A { b: B }`, `struct B { a: A }` both fail with `unknown type name`, not
with a diagnostic that names the cycle.

**Fix sketch**: register every struct name before resolving any field type, then resolve the
fields in a second loop, keeping the duplicate-name check. Add a recursive-struct check beside
`reject_recursive_enums`, since once the names resolve nothing else refuses an inline cycle, and
the backend's layout would recurse on one.

## BUG-160: a generic struct literal with a `&[T]` field and no annotation does not compile

- **Status**: open, confirmed
- **Area**: `hir-lowering` (struct literal field types for an inferred generic instance)
- **Severity**: major. Valid source stops the compiler at LLVM verification

**Minimal repro**

```neuro
struct G<A> { items: &[A], n: u64 }

func main() -> i32 {
    val a = [7, 8]
    val t = G { items: &a, n: 5 }
    t.items[1] + t.n as i32
}
```

Expected: exit code 13. Observed: `module verification failed: "Invalid InsertValueInst
operands!"`. A field whose type names a parameter is lowered without it, so `&a` stays a
pointer instead of being unsized to the `{ ptr, i64 }` view. A generic field that is not a slice
(`struct G<A> { x: A, n: u64 }`) lowers correctly.

**Workaround**: annotate the binding (`val t: G<i32> = G { ... }`), which compiles and exits 13.

**Fix sketch**: infer the instance's type arguments in lowering as the checker does, matching a
`&[A]` field against the argument's unsized element type, then lower each field against its
substituted type.

## BUG-159: a trailing comma on one line is accepted in some lists and refused in others

- **Status**: open, confirmed
- **Area**: `syntax-parsing` (the struct, enum, tuple, pattern and import list loops)
- **Severity**: minor. Nothing miscompiles; a form the language forbids parses

**Minimal repro**

```neuro
struct C { a: i32, b: i32, }

func main() -> i32 {
    val c = C { a: 1, b: 2, }
    val (x, y,) = (c.a, c.b,)
    x + y
}
```

Expected: each comma that sits before a closing delimiter on the same line refused, as
`[1, 2,]` and `f(1, 2,)` are. A trailing comma belongs to a list that spans lines. Observed: exit
code 3. Struct fields, struct literals, enum variants, tuple literals, tuple and struct patterns,
`match` arms and import lists take the comma on one line. Parameter lists, argument lists and array
literals refuse it there.

**Workaround**: none needed. Leave the comma out of a one-line list.

**Fix sketch**: route the remaining list loops through `trailing_comma_before`, which the
parameter, argument and array loops already use, and give the refusal a diagnostic of its own that
names the rule. Programs that compile today stop compiling, so it wants a changelog entry.

## BUG-158: four slices put logic in `lib.rs` and three expose test-only entry points

- **Status**: open, confirmed
- **Area**: `hir-lowering`, `llvm-backend`, `module-resolution`, `argument-binding` (`lib.rs`);
  `mlir-backend` and `syntax-parsing` (public entries); `neurc` (`architecture_tests.rs`)
- **Severity**: minor. Nothing misbehaves, but a rule in `VSA.md` is unchecked and broken

`VSA.md` lets a slice's `lib.rs` hold the entry function's own body and nothing else. `lib.rs`
holds the `Lowerer` and name mangling in `hir-lowering`, `build_module` and the option types in
`llvm-backend`, and more than its entry in `module-resolution` and `argument-binding`.
`lexical-analysis` conforms. It also makes an entry that only a slice's own tests call `pub(crate)`.
`mlir-backend::lower_program` and `translate_to_llvm_ir`, and `syntax-parsing::parse_expr`, are
`pub` and called from tests. The architecture test checks neither rule.

**Workaround**: none needed.

**Fix sketch**: move each `lib.rs` body into a module per slice, make the three test entries
`pub(crate)` and reach them from unit tests, and add a check for both rules to
`architecture_tests.rs`. The driver-facing pairs (`compile` and `compile_to_ir`, `lower_for_link`
and `lower_for_gpu`) stay public and are already named in their slices' CONTEXT.md.

## BUG-152: a method returning its own type nested in itself does not compile

- **Status**: open, confirmed. Narrowed: the checker now stops the chain at 128 nested instances
  with a diagnostic, the depth lowering already enforced, instead of allocating until killed
- **Area**: `semantic-analysis` (`instantiate_impls_for`); `hir-lowering` (`register_instance_methods`)
- **Severity**: major. A well-typed program fails to compile

**Minimal repro**

```neuro
struct W<I> { inner: I }

impl<I> W<I> {
    func wrap(self) -> W<W<I>> { W { inner: self } }
}

func main() -> i32 {
    val w = W { inner: 3 }
    w.wrap().inner.inner
}
```

Expected: exit code 3. Observed: `instantiating 'W' nests generic instances more than 128 deep`.
Before the cap, `neurc check` allocated until the memory cap aborted it. Every instance
registers the signature of each method of its generic impls, and registering `wrap` for `W<i32>`
builds `W<W<i32>>`, whose own `wrap` builds `W<W<W<i32>>>`, with no end. A trait's default method
returning `Wrapper<Self>` does the same, which is why the prelude's iterator adapters are not default
methods on `Iterator`.

**Workaround**: none that keeps the method; a free function `func wrap<I>(w: W<I>) -> W<W<I>>`
instantiates per call and terminates.

**Fix sketch**: register an instance's method signatures when a call first looks the method up, not
when the instance is built, in the checker and in lowering's `register_instance_methods` alike.

## BUG-077: a store through a borrow leaks the tensor it displaces

- **Status**: open, confirmed. Narrowed: a displaced value whose type proves what it owns (a
  user `Drop` type, a `Vec`, a map, a `string`, or a holder of one) is now destroyed at the store
- **Area**: `llvm-backend`; `drop_displaced_through_borrow` in `codegen/drops/mod.rs`
- **Severity**: major. An unbounded leak, one buffer per store, through a `&mut` parameter, a
  `&mut self` receiver or a `*r` store

**Minimal repro**

```neuro
struct Layer { w: Tensor<f32, [4]> }

func reset(l: &mut Layer) { l.w = Tensor::zeros() }

func main() -> i32 {
    mut l = Layer { w: Tensor::zeros() }
    mut i = 0
    while i < 1000 {
        reset(&mut l)
        i += 1
    }
    0
}
```

Expected: each `reset` releases the tensor it displaces, and the last one is released at `l`'s
scope exit. Observed: AddressSanitizer's leak check reports every displaced buffer.

**Root cause**: confirmed in the code. The value behind the borrow may have come from a `pool`
arena, and a callee cannot see whether its caller is inside one, so `may_hold_arena_memory`
leaves any displaced value holding a tensor alone.

**Workaround**: store through the owning binding (`l.w = ...` in the scope that owns `l`).

**Fix sketch**: release through the DLPack `deleter` only when the handle's buffer is not arena
memory, which the release can test as `__neuro_release` does. Regression tests: the repro in a
loop under a leak check, and a tensor field inside and outside a `pool`.

## BUG-027: a size parameter cannot be passed to another generic call

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (generic inference, `unify_array_len` / `seed_turbofish`)
- **Severity**: major. A generic function cannot delegate to another over its own size
  parameter, by inference or explicitly; the compiler rejects, it does not miscompile

A generic function that takes a size parameter (a tensor extent or an array length) cannot
pass that parameter to another generic function. Type parameters forward correctly; only
size and const parameters fail.

**Minimal repro**

```neuro
func sum_n<N>(t: &Tensor<i32, [N]>) -> i32 {
    mut total = 0
    for i in 0..(N as i32) { total = total + t[i] }
    return total
}

func delegate<N>(t: &Tensor<i32, [N]>) -> i32 {
    return sum_n(t)
}

func main() -> i32 {
    val v: Tensor<i32, [3]> = [1, 2, 3]
    return delegate(&v)
}
```

Expected: compiles and returns 6. The callee's `N` is named by the argument's own type, so
it is inferable. Observed:

```text
generic parameter 'N' cannot be inferred from the call arguments;
supply it explicitly with a turbofish, e.g. `f::<...>(...)`
```

The turbofish the message recommends does not work either. `sum_n::<N>(t)` reports

```text
turbofish argument for parameter 'N' has the wrong kind: a const argument was expected
```

so the parameter can be supplied neither way. Calling the same function from a *concrete*
caller works, and the identical program over a type parameter (`func outer<T>(x: T) -> T {
inner(x) }`) works, which is what isolates this to const parameters. Arrays hit it too: a
`func delegate<N>(a: &[i32; N])` calling a `func sum_arr<N>` fails the same way.

A size parameter named as a generic *type's* argument fails the same way. Inside
`impl<T, CAP> Buffer<T, CAP>`, `Self` and `Buffer<T, CAP>` both report
`unknown type name 'CAP'`, because a generic argument that is a bare name resolves as a type.
Type parameters work in that position, so `Self` in `impl<T> Cell<T>` is fine. A signature or
a field that passes a size to a type (`func f<N>(r: &Ring<N>)`) is one more case: the parser's
`kind_impl_extents` re-kinds the names an `impl` header passes, but not these, so `N` is still a
type parameter there.

**Root cause**: two halves, both confirmed in the code.

Inference: `unify_array_len` (`type_checkers/declarations/mod.rs`) matches a symbolic callee
extent against a symbolic argument extent with `(ArrayLen::Param(a), ArrayLen::Param(b)) =>
a == b`. It reports agreement but inserts nothing into the substitution, so the later
"every parameter must be bound" loop in `calls.rs` finds no binding and reports the
parameter as uninferable. When the two spell the parameter differently the same arm returns
`false` and an extent mismatch is reported on top.

Turbofish: `seed_turbofish` accepts a const argument only as `GenericArg::Const`, which is
what the parser produces for a literal. An identifier naming an in-scope const parameter
parses as `GenericArg::Type`, so it lands in the kind-mismatch arm.

**Workaround**: none within a generic function. Inline the callee's body, or make the caller
concrete.

**Fix sketch**: this is bigger than the two arms it appears to be, which is why it is filed
rather than patched. A substitution has to be able to hold a *symbolic* const ("the
caller's parameter `N`") not only a `Type::ConstValue`; `unify_array_len` then binds the
callee's parameter to it, `substitute_array_len` maps it back to an `ArrayLen::Param` in the
caller's frame, and the Copy check in `calls.rs` needs a carve-out for it the way
`ConstValue` already has one. `seed_turbofish` separately has to resolve an identifier
argument against the enclosing function's size and const parameters before deciding its kind,
and `kind_impl_extents` has to re-kind a name a signature or a field passes to a type's size. The
part to settle first is monomorphization: an instantiation of the outer function has to
resolve the inner call's symbolic binding to the concrete extent it was instantiated at, and
whether the existing instantiation walk carries enough context to do that is the question
that decides the shape of the rest. Regression tests want both spellings of the parameter
name, the turbofish form, the array form, and two distinct instantiations of the outer
function so a wrong extent could not pass unnoticed.

## Taking one of these on

An entry here is a self-contained task: repro, root cause where known, and a fix
sketch. If you want to work on one:

1. Open (or claim) an issue naming the `BUG-NNN` id so work isn't duplicated.
2. Follow the normal [contribution workflow](../CONTRIBUTING.md), branch, tests,
   quality gates, DCO sign-off.
3. A bug fix **must** ship with a regression test that fails without the fix. Name it
   after the defect so its purpose is obvious.
4. If an entry turns out to be a language-design decision rather than a patch, say so in
   the issue, some of these need a maintainer ruling before code changes.
