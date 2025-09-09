// Complex NEURO program to test full compilation pipeline
// Tests: functions, variables, control flow, operators, recursion

fn is_prime(n: int) -> bool {
    if n <= 1 {
        return false;
    }
    if n <= 3 {
        return true;
    }
    if n % 2 == 0 || n % 3 == 0 {
        return false;
    }
    
    let mut i = 5;
    while i * i <= n {
        if n % i == 0 || n % (i + 2) == 0 {
            return false;
        }
        i = i + 6;
    }
    return true;
}

fn fibonacci(n: int) -> int {
    if n <= 0 {
        return 0;
    }
    if n == 1 {
        return 1;
    }
    return fibonacci(n - 1) + fibonacci(n - 2);
}

fn logical_test(x: int, y: int) -> bool {
    let is_positive = x > 0 && y > 0;
    let has_zero = x == 0 || y == 0;
    let is_negative = !is_positive && !has_zero;
    
    return is_positive || has_zero || is_negative;
}

fn main() -> int {
    let num = 17;
    let prime_result = is_prime(num);
    
    let fib_result = fibonacci(8);
    
    let logic_result = logical_test(-5, 10);
    
    if prime_result && fib_result > 20 {
        return 1;
    }
    
    if !logic_result {
        return -1;
    }
    
    return 0;
}