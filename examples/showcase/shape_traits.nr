// Showcase: traits and both dispatch forms, alongside earlier features. Integration
// example: trait declarations with a required and a default method, impls for several
// types, a generic bounded by two traits (`<T: Shape + Resizable>`), `impl Trait` and `dyn
// Trait` dispatch, `Self` in signatures, a supertrait reached by a default body, a bound and
// a trait object, fixed arrays, and `@derive(Copy)`. `impl Trait` and `<T: Shape>` are
// monomorphized, one copy per concrete type, so they cost nothing at run time. `dyn Trait` is a single runtime type: the reference carries a value pointer and
// a method-table pointer, and each call goes through the table. A method returning `Self`
// cannot sit in a method table, so `Resizable` is used through bounds only.

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

// `Solid` requires `Shape`, so its default body may call `area` and every `Solid` is a
// `Shape`. A `Square` without `impl Shape` would be refused at `impl Solid`.
trait Solid: Shape {
    func depth(&self) -> i32

    func volume(&self) -> i32 {
        self.area() * self.depth()
    }
}

impl Solid for Square {
    func depth(&self) -> i32 {
        self.side
    }
}

impl Solid for Rect {
    func depth(&self) -> i32 {
        2
    }
}

// `T: Solid` reaches `Shape`'s methods and meets `scaled_area`'s `T: Shape`.
func solid_score<T: Solid>(s: &T) -> i32 {
    s.volume() + scaled_area(s, 1) + s.is_big()
}

// The table behind `&dyn Solid` carries `Shape`'s methods after `Solid`'s own, so one body
// reaches both, overridden `is_big` included.
func dyn_solid(s: &dyn Solid) -> i32 {
    s.volume() + s.area() + s.is_big()
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

    // Square: 125 + 25 + 1. Rect: 48 + 24 + 0.
    val solid = solid_score(&sq) + solid_score(&r) // 223
    val dyn_solids = dyn_solid(&sq) + dyn_solid(&r) // 223

    println("scaled_area(&Square, 2)  bound generic = {a}")
    println("scaled_area(&Rect, 1)    bound generic = {b}")
    println("is_big(): default + override           = {big}")
    println("array for-in sum                       = {acc}")
    println("describe(&impl Shape)    static        = {stat}")
    println("dyn_area over both       dynamic       = {dyn_sum}")
    println("dyn_flag over both       dynamic       = {dyn_flags}")
    println("area_gain over both      Self type     = {gain}")
    println("solid_score over both    supertrait    = {solid}")
    println("dyn_solid over both      supertrait    = {dyn_solids}")

    // 50 + 24 + 1 + 10 + 26 + 49 + 1 + 48 + 223 + 223 = 655. A process exit code keeps the
    // low byte, so the pin reads 143.
    val total = a + b + big + acc + stat + dyn_sum + dyn_flags + gain + solid + dyn_solids
    println("total                                  = {total}")
    total
}
