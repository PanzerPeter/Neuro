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

    // `f16` / `bf16` are not interpolatable — the narrow contract covers binding,
    // copy, `==`, and `as`, and rendering is not on that list. Widen to `f32` to
    // report one, exactly as you widen to compute with one.
    val a_out: f32 = a as f32
    val b_out: f32 = b as f32
    val narrowed_out: f32 = narrowed as f32
    val b_as_f16_out: f32 = b_as_f16 as f32
    val three_out: f32 = three as f32

    println("f16  1.5f16                 = {a_out}")
    println("bf16 2.0bf16                = {b_out}")
    println("f16 -> f32 + 2.5f32         = {widened}")
    println("narrowed back to f16        = {narrowed_out}")
    println("bf16 -> f16                 = {b_as_f16_out}")
    println("scale(0.75f16) via f32      = {three_out}")
    println("f16 -> i32                  = {code}")
    println("f16 is Copy (a == a_copy)   = {same}")

    if same && narrowed == 4.0f16 && b_as_f16 == 2.0f16 {
        return code        // 3 on success
    }
    return 1
}
