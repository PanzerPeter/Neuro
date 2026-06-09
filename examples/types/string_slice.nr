// Neuro Programming Language - String slices `&string` (Phase 1.7 §2.7)
//
// `&string` is a borrowed, non-owning view into UTF-8 data — a string slice.
// There is no separate slice type: `&string` is both "borrow of an owned
// string" and "string slice." A slice is read-only, so the fundamental
// operation it supports is equality: two slices, or a slice and an owned
// string, compare their underlying bytes.

// Compare two borrowed string slices by value (no ownership taken).
func slices_equal(a: &string, b: &string) -> bool {
    a == b
}

// Compare a slice against an owned string — both sides normalize to the same
// `(ptr, len)` fat pointer before the byte compare.
func slice_matches(s: &string) -> bool {
    s == "Neuro"
}

func main() -> i32 {
    val lang: string = "Neuro"
    val same: string = "Neuro"
    val other: string = "Rust"

    // Borrowing never moves: `lang` stays usable after each `&lang`.
    val eq_same: bool = slices_equal(&lang, &same)    // true
    val eq_other: bool = slices_equal(&lang, &other)  // false
    val ne_other: bool = (&lang != &other)            // true
    val literal: bool = slice_matches(&lang)          // true

    // true(1) + false(0) + true(1) + true(1) = 3, minus 3 → 0.
    return (eq_same as i32) + (eq_other as i32) + (ne_other as i32) + (literal as i32) - 3
}
