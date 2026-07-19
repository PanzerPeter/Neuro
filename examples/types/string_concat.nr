// Neuro Programming Language - String concatenation (Phase 1.7)
// Demonstrates the `+` operator joining two strings into a new owned string.
//
// `string + string` allocates a fresh heap buffer and copies both operands'
// bytes in, yielding a new immutable `string`. Operands are read, not consumed
// (like `==`), so they remain usable afterward. A `&string` slice may stand in
// for either side. The result's `.len()` is the sum of the operand lengths.

func main() -> i32 {
    val hello: string = "Hello, "
    val name: string = "Neuro!"

    // "Hello, " (7) + "Neuro!" (6) = "Hello, Neuro!" (13 bytes).
    val greeting: string = hello + name

    // Operands are still valid after the concat.
    if hello == "Hello, " {
        if greeting == "Hello, Neuro!" {
            // A borrowed operand participates the same way.
            val tail: &string = &name
            val again: string = "Hi, " + tail   // "Hi, Neuro!" (10 bytes)
            val n: u64 = again.len()
            return n as i32   // 10
        }
    }

    return 1
}
