// Tensor shape generics: `func f<M, K>(t: Tensor<f32, [M, K]>)`. A bare name in a tensor
// shape is an extent, a compile-time `u64` value, not a type, inferred from the argument's
// shape. One
// template is specialized per distinct set of extents, so it costs nothing at run time over
// a hand-written version. A parameter that appears twice must have the same extent both
// times, so a shape mismatch is a compile error. A `where` clause states the rest, such as
// `where N > 0`, and is checked at the call that supplies the extent. The shape parameter
// is a value, readable in the body.

// M and K are inferred from the argument. Borrowing reads the tensor without
// consuming it, so the caller keeps its own.

func rank_two_size<M, K>(t: &Tensor<i32, [M, K]>) -> i32 {
    return (M * K) as i32
}

// The extent flows into the RETURN type: dropping the first axis leaves a `[K]` row,
// whatever K turned out to be at this call.
func top_row<M, K>(t: &Tensor<i32, [M, K]>) -> Tensor<i32, [K]> {
    return t[0, ..]
}

// A constraint on the extent itself. `N > 0` is checked at each instantiation, not
// inside the template, so the error names the call that broke it.
func last<N>(t: &Tensor<i32, [N]>) -> i32
where N > 0
{
    return t[N - 1]
}

// One template, one in-place update, any width: `-=` writes into the buffer the
// argument already owns rather than allocating a second one.
func decayed<N>(a: Tensor<i32, [N]>, step: &Tensor<i32, [N]>) -> Tensor<i32, [N]>
where N > 0
{
    mut out = a
    out -= step
    return out
}

// A `const` parameter is a value that sizes nothing: `SCALE` multiplies the extent but
// names no axis. Written in the shape it would be an error whose hint is the bare form.
func scaled_width<K, const SCALE: u32>(t: &Tensor<i32, [K]>) -> u64 {
    return K * (SCALE as u64)
}

func main() -> i32 {
    val wide: Tensor<i32, [2, 3]> = [
        [10, 20, 30],
        [40, 50, 60]
    ]
    val tall: Tensor<i32, [4, 2]> = Tensor::<i32, [4, 2]>::ones()

    // Two shapes, one template: each call compiles to its own specialization.
    println("rank_two_size(&wide) = {rank_two_size(&wide)}")
    println("rank_two_size(&tall) = {rank_two_size(&tall)}")

    // K reaches the return type, so the row's own annotation names it.
    val row: Tensor<i32, [3]> = top_row(&wide)
    println("top_row(&wide)[2]    = {row[2]}")

    // The same constrained template at two widths.
    val pair: Tensor<i32, [2]> = [7, 8]
    println("last(&row)           = {last(&row)}")
    println("last(&pair)          = {last(&pair)}")

    // The in-place update inside a shape-generic body.
    val step: Tensor<i32, [3]> = [1, 2, 3]
    val stepped = decayed(row, &step)
    println("decayed(row)[2]      = {stepped[2]}")

    println("scaled_width::<2, 3> = {scaled_width::<2, 3>(&pair)}")

    return rank_two_size(&wide) + last(&pair) + stepped[2]
}
