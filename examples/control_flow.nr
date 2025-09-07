// Demonstrates control flow: if statements and while loops

fn absolute_value(x: int) -> int {
    if x < 0 {
        let zero = 0;
        return zero - x;
    }
    return x;
}

fn simple_loop(n: int) -> int {
    let result = 0;
    let counter = 0;
    
    while counter < n {
        result = result + 1;
        counter = counter + 1;
    }
    
    return result;
}

fn main() -> int {
    let num = 5;
    let positive = absolute_value(num);
    
    let loop_result = simple_loop(3);  // Should be 3
    
    return positive + loop_result;  // Should return 8
}