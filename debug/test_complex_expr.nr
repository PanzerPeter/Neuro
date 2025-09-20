fn test(n: int) -> bool {
    let i = 5;
    while i * i <= n {
        return true;
    }
    return false;
}

fn main() -> int {
    let result = test(10);
    return 0;
}