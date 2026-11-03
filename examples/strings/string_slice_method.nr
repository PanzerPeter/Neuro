// `.slice(range)`: a borrowed `&string` view into a string's UTF-8 bytes. The view is zero
// copy, since strings are immutable. The range may be `a..b` or `a..=b`. Indices are byte
// offsets, and an out-of-bounds range, or one that splits a code point, panics at run time.

func main() -> i32 {
    val s = "hello, world"

    val hello = s.slice(0..5)    // bytes 0..5  → "hello"
    val world = s.slice(7..=11)  // bytes 7..=11 → "world" (inclusive)

    val empty = s.slice(0..0)
    println("s              = {s}")
    println("s.slice(0..5)  = {hello}")
    println("s.slice(7..=11)= {world}")
    println("s.slice(0..0)  = {empty}   (empty slice)")

    mut score: i32 = 0
    if hello == "hello" { score = score + 1 }
    if world == "world" { score = score + 2 }
    if s.slice(0..0) == "" { score = score + 4 }   // empty slice

    // A slice is itself a `&string`, so `.len()` reads its byte span (O(1)).
    score = score + (s.slice(0..5).len() as i32)   // + 5

    println("score          = {score}")
    return score                                    // 1 + 2 + 4 + 5 = 12
}
