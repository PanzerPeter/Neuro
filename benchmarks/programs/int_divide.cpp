// Integer division throughput with a divisor the optimizer cannot pin down.
//
// `/` and `%` are the two operators that carry runtime guards — a zero divisor
// and `MIN / -1` are undefined for the hardware instruction, so the backend
// tests for them. Every divisor here comes out of a Vec, so no range analysis
// can fold those tests away, which makes this the worst case for their cost.
// The running remainder keeps each iteration dependent on the last, so the
// loop cannot be vectorized or folded.

#include <cstdio>
#include <vector>
long long work(int n){
    std::vector<long long> divisors;
    for(int d=0;d<64;d++) divisors.push_back((long long)d + 3);
    long long acc=1;
    for(int i=0;i<n;i++){
        long long k = divisors[i % 64];
        acc = ((acc + (long long)i) / k) + ((acc * 7 + (long long)i) % k) + 1;
    }
    return acc;
}
int main(){ printf("acc = %lld\n", work(20000000)); }
