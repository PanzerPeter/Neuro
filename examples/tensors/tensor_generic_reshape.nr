// Shape-generic reshapes and reductions. A generic body is checked once, before its extents
// have values, so `.reshape`, `.flatten` and the reductions over extent parameters are typed
// symbolically: `[N, 2]` flattens to `[2 * N]`, and a shape may write a constant expression
// over its parameters, `[B, H, D / H]`. Whether the element counts agree is answered at each
// instance, and a mismatch is reported at the call that supplied the extents. A `where`
// clause states the requirement in the signature instead, where a reader sees it.

// Split each row into `H` heads of `D / H` features. The predicate is what a caller must
// meet; without it, `split::<2, 6, 4>` would be refused at its call all the same.
func split<B, D, H>(x: Tensor<f32, [B, D]>) -> Tensor<f32, [B, H, D / H]>
where D % H == 0
{
    x.reshape([B, H, D / H])
}

// The same split with the head width inferred by `-1`: its type is the same `D / H`.
func split_inferred<B, D, H>(x: Tensor<f32, [B, D]>) -> Tensor<f32, [B, H, D / H]>
where D % H == 0
{
    x.reshape([B, H, -1])
}

// Merging the heads again is a flatten of the trailing two axes.
func merge<B, H, W>(x: Tensor<f32, [B, H, W]>) -> Tensor<f32, [B, H * W]> {
    x.flatten(dims: [1, 2])
}

// The largest feature of each head: a reduction along the last axis at any extent.
func head_peaks<B, H, W>(x: &Tensor<f32, [B, H, W]>) -> Tensor<f32, [B, H]> {
    x.max(axis: -1)
}

// A shape parameter flattens to its element count, `S.count()`, and a batch of rows of any
// rank folds into one leading axis.
func cells<S>(x: Tensor<f32, S>) -> Tensor<f32, [S.count()]> {
    x.flatten()
}

func as_rows<S, N>(x: Tensor<f32, [..S, N]>) -> Tensor<f32, [S.count(), N]> {
    x.reshape([-1, N])
}

func main() -> i32 {
    val x: Tensor<f32, [2, 6]> = [[0.0, 1.0, 2.0, 3.0, 4.0, 5.0], [6.0, 7.0, 8.0, 9.0, 10.0, 11.0]]

    // Three heads of two features, then two heads of three.
    val three = split::<2, 6, 3>(x.clone())
    val two = split_inferred::<2, 6, 2>(x.clone())
    println("peaks of 3 heads = {head_peaks(&three).sum()}")
    println("peaks of 2 heads = {head_peaks(&two).sum()}")

    // Splitting and merging gives the rows back.
    val back = merge(split::<2, 6, 3>(x.clone()))
    println("merged row 1     = {back[1, ..].sum()}")

    val batch = Tensor::<f32, [2, 3, 4]>::ones()
    println("cells            = {cells(batch.clone()).sum()}")
    val rows = as_rows(batch)
    println("rows             = {rows.sum(axis: 1).sum()} over {rows.sum(axis: 0).sum()}")

    return head_peaks(&three).sum() as i32
}
