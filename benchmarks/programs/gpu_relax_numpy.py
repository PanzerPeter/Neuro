# The gpu_relax benchmark on the CPU with NumPy: the same 200 float32 steps
# `y = (y + x) * h`, each a vectorized pass, and the same checksum.

import numpy as np

n = 2048
i = np.arange(n).reshape(-1, 1)
j = np.arange(n).reshape(1, -1)
x = ((i * 7 + j * 3) % 17).astype(np.float32)
y = np.zeros((n, n), dtype=np.float32)
h = np.float32(0.5)
for _ in range(200):
    y = (y + x) * h
print(f"checksum = {int((y * np.float32(1024.0)).astype(np.int64).sum())}")
