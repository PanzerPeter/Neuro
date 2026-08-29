// Neuro Type Inference Demo
// Demonstrates contextual type inference for numeric literals
//
// Each inferred binding is printed, so the output is a record of what the
// literal on the right actually became.

// Integer type inference from variable declarations
func demonstrate_variable_inference() -> i32 {
    val x8: i8 = 127           // Literal 127 infers as i8
    val x16: i16 = 1000        // Literal 1000 infers as i16
    val x32: i32 = 100000      // Literal 100000 infers as i32
    val x64: i64 = 5000000000  // Literal 5000000000 infers as i64

    val u8_val: u8 = 255       // Literal 255 infers as u8
    val u32_val: u32 = 4000000 // Literal 4000000 infers as u32

    println("i8   = {x8}")
    println("i16  = {x16}")
    println("i32  = {x32}")
    println("i64  = {x64}")
    println("u8   = {u8_val}")
    println("u32  = {u32_val}")

    return 0
}

// Float type inference
func demonstrate_float_inference() -> i32 {
    val f32_val: f32 = 2.5     // Literal 2.5 infers as f32
    val f64_val: f64 = 2.5     // Literal 2.5 infers as f64

    println("f32  = {f32_val}")
    println("f64  = {f64_val}")

    return 0
}

// Default type inference (no context)
func demonstrate_defaults() -> i32 {
    val default_int = 42       // Defaults to i32
    val default_float = 2.5    // Defaults to f64
    val large_int = 5000000000i64 // Explicit suffix required (too large for i32)

    println("default int   = {default_int}")
    println("default float = {default_float}")
    println("suffixed i64  = {large_int}")

    return 0
}

// Type inference in return expressions
func get_i32() -> i32 {
    42  // Literal 42 infers as i32 from return type
}

// Main entry point
func main() -> i32 {
    val v1 = demonstrate_variable_inference()
    val v2 = demonstrate_float_inference()
    val v3 = demonstrate_defaults()
    val v4 = get_i32()

    println("get_i32()     = {v4}")

    return v1 + v2 + v3 + v4
}
