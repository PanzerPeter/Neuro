// Conditional logic demonstration
fn main() -> int {
    let x = 15;
    let y = 10;
    let result = 0;

    if x > y {
        result = 1;
        print(result);  // Should print 1
    } else {
        result = 0;
        print(result);
    }

    // Another conditional
    if x == 15 {
        result = result + 5;  // result becomes 6
    }

    return result;
}