# The matmul benchmark as a Python programmer would write it: NumPy hands each product
# to its BLAS library. Same matrices, same three rounds, same checksum. Every element is
# a small integer, so the product is exact whatever order BLAS adds in.

import numpy as np

n = 256
i = np.arange(n).reshape(-1, 1)
j = np.arange(n).reshape(1, -1)
a = ((i * 7 + j * 3) % 17).astype(np.float32)
b = ((i * 5 + j * 11) % 13).astype(np.float32)
checksum = 0
for r in range(3):
    a[r, r] += 1.0
    checksum += int((a @ b).astype(np.int64).sum())
print(f"checksum = {checksum}")
