# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

## BUG-020 — an out-of-range float-to-integer cast produces an arbitrary value

**Repro**

```neuro
func main() -> i32 {
    mut f: f64 = 1e300
    println("1e300 as i32 = {f as i32}")
    return 0
}
```

The value printed depends on the optimization level, on the surrounding code, and on
the contents of the stack. `-O 0` happens to print `-2147483648`; `-O 3` prints
whatever the register held. `nan as i32` and an out-of-range cast to an unsigned type
behave the same way.

**Root cause** — the cast lowers to LLVM's `fptosi` / `fptoui`, which are defined only
when the truncated value is representable in the target type and yield `poison`
otherwise. Poison is not a wrong number, it is the absence of a number: the optimizer
is free to fold anything that consumes it, which is why the answer changes with the
build.

The language reference says an out-of-range float-to-integer cast is a compile
*warning* and points at a `.to_checked::<T>()` escape hatch. Neither exists: no warning
is emitted, and `.to_checked` is not implemented. So the case the spec expects to be
diagnosed is instead silently undefined.

**Fix sketch** — needs a ruling first, because three answers are defensible and no two
of them agree: saturate (LLVM has `llvm.fptosi.sat.*` for exactly this, at a cost of a
couple of instructions on x86), panic on the debug tier the way integer overflow and
array bounds do, or keep truncation and add the promised compile warning plus
`.to_checked`. Whichever wins, the poison has to go: today the program's output is not a
function of its source.

## BUG-019 — the most negative value of a signed type has no literal spelling

**Repro**

```neuro
func main() -> i32 {
    val a: i8  = -128            // rejected
    val b: i32 = -2147483648     // rejected
    return 0
}
```

```
integer literal 128 out of range for type i8
integer literal 2147483648 out of range for type i32
```

`i64` and `u64` fail one stage earlier, in the lexer, which cannot tokenize them at
all:

```neuro
val c: i64 = -9223372036854775808i64      // lexical error: invalid number literal
val d: u64 = 18446744073709551615u64      // lexical error: invalid number literal
```

**Workaround** — build the value instead of writing it: `mut b: i32 = -2147483647`
followed by `b = b - 1`, or `mut d: u64 = 9223372036854775807u64` followed by
`d = d * 2u64 + 1u64`.

**Root cause** — two layers, which is why this is one bug and not two. A negation is a
unary operator over a literal rather than part of it, so the checker range-checks the
*positive* magnitude, and `2147483648` does not fit `i32` even though `-2147483648`
does. Underneath that, the lexer carries every integer literal as an `i64`
(`TokenKind::Integer(i64)`), so magnitudes above `i64::MAX` cannot be represented
before a type is even known — which takes out `i64::MIN` and the whole upper half of
`u64` regardless of what the checker does.

**Fix sketch** — the lexer has to carry the magnitude as a `u64` (or a `u128`) and let
the checker decide what it means, which is the change both halves need. The checker then
range-checks a negation over an integer literal against the negated value rather than
the magnitude. Doing only the checker half fixes `i8`/`i16`/`i32` and leaves
`i64`/`u64` broken, which is worse than either end state.

## BUG-018 — a tensor larger than 32768 elements cannot be compiled at `-O 0`

**Repro**

```neuro
func main() -> i32 {
    val w = Tensor::<f32, [784, 128]>::random_normal(mean: 0.0f32, std: 0.02f32)
    return 0
}
```

`neurc compile` (which defaults to `-O 0`) rejects it:

```
`Tensor<f32, [784, 128]>` holds 100352 elements, more than the 32768 a tensor may hold
at `-O 0` ...
```

**Workaround** — compile with `-O 1` or higher. The program is correct; only the `-O 0`
lowering path cannot carry it.

**Root cause** — a tensor is a first-class `[N x T]` LLVM aggregate, so copying one is a
`load` and a `store` of the whole buffer. At `-O 1` and above SROA rewrites that pair into
a `memcpy` and any size works. At `-O 0` nothing does, and SelectionDAG crashes trying to
legalize the monolithic value: `SelectionDAG::ReplaceAllUsesWith` under
`SelectionDAG::Combine`, somewhere above 50k elements. The failure point is not a clean
threshold — it depends on the whole function's DAG — so the backend caps the buffer well
under the smallest observed failure and reports the limit rather than crashing. That cap
is `MAX_O0_TENSOR_ELEMENTS` in `compiler/llvm-backend/src/type_mapping.rs`.

**Fix sketch** — the cap is a symptom; the representation is the defect. A tensor buffer
has to stop being a first-class LLVM value: give it storage of its own and copy it with
`llvm.memcpy`, which needs an owning buffer (heap or arena), a drop at scope exit with
move-out suppression, and `sret` for returning one by value. That is the work the `Tensor
ownership and move semantics` and pool-allocator roadmap items already carry, so the cap
should be deleted when they land rather than patched around before them.

## Taking one of these on

Each entry above is a self-contained task: repro, root cause where known, and a fix
sketch. If you want to work on one:

1. Open (or claim) an issue naming the `BUG-NNN` id so work isn't duplicated.
2. Follow the normal [contribution workflow](../CONTRIBUTING.md), branch, tests,
   quality gates, DCO sign-off.
3. A bug fix **must** ship with a regression test that fails without the fix. Name it
   after the defect so its purpose is obvious.
4. If an entry turns out to be a language-design decision rather than a patch, say so in
   the issue, some of these need a maintainer ruling before code changes.
