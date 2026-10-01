# The vector_sum benchmark with NumPy: each sweep is one vectorized XOR and sum over the
# array instead of an interpreted loop. Same array, same 7000 sweeps, same total.

import numpy as np


def work(n):
    v = np.arange(n, dtype=np.int64) % 97
    acc = 0
    for r in range(7000):
        acc += int((v ^ r).sum())
    return acc


print("acc =", work(50000))
