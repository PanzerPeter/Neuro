# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

## BUG-021 — negating an unsigned integer literal wraps instead of erroring

**Repro**

```neuro
func main() -> i32 {
    val x: u8 = -1
    println("literal -1 as u8 = {x}")     // prints 255
    return 0
}
```

The same value computed rather than written aborts instead:

```neuro
func main() -> i32 {
    mut z: u8 = 0u8
    z = z - 1u8                            // panic: integer overflow
    return 0
}
```

Two spellings of the same quantity disagree: written as a literal it silently becomes
`255`, computed it is an overflow panic on the debug tier. Both cannot be right.

**Root cause** — the checker range-checks the literal `1`, which fits `u8`, and then
types the negation as its operand's type. Nothing ever asks whether the value the
expression *denotes* is representable. The negation itself lowers to a plain wrapping
`sub 0, x` with no overflow guard, so the wrap is invisible at run time as well.

**Workaround** — write the intended value directly (`val x: u8 = 255`), or use a signed
type if a negative value is what was meant.

**Fix sketch** — needs a ruling first. Rejecting `-1` for an unsigned type is what the
range check would do if it looked at the denoted value, and is what most languages
choose, but it turns programs that compile today into compile errors. The alternative is
to declare unary `-` on an unsigned type a defined wrapping operation and say so in the
language reference, which then leaves it inconsistent with `-` the binary operator on
the debug tier. The checker already has the hook: `check_unary_expr` range-checks a
negation over a literal against the negated value, and deliberately restricts that to
signed targets.

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
move-out suppression, and `sret` for returning one by value. The `Tensor ownership and
move semantics` roadmap item has since landed and did **not** change the representation —
it shipped the ownership *surface* (`.clone()`, `.to(device)`) on the existing by-value
buffer, which is orthogonal to how that buffer is stored. The storage change is the
pool-allocator item's work, and the cap should be deleted when it lands rather than
patched around before it.

Running the middle-end `sroa` pass at `-O 0` is **not** a shortcut past that work. With
the cap lifted, adding it does let a large tensor be constructed and cloned inside one
function, but a function *returning* a large tensor by value still fails to compile —
there is no `sret`, so the value has to cross the call boundary whole. The by-value
representation is the defect at every level, not just in one lowering path.

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
