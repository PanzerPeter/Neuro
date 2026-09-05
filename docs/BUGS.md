# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

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
