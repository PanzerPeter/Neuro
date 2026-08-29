// Demonstrates the `prefer-loop-over-while-true` lint introduced in Phase 1.5.
//
// The `@allow(prefer_loop_over_while_true)` attribute on this function
// silences the lint that would otherwise fire on `while true { ... }`.
// Without the attribute, neurc compiles successfully and prints a warning
// to stderr suggesting `loop { ... }` as the canonical infinite loop form.
//
// The lint is a compile-time diagnostic, so nothing about it reaches the
// running program: the counter below prints and exits exactly as it would
// under `loop`.

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
