fn test() -> int {
    let i = 5;
    while i > 0 {
        i = i - 1;
    }
    return i;
}

fn main() -> int {
    let result = test();
    return 0;
}