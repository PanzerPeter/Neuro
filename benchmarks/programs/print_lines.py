# Formatted standard output: 200000 interpolated lines. Measures the cost of
# rendering a value into a string and getting the bytes to fd 1 — a path
# dominated by formatting and syscalls rather than by arithmetic.

for i in range(600000): print(f"line {i}")
