// Reductions on the GPU: 1000 rounds of a whole-tensor `.sum()` and a row `.sum(1)` over a
// [2048, 2048] f32 tensor that stays on the device, with one element bumped between rounds
// so no round can be hoisted. Every element is 0 to 3, so each total stays below 2^24 and is
// exact in f32 whatever order it is added in, which is what lets the checksum match NumPy's
// pairwise sums to the digit. Needs an NVIDIA GPU; the harness skips the row without one.

func main() -> i32 {
    mut x = Tensor::<f32, [2048, 2048]>::zeros()
    for i in 0..2048 {
        for j in 0..2048 {
            x[i, j] = ((i * 7 + j * 3) % 4) as f32
        }
    }
    mut g = x.to(Device::GPU(0))
    mut checksum: i64 = 0
    for r in 0..1000 {
        g[r, r] = 3.0f32
        checksum = checksum + g.sum() as i64
        val rows = g.sum(1).to(Device::CPU)
        checksum = checksum + rows[r] as i64
    }
    println("checksum = {checksum}")
    return 0
}
