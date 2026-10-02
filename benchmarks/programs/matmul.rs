// Dense matrix product, the kernel under every layer: two 256x256 f32 matrices,
// three times, with one element of the left operand bumped between rounds so no
// round can be hoisted out of the loop. Every element is a small integer, so each
// sum is exact in f32 and all implementations agree to the digit no matter what
// order they add in. The checksum walks every element. The loops run i-k-j over
// row slices, the cache-friendly order that vectorizes and drops the bounds checks.

fn main() {
    const N: usize = 256;
    let mut a = vec![0.0f32; N * N];
    let mut b = vec![0.0f32; N * N];
    let mut c = vec![0.0f32; N * N];
    for i in 0..N {
        for j in 0..N {
            a[i * N + j] = ((i * 7 + j * 3) % 17) as f32;
            b[i * N + j] = ((i * 5 + j * 11) % 13) as f32;
        }
    }
    let mut checksum: i64 = 0;
    for r in 0..3 {
        a[r * N + r] += 1.0;
        for (a_row, c_row) in a.chunks_exact(N).zip(c.chunks_exact_mut(N)) {
            c_row.fill(0.0);
            for (&aik, b_row) in a_row.iter().zip(b.chunks_exact(N)) {
                for (cij, &bkj) in c_row.iter_mut().zip(b_row) {
                    *cij += aik * bkj;
                }
            }
        }
        checksum += c.iter().map(|&x| x as i64).sum::<i64>();
    }
    println!("checksum = {checksum}");
}
