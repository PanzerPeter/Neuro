// Moving tensors to a GPU: `.to(Device::GPU(0))` and `.to(Device::CPU)`. `.to(device)`
// consumes a tensor and returns it with its buffer on that device. A `@gpu` call given a
// device tensor reads it in place and leaves its result on the GPU, so a loop of `@gpu`
// calls keeps the data resident until `.to(Device::CPU)`. Outside `@gpu`, float and integer
// operations run on the device their tensors live on: operators, `@`, compound assignment,
// reductions, sorts, slices, `.t()`, `einsum`, the elementwise math methods, `.map`, `.zip`
// and `.reduce`, element reads and writes, and `.clone()`. Integer operations keep the
// host's checks. An operation with no device form (on a `bf16` tensor, say) aborts and asks
// for `.to(Device::CPU)` first. `Device::GPU(1)` and up name the other GPUs on a multi-GPU
// machine. Like any bare `@gpu` program, this one stops at startup without a usable NVIDIA
// GPU.

@gpu
func step(state: &Tensor<f32, [8, 8]>, delta: &Tensor<f32, [8, 8]>, rate: f32) -> Tensor<f32, [8, 8]> {
    val moved = delta * rate
    state + moved
}

func main() -> i32 {
    val delta = Tensor::<f32, [8, 8]>::ones() * 0.5f32

    // Both inputs cross to the GPU once. `clone` keeps a host copy of `delta` to check
    // the answer against.
    mut state = Tensor::<f32, [8, 8]>::ones().to(Device::GPU(0))
    val resident = delta.clone().to(Device::GPU(0))
    mut i = 0
    while i < 10 {
        state = step(&state, &resident, 2.0f32)
        i += 1
    }

    // Still on the GPU: no `@gpu` function, and nothing crosses to the host but the two
    // numbers printed.
    val halved = &state * 0.5f32
    mut copy = halved.clone()
    copy[0, 0] = 0.0f32
    println("on the device: sum {halved.sum()}, row max {halved.max(axis: 1)[7]}, first {copy[0, 0]} and {halved[0, 0]}")

    val (top, row) = halved.topk(k: 1, axis: 0)
    println("top of column 3: {top[0, 3]} in row {row[0, 3]}")

    // A slice, a closure called once per element, and an in-place update, all on the GPU.
    val corner = halved[0..2, 6..8]
    mut shifted = corner.map(|v: f32| -> f32 { v - 1.0f32 })
    shifted *= 2.0f32
    println("a corner, shifted and doubled on the device: {shifted[1, 1]}")

    // Integer tensors run on the GPU too, with the checks the host makes: an element that
    // overflowed here in a debug build, or a zero divisor in any build, would stop the
    // program at the operation with the host's own `panic:` line.
    val counts: Tensor<i32, [4]> = [7, 12, 3, 9]
    val on_device = counts.to(Device::GPU(0))
    val doubled = (&on_device * 2 - &on_device / 3).sort(descending: true)
    println("integer counts on the device: largest {doubled[0]}, total {doubled.sum()}")

    val result = state.to(Device::CPU)

    mut expected = Tensor::<f32, [8, 8]>::ones()
    mut j = 0
    while j < 10 {
        expected += &delta * 2.0f32
        j += 1
    }
    println("after 10 steps: {result[0, 0]} on the GPU, {expected[0, 0]} on the host")
    return result[7, 7] as i32
}
