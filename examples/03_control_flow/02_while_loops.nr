//! Example: While Loops and Iteration
//!
//! Purpose: Demonstrates iterative execution with while loops
//!
//! Concepts covered:
//! - Basic while loop syntax
//! - Loop counters and conditions
//! - Accumulator patterns
//! - Complex loop conditions
//! - Nested while loops
//! - Loop variable modifications
//!
//! Expected output: Results from various loop calculations
//! Compilation: neurc run examples/03_control_flow/02_while_loops.nr

// Simple counting loop
fn count_to_n(n: int) -> int {
    let mut counter = 0;
    let mut sum = 0;

    while counter < n {
        counter = counter + 1;
        sum = sum + counter;  // Sum of 1 + 2 + ... + n
    }

    return sum;
}

// Countdown loop
fn countdown_from(start: int) -> int {
    let mut current = start;
    let mut product = 1;

    while current > 0 {
        product = product * current;
        current = current - 1;
    }

    return product;  // Factorial of start
}

// Loop with complex condition
fn sum_until_limit(start: int, limit: int) -> int {
    let mut current = start;
    let mut sum = 0;
    let mut count = 0;

    // Continue until sum exceeds limit OR we've done 10 iterations
    while (sum < limit) && (count < 10) {
        sum = sum + current;
        current = current + 1;
        count = count + 1;
    }

    return sum;
}

// Accumulator pattern - collecting values
fn sum_even_numbers(max: int) -> int {
    let mut i = 0;
    let mut sum = 0;

    while i <= max {
        if (i % 2) == 0 {  // If i is even
            sum = sum + i;
        }
        i = i + 1;
    }

    return sum;  // Sum of all even numbers from 0 to max
}

// Finding patterns with loops
fn find_first_divisible(start: int, divisor: int) -> int {
    let mut current = start;

    while (current % divisor) != 0 {
        current = current + 1;
    }

    return current;  // First number >= start that's divisible by divisor
}

// Nested loops - multiplication table calculation
fn multiplication_table_sum(size: int) -> int {
    let mut i = 1;
    let mut total = 0;

    while i <= size {
        let mut j = 1;

        while j <= size {
            total = total + (i * j);  // Add i*j to total
            j = j + 1;
        }

        i = i + 1;
    }

    return total;  // Sum of all products in size x size table
}

// Loop with multiple variables
fn fibonacci_sum(n: int) -> int {
    let mut a = 0;
    let mut b = 1;
    let mut sum = 0;
    let mut count = 0;

    while count < n {
        sum = sum + a;        // Add current fibonacci number to sum
        let temp = a + b;     // Calculate next fibonacci number
        a = b;
        b = temp;
        count = count + 1;
    }

    return sum;  // Sum of first n fibonacci numbers
}

// Digital root calculation (repeatedly sum digits)
fn digital_root(number: int) -> int {
    let mut n = number;

    while n >= 10 {
        let mut digit_sum = 0;
        let mut temp = n;

        // Sum all digits of current number
        while temp > 0 {
            digit_sum = digit_sum + (temp % 10);
            temp = temp / 10;
        }

        n = digit_sum;
    }

    return n;  // Single digit result
}

// Power calculation using loops
fn power_with_loop(base: int, exponent: int) -> int {
    let mut result = 1;
    let mut exp = exponent;

    while exp > 0 {
        result = result * base;
        exp = exp - 1;
    }

    return result;
}

// Greatest Common Divisor using loops
fn gcd_iterative(a: int, b: int) -> int {
    let mut x = a;
    let mut y = b;

    while y != 0 {
        let temp = y;
        y = x % y;
        x = temp;
    }

    return x;
}

// Reverse number calculation
fn reverse_number(n: int) -> int {
    let mut num = n;
    let mut reversed = 0;

    while num > 0 {
        let digit = num % 10;
        reversed = (reversed * 10) + digit;
        num = num / 10;
    }

    return reversed;
}

// Loop with early termination condition
fn find_perfect_square(start: int) -> int {
    let mut current = start;
    let mut found = 0;

    while found == 0 {
        let mut i = 1;
        let mut is_perfect = 0;

        // Check if current is a perfect square
        while (i * i) <= current {
            if (i * i) == current {
                is_perfect = 1;
            }
            i = i + 1;
        }

        if is_perfect == 1 {
            found = current;
        } else {
            current = current + 1;
        }
    }

    return found;
}

fn main() -> int {
    // Test basic counting
    let sum_5 = count_to_n(5);      // 1+2+3+4+5 = 15
    print(sum_5);  // Output: 15

    // Test countdown (factorial)
    let fact_5 = countdown_from(5);  // 5! = 120
    print(fact_5);  // Output: 120

    // Test complex condition
    let limited_sum = sum_until_limit(1, 20);  // Sum until > 20 or 10 iterations
    print(limited_sum);  // Output: depends on calculation

    // Test even number sum
    let even_sum = sum_even_numbers(10);  // 0+2+4+6+8+10 = 30
    print(even_sum);  // Output: 30

    // Test finding divisible number
    let divisible = find_first_divisible(7, 3);  // First number >= 7 divisible by 3 is 9
    print(divisible);  // Output: 9

    // Test nested loops
    let table_sum = multiplication_table_sum(3);  // Sum of 3x3 multiplication table
    print(table_sum);  // Output: calculated sum

    // Test fibonacci sum
    let fib_sum = fibonacci_sum(6);  // Sum of first 6 fibonacci numbers
    print(fib_sum);  // Output: 0+1+1+2+3+5 = 12

    // Test digital root
    let root_123 = digital_root(123);  // 1+2+3=6
    let root_999 = digital_root(999);  // 9+9+9=27 -> 2+7=9
    print(root_123);  // Output: 6
    print(root_999);  // Output: 9

    // Test power calculation
    let power_result = power_with_loop(2, 5);  // 2^5 = 32
    print(power_result);  // Output: 32

    // Test GCD
    let gcd_result = gcd_iterative(48, 18);  // GCD(48,18) = 6
    print(gcd_result);  // Output: 6

    // Test number reversal
    let reversed = reverse_number(1234);  // 4321
    print(reversed);  // Output: 4321

    // Test perfect square finding
    let perfect = find_perfect_square(10);  // First perfect square >= 10 is 16
    print(perfect);  // Output: 16

    // Test multiple loop results
    let combined = count_to_n(3) + sum_even_numbers(4);  // 6 + 6 = 12
    print(combined);  // Output: 12

    // Demonstrate loop variable progression
    let mut demo_counter = 1;
    let mut demo_result = 0;

    while demo_counter <= 4 {
        demo_result = demo_result + (demo_counter * demo_counter);  // Sum of squares
        print(demo_counter);  // Output: 1, 2, 3, 4
        demo_counter = demo_counter + 1;
    }

    print(demo_result);  // Output: 1+4+9+16 = 30

    return 0;
}