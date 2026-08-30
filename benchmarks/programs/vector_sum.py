# Growable-collection throughput: fill a Vec, then sweep it repeatedly with an
# index. Measures push/grow cost, indexed load cost, and whether the sweep
# vectorizes. The XOR against the outer counter stops the repeat loop from
# being folded into a single multiply.

def work(n):
    v=[i%97 for i in range(n)]
    acc=0
    for r in range(7000):
        for j in range(n): acc+=v[j]^r
    return acc
print("acc =", work(50000))
