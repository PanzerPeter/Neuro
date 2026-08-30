// Growable-collection throughput: fill a Vec, then sweep it repeatedly with an
// index. Measures push/grow cost, indexed load cost, and whether the sweep
// vectorizes. The XOR against the outer counter stops the repeat loop from
// being folded into a single multiply.

#include <cstdio>
#include <vector>
long long work(int n){
    std::vector<long long> v;
    for(int i=0;i<n;i++) v.push_back((long long)i % 97);
    long long acc=0;
    for(int r=0;r<7000;r++) for(int j=0;j<n;j++) acc+=v[j]^(long long)r;
    return acc;
}
int main(){ printf("acc = %lld\n", work(50000)); }
