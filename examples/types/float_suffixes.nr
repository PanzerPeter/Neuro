// Float literal suffixes: `f32` and `f64` pin a literal's type without a variable
// annotation. Each suffixed literal is printed. An `f32` prints the value it holds, not the
// decimal that was typed: `3.14159f32` renders as 3.141590118408203.

func scale_f32(x: f32, k: f32) -> f32 {
    return x * k
}

func scale_f64(x: f64, k: f64) -> f64 {
    return x * k
}

func main() -> i32 {
    val pi32: f32 = 3.14159f32
    val pi64: f64 = 3.141592653589793f64

    val lr      = 1.5e-3f32
    val epsilon = 1e-10f64

    val out32 = scale_f32(pi32, lr)
    val out64 = scale_f64(pi64, epsilon)

    val plain = 2.0
    val also: f64 = plain * out64

    println("3.14159f32              = {pi32}")
    println("3.141592653589793f64    = {pi64}")
    println("1.5e-3f32               = {lr}")
    println("1e-10f64                = {epsilon}")
    println("f32 product             = {out32}")
    println("f64 product             = {out64}")
    println("unsuffixed 2.0 (f64)    = {plain}")
    println("doubled f64 product     = {also}")

    return 0
}
