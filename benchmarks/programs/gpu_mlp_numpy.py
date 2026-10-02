# The gpu_mlp benchmark on the CPU with NumPy, whose BLAS is what a Python programmer
# reaches for: the same two-layer forward pass over the same weights, twenty rounds,
# and the same checksum.

import numpy as np

i = np.arange(1024).reshape(-1, 1)
x = ((i * 7 + np.arange(1024) * 3) % 4).astype(np.float32)
w1 = ((i * 5 + np.arange(1024) * 11) % 5 - 2).astype(np.float32)
w2 = ((i * 3 + np.arange(256) * 13) % 3 - 1).astype(np.float32)
b1 = (np.arange(1024) % 7 - 3).astype(np.float32)
b2 = (np.arange(256) % 5 - 2).astype(np.float32)
checksum = 0
for r in range(20):
    x[r, r] += 1.0
    out = np.maximum(x @ w1 + b1, 0) @ w2 + b2
    checksum += int(out.astype(np.int64).sum())
print(f"checksum = {checksum}")
