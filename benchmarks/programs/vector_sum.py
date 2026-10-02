# Growable-collection throughput: fill a list, then sweep it repeatedly. Measures
# append cost, load cost, and how fast a sweep runs. Each sweep is a list
# comprehension summed by the builtin, the fastest loop plain Python has. The XOR
# against the outer counter stops the repeat loop from being folded into a single
# multiply.

def work(n):
    v=[i%97 for i in range(n)]
    acc=0
    for r in range(7000):
        acc+=sum([x^r for x in v])
    return acc
print("acc =", work(50000))
