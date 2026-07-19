// f16 / bf16 half-precision primitives
//
// `f16` (IEEE-754 half) and `bf16` (bfloat16) are first-class scalar primitives
// with a deliberately narrow contract: binding, copy, `==`/`!=`, and `as`-cast
// to/from any numeric type. They have NO scalar arithmetic — half-precision ALUs
// are not portable, so the language directs you to compute in `f32` and cast back.
// Their payoff is bulk tensor compute (Phase 3), where the restriction lifts.
//
// Half-precision literals always carry their suffix: `1.5f16`, `0.02bf16`.

func scale(x: f16) -> f16 {
    // Arithmetic happens in f32, then narrows back to f16.
    return (x as f32 * 4.0f32) as f16
}

func main() -> i32 {
    val a: f16 = 1.5f16
    val b: bf16 = 2.0bf16

    // f16/bf16 are Copy: the source binding stays valid after a bind.
    val a_copy = a
    val same: bool = a == a_copy

    // The prescribed arithmetic workaround: widen to f32, compute, narrow back.
    val widened: f32 = a as f32 + 2.5f32     // 4.0
    val narrowed: f16 = widened as f16

    // as-cast across formats and to integers.
    val b_as_f16: f16 = b as f16
    val three: f16 = scale(0.75f16)          // 0.75 * 4 = 3.0
    val code: i32 = three as i32             // 3

    if same && narrowed == 4.0f16 && b_as_f16 == 2.0f16 {
        return code        // 3 on success
    }
    return 1
}
