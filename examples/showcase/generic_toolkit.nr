// Showcase: generics, const generics, turbofish, and where-clauses working together
// with prior features.
//
// Cumulative integration example combining:
//   generic structs + generic inherent impls with monomorphization
//   · generic functions with monomorphization
//   · const generic parameters + turbofish + `where` value predicates
//   · fixed-size arrays
//   · enums with associated data + pattern matching
//   · tuples as generic type arguments
//   · non-`Copy` type arguments: a struct holding a `string`, moved by value
//   · `Drop` destructors observed through a generic call boundary
//   · if-expressions and implicit returns
//
// Each generic template below (function, struct, or impl) is emitted once per
// concrete type or const argument, so `T` and `N` cost nothing at runtime.

enum Shape {
    Circle(i32),
    Rect(i32, i32),
}

// A generic struct: a heterogeneous pair. The type arguments are inferred from the
// field values at the literal, then monomorphized into a concrete struct.
struct Pair<T, U> {
    first: T,
    second: U,
}

// A generic struct with a generic inherent impl. `Cell<T>::get` is specialized per
// element type; here it is used at both i32 and bool.
struct Cell<T> {
    value: T,
}

impl<T> Cell<T> {
    func get(&self) -> T {
        self.value
    }
}

// A const-generic struct: `CAP` is a compile-time value, inferred from the field
// array's length, so `[T; CAP]` is sized concretely per instance.
struct Buffer<T, const CAP: u32> {
    data: [T; CAP],
    count: u32,
}

// The classic identity template: reused here at i32, at a tuple type, and at the two
// non-`Copy` types below. A type argument carries no `Copy` requirement — the body is
// move-checked once against an abstract `T` that is treated as non-`Copy`, so it is
// already valid for every instantiation.
func identity<T>(x: T) -> T {
    x
}

// A non-`Copy` struct: `string` owns a buffer, so `Label` moves rather than copies.
struct Label {
    text: string,
}

// A `Drop` type, to observe the by-value boundary: the destructor must run exactly
// once per value whether the generic callee swallows it or hands it back.
struct Guard {
    sink: &mut i32,
}

impl Drop for Guard {
    func drop(&mut self) {
        *self.sink = *self.sink + 1
    }
}

// Consumes its argument: at `T = Guard` the value dies here, in the callee.
func swallow<T>(v: T) -> i32 {
    1
}

// Selects one of two same-typed values (uses a bool and an if-expression).
func choose<T>(cond: bool, a: T, b: T) -> T {
    if cond { a } else { b }
}

// Two type parameters: returns only its second argument.
func second<T, U>(a: T, b: U) -> U {
    b
}

// A const-generic function: `N` is inferred from the array argument's length, and the
// `where` predicate guarantees the array is non-empty at every instantiation.
func sum_all<const N: u32>(a: [i32; N]) -> i32 where N > 0 {
    mut total: i32 = 0
    for x in a {
        total = total + x
    }
    total
}

// Non-generic helper: deconstructs an enum via pattern matching.
func area(s: Shape) -> i32 {
    match s {
        Shape::Circle(r) => 3 * r * r,
        Shape::Rect(w, h) => w * h,
    }
}

func main() -> i32 {
    val base = identity(10)          // identity<i32>            -> 10
    val pick = choose(true, base, 0) // choose<i32>             -> 10
    val k = second(2.5, 4)           // second<f64, i32>        -> 4
    val t = identity((3, 5))         // identity<(i32, i32)>    -> (3, 5)
    val a = area(Shape::Rect(2, 6))  // enum + match            -> 12

    val p = Pair { first: 6, second: 9 }   // Pair<i32, i32> (inferred)
    val cell = Cell { value: 7 }           // Cell<i32>
    val flag = Cell { value: true }        // Cell<bool>: distinct instance
    val cv = cell.get()                    // generic impl method     -> 7
    val bonus = if flag.get() { p.first } else { 0 }   // -> 6

    val id2 = identity::<i32>(8)           // turbofish            -> 8
    val nums: [i32; 3] = [4, 5, 6]
    val s = sum_all(nums)                  // const generic + where -> 15

    val buf = Buffer { data: [1, 2, 3, 0], count: 3 }  // Buffer<i32, 4> (CAP inferred)
    val c = buf.count as i32               // -> 3
    val d = buf.data[1]                    // -> 2

    // Non-`Copy` type arguments. `lab` is moved into identity<Label> and back out; the
    // original binding is dead from here on, exactly as a plain `val moved = lab` would
    // leave it.
    val lab = Label { text: "tensor" }
    val kept = identity(lab)               // identity<Label>: moved in, moved out
    val word = identity("axis")            // identity<string>

    // Two guards cross the same boundary in opposite directions: one is consumed by the
    // callee, one is returned to a fresh owner here. Both destructors run exactly once.
    mut dropped: i32 = 0
    {
        val g1 = Guard { sink: &mut dropped }
        val gone = swallow(g1)             // swallow<Guard>: dropped in the callee
        val g2 = Guard { sink: &mut dropped }
        val back = identity(g2)            // identity<Guard>: dropped at this scope's end
    }
    val destroyed = dropped                // -> 2

    println("identity<i32>(10)          = {base}")
    println("choose<i32>(true, 10, 0)   = {pick}")
    println("second<f64, i32>(2.5, 4)   = {k}")
    println("identity<(i32, i32)>       = ({t.0}, {t.1})")
    println("area(Shape::Rect(2, 6))    = {a}")
    println("Pair<i32, i32>             = ({p.first}, {p.second})")
    println("Cell<i32>::get()           = {cv}")
    println("Cell<bool> gated bonus     = {bonus}")
    println("identity::<i32>(8)         = {id2}")
    println("sum_all<3> where N > 0     = {s}")
    println("Buffer<i32, 4> count/data  = {c}, {d}")
    println("identity<Label> (non-Copy) = {kept.text}")
    println("identity<string>           = {word}")
    println("Guard destructors run      = {destroyed}")

    // 10 + 10 + 4 + 3 + 5 + 12 + 7 + 6 + 8 + 15 + 3 + 2 + 2 = 87
    val total = pick + base + k + t.0 + t.1 + a + cv + bonus + id2 + s + c + d + destroyed
    println("total                      = {total}")
    return total
}
