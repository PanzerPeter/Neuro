// Writing a GPU kernel by hand: the `@kernel` attribute.
//
// Where `@gpu` takes whole-tensor arithmetic and turns each operation into a kernel for
// you, `@kernel(threads: [...])` runs the body you write once per GPU thread. The
// compiler launches one thread per element of the first `KernelOut` parameter, in
// blocks shaped `threads`. Inside the body, `thread_id` is the thread's position in that
// grid and `block_id` its block's, each read one axis at a time: `.x`, `.y`, `.z`.
//
// The grid rounds up to whole blocks, so a 37 x 45 output in 16 x 16 blocks gets a 3 x 3
// grid of 48 x 48 threads, and the threads past the edge have no element. That is why
// the body checks `row < 37 && col < 45` before it writes. Every index is also checked
// at run time: a thread that reads or writes past an extent stops the program with a
// `panic:`, as an out-of-range index does on the CPU.
//
// WHAT A KERNEL TAKES: tensors it reads as `Tensor`, tensors it writes as
// `KernelOut<Tensor>`, and numbers or `bool`s by value. It returns nothing; what it
// computes is what it writes. The call borrows every tensor: pass an input as `a` (you
// still own it afterwards) and an output as `&mut r`. Inside the body an output is only
// ever indexed, and the index, read or write, goes in an `unsafe { }` block:
// `unsafe { out[i] = v }`. The compiler cannot prove that no other thread writes that
// element, so `unsafe` marks where you vouch for it. The handle cannot be stored, passed on or returned.
// The body may use locals, arithmetic, casts, `if`, `while`, `loop`, `for` over a range,
// `break`, `continue` and `return`, and tensor elements. It cannot call functions yet.
//
// A KERNEL REQUIRES A GPU. With no usable NVIDIA GPU the program stops at startup, before
// `main` runs. Compiling one needs a `neurc` built with the MLIR backend (`--features mlir`).

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
