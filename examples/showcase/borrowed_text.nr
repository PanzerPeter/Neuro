// Showcase — explicit lifetime annotations working together with prior features.
//
// Cumulative integration example combining:
//   explicit lifetime annotations `<'a>` on reference parameters + return
//   · immutable borrows `&string` and borrowed string slices
//   · builtin `.len()` / `.slice(range)` / `.char_slice(range)` on strings
//   · if-expressions and implicit returns
//   · a lifetime alongside a type parameter, monomorphized on the type only
//
// A lifetime parameter is a pure well-formedness annotation: it is validated against
// the declared parameter list and then erased, so `&'a string` is exactly `&string`
// at runtime and every borrow below costs nothing.

// The classic signature: two borrows sharing one lifetime, returning a borrow
// that lives as long as both inputs.
func longest<'a>(a: &'a string, b: &'a string) -> &'a string {
    if a.len() > b.len() { a } else { b }
}

// A lifetime alongside no other generics: still an ordinary, concrete function that
// returns a zero-copy borrowed sub-slice of its input.
func head<'a>(s: &'a string) -> &string {
    s.slice(0..2)
}

// The codepoint-indexed companion, under the same lifetime contract. `.slice` would
// be wrong here: the caller counts characters, and a byte range through non-ASCII
// text either lands mid-code-point (a panic) or cuts in the wrong place.
func first_chars<'a>(s: &'a string) -> &string {
    s.char_slice(0..2)
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

    println("longest(&left, &right) = {winner}")
    println("head(winner)           = {prefix}   (zero-copy slice)")
    println("tagged_len(winner, _)  = {n}")
    println("left still bound       = {left}")

    // Multi-byte text is where the two index units part company: 'ö' is one character
    // but two bytes, so the same 0..2 range means different things.
    val label = "größe"
    val chars = first_chars(&label)       // 2 characters -> "gr"
    val bytes = label.slice(0..2)         // 2 bytes      -> "gr", the same here
    val three = label.char_slice(0..3)    // 3 characters -> "grö", 4 bytes
    println("label                  = {label}   ({label.len()} bytes, 5 characters)")
    println("first_chars(&label)    = {chars}")
    println("label.char_slice(0..3) = {three}   ({three.len()} bytes)")

    // 8 (len "spectrum") + 2 (len "sp") + 8 (tagged_len) = 18, plus the character
    // slices: 2 ("gr") + 4 ("grö") = 24.
    val total = winner.len() as i32
        + prefix.len() as i32
        + n
        + chars.len() as i32
        + three.len() as i32
    val agree = if chars == bytes { 1 } else { 0 }
    println("total                  = {total}")
    return total + agree
}
