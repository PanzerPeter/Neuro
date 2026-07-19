// Neuro Programming Language - Builtin Method Dispatch (Phase 1.5)
// Demonstrates the first intrinsic method on a builtin type: string.len()
//
// `.len()` reads the byte length stored in the string fat pointer (ptr, len).
// It is O(1) — no scan — and returns a u64. The length excludes the null terminator.

func main() -> i32 {
    val greeting: string = "hello, world"

    // 12 bytes: "hello, world" has no multi-byte code points.
    val n: u64 = greeting.len()

    // The empty string has length 0.
    val empty: string = ""
    val zero: u64 = empty.len()

    if n == 12u64 {
        if zero == 0u64 {
            return 0
        }
    }
    return 1
}
