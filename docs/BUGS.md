# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

## BUG-171: a binding named like a size parameter is accepted and reads the parameter

- **Status**: open, confirmed
- **Area**: `semantic-analysis` / `hir-lowering` (a size parameter read as a value)
- **Severity**: high. Valid source compiles and gives a wrong answer with no diagnostic

**Minimal repro**

```neuro
func s<N>(a: [i32; N]) -> u64 {
    val N = 100u64
    N
}

func main() -> i32 {
    s([1]) as i32
}
```

Expected: exit code 100, since a later `val` shadows a name for the rest of its scope. Observed:
exit code 1. The `N` after the `val` still reads the extent parameter. A `const N: u32`
parameter behaves the same way.

**Root cause**: not traced. A size parameter read as a value resolves before the local scope is
consulted, so the binding is made and never read.

**Workaround**: give the local another name.

**Fix sketch**: ruled: the language refuses the binding. A size, shape or `const` parameter is
a value and a type-level extent at once, so a local of the same name would split `N` from
`[T; N]` in one body (Rust refuses the same shadow). The checker reports any binding (`val`,
`mut`, pattern, loop or closure parameter) that takes a generic parameter's name, naming the
parameter. Regression test: the repro above is rejected, for a size and a `const` parameter.

## BUG-170: reading a field of a temporary with a `Drop` impl skips its destructor

- **Status**: open, confirmed
- **Area**: `llvm-backend` (`drop_temporary_except` in `codegen/drops/mod.rs`)
- **Severity**: moderate. Valid source compiles and runs, but the destructor never runs
  and every field the reader did not take leaks

**Minimal repro**

```neuro
struct Holder {
    v: Vec<i32>,
    w: Vec<i32>,
}

impl Drop for Holder {
    func drop(&mut self) {
        println("drop")
    }
}

func make() -> Holder {
    mut v: Vec<i32> = Vec::new()
    v.push(1)
    mut w: Vec<i32> = Vec::new()
    w.push(2)
    Holder { v: v, w: w }
}

func main() -> i32 {
    mut total: u64 = 0
    for i in 0..3 {
        total += make().v.len()
    }
    return total as i32
}
```

Expected: `drop` is printed three times. Observed: nothing is printed, and LeakSanitizer
reports the lost `Vec` buffers. A tensor field (`make().b.sum()`) behaves the same way.

**Root cause**: a field read out of a fresh temporary moves the field out and releases the
rest. When the struct has its own `drop`, that release is skipped so the destructor never
reads a moved field, and nothing else ever releases the temporary. Moving a field out of a named
`Drop` value is a compile error; a fresh temporary has no binding, so the checker's move rule does
not reach it, and `take(make().v)` still compiles and leaks.

**Workaround**: bind the value first (`val h = make()`), so its scope runs the destructor.

**Fix sketch**: refuse a by-value read of an owning field of a `Drop` temporary in the checker,
as for a named value. Then every remaining reader only borrows the field (a method receiver, a
`&` operand), so codegen can hold the whole temporary to the end of the statement
(`hold_to_statement_end`) instead of moving the field out, and the destructor and the field
drops run there.

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

## BUG-166: a tensor expression with more than 4095 checks is an internal error

- **Status**: open, confirmed
- **Area**: `mlir-backend` (the `MAX_SITES` cap in `tensor_arithmetic.rs`, `tensor_compound.rs` and
  `kernel/mod.rs`)
- **Severity**: minor. Only a pathological expression reaches it, and only at `-O 0`

**Minimal repro**: at `-O 0`, one statement summing 4200 `i32` tensor terms
(`val r = &a + &a + ...` with `a: Tensor<i32, [2]>`).

Expected: the program compiles. Observed: `a tensor operation has no MLIR lowering`.

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
pattern` at the inner `(`. `()` is the unit type's one value, so a pattern spelled `()` that
matches it is the natural reading. The pattern rules have no tuple pattern and no parenthesized
pattern in a `match`, so the form needs a rule in the specification before it needs code.

**Workaround**: write `Ok(_)`.

**Fix sketch**: state `()` as a pattern that matches the unit value, then parse it as one (no
literal stands for `()` today, so it needs a pattern form of its own in the AST, the checker and
both lowerings), and let exhaustiveness count it as covering `()`.

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
  `mlir-backend` and `syntax-parsing` (public entries); `neuroc` (`architecture_tests.rs`)
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
Before the cap, `neuroc check` allocated until the memory cap aborted it. Every instance
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
