// Neuro Programming Language - Immutable borrows (Phase 1.7 §2.4)
//
// An immutable borrow `&T` is a non-owning reference to a value. It lets a
// function read a value without taking ownership, so the caller keeps using
// its binding afterward. The borrow expression `&x` references the place `x`.
//
// Borrowing never moves the borrowed value, and `&T` is itself Copy.
// Method and field access auto-deref through the borrow.

struct Point {
    x: i64,
    y: i64
}

impl Point {
    func sum(&self) -> i64 {
        self.x + self.y
    }
}

// `s` is borrowed, not moved — the caller's string stays valid.
func describe(s: &string) -> u64 {
    s.len()                       // auto-deref: `.len()` on a &string
}

// Borrow a struct and read a field through the reference.
func read_x(p: &Point) -> i64 {
    p.x
}

// Borrow a struct and call a method through the reference.
func read_sum(p: &Point) -> i64 {
    p.sum()
}

func main() -> i32 {
    val msg: string = "Neuro"
    val n: u64 = describe(&msg)   // borrow — msg is NOT moved
    val again: u64 = msg.len()    // still valid: borrowing never consumes

    val pt = Point { x: 3, y: 4 }
    val px: i64 = read_x(&pt)     // 3
    val ps: i64 = read_sum(&pt)   // 7
    val direct: i64 = pt.sum()    // pt still usable after being borrowed

    // n (5) + again (5) + px (3) + ps (7) + direct (7) = 27, minus 27 → 0.
    return (n as i32) + (again as i32) + (px as i32) + (ps as i32) + (direct as i32) - 27
}
