// NEURO Type Inference Demo
// Demonstrates contextual type inference for numeric literals

// Integer type inference from variable declarations
func demonstrate_variable_inference() -> i32 {
    val x8: i8 = 127           // Literal 127 infers as i8
    val x16: i16 = 1000        // Literal 1000 infers as i16
    val x32: i32 = 100000      // Literal 100000 infers as i32
    val x64: i64 = 5000000000  // Literal 5000000000 infers as i64

    val u8_val: u8 = 255       // Literal 255 infers as u8
    val u32_val: u32 = 4000000 // Literal 4000000 infers as u32

    return 0
}

// Float type inference
func demonstrate_float_inference() -> i32 {
    val f32_val: f32 = 2.5     // Literal 2.5 infers as f32
    val f64_val: f64 = 2.5     // Literal 2.5 infers as f64

    return 0
}

// Type inference in function parameters
func add_i16(a: i16, b: i16) -> i16 {
    a + b
}

func add_u64(a: u64, b: u64) -> u64 {
    a + b
}

func demonstrate_function_param_inference() -> i32 {
    val result1: i16 = add_i16(100, 200)  // Literals 100, 200 infer as i16
    val result2: u64 = add_u64(1000, 2000) // Literals 1000, 2000 infer as u64

    return 0
}

// Type inference in return expressions
func get_i32() -> i32 {
    42  // Literal 42 infers as i32 from return type
}

func get_i64() -> i64 {
    5000000000  // Literal 5000000000 infers as i64 from return type
}

// Default type inference (no context)
func demonstrate_defaults() -> i32 {
    val default_int = 42      // Defaults to i32
    val default_float = 2.5   // Defaults to f64
    val large_int = 5000000000 // Auto-promotes to i64 (too large for i32)

    return 0
}

// Main entry point
func main() -> i32 {
    val _ = demonstrate_variable_inference()
    val _ = demonstrate_float_inference()
    val _ = demonstrate_function_param_inference()
    val _ = get_i32()
    val _ = get_i64()
    val _ = demonstrate_defaults()

    return 0
}
