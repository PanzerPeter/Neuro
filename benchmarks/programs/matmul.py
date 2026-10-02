# Dense matrix product, the kernel under every layer: two 256x256 matrices, three
# times, with one element of the left operand bumped between rounds so no round can be
# hoisted out of the loop. Every element is a small integer, so each sum is exact and
# all implementations agree to the digit. The checksum walks every element. Plain
# Python on purpose: NumPy hands the product to a BLAS library, which measures that
# library rather than the language (matmul_numpy.py is that row). Each dot product is
# `sum(map(mul, ...))` over a row of `a` and a column of `b`, transposed once, the
# fastest inner loop plain Python has.

from operator import mul

n = 256
a = [[float((i * 7 + j * 3) % 17) for j in range(n)] for i in range(n)]
b = [[float((i * 5 + j * 11) % 13) for j in range(n)] for i in range(n)]
columns = list(zip(*b))
checksum = 0
for r in range(3):
    a[r][r] += 1.0
    for row in a:
        for col in columns:
            checksum += int(sum(map(mul, row, col)))
print(f"checksum = {checksum}")
