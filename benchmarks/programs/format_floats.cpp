// Float-to-text throughput: 600000 interpolated floating-point holes.
//
// Rendering a double at a fixed precision is what a training loop does every time
// it reports a loss, and it is a different code path from `print_lines`'s integer
// hole — the digits come from the C library's conversion rather than from a digit
// loop. The value is irrational-looking on purpose: a short decimal expansion
// would let the conversion finish early and measure the wrong thing.

#include <cstdio>
int main(){
    for(int i=0;i<600000;i++){
        double x = (double)i * 1.41421356237309 - 0.618033988749895;
        printf("loss %.6f\n", x);
    }
}
