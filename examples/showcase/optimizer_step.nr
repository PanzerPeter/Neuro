// Showcase: a weight update written in place, the shape a training step has.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   tensor literal coercion + the construction helpers · a struct holding a
//   tensor field · an enum matched to pick a schedule · a `for` loop · borrowed
//   tensor parameters · string interpolation with the format mini-language ·
//   2B's in-place compound assignment, which is the reason the weight buffer
//   keeps one address across every step below · 2B's slicing and indexing,
//   which is what finally reads the updated elements back out · and 2B's shape
//   generics, which let the step below be written ONCE for every width a layer
//   in this program has.
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

// One descent step, written once for every width in the program. `N` is a
// compile-time value inferred from the operands' own shape, so `descend` below is
// specialized per width with nothing left to check at run time — and passing two
// tensors of DIFFERENT widths to it is a compile error, not a silent misalignment.
// `where N > 0` rules out the rank-1 tensor with no elements to step.
func descend<N>(w: Tensor<i32, [N]>, step: &Tensor<i32, [N]>) -> Tensor<i32, [N]>
where N > 0
{
    // The update is in place: `w`'s buffer is written, not replaced, exactly as it
    // is in the concrete loop below.
    mut out = w
    out -= step
    return out
}

// The extent reaches the RETURN type too: dropping the leading axis of an `[M, K]`
// matrix leaves a `[K]` row at whatever width K was inferred to be.
func first_row<M, K>(m: &Tensor<i32, [M, K]>) -> Tensor<i32, [K]> {
    return m[0, ..]
}

// Every element read back and compared, one axis position at a time. Indexing a
// borrowed tensor reads it without consuming it, so the caller still owns the
// counters afterwards and the same tensor can be checked twice.
func count_zeros(diff: &Tensor<i32, [4]>) -> i32 {
    mut zeros = 0
    for i in 0..4 {
        if diff[i] == 0 {
            zeros = zeros + 1
        }
    }
    return zeros
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

    // Subtracting the expectation leaves zeros, and every element says so.
    val expected: Tensor<i32, [4]> = [6, 6, 6, 6]
    counters -= &expected
    val matched = count_zeros(&counters)
    // The exit code below is an assertion, not a report: one wrong element stops the
    // program here rather than letting it return a success code.
    assert(matched == 4)
    println("counters [4]    updated in place, {matched} of 4 elements matched")

    // One template, two widths. The `[4]` counters and a `[2]` pair below both go
    // through `descend`, which the compiler specializes per extent; the borrowed
    // step keeps its own operand alive across the call.
    val four: Tensor<i32, [4]> = [1, 1, 1, 1]
    val four_step: Tensor<i32, [4]> = [1, 1, 1, 1]
    val cleared = descend(four, &four_step)
    val pair: Tensor<i32, [2]> = [9, 9]
    val pair_step: Tensor<i32, [2]> = [4, 5]
    val trimmed = descend(pair, &pair_step)
    assert(cleared[3] == 0)
    assert(trimmed[1] == 4)
    println("descend<N>      one template, [4] -> {cleared[3]} and [2] -> {trimmed[1]}")

    // A shape parameter inferred from a rank-2 tensor and handed back in the result's
    // type: the grid is [2, 3], so its first row is a [3].
    val grid: Tensor<i32, [2, 3]> = [
        [3, 6, 9],
        [1, 1, 1]
    ]
    val row: Tensor<i32, [3]> = first_row(&grid)
    assert(row[2] == 9)
    println("first_row<M, K> [2, 3] -> a [3] row ending {row[2]}")

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

    // A row of the weight matrix, read out by slicing: an axis given a position is
    // dropped, so `weights[0, ..]` is a rank-1 tensor of the first neuron's 32
    // inputs, and the eight decay steps above left every one of them alone.
    val first_neuron: Tensor<f32, [32]> = weights[0, ..]
    val unchanged = first_neuron[7] == weights[0, 7]
    assert(unchanged)
    println("weights[0, ..]  is a [32] row, stable across the steps: {unchanged}")

    // A sub-block of the same matrix, both axes sliced. The slice is a copy of its
    // own, so it outlives the moves below.
    val block: Tensor<f32, [2, 2]> = weights[0..2, 0..=1]
    val diagonal_matches = block[1, 1] == weights[1, 1]
    assert(diagonal_matches)
    println("weights[0..2, 0..=1] is a [2, 2] block, corner matches: {diagonal_matches}")

    // The trained buffers move into the layer; they are not copied.
    val layer = Layer { weights: weights, bias: bias, parameters: 2080 }
    val line = layer.describe()
    println("{line:<28}= {layer.parameters:>6} params, one buffer each")

    return layer.parameters % 256
}
