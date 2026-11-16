// Tensor element generics: `func f<T: Float>(x: Tensor<T, [N]>)`. The element type is a type
// parameter, and its bound says what the body may do with it. `Numeric` gives arithmetic,
// comparisons and `T::zero()` / `T::one()`; `Integer` adds `%` and the bitwise operators;
// `Float` adds the math methods, `.is_nan()` and `T::from_f64(x)`. An unbounded `T` still
// moves and indexes. The body is checked once against the bound and compiled once per
// element type it is called with.

// A float literal takes the parameter's type, as it takes `f32` beside an `f32`.
func lerp<T: Float>(a: T, b: T, t: T) -> T {
    a + (b - a) * t
}

// Any numeric element: one dense layer, `x @ w + b`, at whatever type the weights hold.
struct Dense<T> {
    w: Tensor<T, [3, 2]>,
    b: Tensor<T, [1, 2]>,
}

impl<T: Numeric> Dense<T> {
    func forward(&self, x: &Tensor<T, [1, 3]>) -> Tensor<T, [1, 2]> {
        x @ &self.w + &self.b
    }
}

// Root mean square with a small guard against zero. `T::from_f64` builds the guard in the
// element's own type, rounded once.
func rms<T: Float>(x: &Tensor<T, [4]>) -> T {
    ((x * x).mean() + T::from_f64(1e-6)).sqrt()
}

// An integer element: `%` and `&` are the `Integer` bound's.
func odd_count<T: Integer>(x: &Tensor<T, [4]>) -> T {
    mut count = T::zero()
    for i in 0..4 {
        count += x[i] & T::one()
    }
    count % 5
}

// No bound: the body only moves and indexes, so any element type will do.
func last<T>(x: Tensor<T, [4]>) -> T {
    x[3]
}

func main() -> i32 {
    println("lerp f64        = {lerp(0.0, 10.0, 0.25)}")
    println("lerp bf16       = {lerp(1.0bf16, 3.0bf16, 0.5bf16)}")

    val wide = Dense {
        w: Tensor::<f32, [3, 2]>::from([[1.0, 0.0], [0.0, 1.0], [1.0, 1.0]]),
        b: Tensor::<f32, [1, 2]>::from([[0.5, -0.5]]),
    }
    val exact = Dense {
        w: Tensor::<i64, [3, 2]>::from([[1, 0], [0, 1], [1, 1]]),
        b: Tensor::<i64, [1, 2]>::from([[10, 20]]),
    }
    val y = wide.forward(&Tensor::<f32, [1, 3]>::from([[1.0, 2.0, 3.0]]))
    val z = exact.forward(&Tensor::<i64, [1, 3]>::from([[1, 2, 3]]))
    println("forward f32     = [{y[0, 0]}, {y[0, 1]}]")
    println("forward i64     = [{z[0, 0]}, {z[0, 1]}]")

    val h = Tensor::<f16, [4]>::from([1.0, -1.0, 2.0, -2.0])
    println("rms f16         = {rms(&h)}")
    val counts = Tensor::<u8, [4]>::from([3, 8, 5, 7])
    println("odd_count u8    = {odd_count(&counts)}")
    val flags = Tensor::<bool, [4]>::from([false, false, true, true])
    println("last bool       = {last(flags)}")

    z[0, 1] as i32
}
