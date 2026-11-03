// Demonstrates the `prefer_loop_over_while_true` lint and its `@allow` attribute. Without
// the `@allow` on this function, neurc compiles the `while true` loop and prints a warning
// to stderr suggesting `loop`. The lint is compile-time only, so the program's output is
// the same either way.

@allow(prefer_loop_over_while_true)
func main() -> i32 {
    mut i: i32 = 0
    while true {
        if i == 7 {
            break
        }
        i = i + 1
    }
    println("while true counted to {i}")
    return i
}
