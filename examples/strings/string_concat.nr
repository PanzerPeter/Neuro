// `+` joins two strings into a new owned `string`. `string + string` allocates a fresh heap
// buffer and copies both operands in. Operands are read, not consumed, so they remain
// usable. A `&string` may stand in for either side, and the result's `.len()` is the sum of
// the operand lengths.

func main() -> i32 {
    val hello: string = "Hello, "
    val name: string = "Neuro!"

    // "Hello, " (7) + "Neuro!" (6) = "Hello, Neuro!" (13 bytes).
    val greeting: string = hello + name

    println("hello + name    = {greeting}")
    println("hello after     = {hello}   (operands are read, not consumed)")
    println("name after      = {name}")

    // Operands are still valid after the concat.
    if hello == "Hello, " {
        if greeting == "Hello, Neuro!" {
            // A borrowed operand participates the same way.
            val tail: &string = &name
            val again: string = "Hi, " + tail   // "Hi, Neuro!" (10 bytes)
            val n: u64 = again.len()
            println("literal + &name = {again}")
            println("its length      = {n}")
            return n as i32   // 10
        }
    }

    return 1
}
