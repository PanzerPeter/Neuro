// Recursion depth and call overhead: naive Fibonacci, which is nothing but calls.
// Measures how much of a call the backend can remove. One call, not a loop around
// one: a pure function is loop-invariant, so a repeat count would be hoisted away by
// the compiled implementations and not by the interpreted one.

#include <cstdio>

int fib(int n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }

int main() { printf("fib = %d\n", fib(37)); }
