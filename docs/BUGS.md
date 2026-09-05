# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

## BUG-022 — constant folding wraps silently on overflow

**Repro**

```neuro
const C: u8 = 200u8 + 100u8

func main() -> i32 {
    println("const sum = {C}")     // prints 44
    return 0
}
```

The same arithmetic in a function body aborts on the debug tier:

```neuro
func main() -> i32 {
    mut a: u8 = 200u8
    val c: u8 = a + 100u8                  // panic: integer overflow
    return 0
}
```

A `const` initializer is evaluated by the compiler, so the debug tier's overflow panic has
nowhere to fire; the folder produces a value instead, and it produces the wrapped one. The
result is that a quantity written in a `const` and the same quantity computed in a function
disagree, on the tier whose whole purpose is to make that disagreement impossible.

**Root cause** — `fold_const` in `compiler/llvm-backend/src/codegen/expressions/literals.rs`
uses `wrapping_add` / `wrapping_sub` / `wrapping_mul` / `wrapping_neg` throughout and has no
error path for overflow. Every operator is affected, not one of them.

**Workaround** — none needed for correctness if the wrap was intended; write the folded value
directly when it was not. `docs/language-reference/types.md` documents the current behaviour
("compile-time constant folding always uses wrapping arithmetic regardless of optimization
level"), so a program relying on it is not relying on an accident.

**Fix sketch** — the ruling that settles it is already made for run-time arithmetic (debug
panics, release wraps), and a constant expression has no run-time tier to defer to: a `const`
whose initializer overflows
cannot produce a defined value under the debug rule and under the release rule at once. Rejecting
it at compile time is the only answer that does not make the tier observable in a value. That
means `fold_const` returning a `Result` that carries the overflow, and a diagnostic naming the
operator and the type. Deliberately NOT bundled with the unary-negation fix that closed BUG-021:
fixing negation alone inside the folder would recreate exactly the asymmetry BUG-021 was about,
with `const N: u8 = -ONE` rejected while `const N: u8 = 0u8 - ONE` still wrapped. All operators
move together or none do. The documentation sentence above is part of the change.

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
buffer, which is orthogonal to how that buffer is stored. The cap should be deleted when
the storage change lands rather than patched around before it.

That change belongs to the **DLPack standardization** item, which is the next open tensor
line — not to the pool allocator two sub-phases later, where this entry used to file it.
DLPack is a `{data*, ndim, shape, strides}` descriptor whose `data` field is a pointer to an
out-of-line buffer, and an SSA aggregate has no address to put there. The item after it
promises more of the same: an in-place compound assignment must leave "the buffer address
stable, so raw pointers held by the runtime, by an optimizer's state, or by a Python DLPack
consumer stay valid", which a by-value representation cannot offer at all. The pool allocator
*consumes* a handle that already exists — it registers one on construction and batches its
release — so it is a policy layer over this representation, not the thing that introduces it.
Filed under the pool allocator, the storage change sat downstream of the first two items that
cannot be built without it.

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
