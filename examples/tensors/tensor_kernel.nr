// Writing a GPU kernel by hand: the `@kernel` attribute. `@kernel(threads: [...])` runs the
// body once per GPU thread, one thread per element of the first `KernelOut` parameter, in
// blocks shaped `threads`. `thread_id` and `block_id` give a thread's position, read one
// axis at a time with `.x`, `.y` and `.z`. The grid rounds up to whole blocks, so threads
// past the edge have no element and the body checks its bounds before it writes. Every
// index is checked at run time: an out-of-range access stops the program with a `panic:`.
// Tensors are passed as `Tensor` (read) or `KernelOut<Tensor>` (written), and numbers or
// `bool`s by value. The call borrows every tensor, so pass an input as `a` and an output as
// `&mut r`. An output is only ever indexed, inside `unsafe { }`, because the compiler
// cannot prove that no other thread writes that element. The body may use locals,
// arithmetic, casts, control flow and tensor elements, but not function calls. A kernel
// needs a GPU and a Linux `neurc`.

// One thread per output element: add, then clamp at zero.

@kernel(threads: [16, 16])
func add_relu(a: Tensor<f32, [37, 45]>, b: Tensor<f32, [37, 45]>, out: KernelOut<Tensor<f32, [37, 45]>>) {
    val row = thread_id.x
    val col = thread_id.y
    if row < 37 && col < 45 {
        val sum = a[row, col] + b[row, col]
        // SAFETY: thread (row, col) is the only one to write out[row, col].
        unsafe { out[row, col] = if sum > 0.0 { sum } else { 0.0 } }
    }
}

// One thread per row, looping over its columns; a second output records which block
// ran each row, 4 rows to a block.
@kernel(threads: [4])
func row_max(m: Tensor<i32, [10, 6]>, out: KernelOut<Tensor<i32, [10]>>, block_of: KernelOut<Tensor<u32, [10]>>) {
    val row = thread_id.x
    if row >= 10 {
        return
    }
    mut best = m[row, 0]
    for col in 1..6 {
        if m[row, col] > best {
            best = m[row, col]
        }
    }
    // SAFETY: each thread writes only its own row's slot of both outputs.
    unsafe {
        out[row] = best
        block_of[row] = block_id.x
    }
}

func main() -> i32 {
    mut a: Tensor<f32, [37, 45]> = Tensor::zeros()
    for i in 0..37 {
        for j in 0..45 {
            a[i, j] = (i - j) as f32
        }
    }
    val b = Tensor::<f32, [37, 45]>::ones() * 0.5f32
    mut clamped: Tensor<f32, [37, 45]> = Tensor::zeros()
    add_relu(a, b, &mut clamped)
    println("add_relu: [0, 3] = {clamped[0, 3]}, [36, 0] = {clamped[36, 0]}")

    mut m: Tensor<i32, [10, 6]> = Tensor::zeros()
    for i in 0..10 {
        for j in 0..6 {
            m[i, j] = (i * 7 + j * 5) % 11
        }
    }
    mut maxima: Tensor<i32, [10]> = Tensor::zeros()
    mut blocks: Tensor<u32, [10]> = Tensor::zeros()
    row_max(m, &mut maxima, &mut blocks)
    println("row_max: row 0 = {maxima[0]}, row 9 = {maxima[9]}, row 9 ran in block {blocks[9]}")

    // Both results checked against the same work done on the host.
    mut agree = 0
    if clamped[36, 0] == 36.5f32 && clamped[0, 3] == 0.0f32 { agree += 1 }
    mut host_max = m[9, 0]
    for j in 1..6 {
        if m[9, j] > host_max { host_max = m[9, j] }
    }
    if maxima[9] == host_max && blocks[9] == 2u32 { agree += 1 }
    return agree
}
