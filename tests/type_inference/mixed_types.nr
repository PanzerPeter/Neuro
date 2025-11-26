// Test: Complex program with mixed type inferences

func add_i16(a: i16, b: i16) -> i16 {
    a + b
}

func add_u64(a: u64, b: u64) -> u64 {
    a + b
}

func main() -> i32 {
    val x: i16 = 3
    val y: i16 = add_i16(x, 4)

    val a: u64 = 100
    val b: u64 = add_u64(a, 200)

    return 7
}
