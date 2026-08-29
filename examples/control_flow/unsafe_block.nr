// Neuro Example: unsafe blocks (Phase 1.7 groundwork)
// Demonstrates: `unsafe { }` as an expression that evaluates to its trailing
// value, used both bound to a `val` and as the function's implicit return.
//
// In Phase 1.7 `unsafe` is a reserved keyword and a distinct AST node, but it
// is inert: it carries no special semantics yet and behaves exactly like a
// bare block. The kernel-aliasing relaxation it will eventually gate arrives
// with the GPU phase. Each of the three uses prints its value, which is the
// clearest way to show it really is just a block.

func main() -> i32 {
    // unsafe block bound to a value — evaluates to its trailing expression
    val base = unsafe {
        val a = 10
        val b = 7
        a + b          // 17
    }
    println("unsafe as value    = {base}")

    // unsafe block used as a void statement
    mut total: i32 = 0
    unsafe {
        total = base + 8   // 25
    }
    println("unsafe as statement -> total = {total}")

    val returned = unsafe { total - 1 }
    println("unsafe as return   = {returned}")

    // unsafe block as the implicit return value
    unsafe { returned }    // 24
}
