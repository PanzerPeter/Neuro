// Demonstrates variables, functions, and basic operations

fn add(a: int, b: int) -> int {
    return a + b;
}

fn multiply(x: int, y: int) -> int {
    return x * y;
}

fn calculate(base: int) -> int {
    let doubled = multiply(base, 2);
    let result = add(doubled, 10);
    return result;
}

fn main() -> int {
    let num = 5;
    let final_result = calculate(num);
    return final_result;  // Should return 20: (5 * 2) + 10
}