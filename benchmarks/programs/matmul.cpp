// Dense matrix product, the kernel under every layer: a naive i-j-k product of two
// 256x256 float matrices, three times, with one element of the left operand bumped
// between rounds so no round can be hoisted out of the loop. Every element is a small
// integer, so each sum is exact in float and all three implementations agree to the
// digit no matter what order they add in. The checksum walks every element.
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
        for (int i = 0; i < n; i++)
            for (int j = 0; j < n; j++) {
                float s = 0.0f;
                for (int k = 0; k < n; k++) s += a[i * n + k] * b[k * n + j];
                c[i * n + j] = s;
            }
        for (int i = 0; i < n * n; i++) checksum += (long long)c[i];
    }
    std::printf("checksum = %lld\n", checksum);
    return 0;
}
