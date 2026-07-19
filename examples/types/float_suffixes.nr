// Demonstrates float literal type suffixes.
// Suffixes pin the type of a float literal without a variable annotation.

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

    return 0
}
