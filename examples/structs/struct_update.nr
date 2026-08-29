// Struct field-init shorthand and functional-update syntax — Phase 2A
//
// Demonstrates `Point { x, y }` shorthand and `Point { x: 10, ..base }` update.
// Both structs are printed so the inherited field is visibly the base's.

struct Point {
    x: i32,
    y: i32
}

func main() -> i32 {
    val x = 3
    val y = 4

    // Shorthand: bare field names bind the same-named locals.
    val p = Point { x, y }              // Point { x: 3, y: 4 }
    println("p       = ({p.x}, {p.y})")

    // Functional update: y is inherited from p, x is overridden.
    val shifted = Point { x: 10, ..p }  // Point { x: 10, y: 4 }
    println("shifted = ({shifted.x}, {shifted.y})")

    val total = shifted.x + shifted.y
    println("sum     = {total}")

    return total                        // 10 + 4 = 14
}
