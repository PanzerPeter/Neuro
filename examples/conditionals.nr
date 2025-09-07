// Demonstrates conditional logic with if statements

fn max(a: int, b: int) -> int {
    if a > b {
        return a;
    }
    return b;
}

fn sign_check(x: int) -> int {
    if x > 0 {
        return 1;
    }
    if x < 0 {
        return -1;
    }
    return 0;
}

fn main() -> int {
    let first = 10;
    let second = 25;
    
    let maximum = max(first, second);  // Should be 25
    let sign = sign_check(maximum);    // Should be 1
    
    return maximum + sign;  // Should return 26
}