// Showcase — generic structs, impls, and functions working together with prior features.
//
// Cumulative integration example combining:
//   generic structs + generic inherent impls with monomorphization (§3.8)
//   · generic functions with monomorphization (§3.8)
//   · enums with associated data + pattern matching (§3.5, §3.6)
//   · tuples as generic type arguments (§3.2)
//   · if-expressions and implicit returns
//
// Each generic template below (function, struct, or impl) is emitted once per
// concrete type argument, so `T` costs nothing at runtime.

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

// The classic identity template — reused here at i32 and at a tuple type.
func identity<T>(x: T) -> T {
    x
}

// Selects one of two same-typed values (uses a bool and an if-expression).
func choose<T>(cond: bool, a: T, b: T) -> T {
    if cond { a } else { b }
}

// Two type parameters: returns only its second argument.
func second<T, U>(a: T, b: U) -> U {
    b
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
    val flag = Cell { value: true }        // Cell<bool> — distinct instance
    val cv = cell.get()                    // generic impl method     -> 7
    val bonus = if flag.get() { p.first } else { 0 }   // -> 6

    // 10 + 10 + 4 + 3 + 5 + 12 + 7 + 6 = 57
    return pick + base + k + t.0 + t.1 + a + cv + bonus
}
