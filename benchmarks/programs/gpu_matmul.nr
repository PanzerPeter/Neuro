// Dense matrix product on the GPU: `@` in a `@gpu` function over two [2048, 2048] f32
// tensors, five rounds, with one element of the left operand bumped between rounds so
// no round can be hoisted. The right operand moves to the GPU once; the left one is
// staged each call. Each product comes back to the host for the checksum, which walks
// every element. Every element is a small integer, so each sum is exact in f32 and
// matches NumPy's and PyTorch's to the digit. Needs an NVIDIA GPU; the harness skips
// the row without one.

@gpu
func product(a: &Tensor<f32, [2048, 2048]>, b: &Tensor<f32, [2048, 2048]>) -> Tensor<f32, [2048, 2048]> {
    a @ b
}

func main() -> i32 {
    mut a = Tensor::<f32, [2048, 2048]>::zeros()
    mut b = Tensor::<f32, [2048, 2048]>::zeros()
    for i in 0..2048 {
        for j in 0..2048 {
            a[i, j] = ((i * 7 + j * 3) % 17) as f32
            b[i, j] = ((i * 5 + j * 11) % 13) as f32
        }
    }
    val b_gpu = b.to(Device::GPU(0))
    mut checksum: i64 = 0
    for r in 0..5 {
        a[r, r] = a[r, r] + 1.0f32
        val c = product(&a, &b_gpu).to(Device::CPU)
        for i in 0..2048 {
            for j in 0..2048 {
                checksum = checksum + c[i, j] as i64
            }
        }
    }
    println("checksum = {checksum}")
    return 0
}
