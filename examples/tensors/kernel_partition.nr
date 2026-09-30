// Writing from a GPU kernel so that no two threads touch one element: `out.partition(...)`.
//
// `out[i] = v` in a `@kernel` writes wherever `i` says, and nothing stops two threads
// from writing the same element. `out.partition(|base, slice| { ... })` instead hands
// each thread its own run of `out`, so no two threads can meet. The grid runs one thread
// per element of the kernel's first `KernelOut` (the grid tensor). Partitioning a tensor
// with `k` times as many elements gives each of those threads `k` of them, in row-major
// order: `slice` is the run, indexed from 0 up to `slice.len()`, and `base` is the
// row-major position of its first element. A thread in the overhang past the grid
// tensor's edge gets no run, so the body needs no `row < M` guard.
//
// `.flat(i)` reads any tensor at row-major position `i`, whatever its rank. That is how
// a run's own index turns back into the input element it sits over: `base + i`.
//
// The element count has to divide evenly among the grid's threads, and the compiler
// refuses a partition where it does not. Like any kernel, this needs a GPU and a
// `neurc` built with `--features mlir`.

// One thread per row of `m`: the grid tensor is `totals`, 6 rows in blocks of 4, so the
// last 2 threads of the second block own nothing. A thread's run of `scaled` is its
// row's 5 elements, and its run of `totals` is its one total.
@kernel(threads: [4])
func scale_rows(m: Tensor<f32, [6, 5]>, factor: Tensor<f32, [6]>, totals: KernelOut<Tensor<f32, [6]>>, scaled: KernelOut<Tensor<f32, [6, 5]>>) {
    scaled.partition(|base, row| {
        val f = factor.flat(base / row.len())
        for i in 0u64..row.len() {
            row[i] = m.flat(base + i) * f
        }
    })
    totals.partition(|base, total| {
        mut sum = 0.0f32
        for k in 0u64..5u64 {
            sum += m.flat(base * 5u64 + k)
        }
        total[0] = sum * factor.flat(base)
    })
}

func main() -> i32 {
    mut m: Tensor<f32, [6, 5]> = Tensor::zeros()
    mut factor: Tensor<f32, [6]> = Tensor::zeros()
    for i in 0..6 {
        factor[i] = (i + 1) as f32
        for j in 0..5 {
            m[i, j] = (i * 5 + j) as f32
        }
    }
    mut totals: Tensor<f32, [6]> = Tensor::zeros()
    mut scaled: Tensor<f32, [6, 5]> = Tensor::zeros()
    scale_rows(m, factor, &mut totals, &mut scaled)
    println("row 0: {scaled[0, 0]} {scaled[0, 4]}, total {totals[0]}")
    println("row 5: {scaled[5, 0]} {scaled[5, 4]}, total {totals[5]}")

    // Every element checked against the same work done on the host.
    mut agree = 0
    for i in 0..6 {
        mut sum = 0.0f32
        for j in 0..5 {
            if scaled[i, j] == m[i, j] * factor[i] { agree += 1 }
            sum += m[i, j]
        }
        if totals[i] == sum * factor[i] { agree += 1 }
    }
    return agree
}
