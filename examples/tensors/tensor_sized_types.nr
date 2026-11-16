// Types sized by bare names: `struct Dense<IN, OUT>`. A generic name a struct uses as a tensor
// extent or an array length is a size parameter, a compile-time `u64`, exactly as in a
// function signature. `impl<IN, OUT> Dense<IN, OUT>` passes the names on to the struct's sizes,
// so they are sizes there too, and the method bodies read them as values. Each distinct
// `Dense<3, 2>` is its own specialized struct. A `const NAME: T` parameter is kept for a value
// that sizes nothing; writing one as an extent is an error whose hint is the bare name.

// One fully connected layer: `IN` inputs, `OUT` outputs.
struct Dense<IN, OUT> {
    w: Tensor<f32, [IN, OUT]>,
    b: Tensor<f32, [OUT]>,
}

impl<IN, OUT> Dense<IN, OUT> {
    // The extents flow into the signature, so a layer only accepts a row of its own width,
    // and the result has its own output width.
    func forward(&self, x: &Tensor<f32, [1, IN]>) -> Tensor<f32, [1, OUT]> {
        x @ &self.w + &self.b
    }

    // `IN` and `OUT` read as `u64` values in the body.
    func parameters(&self) -> u64 {
        IN * OUT + OUT
    }
}

// An array length sizes a struct the same way: `CAP` is inferred from the literal.
struct History<CAP> {
    seen: [i32; CAP],
}

impl<CAP> History<CAP> {
    func capacity(&self) -> u64 {
        CAP
    }
}

func main() -> i32 {
    val hidden: Dense<3, 2> = Dense {
        w: [[1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
        b: [0.5, -0.5],
    }
    val head: Dense<2, 1> = Dense { w: [[1.0], [2.0]], b: [0.0] }

    // `hidden`'s `OUT` and `head`'s `IN` are both 2, so the two layers chain. A `Dense<3, 1>`
    // in place of `head` would be a compile error on the shared extent.
    val x: Tensor<f32, [1, 3]> = [[1.0, 2.0, 3.0]]
    val h = hidden.forward(&x)
    val y = head.forward(&h)
    println("hidden(x) = [{h[0, 0]}, {h[0, 1]}]")
    println("head(h)   = {y[0, 0]}")

    val total = hidden.parameters() + head.parameters()
    println("parameters: {hidden.parameters()} + {head.parameters()} = {total}")

    val history = History { seen: [4, 1, 3, 2] }
    println("history holds {history.capacity()}")

    return y[0, 0] as i32 + total as i32 + history.capacity() as i32
}
