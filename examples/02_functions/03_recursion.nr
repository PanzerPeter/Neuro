//! Example: Recursive Functions
//!
//! Purpose: Demonstrates recursive function patterns and techniques
//!
//! Concepts covered:
//! - Basic recursion with base cases
//! - Tail recursion patterns
//! - Multiple recursive calls
//! - Recursive mathematical functions
//! - Stack depth considerations
//!
//! Expected output: Results from various recursive calculations
//! Compilation: neurc run examples/02_functions/03_recursion.nr

// Classic factorial function - single recursive call
fn factorial(n: int) -> int {
    // Base case: factorial of 0 or 1 is 1
    if n <= 1 {
        return 1;
    } else {
        // Recursive case: n! = n * (n-1)!
        return n * factorial(n - 1);
    }
}

// Fibonacci sequence - multiple recursive calls
fn fibonacci(n: int) -> int {
    // Base cases
    if n <= 0 {
        return 0;
    } else {
        if n == 1 {
            return 1;
        } else {
            // Recursive case: fib(n) = fib(n-1) + fib(n-2)
            return fibonacci(n - 1) + fibonacci(n - 2);
        }
    }
}

// Power function - recursive exponentiation
fn power(base: int, exponent: int) -> int {
    // Base case: any number to the power of 0 is 1
    if exponent == 0 {
        return 1;
    } else {
        if exponent == 1 {
            return base;
        } else {
            // Recursive case: base^exp = base * base^(exp-1)
            return base * power(base, exponent - 1);
        }
    }
}

// Sum of numbers from 1 to n (recursive)
fn sum_to_n(n: int) -> int {
    // Base case
    if n <= 0 {
        return 0;
    } else {
        if n == 1 {
            return 1;
        } else {
            // Recursive case: sum(n) = n + sum(n-1)
            return n + sum_to_n(n - 1);
        }
    }
}

// Greatest Common Divisor using Euclidean algorithm
fn gcd(a: int, b: int) -> int {
    // Base case: gcd(a, 0) = a
    if b == 0 {
        return a;
    } else {
        // Recursive case: gcd(a, b) = gcd(b, a mod b)
        return gcd(b, a % b);
    }
}

// Count digits in a number
fn count_digits(n: int) -> int {
    // Handle negative numbers by taking absolute value
    let num = n;
    if num < 0 {
        num = -num;
    }

    // Base case: single digit numbers have 1 digit
    if num < 10 {
        return 1;
    } else {
        // Recursive case: count digits of n/10 and add 1
        return 1 + count_digits(num / 10);
    }
}

// Recursive helper for tail recursion example
fn factorial_tail_helper(n: int, accumulator: int) -> int {
    if n <= 1 {
        return accumulator;
    } else {
        return factorial_tail_helper(n - 1, accumulator * n);
    }
}

// Tail-recursive factorial (more efficient)
fn factorial_tail(n: int) -> int {
    return factorial_tail_helper(n, 1);
}

// Binary search in a sorted range (recursive)
fn binary_search_range(target: int, low: int, high: int) -> int {
    // Base case: range is empty
    if low > high {
        return -1;  // Not found
    }

    let mid = (low + high) / 2;

    // For demonstration, we'll simulate an array [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    // In a real implementation, this would access an actual array
    let mid_value = mid;  // Simplified: assume array[i] = i for i in [1..10]

    if mid_value == target {
        return mid;  // Found
    } else {
        if mid_value < target {
            // Search right half
            return binary_search_range(target, mid + 1, high);
        } else {
            // Search left half
            return binary_search_range(target, low, mid - 1);
        }
    }
}

fn main() -> int {
    // Test factorial function
    let fact_5 = factorial(5);    // 5! = 120
    let fact_7 = factorial(7);    // 7! = 5040
    print(fact_5);  // Output: 120
    print(fact_7);  // Output: 5040

    // Test Fibonacci sequence
    let fib_6 = fibonacci(6);     // F(6) = 8
    let fib_8 = fibonacci(8);     // F(8) = 21
    print(fib_6);   // Output: 8
    print(fib_8);   // Output: 21

    // Test power function
    let power_2_3 = power(2, 3);  // 2^3 = 8
    let power_3_4 = power(3, 4);  // 3^4 = 81
    print(power_2_3);  // Output: 8
    print(power_3_4);  // Output: 81

    // Test sum to n
    let sum_5 = sum_to_n(5);      // 1+2+3+4+5 = 15
    let sum_10 = sum_to_n(10);    // 1+2+...+10 = 55
    print(sum_5);   // Output: 15
    print(sum_10);  // Output: 55

    // Test GCD
    let gcd_48_18 = gcd(48, 18);  // GCD(48, 18) = 6
    let gcd_17_13 = gcd(17, 13);  // GCD(17, 13) = 1
    print(gcd_48_18);  // Output: 6
    print(gcd_17_13);  // Output: 1

    // Test digit counting
    let digits_123 = count_digits(123);    // 3 digits
    let digits_5 = count_digits(5);        // 1 digit
    let digits_neg = count_digits(-456);   // 3 digits
    print(digits_123);  // Output: 3
    print(digits_5);    // Output: 1
    print(digits_neg);  // Output: 3

    // Test tail-recursive factorial
    let tail_fact_6 = factorial_tail(6);   // 6! = 720
    print(tail_fact_6);  // Output: 720

    // Test binary search (searching in range 1-10)
    let search_7 = binary_search_range(7, 1, 10);    // Should find 7 at position 7
    let search_15 = binary_search_range(15, 1, 10);  // Should return -1 (not found)
    print(search_7);   // Output: 7
    print(search_15);  // Output: -1

    // Demonstrate recursion depth with larger numbers
    let large_fact = factorial(10);  // 10! = 3628800
    print(large_fact);  // Output: 3628800

    // Compare regular vs tail recursion results (should be same)
    let regular = factorial(6);       // 720
    let tail_rec = factorial_tail(6); // 720
    let difference = regular - tail_rec;  // Should be 0
    print(difference);  // Output: 0

    return 0;
}