// Copy trait + @derive(Copy, Clone) — Phase 1.7 (§2.3)
//
// A struct that derives Copy is duplicated on assignment, so the source
// binding stays valid (no move). A struct that derives Clone supports an
// explicit `.clone()` deep copy. Both yield independent values.

@derive(Copy, Clone)
struct Point {
    x: i32,
    y: i32
}

func main() -> i32 {
    val a = Point { x: 3, y: 4 }

    // Copy: `a` is duplicated into `b`; `a` remains usable afterwards.
    val b = a
    val sum_a = a.x + a.y       // 7 — `a` is still valid after the copy

    // Clone: an explicit independent copy via @derive(Clone).
    val c = b.clone()
    val sum_c = c.x + c.y       // 7

    return sum_a + sum_c        // 7 + 7 = 14
}
