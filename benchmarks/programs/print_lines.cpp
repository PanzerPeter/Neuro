// Formatted standard output: 200000 interpolated lines. Measures the cost of
// rendering a value into a string and getting the bytes to fd 1 — a path
// dominated by formatting and syscalls rather than by arithmetic.

#include <cstdio>
int main(){ for(int i=0;i<600000;i++) printf("line %d\n", i); }
