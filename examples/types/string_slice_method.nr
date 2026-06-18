// Neuro Programming Language - String `.slice(range)` (Phase 1.7 §2.7)
//
// `.slice(range)` returns a borrowed `&string` view into the receiver's UTF-8
// data — zero copy, since strings are immutable. The range may be exclusive
// (`a..b`) or inclusive (`a..=b`). Indices are byte offsets; an out-of-bounds
// range or one that splits a UTF-8 code point panics at runtime (§1.2).

func main() -> i32 {
    val s = "hello, world"

    val hello = s.slice(0..5)    // bytes 0..5  → "hello"
    val world = s.slice(7..=11)  // bytes 7..=11 → "world" (inclusive)

    mut score: i32 = 0
    if hello == "hello" { score = score + 1 }
    if world == "world" { score = score + 2 }
    if s.slice(0..0) == "" { score = score + 4 }   // empty slice

    // A slice is itself a `&string`, so `.len()` reads its byte span (O(1)).
    score = score + (s.slice(0..5).len() as i32)   // + 5

    return score                                    // 1 + 2 + 4 + 5 = 12
}
