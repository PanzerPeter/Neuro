# Dense matrix product, the kernel under every layer: a naive i-j-k product of two
# 256x256 matrices, three times, with one element of the left operand bumped between
# rounds so no round can be hoisted out of the loop. Every element is a small integer,
# so each sum is exact and all three implementations agree to the digit. The checksum
# walks every element. Plain Python on purpose, like every other row: NumPy hands the
# product to a BLAS library, which measures that library rather than the language.

n = 256
a = [[float((i * 7 + j * 3) % 17) for j in range(n)] for i in range(n)]
b = [[float((i * 5 + j * 11) % 13) for j in range(n)] for i in range(n)]
checksum = 0
for r in range(3):
    a[r][r] += 1.0
    for i in range(n):
        row = a[i]
        for j in range(n):
            s = 0.0
            for k in range(n):
                s += row[k] * b[k][j]
            checksum += int(s)
print(f"checksum = {checksum}")
