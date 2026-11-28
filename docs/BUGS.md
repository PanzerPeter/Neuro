# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

## BUG-176: a borrowing method called by path takes its receiver by value

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (path-call signature of a method); `hir-lowering` (`lower_assoc_call`,
  `lower_generic_assoc_call`)
- **Severity**: major. A well-typed program fails to compile

**Minimal repro**

```neuro
struct V { inner: i32 }

impl V {
    func peek(&self) -> i32 { self.inner }
    func put(&mut self, v: i32) { self.inner = v }
}

func main() -> i32 {
    val w = V { inner: 3 }
    mut m = V { inner: 1 }
    V::put(&mut m, 9)
    V::peek(&w) + m.inner
}
```

Expected: exit code 12, since `&self` is `self: &Self` and `&mut self` is `self: &mut Self`, so
the path form takes `&V` and `&mut V` as its first argument. Observed: `type mismatch: expected V,
found &mut V` and `expected V, found &V`. The checker types the receiver of every method called
by path as `Self`, so `V::put(m, 9)` is accepted instead: it moves `m` and the write is lost.
Generic types (`W::peek(w)`) behave the same.

**Workaround**: call it as a method, `w.peek()` and `m.put(9)`.

**Fix sketch**: give the path form's first parameter the `self` kind's type in the checker, and
pass the borrow through in lowering (the backend already takes `&self` by value and `&mut self`
by pointer). `V::peek(w)` is then refused, as the spec's desugaring says.

## BUG-175: a trait method returning its own type nested in itself does not compile

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (`instantiate_impls_for`); `hir-lowering` (`register_instance_methods`)
- **Severity**: major. A well-typed program fails to compile

**Minimal repro**

```neuro
trait Wrap {
    func wrap(self) -> W<Self>
}

struct W<I> { inner: I }

impl<I> Wrap for W<I> {
    func wrap(self) -> W<W<I>> { W { inner: self } }
}

func main() -> i32 {
    val w = W { inner: 3 }
    w.wrap().inner.inner
}
```

Expected: exit code 3. Observed: `instantiating 'W' nests generic instances more than 128 deep`.
An inherent method of this shape compiles: a level of the chain registers its inherent methods
when a use reaches it. Trait impls still register in full at every level, because lowering uses
some of them without a call to hook (a destructor, an operator, the iteration protocol, a `dyn`
vtable), so `wrap` on each level builds the next one. A trait's default method returning
`Wrapper<Self>` does the same, which is why the prelude's iterator adapters are not default
methods on `Iterator`.

**Workaround**: declare the method in an inherent `impl<I> W<I>` instead.

**Fix sketch**: defer a nested level's trait impls too, and register them from each implicit use
in lowering (drop glue, operator and protocol lowering, `dyn` coercion) as well as from calls.

## BUG-173: a device traversal whose function comes through a local panics

- **Status**: open, confirmed
- **Area**: `mlir-backend` (device traversals, `kernel/body/traversal.rs`)
- **Severity**: moderate. Valid source compiles, then panics at run time where the spec gives an
  answer

**Minimal repro**

```neuro
func main() -> i32 {
    val h: Tensor<f32, [4]> = [1.0, 2.0, 3.0, 4.0]
    val g = h.to(Device::GPU(0))
    val f = |x: f32| x * 2.0
    val r = g.map(f)
    val back = r.to(Device::CPU)
    println("{back[3]}")
    0
}
```

Expected: `8.0`, with a compile warning that the traversal runs on the host. The language
reference says a function passed through a local is a value only the run time knows, so its
traversal copies the operand to the host, runs there, and the compiler warns. Observed: no warning, and the run panics at the `.map`
with "this tensor lives on a GPU, where host code cannot read it".

**Root cause**: the outlined traversal has its host body alone, and nothing copies a device operand
to the host before calling it.

**Workaround**: pass the closure literal directly (`g.map(|x: f32| x * 2.0)`), or move the tensor to
the host first.

**Fix sketch**: copy a device operand to the host before the outlined traversal's host body runs, and
emit the warning the language reference names at compile time.

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
