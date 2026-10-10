// f16 / bf16 half-precision primitives
//
// `f16` (IEEE-754 half) and `bf16` (bfloat16) are ordinary floating point: arithmetic,
// the six comparisons, the math methods and `.to_checked`, like `f32`. Each operation
// computes in `f32` and rounds its answer to the operand type once, so the result is the
// correctly rounded one on every machine. A chain rounds after every step; nothing is
// carried at a wider precision unless the program asks with `as f32`.
//
// A literal takes its type from context (`val h: f16 = 0.5`, `h * 2.0`) or from its
// suffix (`1.5f16`, `0.02bf16`). There is no implicit widening: an `f16` meets only `f16`.

func scale(x: f16) -> f16 {
    return x * 4.0
}

func main() -> i32 {
    val a: f16 = 1.5
    val b = 2.0bf16

    // f16/bf16 are Copy: the source binding stays valid after a bind.
    val a_copy = a
    val same: bool = a == a_copy

    // `1 + 2^-8` is a tie in bf16 and rounds to 1, so adding the step twice stays at 1.
    // Summed in f32 and rounded at the end, the same two steps reach 1.0078125.
    val step: bf16 = 0.00390625
    val stepwise = 1.0bf16 + step + step
    val accumulated = (1.0bf16 as f32 + step as f32 + step as f32) as bf16

    val third = 1.0f16 / 3.0
    val root = b.sqrt()
    val three = scale(0.75)
    val ordered = third < a && root > 1.0

    // `.to_checked` answers `None` where `as` would saturate.
    val code = three.to_checked::<i32>() ?? 0
    val too_big = 300.0f16.to_checked::<u8>() ?? 0

    // Rendering is not on the half surface yet: widen to `f32` to print one.
    val a_out = a as f32
    val third_out = third as f32
    val root_out = root as f32
    val stepwise_out = stepwise as f32
    val accumulated_out = accumulated as f32

    println("f16 1.5                     = {a_out}")
    println("f16 1 / 3                   = {third_out}")
    println("bf16 2.0.sqrt()             = {root_out}")
    println("bf16 1 + 2^-8 + 2^-8        = {stepwise_out}")
    println("same sum carried in f32     = {accumulated_out}")
    println("third < a && root > 1       = {ordered}")
    println("scale(0.75) to_checked i32  = {code}")
    println("300.0f16 to_checked u8      = {too_big}")
    println("f16 is Copy (a == a_copy)   = {same}")

    if same && ordered && stepwise == 1.0 && too_big == 0 {
        return code        // 3 on success
    }
    return 1
}
