// Showcase: a weight update written in place, the shape a training step has.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   tensor literal coercion + the construction helpers · a struct holding a
//   tensor field · an enum matched to pick a schedule · a `for` loop · borrowed
//   tensor parameters · string interpolation with the format mini-language ·
//   2B's in-place compound assignment, which is the reason the weight buffer
//   keeps one address across every step below · 2B's slicing and indexing,
//   which is what finally reads the updated elements back out · 2B's shape
//   generics, which let the step below be written ONCE for every width a layer
//   in this program has · 2C's by-value tensor operators with their
//   broadcast shape rules, which give the same step its allocating form · and
//   2C's `@` operator, which turns those weights into an actual forward pass.
//
// `w -= g` updates the buffer `w`'s DLPack handle already addresses. Nothing is
// allocated, no handle is replaced, and a pointer held by an optimizer or by a
// foreign DLPack consumer stays valid across the statement. The by-value
// desugaring `w = w - g` would build a second tensor per step and invalidate
// both, which is why the compound operators are their own dispatch.
//
// The last two blocks are where the sub-phase's own features meet. `@` contracts
// an axis rather than walking one, so a `[4, 64]` batch against the `[64, 32]`
// weight matrix gives `[4, 32]` activations — and the bias that is then added to
// them is a `[32]` row broadcast down all four, which is the element-wise rule
// from the block above doing its job on the matrix product's result.
//
// 2C's by-value operators are the other half of that pair, and the block below is
// where the two meet: `&w - &(lr * g)` ALLOCATES a fresh tensor and
// reads two borrows, which is exactly what makes it the wrong thing to run per
// step and the right thing to compute a candidate with. Its operands broadcast:
// a scalar learning rate stretches across every element, and a per-neuron bias
// row stretches down the rows of the matrix it is added to.

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

    // 2C's by-value operators, at the same width. A scalar broadcasts across every
    // element, so `lr * gradient` is the scaled step written the way it is on paper; the
    // result is a fresh tensor, which is why both operands can be borrows and why
    // neither `base` nor `gradient` is consumed here. Every value below is exactly
    // representable, so the equalities are equalities and not tolerances.
    val lr = 0.5f32
    val base = Tensor::<f32, [64, 32]>::ones()
    val gradient = Tensor::<f32, [64, 32]>::ones()
    val scaled = lr * gradient
    val candidate = &base - &scaled
    val moved_by = base[0, 0] - candidate[0, 0]
    assert(moved_by == 0.5f32)
    println("by value       lr * grad moved element [0, 0] by {moved_by}")

    // A rank-1 row aligns at the TRAILING axis and repeats down the leading one, so a
    // [32] bias reaches all 64 neurons of a [64, 32] matrix from one row of numbers.
    // Column 1 is the only one the bias moves, and it moves in every row alike.
    val offsets: Tensor<f32, [32]> = [
        0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0
    ]
    val biased = &candidate + &offsets
    assert(biased[0, 1] - biased[0, 0] == 1.0f32)
    assert(biased[63, 1] - biased[63, 0] == 1.0f32)
    assert(biased[0, 2] == candidate[0, 2])
    println("broadcast      a [32] bias reached row 0 and row 63 alike")

    // A forward pass, at the width the layer actually has. `@` contracts the 64
    // inputs away: a [4, 64] batch against the [64, 32] weights is [4, 32]
    // activations, one row per sample. Every weight is 0.5 and every input 1.0, so
    // each activation is 64 * 0.5 exactly, and the equality below is an equality.
    val batch = Tensor::<f32, [4, 64]>::ones()
    val activations = &batch @ &candidate
    assert(activations[0, 0] == 32.0f32)
    assert(activations[3, 31] == 32.0f32)
    println("batch @ w      [4, 64] @ [64, 32] -> [4, 32], every unit at {activations[0, 0]}")

    // The bias is the [32] row from above, broadcast down all four sample rows: the
    // element-wise rule applied to what `@` just produced, which is the whole shape
    // of a linear layer.
    val shifted = &activations + &offsets
    assert(shifted[0, 1] - shifted[0, 0] == 1.0f32)
    assert(shifted[3, 1] - shifted[3, 0] == 1.0f32)
    println("+ bias         a [32] row reached all 4 sample rows alike")

    // The third broadcast form, and the one the other two do not cover: an extent of 1
    // STRETCHES. A [4, 1] per-sample weight is a column of four numbers, and it reaches
    // all 32 activations of its own row while leaving the other rows to their own factor.
    // The [32] bias above went the other way, down the rows; this goes across them.
    val sample_weights: Tensor<f32, [4, 1]> = [[1.0], [2.0], [3.0], [4.0]]
    val weighted = &shifted * &sample_weights
    assert(weighted[0, 0] == shifted[0, 0])
    assert(weighted[1, 0] == shifted[1, 0] * 2.0f32)
    assert(weighted[3, 31] == shifted[3, 31] * 4.0f32)
    println("* [4, 1]       a stretched axis scaled each sample row on its own")

    // The trained buffers move into the layer; they are not copied.
    val layer = Layer { weights: weights, bias: bias, parameters: 2080 }
    val line = layer.describe()
    println("{line:<28}= {layer.parameters:>6} params, one buffer each")

    return layer.parameters % 256
}
