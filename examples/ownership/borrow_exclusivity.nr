// Borrow exclusivity: the flow-sensitive aliasing rules. Any number of shared `&T` borrows
// of a place may be live at once. A `&mut T` borrow is exclusive: while it is live, no
// other borrow of the same place may exist. While a `&mut` of a place is live, the place
// may not be read, written or moved through its own name, so every access goes through the
// borrow. A borrow held by a binding lasts until that binding leaves scope, and a borrow
// passed to a function ends with the call.

func inc(n: &mut i32) {
    *n = *n + 1
}

func sum2(a: &i32, b: &i32) -> i32 {
    *a + *b
}

func main() -> i32 {
    mut total: i32 = 0

    // Many shared borrows of the same place may coexist.
    val x: i32 = 10
    val a: &i32 = &x
    val b: &i32 = &x
    total = total + sum2(a, b)        // 0 + 20 = 20
    println("two shared &i32 of x  -> {total}")

    // A `&mut` passed to a call is released when the call returns, so the next
    // call may take its own exclusive borrow of the same place.
    mut counter: i32 = 0
    inc(&mut counter)                 // 1
    inc(&mut counter)                 // 2
    total = total + counter           // 20 + 2 = 22
    println("two &mut calls        -> counter {counter}, total {total}")

    // A mutable borrow confined to an inner scope ends at the closing brace,
    // freeing the place for a later exclusive borrow.
    mut value: i32 = 5
    if true {
        val r: &mut i32 = &mut value
        *r = *r + 3                   // value = 8
    }
    if true {
        val rd: &mut i32 = &mut value
        *rd = *rd + 1                 // value = 9
    }
    // Both borrows have ended, so `value` is readable through its own name again.
    total = total + value             // 22 + 9 = 31
    println("scoped &mut, then new -> value {value}, total {total}")

    return total
}
