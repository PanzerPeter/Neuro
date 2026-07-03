// Showcase — generic functions working together with prior features.
//
// Cumulative integration example combining:
//   generic functions with monomorphization (§3.8)
//   · enums with associated data + pattern matching (§3.5, §3.6)
//   · tuples as generic type arguments (§3.2)
//   · if-expressions and implicit returns
//
// Each generic function below is a single template; the compiler emits one
// specialized copy per concrete type argument, so `T` costs nothing at runtime.

enum Shape {
    Circle(i32),
    Rect(i32, i32),
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

    // 10 + 10 + 4 + 3 + 5 + 12 = 44
    return pick + base + k + t.0 + t.1 + a
}
