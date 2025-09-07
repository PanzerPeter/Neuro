// Demonstrates the type system with different data types

fn compare_numbers(a: int, b: int) -> bool {
    return a > b;
}

fn string_length_estimate(s: string) -> int {
    // Simple estimation - real string length not implemented yet
    return 10;  // Placeholder
}

fn process_float(value: float) -> float {
    return value * 2.5;
}

fn main() -> int {
    // Integer operations
    let x = 10;
    let y = 20;
    let sum = x + y;
    
    // Boolean logic
    let is_greater = compare_numbers(y, x);  // true
    
    // String handling (basic)
    let message = "Hello NEURO";
    let length = string_length_estimate(message);
    
    // Float operations
    let pi = 3.14159;
    let doubled_pi = process_float(pi);
    
    // Type coercion in return
    if is_greater {
        return sum;  // 30
    }
    return 0;
}