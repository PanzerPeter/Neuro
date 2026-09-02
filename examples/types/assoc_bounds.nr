// Constraining an associated type in a bound — `Trait<Assoc = T>`.
//
// A trait that declares `type Item` says a member exists; only an impl says what it
// is. That is enough to call a method on a concrete value, but not through a type
// parameter: `T: Source` erases the implementor, and with it the only thing that
// answers `Self::Item`. Writing `T: Source<Item = i32>` puts the answer in the bound,
// so a generic body can call a method whose signature names the associated type.
//
// The form is one bound, written in three places: the parameter list, a `where`
// clause, and argument-position `impl Trait`, which is that same bound spelled
// anonymously. In return position it states what the caller gets.

trait Source {
    type Item

    func first(&self) -> Self::Item
}

@derive(Copy)
struct Counter {
    n: i32,
}

@derive(Copy)
struct Stride {
    step: i32,
}

@derive(Copy)
struct Gauge {
    reading: f64,
}

impl Source for Counter {
    type Item = i32

    func first(&self) -> Self::Item { self.n }
}

impl Source for Stride {
    type Item = i32

    func first(&self) -> Self::Item { self.step * 3 }
}

// A different implementor of the same trait, binding `Item` to a different type. It
// satisfies a bare `T: Source` bound, but NOT `Source<Item = i32>` — passing it to any
// of the functions below is a compile error naming both types.
impl Source for Gauge {
    type Item = f64

    func first(&self) -> Self::Item { self.reading }
}

// The bound in the parameter list. One body, monomorphized per implementor.
func doubled<T: Source<Item = i32>>(src: &T) -> i32 {
    src.first() * 2
}

// The same bound in a `where` clause.
func negated<T>(src: &T) -> i32 where T: Source<Item = i32> {
    0 - src.first()
}

// The same bound again, spelled anonymously: `impl Trait` in argument position is
// shorthand for a type parameter that appears nowhere else.
func plain(src: &impl Source<Item = i32>) -> i32 {
    src.first()
}

// In return position the constraint is a promise to the caller: whatever concrete type
// this body constructs, its `Item` is `i32`.
func make(seed: i32) -> impl Source<Item = i32> {
    Counter { n: seed }
}

func main() -> i32 {
    val c = Counter { n: 5 }
    val s = Stride { step: 4 }

    // One generic body, two implementors, both constrained to the same `Item`.
    val a = doubled(&c)     // 10
    val b = doubled(&s)     // 24
    println("doubled(Counter 5) -> {a}")
    println("doubled(Stride 4)  -> {b}")

    val n = negated(&c)     // -5
    val p = plain(&s)       // 12
    println("negated(Counter 5) -> {n}")
    println("plain(Stride 4)    -> {p}")

    val r = make(7).first() // 7
    println("make(7).first()    -> {r}")

    // The `Gauge` above is reachable through the trait, just not through these bounds:
    // its `Item` is f64, so it answers `first()` with a float.
    val g = Gauge { reading: 2.5 }
    val f = g.first() as i32 // 2
    println("Gauge.first()      -> {f}")

    val total = a + b + n + p + r + f
    println("total              -> {total}")
    total                   // 10 + 24 - 5 + 12 + 7 + 2 = 50
}
