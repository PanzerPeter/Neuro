// Element-wise throughput on the GPU: 200 relaxation steps `y = (y + x) * h` over two
// [2048, 2048] f32 tensors that stay on the device, so each step is one fused kernel
// reading two buffers and writing a third, and nothing crosses the bus until the end.
// IEEE addition and multiplication give the same bits everywhere (no contraction into
// a fused multiply-add), so the checksum of the scaled result matches NumPy's exactly.
// Needs a neurc built with `--features mlir` and an NVIDIA GPU; the harness skips the
// row without either.

@gpu
func relax(y: &Tensor<f32, [2048, 2048]>, x: &Tensor<f32, [2048, 2048]>, h: f32) -> Tensor<f32, [2048, 2048]> {
    (y + x) * h
}

func main() -> i32 {
    mut x = Tensor::<f32, [2048, 2048]>::zeros()
    for i in 0..2048 {
        for j in 0..2048 {
            x[i, j] = ((i * 7 + j * 3) % 17) as f32
        }
    }
    val x_gpu = x.to(Device::GPU(0))
    mut y = Tensor::<f32, [2048, 2048]>::zeros().to(Device::GPU(0))
    for _ in 0..200 {
        y = relax(&y, &x_gpu, 0.5f32)
    }
    val host = y.to(Device::CPU)
    mut checksum: i64 = 0
    for i in 0..2048 {
        for j in 0..2048 {
            checksum = checksum + (host[i, j] * 1024.0f32) as i64
        }
    }
    println("checksum = {checksum}")
    return 0
}
