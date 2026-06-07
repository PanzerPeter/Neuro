// Neuro Programming Language - Move semantics by default (Phase 1.7 §2.2)
//
// Non-Copy owned values (today: `string`) are MOVED when bound, assigned,
// returned, or passed by value. After a move the source binding is invalid;
// reading it is a compile error. `.clone()` is the explicit opt-out.
//
// Copy scalars (integers, floats, bool) are duplicated, never moved.

func consume(s: string) -> u64 {
    // `s` is moved into this function and dropped at the end of its scope.
    s.len()
}

func main() -> i32 {
    // A move on assignment: `original` is moved into `taken`.
    val original: string = "neuro"
    val taken: string = original
    // Reading `original` here would be a compile error: use of moved value.
    // val bad: u64 = original.len()   // <- rejected by the borrow checker

    // `.clone()` sidesteps the move — both bindings stay valid.
    val a: string = "hello"
    val b: string = a.clone()
    val both_ok: bool = a == b          // reads `a` after cloning: fine

    // A conditional move does not leak past the branch it lives in.
    val msg: string = "hi"
    if both_ok {
        val len_in_branch: u64 = consume(msg)   // moves `msg` on this path only
    }
    val len_after: u64 = msg.len()      // valid: the move above was conditional

    // Copy scalars are duplicated, so the source stays usable.
    val n: i32 = 5
    val m: i32 = n
    val sum: i32 = n + m                // both `n` and `m` are still valid

    return taken.len() as i32 - 5       // "neuro" has length 5 → 0
}
