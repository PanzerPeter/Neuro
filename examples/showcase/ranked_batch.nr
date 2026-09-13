// Showcase: ordering a tensor axis alongside the rest of the tensor surface.
//
// Cumulative integration example combining:
//   `.sort()` / `.argsort()` / `.topk(k:, axis:)` on a tensor
//   · named dimensions (`[batch: 3, classes: 4]`) · tensor literal coercion
//   · reductions `.max(axis:)` / `.sum()` / `.mean()` · tensor slicing and indexing
//   · `.t()` shape manipulation · in-place `+=` on a tensor
//   · a struct holding a tensor field with an `&self` method
//   · `for` over a range and over a reversed range · string interpolation
//
// The point of the combination: an argsort index is an ordinary integer, so it reads
// BACK into the tensor that produced it and into anything indexed the same way. That is
// what turns "which class scored highest" into a label lookup, which a reduction alone
// cannot do: `.max()` answers WHAT the best score was and never WHERE it was.
//
// Expected exit code: 130

// A scoreboard owns its logits and reads them without giving them up.
struct Batch {
    logits: Tensor<f64, [batch: 3, classes: 4]>
}

impl Batch {
    // A selection reads its receiver, so `&self` is enough: the field is never moved.
    func winners(&self) -> Tensor<i32, [batch: 3]> {
        val ranked: Tensor<i32, [batch: 3, classes: 4]> = self.logits.argsort(descending: true)
        // Column 0 of a descending argsort is the best class for each row.
        ranked[.., 0]
    }

    func peak(&self) -> Tensor<f64, [batch: 3]> {
        self.logits.max(axis: classes)
    }

    // A confidence margin: how far the best score beats the runner-up, per row. It needs
    // the two best of each row, which is what `.topk` selects and what no single
    // reduction can answer.
    func margin(&self) -> f64 {
        val (top, _) = self.logits.topk(k: 2, axis: classes)
        mut total = 0.0
        for row in 0..3 {
            total = total + (top[row, 0] - top[row, 1])
        }
        total / 3.0
    }

    // A shape cast CONSUMES its receiver, so the field is cloned rather than moved out of
    // the struct that owns it: transposing makes the classes the rows, and the same
    // default axis then orders each class ACROSS the batch.
    func by_class(&self) -> Tensor<f64, [classes: 4, batch: 3]> {
        self.logits.clone().t().sort()
    }
}

func main() -> i32 {
    val batch = Batch {
        logits: [
            [0.5, 2.5, 1.0, 0.25],
            [3.0, 0.5, 0.75, 1.5],
            [1.25, 1.0, 0.5, 4.0]
        ]
    }

    val picks: Tensor<i32, [batch: 3]> = batch.winners()
    val peaks: Tensor<f64, [batch: 3]> = batch.peak()
    for row in 0..3 {
        val at = row as u64
        println("row {row}: class {picks[at]} at {peaks[at]:.2}")
    }

    println("mean margin = {batch.margin():.3}")

    val sorted: Tensor<f64, [classes: 4, batch: 3]> = batch.by_class()
    println("class 0 across the batch = {sorted[0, 0]:.2}, {sorted[0, 1]:.2}, {sorted[0, 2]:.2}")

    // Counting down, so the last class is reported first. An argsort pick is an
    // ordinary integer, so it compares against a class number directly.
    for class in (0..4).rev() {
        mut rows: i32 = 0
        for row in 0..3 {
            val at = row as u64
            if picks[at] == class {
                rows = rows + 1
            }
        }
        println("class {class}: {rows} row(s)")
    }

    // In-place `+=` on the reduction's own result: every peak is lifted by the same
    // floor, in the buffer `.max(axis:)` already allocated.
    mut lifted: Tensor<f64, [batch: 3]> = batch.peak()
    val floor: Tensor<f64, [batch: 3]> = [0.5, 0.5, 0.5]
    lifted += floor
    println("lifted peaks = {lifted[0]:.2}, {lifted[1]:.2}, {lifted[2]:.2}")

    // A whole-tensor reduction over each piece, folded into one exit code.
    val total = batch.logits.sum()
    val mean = batch.logits.mean()
    println("total = {total:.2}, mean = {mean:.3}, lifted = {lifted.sum():.2}")

    val picked = picks[0] * 100 + picks[1] * 10 + picks[2]
    println("picked = {picked}")
    picked + (lifted.sum() as i32) + (total as i32)
}
