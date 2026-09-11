// Showcase: a batch of token embeddings, with every tensor axis named.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   named tensor dimensions · structs holding tensor fields · a trait with an
//   `impl` per layer kind · shape-generic functions whose extents are inferred
//   from the argument · tensor slicing, which keeps the surviving axis's name ·
//   in-place `+=` on a tensor · a fixed-size array walked by `for`-in · string
//   interpolation with the format mini-language.
//
// The point of the names is the signature: `[batch: 2, seq: 3, embed: 4]` says
// which axis is which, and a caller that hands over a `[seq: .., batch: ..]`
// tensor is a compile error rather than a silently transposed batch. Names are
// checked only where both sides write one, so `row_sum` below takes an unnamed
// `[N]` and accepts a named `[embed: 4]` row without ceremony.

trait Stage {
    func label(&self) -> string
    func width(&self) -> i32
}

// The embedding table: one row per token in the vocabulary.
struct Embedding {
    table: Tensor<i32, [vocab: 4, embed: 4]>
}

impl Stage for Embedding {
    func label(&self) -> string {
        return "embedding [vocab: 4, embed: 4]"
    }

    func width(&self) -> i32 {
        return 4
    }
}

// A projection whose two axes are named for what they connect.
struct Projection {
    weights: Tensor<i32, [embed: 4, hidden: 2]>
}

impl Stage for Projection {
    func label(&self) -> string {
        return "projection [embed: 4, hidden: 2]"
    }

    func width(&self) -> i32 {
        return 2
    }
}

// Shape-generic and axis-agnostic: the extent is inferred, and the unnamed `[N]`
// accepts a row whose axis the caller has named.
func row_sum<N>(row: &Tensor<i32, [N]>) -> i32
where N > 0
{
    mut total = 0
    // The extent is a `const N: u32`, so the range it drives is cast to the `i32`
    // the running total is written in.
    for i in 0..(N as i32) {
        total = total + row[i]
    }
    return total
}

// The names make the axis order part of the signature: a `[seq, batch, embed]`
// tensor cannot be passed here even though it has the same three extents.
func batch_total(batch: &Tensor<i32, [batch: 2, seq: 3, embed: 4]>) -> i32 {
    mut total = 0
    for b in 0..2 {
        for s in 0..3 {
            // Two positions drop two axes, so this slice is the `[embed: 4]`
            // vector for one token, and `row_sum` reads it by borrow.
            val token: Tensor<i32, [embed: 4]> = batch[b, s, ..]
            total = total + row_sum(&token)
        }
    }
    return total
}

func main() -> i32 {
    val embedding = Embedding {
        table: Tensor::<i32, [vocab: 4, embed: 4]>::ones()
    }
    val projection = Projection {
        weights: Tensor::<i32, [embed: 4, hidden: 2]>::ones()
    }

    // A tensor built without names is the same type as one that has them, so the
    // annotation may name the axes of a value the constructor left anonymous.
    val batch: Tensor<i32, [batch: 2, seq: 3, embed: 4]> = Tensor::<i32, [2, 3, 4]>::ones()

    val embedding_line = embedding.label()
    println("{embedding_line:<34}width = {embedding.width()}")
    val projection_line = projection.label()
    println("{projection_line:<34}width = {projection.width()}")

    // Slicing keeps the name of every axis that survives: dropping `vocab` leaves
    // the `[embed: 4]` row that `row_sum` then folds.
    val first_row: Tensor<i32, [embed: 4]> = embedding.table[0, ..]
    println("embedding row 0 sum               = {row_sum(&first_row):>4}")

    // `+=` writes into the buffer `counts` already owns: no reallocation, and the
    // named axis is unchanged by the update.
    mut counts: Tensor<i32, [hidden: 2]> = [10, 20]
    val bump: Tensor<i32, [hidden: 2]> = [1, 2]
    counts += bump
    println("counts after bump                 = {counts[0]:>4}{counts[1]:>4}")

    val widths = [4, 2]
    mut declared = 0
    for width in widths {
        declared = declared + width
    }

    val total = batch_total(&batch)
    println("batch total over every token      = {total:>4}")
    println("declared widths                   = {declared:>4}")

    return total + declared + counts[1] + row_sum(&first_row)
}
