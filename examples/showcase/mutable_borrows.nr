// Mutable borrows `&mut T` and the dereference operator `*`.
//
// A `&mut T` reference grants write access to a `mut` binding without taking
// ownership. The value is read and written through the `*` dereference operator.
// Mutating through the reference is visible at the original binding after the
// call returns — the borrow points at the caller's storage, it does not copy.

// Increment the integer the reference points at, in place.
func increment(n: &mut i32) {
    *n = *n + 1
}

// Add `delta` to the referent.
func add_into(n: &mut i32, delta: i32) {
    *n = *n + delta
}

func main() -> i32 {
    mut counter: i32 = 40
    println("start                = {counter}")

    increment(&mut counter)   // 40 -> 41
    println("increment(&mut c)    = {counter}")
    increment(&mut counter)   // 41 -> 42
    println("increment(&mut c)    = {counter}")
    add_into(&mut counter, 5) // 42 -> 47
    println("add_into(&mut c, 5)  = {counter}")

    // Read back through a local mutable reference.
    val r: &mut i32 = &mut counter
    val current: i32 = *r     // 47
    println("*r through &mut i32  = {current}")

    return current            // process exit code 47
}
