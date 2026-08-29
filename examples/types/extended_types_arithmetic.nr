// Arithmetic across every extended integer width, one operation each.
//
// Each result is printed; `main` returns 0, so the printed values are what this
// example asserts.

func test_i8(x: i8, y: i8) -> i8 {
    return x + y
}

func test_i16(x: i16, y: i16) -> i16 {
    return x - y
}

func test_u8(x: u8, y: u8) -> u8 {
    return x + y
}

func test_u16(x: u16, y: u16) -> u16 {
    return x - y
}

func test_u32(x: u32, y: u32) -> u32 {
    return x + y
}

func test_u64(x: u64, y: u64) -> u64 {
    return x - y
}

func main() -> i32 {
    val a: i8 = test_i8(100i8, 27i8)              // 127, the i8 maximum
    val b: i16 = test_i16(1000i16, 1500i16)       // -500
    val c: u8 = test_u8(200u8, 55u8)              // 255, the u8 maximum
    val d: u16 = test_u16(1000u16, 250u16)        // 750
    val e: u32 = test_u32(4000000000u32, 294967295u32)  // 4294967295, the u32 maximum
    val f: u64 = test_u64(10000000000u64, 1u64)   // 9999999999

    println("i8   100 + 27                  = {a}")
    println("i16  1000 - 1500               = {b}")
    println("u8   200 + 55                  = {c}")
    println("u16  1000 - 250                = {d}")
    println("u32  4000000000 + 294967295    = {e}")
    println("u64  10000000000 - 1           = {f}")

    return 0
}
