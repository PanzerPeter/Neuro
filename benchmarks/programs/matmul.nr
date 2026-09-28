// Dense matrix product, the kernel under every layer: `@` on two [256, 256] f32
// tensors, three times, with one element of the left operand bumped between rounds
// so no round can be hoisted out of the loop. Every element is a small integer, so
// each sum is exact in f32 and all three implementations agree to the digit no
// matter what order they add in. The checksum walks every element of every product.

func main() -> i32 {
    mut a = Tensor::<f32, [256, 256]>::zeros()
    mut b = Tensor::<f32, [256, 256]>::zeros()
    for i in 0..256 {
        for j in 0..256 {
            a[i, j] = ((i * 7 + j * 3) % 17) as f32
            b[i, j] = ((i * 5 + j * 11) % 13) as f32
        }
    }
    mut checksum: i64 = 0
    for r in 0..3 {
        a[r, r] = a[r, r] + 1.0f32
        val c = &a @ &b
        for i in 0..256 {
            for j in 0..256 {
                checksum = checksum + c[i, j] as i64
            }
        }
    }
    println("checksum = {checksum}")
    return 0
}
