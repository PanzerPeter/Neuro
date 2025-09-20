// Loop demonstration
fn main() -> int {
    let counter = 0;
    let sum = 0;

    // Calculate sum from 1 to 5
    while counter < 5 {
        counter = counter + 1;
        sum = sum + counter;
        print(sum);  // Progress: 1, 3, 6, 10, 15
    }

    return sum;  // Should return 15
}