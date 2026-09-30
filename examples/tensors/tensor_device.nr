// Moving tensors to a GPU: `.to(Device::GPU(0))` and `.to(Device::CPU)`.
//
// `.to(device)` consumes a tensor and returns it with its buffer on the device named.
// A `@gpu` function reads a tensor that already lives on the GPU in place, and a call
// given one leaves its result on the GPU as well. A loop of `@gpu` calls over device
// tensors therefore keeps its data on the GPU the whole way, and only `.to(Device::CPU)`
// brings the answer back.
//
// Outside `@gpu`, a tensor operation runs on the device its tensors live on too: `+ - * /`
// and `@` on float tensors, `.sum()` / `.mean()` / `.max()` / `.min()`, reading or writing
// one element, and `.clone()`. An operation with no device form (`.map`, a slice, integer
// arithmetic) aborts with a diagnostic asking for `.to(Device::CPU)` first.
//
// On a machine with several GPUs, `Device::GPU(1)` and up name the others, and a `@gpu`
// call runs on the GPU its tensors are on. Like every program with a bare `@gpu`
// function, this one stops at startup when there is no usable NVIDIA GPU.

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
