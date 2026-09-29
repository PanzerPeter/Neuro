// Running tensor code on a GPU: the `@gpu` attribute.
//
// `@gpu` pins a function's body to the GPU. The compiler turns each operation in it into
// a kernel, and every call copies the tensor arguments to the device, runs the kernels
// and copies the result back, so callers still pass and receive ordinary tensors. The
// program below computes each result a second time on the host and compares.
//
// BARE `@gpu` REQUIRES A GPU. With no usable NVIDIA GPU the program stops at startup,
// before `main` runs, with a `panic:` line saying why. It never quietly runs the body on
// the CPU instead, and the same rule holds while compiling: a body the GPU path cannot
// lower is a compile error at the function, not a host fallback.
//
// WHAT A `@gpu` BODY MAY CONTAIN: straight-line element-wise `+ - * /` and `@` over
// `f32` / `f64` tensors of static shape and rank 1 or more, with scalar or tensor
// parameters (owned or `&`) and a tensor result. Compiling one needs a `neurc` built
// with the MLIR backend (`--features mlir`).

// A blend of two images of the same size, one kernel per operator.
@gpu
func blend(a: &Tensor<f32, [4, 6]>, b: &Tensor<f32, [4, 6]>, t: f32) -> Tensor<f32, [4, 6]> {
    val scaled = a * t
    scaled + b
}

// A generic function works too: each instance is compiled for the GPU with its own shape.
@gpu
func project<M, N, K>(w: &Tensor<f64, [M, K]>, x: &Tensor<f64, [K, N]>) -> Tensor<f64, [M, N]> {
    w @ x
}

func main() -> i32 {
    val a = Tensor::<f32, [4, 6]>::ones()
    val b = &a * 0.25f32
    val gpu = blend(&a, &b, 3.0f32)
    val host = &a * 3.0f32 + &b
    println("blend: {gpu[0, 0]} on the GPU, {host[0, 0]} on the host")

    val w: Tensor<f64, [2, 3]> = [
        [1.0, 2.0, 3.0],
        [4.0, 5.0, 6.0]
    ]
    val x: Tensor<f64, [3, 2]> = [
        [7.0, 8.0],
        [9.0, 10.0],
        [11.0, 12.0]
    ]
    val p = project(&w, &x)
    println("project: [[{p[0, 0]}, {p[0, 1]}], [{p[1, 0]}, {p[1, 1]}]]")

    mut matches = 0
    if gpu[3, 5] == host[3, 5] { matches += 1 }
    if p[1, 1] == 154.0 { matches += 1 }
    return matches
}
