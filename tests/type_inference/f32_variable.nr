// Test: f32 type inference in variable declaration
// Literal 3.14 should infer as f32, not f64

func main() -> i32 {
    val x: f32 = 3.14
    return 0
}
