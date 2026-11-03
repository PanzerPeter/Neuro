// Returned references and lifetime elision. A function returning a reference may borrow one
// of its reference parameters. With a single input reference lifetime, elision applies it
// to the output, so the result lives as long as the caller's borrow. Returning a borrow of
// a function-local value is rejected, because it would dangle.

// Returns its borrowed input unchanged; the output lifetime is the input's.

func identity(r: &i32) -> &i32 {
    r
}

// Picks the first of two borrowed inputs; both candidates outlive the call.
func first(a: &i32, b: &i32) -> &i32 {
    a
}

func main() -> i32 {
    val x: i32 = 30
    val y: i32 = 12

    // Each returned reference borrows a binding that outlives the call.
    val same: &i32 = identity(&x)
    val pick: &i32 = first(&x, &y)

    println("identity(&x)   -> {*same}")
    println("first(&x, &y)  -> {*pick}   (y is {y}, and never returned)")

    val total = *same + *pick   // 30 + 30 = 60
    println("total          -> {total}")
    return total
}
