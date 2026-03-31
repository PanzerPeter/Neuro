// Structs example — Phase 2 feature
//
// Demonstrates struct definition, instantiation, field access, and mutation.

struct Point {
    x: f64,
    y: f64
}

struct Rectangle {
    width: i32,
    height: i32
}

func main() -> i32 {
    // Struct instantiation
    val p = Point { x: 3.0, y: 4.0 }

    // Field access
    val px = p.x

    // Mutable struct with field assignment
    mut origin = Point { x: 0.0, y: 0.0 }
    origin.x = 1.0
    origin.y = 2.0

    // Struct with integer fields — area computed inline
    // (struct as function parameter is a Phase 2 feature)
    val rect = Rectangle { width: 10, height: 5 }
    val result = rect.width * rect.height

    return result
}
