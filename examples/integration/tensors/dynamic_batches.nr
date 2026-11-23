// Scoring batches whose size is known only at run time. Integration example: named dynamic
// extents `?B` bound at a call, an annotation, a constructor, a struct literal and a
// `std::safetensors` read, together with a generic `Float` element, `@`, reductions with
// `axis:`, `.topk`, slicing, a closure through `.map`, a `match` on `Option`, `?` over
// `IoError`, and `.as_shape` narrowing back to a static shape.

// One batch of rows and the label of each row. The literal checks the two agree on `B`.
struct Batch {
    rows: Tensor<f32, [?B, 3]>,
    labels: Tensor<i32, [?B]>,
}

// Row `i` is `[i + 1, 2 * (i + 1), 1]`, labelled with `i % 3`.
func make_batch(n: u64) -> Batch {
    mut rows = Tensor::<f32, [?, 3]>::zeros(dims: [n])
    mut labels = Tensor::<i32, [?]>::zeros(dims: [n])
    mut i: u64 = 0
    while i < n {
        rows[i, 0] = (i + 1) as f32
        rows[i, 1] = (2 * (i + 1)) as f32
        rows[i, 2] = 1.0
        labels[i] = (i % 3) as i32
        i += 1
    }
    Batch { rows: rows, labels: labels }
}

// The batch goes through a weights file and back. The read of the rows binds `B` from the
// stored shape, the read of the labels is checked against it, and so is the struct literal.
func reload(batch: &Batch, path: &string) -> Result<Batch, IoError> {
    mut out = std::safetensors::Writer::new()
    out.add("rows", &batch.rows)
    out.add("labels", &batch.labels)
    out.save(path)?
    val file = std::safetensors::open(path)?
    val rows: Tensor<f32, [?B, 3]> = file.tensor("rows")?
    val labels: Tensor<i32, [?B]> = file.tensor("labels")?
    std::fs::remove_file(path)?
    Result::Ok(Batch { rows: rows, labels: labels })
}

// Generic over the element, dynamic over the batch: one body per element type.
func mean_of<T: Float>(x: &Tensor<T, [?B]>) -> T {
    x.mean()
}

// The scores keep the batch name, so the per-row maximum lines up with the labels.
func best_scores(rows: &Tensor<f32, [?B, 3]>, w: &Tensor<f32, [3, 2]>) -> Tensor<f32, [?B]> {
    (rows @ w).max(axis: 1)
}

func summary(batch: &Batch, w: &Tensor<f32, [3, 2]>) -> f32 {
    // A field read sees `?`; the annotation names it again, checked at run time.
    val rows: Tensor<f32, [?B, 3]> = batch.rows.clone()
    val best = best_scores(&rows, w)
    val first_two = rows[0..2, ..].sum()
    println("  B = {B}, best mean = {mean_of(&best)}, first two rows sum to {first_two}")
    val (top, at) = best.topk(k: 2)
    println("  top two scores {top[0]} and {top[1]} at rows {at[0]} and {at[1]}")
    top.sum()
}

func main() -> i32 {
    val w: Tensor<f32, [3, 2]> = [
        [1.0, 0.0],
        [0.0, 1.0],
        [0.5, 0.5]
    ]
    mut total: f32 = 0.0
    mut n: u64 = 2
    while n <= 4 {
        println("batch of {n}:")
        val made = make_batch(n)
        val batch = reload(&made, "neuro_example_batch.safetensors") ?? panic("cannot reload the batch")
        total += summary(&batch, &w)
        n += 1
    }
    println("total of the top scores = {total}")

    // A batch of exactly three rows narrows to a static shape; any other size does not.
    val three = make_batch(3)
    val labelled = three.labels.map(|l: i32| -> i32 { l * 10 })
    val narrowed = match labelled.as_shape::<[3]>() {
        Some(fixed) => fixed[2],
        None => -1,
    }
    println("third label times ten = {narrowed}")
    return total as i32 + narrowed
}
