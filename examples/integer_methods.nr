// NEURO Programming Language - Integer Primitive Methods (Phase 1.5 §1.2, §1.4)
// Demonstrates the wrapping / saturating arithmetic intrinsics and the right-shift
// method on builtin integer types. All dispatch through the builtin-method path:
// they are compiler-known intrinsics, not user-defined impl methods.

func main() -> i32 {
    val a: u8 = 200
    val b: u8 = 100

    // wrapping_add: 200 + 100 = 300, wraps modulo 256 -> 44 (always wraps,
    // never traps, even in debug builds).
    val wrapped: u8 = a.wrapping_add(b)        // 44

    // saturating_add: clamps to the u8 maximum instead of wrapping.
    val saturated: u8 = a.saturating_add(b)    // 255

    // saturating_sub on unsigned clamps at 0 rather than underflowing.
    val floored: u8 = b.saturating_sub(a)      // 0

    // shr: logical right shift on an unsigned value (0b1111_0000 >> 4 = 0b1111).
    val mask: u8 = 240
    val shifted: u8 = mask.shr(4)              // 15

    // signed arithmetic shift keeps the sign bit.
    val neg: i32 = -16
    val arith: i32 = neg.shr(2)                // -4

    if wrapped == 44u8 {
        if saturated == 255u8 {
            if floored == 0u8 {
                if shifted == 15u8 {
                    if arith == -4 {
                        return 0
                    }
                }
            }
        }
    }
    return 1
}
