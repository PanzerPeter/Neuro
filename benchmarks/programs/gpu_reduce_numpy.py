# The gpu_reduce benchmark on the CPU with NumPy: the same 1000 rounds of a whole sum and
# a row sum over a float32 matrix, and the same checksum.

import numpy as np

n = 2048
i = np.arange(n).reshape(-1, 1)
j = np.arange(n).reshape(1, -1)
x = ((i * 7 + j * 3) % 4).astype(np.float32)
checksum = 0
for r in range(1000):
    x[r, r] = 3.0
    checksum += int(x.sum())
    checksum += int(x.sum(axis=1)[r])
print(f"checksum = {checksum}")
