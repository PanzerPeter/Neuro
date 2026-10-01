# The mandelbrot benchmark with NumPy: every pixel iterates at once, and a pixel leaves
# the working set when it escapes. Each pixel runs the scalar version's float64
# operations in the same order, so every escape count, and the total, is identical.

import numpy as np

w, h, max_iter = 1000, 1000, 400
x0 = np.tile(np.arange(w) / w * 3.5 - 2.5, h)
y0 = np.repeat(np.arange(h) / h * 2.0 - 1.0, w)
x = np.zeros(w * h)
y = np.zeros(w * h)
count = np.zeros(w * h, dtype=np.int64)
active = np.arange(w * h)
for _ in range(max_iter):
    xa = x[active]
    ya = y[active]
    xx = xa * xa
    yy = ya * ya
    keep = ~(xx + yy > 4.0)
    active = active[keep]
    if active.size == 0:
        break
    xa, ya, xx, yy = xa[keep], ya[keep], xx[keep], yy[keep]
    x[active] = xx - yy + x0[active]
    y[active] = 2.0 * xa * ya + y0[active]
    count[active] += 1
print("mandel =", int(count.sum()))
