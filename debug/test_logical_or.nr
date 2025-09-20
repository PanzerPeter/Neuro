fn test(n: int) -> bool {
    if n % 2 == 0 || n % 3 == 0 {
        return true;
    }
    return false;
}

fn main() -> int {
    let result = test(6);
    return 0;
}