// Showcase — explicit lifetime annotations working together with prior features.
//
// Cumulative integration example combining:
//   explicit lifetime annotations `<'a>` on reference parameters + return (§2.6)
//   · immutable borrows `&string` and borrowed string slices (§2.4, §2.7)
//   · builtin `.len()` / `.slice(range)` on strings (§2.7)
//   · if-expressions and implicit returns (§1.8)
//   · a lifetime alongside a type parameter, monomorphized on the type only (§3.8)
//
// A lifetime parameter is a pure well-formedness annotation: it is validated against
// the declared parameter list and then erased, so `&'a string` is exactly `&string`
// at runtime and every borrow below costs nothing.

// The classic §2.6 signature: two borrows sharing one lifetime, returning a borrow
// that lives as long as both inputs.
func longest<'a>(a: &'a string, b: &'a string) -> &'a string {
    if a.len() > b.len() { a } else { b }
}

// A lifetime alongside no other generics: still an ordinary, concrete function that
// returns a zero-copy borrowed sub-slice of its input.
func head<'a>(s: &'a string) -> &string {
    s.slice(0..2)
}

// A lifetime mixed with a type parameter: only `T` drives monomorphization, the
// lifetime just annotates the borrow.
func tagged_len<'a, T>(s: &'a string, _tag: T) -> i32 {
    s.len() as i32
}

func main() -> i32 {
    val left = "neon"
    val right = "spectrum"

    val winner = longest(&left, &right)   // borrows both, returns the longer -> "spectrum"
    val prefix = head(winner)             // zero-copy 2-byte slice -> "sp"
    val n = tagged_len(winner, true)      // lifetime + type param -> 8

    // 8 (len "spectrum") + 2 (len "sp") + 8 (tagged_len) = 18
    return winner.len() as i32 + prefix.len() as i32 + n
}
