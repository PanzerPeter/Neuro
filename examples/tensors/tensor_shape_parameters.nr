// Shape parameters: `func f<S>(t: Tensor<f32, S>)`. A bare name written where a whole
// shape goes is a shape parameter. It stands for any number of extents, so one template
// serves a vector, a matrix and a batch alike, and each distinct shape gets its own
// specialization. `..S` splices one into a longer shape: in `[..S, N]` the last extent is
// `N` and `S` takes the leading ones, possibly none. In a body, `S.rank()` and `S.count()`
// read the shape as `u64` values. An operation that needs a rank, such as an index or a
// transpose, needs the axes it reads written in the shape.

// Element-wise arithmetic is valid at every rank, so this needs nothing but `S`.
func leaky<S>(x: &Tensor<f32, S>) -> Tensor<f32, S> {
    x * 0.75 + x * 0.25
}

// The shape read as values.
func describe<S>(x: &Tensor<f32, S>) -> u64 {
    S.count() * 10 + S.rank()
}

// A trailing bias broadcast over any number of leading axes.
func add_bias<S, N>(x: Tensor<f32, [..S, N]>, b: &Tensor<f32, [N]>) -> Tensor<f32, [..S, N]> {
    x + b
}

// A shape parameter is passed on to another generic call, inferred or by turbofish.
func twice<S>(x: &Tensor<f32, S>) -> Tensor<f32, S> {
    leaky(x) + leaky::<S>(x)
}

// A constructor takes its shape from the type, so it builds a tensor of any shape.
func filled<S>(value: f32) -> Tensor<f32, S> {
    Tensor::<f32, S>::ones() * value
}

// A type sizes its fields the same way, and its `impl` reads the same parameter.
struct Gain<S> {
    g: Tensor<f32, S>
}

impl<S> Gain<S> {
    func apply(&self, x: &Tensor<f32, S>) -> Tensor<f32, S> {
        &self.g * x
    }

    func cells(&self) -> u64 {
        S.count()
    }
}

func main() -> i32 {
    val v: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val m: Tensor<f32, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
    val batch = Tensor::<f32, [2, 2, 3]>::ones()

    println("describe(&v)     = {describe(&v)}")
    println("describe(&m)     = {describe(&m)}")
    println("describe(&batch) = {describe(&batch)}")

    println("twice(&m).sum()  = {twice(&m).sum()}")

    val b: Tensor<f32, [3]> = [0.5, 1.0, 1.5]
    val shifted = add_bias(batch, &b)
    println("add_bias sum     = {shifted.sum()}")

    // The turbofish names the shape, since no argument does.
    val half = filled::<[2, 3]>(0.5)
    val gain = Gain { g: half }
    println("gain.apply sum   = {gain.apply(&m).sum()}")

    val small: Gain<[3]> = Gain { g: [2.0, 2.0, 2.0] }
    println("cells            = {gain.cells()} and {small.cells()}")

    return (describe(&m) + small.cells()) as i32
}
