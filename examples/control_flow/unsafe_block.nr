// `unsafe { }` as an expression that evaluates to its trailing value, bound to a `val` and
// used as an implicit return. The one thing `unsafe` permits is a raw index read or write
// of a `KernelOut` inside a `@kernel` (see `tensors/tensor_kernel.nr`). Anywhere else it is
// a bare block.

func main() -> i32 {
    // unsafe block bound to a value: evaluates to its trailing expression
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
