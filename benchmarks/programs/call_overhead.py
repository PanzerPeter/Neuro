# Recursion depth and call overhead: naive Fibonacci, which is nothing but calls.
# Measures how much of a call the backend can remove. One call, not a loop around
# one: a pure function is loop-invariant, so a repeat count would be hoisted away by
# the compiled implementations and not by the interpreted one.

import sys

def fib(n):
    return n if n < 2 else fib(n - 1) + fib(n - 2)

sys.setrecursionlimit(10000)
print("fib =", fib(37))
