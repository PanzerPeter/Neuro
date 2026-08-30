// Neuro Programming Language - String `.char_slice(range)` (Phase 2A)
//
// `.char_slice(range)` is the codepoint-indexed companion to `.slice(range)`. Both
// return a borrowed `&string` view — zero copy, since strings are immutable — but
// `.slice` counts bytes while `.char_slice` counts Unicode code points, walking the
// UTF-8 bytes to find each endpoint. On ASCII the two agree; on anything else they do
// not, which is the whole reason `.char_slice` exists.
//
// Because a code point index can never land inside a code point, `.char_slice` has no
// boundary rule to break: its only runtime failure is a range that runs past the last
// character or one whose bounds are reversed, which panics like `.slice`.

func main() -> i32 {
    val s = "héllo"          // 5 characters, 6 bytes: 'é' takes two

    println("s                  = {s}")
    println("s.len()            = {s.len()}   (bytes, not characters)")

    // The same range, read two ways.
    val by_char = s.char_slice(0..3)   // 3 characters -> "hél"
    val by_byte = s.slice(0..3)        // 3 bytes      -> "hé"
    println("s.char_slice(0..3) = {by_char}")
    println("s.slice(0..3)      = {by_byte}")

    // Inclusive ranges work the same as on `.slice`.
    val tail = s.char_slice(3..=4)     // characters 3 and 4 -> "lo"
    println("s.char_slice(3..=4)= {tail}")

    // The character count is a legal upper bound, so the end of the string is
    // addressable and yields an empty slice.
    val empty = s.char_slice(5..5)
    println("s.char_slice(5..5) = [{empty}]   (empty slice)")

    // The result is an ordinary `&string`, so it chains and compares byte-wise.
    val nested = s.char_slice(0..3).char_slice(1..2)
    println("nested             = {nested}")

    mut score: i32 = 0
    if by_char == "hél" { score = score + 1 }
    if by_byte == "hé" { score = score + 2 }
    if by_char != by_byte { score = score + 4 }
    if nested == "é" { score = score + 8 }

    score = score + (by_char.len() as i32)   // + 4 bytes for those 3 characters

    println("score              = {score}")
    return score                              // 1 + 2 + 4 + 8 + 4 = 19
}
