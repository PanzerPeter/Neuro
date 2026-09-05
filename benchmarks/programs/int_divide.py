# Integer division throughput with a divisor the optimizer cannot pin down.
#
# `/` and `%` are the two operators that carry runtime guards — a zero divisor
# and `MIN / -1` are undefined for the hardware instruction, so the backend
# tests for them. Every divisor here comes out of a list, so no range analysis
# can fold those tests away, which makes this the worst case for their cost.
# The running accumulator keeps each iteration dependent on the last, so the
# loop cannot be vectorized or folded.
#
# Every operand stays positive, so Python's flooring `//` and `%` agree with the
# truncating division the other two implementations use.

def work(n):
    divisors=[d+3 for d in range(64)]
    acc=1
    for i in range(n):
        k=divisors[i%64]
        acc=(acc+i)//k+(acc*7+i)%k+1
    return acc
print("acc =", work(20000000))
