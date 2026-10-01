# The gpu_matmul benchmark on the CPU with NumPy, whose BLAS is what a Python
# programmer reaches for: same matrices, same five rounds, same checksum.

import numpy as np

n = 2048
i = np.arange(n).reshape(-1, 1)
j = np.arange(n).reshape(1, -1)
a = ((i * 7 + j * 3) % 17).astype(np.float32)
b = ((i * 5 + j * 11) % 13).astype(np.float32)
checksum = 0
for r in range(5):
    a[r, r] += 1.0
    checksum += int((a @ b).astype(np.int64).sum())
print(f"checksum = {checksum}")
