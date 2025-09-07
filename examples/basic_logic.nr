// Basic logical operations

fn is_positive(x: int) -> bool {
    return x > 0;
}

fn main() -> int {
    let value = 10;
    let positive = is_positive(value);
    
    if positive {
        return 1;
    }
    
    return 0;
}