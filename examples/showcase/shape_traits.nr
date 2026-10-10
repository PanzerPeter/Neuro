// Showcase: trait declarations and both dispatch forms working together with
// prior features.
//
// Cumulative integration example combining:
//   trait declarations with a required and a default method
//   · trait implementations for multiple concrete types
//   · trait-bounded generic function, monomorphized per type
//   · `impl Trait` static dispatch in argument position
//   · `dyn Trait` dynamic dispatch through a trait object
//   · `Self` in a trait signature, a default method and impl bodies
//   · a generic with two bounds, `<T: Shape + Resizable>`
//   · structs + `&self` methods
//   · fixed-size arrays with iteration
//   · @derive(Copy) + if-expressions and implicit returns
//
// A trait bound can be satisfied two ways, and the keyword chooses. `impl Trait`
// and a named `<T: Shape>` are both monomorphized: the compiler emits one
// specialized copy per concrete type, so they cost nothing at runtime. `dyn Trait`
// instead produces a single runtime type: the reference carries a pointer to the
// value plus a pointer to that type's method table, and the call jumps through the
// table. That is what lets ONE `dyn` function serve shapes of different types.
//
// `Self` names whichever type implements a trait. A method returning it cannot sit
// in a method table, because the caller of a `dyn` call would not know the size of
// what comes back, so `Resizable` is used through bounds only.

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

trait Resizable {
    func grown(&self, by: i32) -> Self

    // A default naming `Self` is copied into each impl, where it means that type.
    func grown_twice(&self, by: i32) -> Self {
        self.grown(by).grown(by)
    }
}

impl Resizable for Square {
    func grown(&self, by: i32) -> Self {
        Self { side: self.side + by }
    }
}

impl Resizable for Rect {
    func grown(&self, by: i32) -> Self {
        Self { width: self.width + by, height: self.height + by }
    }
}

// Through the bound, `grown_twice` returns `T`, so `area()` resolves on the result.
func area_gain<T: Shape + Resizable>(s: &T, by: i32) -> i32 {
    s.grown_twice(by).area() - s.area()
}

// Trait-bounded generic: dispatches `area()` through the bound, monomorphized per T.
func scaled_area<T: Shape>(s: &T, factor: i32) -> i32 {
    s.area() * factor
}

// Static dispatch, anonymous generic form: `&impl Shape` is shorthand for the
// `<T: Shape>(s: &T)` above and compiles to exactly the same specialized code.
func describe(s: &impl Shape) -> i32 {
    s.area() + s.is_big()
}

// Dynamic dispatch: ONE function body serves every implementor. The concrete type
// is not known here: `area` is reached through the trait object's method table.
func dyn_area(s: &dyn Shape) -> i32 {
    s.area()
}

// Dynamic dispatch reaching a default vs. an overridden method: Square inherits
// `is_big`, Rect overrides it, and the vtable selects correctly at runtime.
func dyn_flag(s: &dyn Shape) -> i32 {
    s.is_big()
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

    // Static dispatch through `impl Trait`: 25 + 1 = 26.
    val stat = describe(&sq)

    // Dynamic dispatch: the same two functions applied to two different concrete
    // types. 25 + 24 = 49 areas, and 1 + 0 = 1 flags.
    val dyn_sum = dyn_area(&sq) + dyn_area(&r)   // 49
    val dyn_flags = dyn_flag(&sq) + dyn_flag(&r) // 1

    // `Self` through two bounds: Square 5 -> 7 gains 49 - 25, Rect 4x6 -> 6x8
    // gains 48 - 24.
    val gain = area_gain(&sq, 1) + area_gain(&r, 1) // 48

    println("scaled_area(&Square, 2)  bound generic = {a}")
    println("scaled_area(&Rect, 1)    bound generic = {b}")
    println("is_big(): default + override           = {big}")
    println("array for-in sum                       = {acc}")
    println("describe(&impl Shape)    static        = {stat}")
    println("dyn_area over both       dynamic       = {dyn_sum}")
    println("dyn_flag over both       dynamic       = {dyn_flags}")
    println("area_gain over both      Self type     = {gain}")

    // 50 + 24 + 1 + 10 + 26 + 49 + 1 + 48 = 209
    val total = a + b + big + acc + stat + dyn_sum + dyn_flags + gain
    println("total                                  = {total}")
    total
}
