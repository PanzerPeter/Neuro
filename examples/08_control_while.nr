fn sum_to(n: int) -> int {
    let mut i = 0;
    let mut s = 0;
    while i < n {
        s = s + i;
        i = i + 1;
    }
    return s;
}

fn main() -> int { return sum_to(5); }

