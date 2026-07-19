// Borrow exclusivity — the flow-sensitive aliasing rules.
//
// The borrow checker enforces two coexistence rules at compile time:
//   * any number of shared `&T` borrows of a place may be live at once;
//   * a `&mut T` borrow is exclusive — while it is live, no other borrow
//     (shared or mutable) of the same place may exist.
//
// A borrow held by a binding lives until that binding leaves scope; a borrow
// passed to a function ends with the call. This program uses only borrow
// patterns the checker accepts, then returns a value derived from them.

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

    // A `&mut` passed to a call is released when the call returns, so the next
    // call may take its own exclusive borrow of the same place.
    mut counter: i32 = 0
    inc(&mut counter)                 // 1
    inc(&mut counter)                 // 2
    total = total + counter           // 20 + 2 = 22

    // A mutable borrow confined to an inner scope ends at the closing brace,
    // freeing the place for a later exclusive borrow.
    mut value: i32 = 5
    if true {
        val r: &mut i32 = &mut value
        *r = *r + 3                   // value = 8
    }
    val rd: &mut i32 = &mut value
    *rd = *rd + 1                     // value = 9
    total = total + value             // 22 + 9 = 31

    return total
}
