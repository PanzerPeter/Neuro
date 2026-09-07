// Showcase: a weight update written in place, the shape a training step has.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   tensor literal coercion + the construction helpers · a struct holding a
//   tensor field · an enum matched to pick a schedule · a `for` loop · borrowed
//   tensor parameters · string interpolation with the format mini-language ·
//   and 2B's in-place compound assignment, which is the reason the weight
//   buffer keeps one address across every step below.
//
// `w -= g` updates the buffer `w`'s DLPack handle already addresses. Nothing is
// allocated, no handle is replaced, and a pointer held by an optimizer or by a
// foreign DLPack consumer stays valid across the statement. The by-value
// desugaring `w = w - g` would build a second tensor per step and invalidate
// both, which is why the compound operators are their own dispatch.

enum Schedule {
    Constant,
    Frozen
}

// A layer, once its weights have been trained: 2080 parameters across two
// buffers, both moved into the struct.
struct Layer {
    weights: Tensor<f32, [64, 32]>,
    bias: Tensor<f32, [32]>,
    parameters: i32
}

impl Layer {
    func describe(&self) -> string {
        return "layer  [64, 32] + bias [32]"
    }
}

// The step every iteration subtracts. `Frozen` yields a zero step, so a schedule
// is a choice of tensor and the loop below does not change shape with it.
func step_for(schedule: Schedule) -> Tensor<i32, [4]> {
    return match schedule {
        Schedule::Constant => Tensor::<i32, [4]>::ones(),
        Schedule::Frozen => Tensor::<i32, [4]>::zeros()
    }
}

// Adding a value to `i32`'s maximum overflows exactly when that value is
// positive, and adding it to the minimum exactly when it is negative, so a
// program that survives both has proved every element of `diff` is zero. It is
// the only assertion a tensor can carry today: reading an element back needs
// indexing, which is a later 2B item.
func assert_all_zero(diff: &Tensor<i32, [4]>) {
    mut high: Tensor<i32, [4]> = [2147483647, 2147483647, 2147483647, 2147483647]
    high += diff
    mut low: Tensor<i32, [4]> = [-2147483648, -2147483648, -2147483648, -2147483648]
    low += diff
}

func main() -> i32 {
    // Literal coercion: the annotation makes this a tensor and types its leaves.
    mut counters: Tensor<i32, [4]> = [40, 30, 20, 10]
    val step = step_for(Schedule::Constant)

    // Four descent steps against the same borrowed operand. A borrow is read
    // rather than consumed, so one gradient serves every iteration.
    for i in 0..4 {
        counters -= &step
    }

    // The rest of the family, chained on the same buffer.
    val two = Tensor::<i32, [4]>::from([2, 2, 2, 2])
    counters *= &two                            // 72, 52, 32, 12
    counters /= &two                            // 36, 26, 16,  6
    val ten: Tensor<i32, [4]> = [10, 10, 10, 10]
    counters %= &ten                            // 6, 6, 6, 6

    // Subtracting the expectation leaves zeros, and the bracket proves it: a
    // single wrong element aborts the program instead of returning below.
    val expected: Tensor<i32, [4]> = [6, 6, 6, 6]
    counters -= &expected
    assert_all_zero(&counters)
    println("counters [4]    updated in place, every element matched")

    // The same update at a realistic width: 2080 f32 parameters, stepped eight
    // times without a single allocation after the first.
    mut weights = Tensor::<f32, [64, 32]>::random_normal(mean: 0.0f32, std: 0.02f32)
    mut bias = Tensor::<f32, [32]>::zeros()
    val decay = Tensor::<f32, [64, 32]>::zeros()
    val bias_decay = Tensor::<f32, [32]>::zeros()
    for i in 0..8 {
        weights -= &decay
        bias -= &bias_decay
    }

    // The trained buffers move into the layer; they are not copied.
    val layer = Layer { weights: weights, bias: bias, parameters: 2080 }
    val line = layer.describe()
    println("{line:<28}= {layer.parameters:>6} params, one buffer each")

    return layer.parameters % 256
}
