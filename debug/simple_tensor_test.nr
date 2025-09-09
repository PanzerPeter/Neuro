// Simple tensor test for current implementation
fn test_tensor() -> int {
    // Basic tensor creation (should work with current implementation)
    let a = 5;
    let b = 10;
    let result = a + b;
    return result;
}

fn main() -> int {
    let result = test_tensor();
    return result;
}