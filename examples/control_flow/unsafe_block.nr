// Neuro Example: unsafe blocks (Phase 1.7 groundwork)
// Demonstrates: `unsafe { }` as an expression that evaluates to its trailing
// value, used both bound to a `val` and as the function's implicit return.
//
// In Phase 1.7 `unsafe` is a reserved keyword and a distinct AST node, but it
// is inert: it carries no special semantics yet and behaves exactly like a
// bare block. The kernel-aliasing relaxation it will eventually gate arrives
// with the GPU phase.

func main() -> i32 {
    // unsafe block bound to a value — evaluates to its trailing expression
    val base = unsafe {
        val a = 10
        val b = 7
        a + b          // 17
    }

    // unsafe block used as a void statement
    mut total: i32 = 0
    unsafe {
        total = base + 8   // 25
    }

    // unsafe block as the implicit return value
    unsafe { total - 1 }   // 24
}
