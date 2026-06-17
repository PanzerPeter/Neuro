// Returned references and lifetime elision (§2.6).
//
// A function that returns a reference may borrow one of its reference
// parameters: under lifetime elision, a single input reference lifetime is
// applied to the output, so the returned borrow lives as long as the caller's
// borrow. The borrow checker rejects returning a reference to a function-local
// value — it would dangle — so only borrows that outlive the call are allowed.

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

    return *same + *pick        // 30 + 30 = 60
}
