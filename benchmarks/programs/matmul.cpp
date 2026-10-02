// Dense matrix product, the kernel under every layer: two 256x256 float matrices,
// three times, with one element of the left operand bumped between rounds so no round
// can be hoisted out of the loop. Every element is a small integer, so each sum is
// exact in float and all implementations agree to the digit no matter what order they
// add in. The checksum walks every element. The loops run i-k-j, the cache-friendly
// order that walks both `b` and `c` along a row and so vectorizes.
#include <cstdio>
#include <vector>

int main() {
    const int n = 256;
    std::vector<float> a(n * n), b(n * n), c(n * n);
    for (int i = 0; i < n; i++)
        for (int j = 0; j < n; j++) {
            a[i * n + j] = float((i * 7 + j * 3) % 17);
            b[i * n + j] = float((i * 5 + j * 11) % 13);
        }
    long long checksum = 0;
    for (int r = 0; r < 3; r++) {
        a[r * n + r] += 1.0f;
        for (int i = 0; i < n; i++) {
            float* ci = &c[i * n];
            for (int j = 0; j < n; j++) ci[j] = 0.0f;
            for (int k = 0; k < n; k++) {
                const float aik = a[i * n + k];
                const float* bk = &b[k * n];
                for (int j = 0; j < n; j++) ci[j] += aik * bk[j];
            }
        }
        for (int i = 0; i < n * n; i++) checksum += (long long)c[i];
    }
    std::printf("checksum = %lld\n", checksum);
    return 0;
}
