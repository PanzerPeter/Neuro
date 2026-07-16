// Showcase — trait declarations working together with prior features.
//
// Cumulative integration example combining:
//   trait declarations with a required and a default method (§3.9)
//   · trait implementations for multiple concrete types (§3.9)
//   · trait-bounded generic function, monomorphized per type (§3.8, §3.9)
//   · structs + `&self` methods (§3.3, §3.4)
//   · fixed-size arrays with iteration (§3.1)
//   · @derive(Copy) + if-expressions and implicit returns
//
// A trait is fully erased at compile time: each `impl Trait for Type` lowers to
// ordinary methods, and the trait-bounded `scaled_area` is monomorphized into one
// specialized copy per shape — so traits cost nothing at runtime.

trait Shape {
    // Required: every shape must define its own area.
    func area(&self) -> i32

    // Default: a coarse "is this big?" flag reusing the required method. An
    // implementor inherits this unless it provides its own.
    func is_big(&self) -> i32 {
        if self.area() > 20 { 1 } else { 0 }
    }
}

@derive(Copy)
struct Square {
    side: i32,
}

@derive(Copy)
struct Rect {
    width: i32,
    height: i32,
}

impl Shape for Square {
    func area(&self) -> i32 {
        self.side * self.side
    }
}

impl Shape for Rect {
    func area(&self) -> i32 {
        self.width * self.height
    }
    // Overrides the default: a rectangle counts as big past a larger threshold.
    func is_big(&self) -> i32 {
        if self.area() > 30 { 1 } else { 0 }
    }
}

// Trait-bounded generic: dispatches `area()` through the bound, monomorphized per T.
func scaled_area<T: Shape>(s: &T, factor: i32) -> i32 {
    s.area() * factor
}

func main() -> i32 {
    val sq = Square { side: 5 }     // area 25
    val r = Rect { width: 4, height: 6 } // area 24

    // Trait-bounded generic dispatch, once per concrete shape type.
    val a = scaled_area(&sq, 2)     // 50
    val b = scaled_area(&r, 1)      // 24

    // Default method (Square inherits) + overridden method (Rect provides its own).
    val big = sq.is_big() + r.is_big()  // 1 (25>20) + 0 (24<=30) = 1

    // Combine with arrays: sum a small fixed-size array.
    val weights: [i32; 3] = [2, 3, 5]
    mut acc: i32 = 0
    for w in weights {
        acc = acc + w               // 10
    }

    // 50 + 24 + 1 + 10 = 85 ... trim to a small exit code.
    val total = a + b + big + acc   // 85
    total - 78                      // 7
}
