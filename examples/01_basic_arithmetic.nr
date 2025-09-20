// Basic arithmetic operations demonstration
fn main() -> int {
    let a = 10;
    let b = 5;

    let addition = a + b;       // 15
    let subtraction = a - b;    // 5
    let multiplication = a * b; // 50
    let division = a / b;       // 2

    let result = addition + subtraction + multiplication + division;
    print(result);  // Should print 72

    return result;
}