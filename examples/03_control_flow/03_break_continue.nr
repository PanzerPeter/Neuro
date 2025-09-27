//! Example: Break and Continue Statements
//!
//! Purpose: Demonstrates loop control with break and continue statements
//!
//! Concepts covered:
//! - Break statement for early loop exit
//! - Continue statement for skipping iterations
//! - Break vs return differences
//! - Continue with complex conditions
//! - Break and continue in nested loops
//! - Loop control patterns
//!
//! Expected output: Results showing selective iteration and early termination
//! Compilation: neurc run examples/03_control_flow/03_break_continue.nr

// Basic break example - finding first divisor
fn find_first_divisor(n: int) -> int {
    let mut i = 2;

    while i < n {
        if (n % i) == 0 {
            break;  // Exit loop when we find a divisor
        }
        i = i + 1;
    }

    if i < n {
        return i;   // Found a divisor
    } else {
        return -1;  // No divisor found (prime number)
    }
}

// Basic continue example - sum only even numbers
fn sum_even_with_continue(limit: int) -> int {
    let mut i = 0;
    let mut sum = 0;

    while i < limit {
        i = i + 1;

        if (i % 2) != 0 {
            continue;  // Skip odd numbers
        }

        sum = sum + i;  // This only executes for even numbers
    }

    return sum;
}

// Break with complex condition
fn find_target_sum(target: int) -> int {
    let mut i = 1;
    let mut sum = 0;

    while i <= 100 {  // Maximum 100 iterations
        sum = sum + i;

        if sum >= target {
            break;  // Stop when we reach or exceed target
        }

        i = i + 1;
    }

    return i;  // Return the number where we stopped
}

// Continue with multiple conditions
fn process_numbers(max: int) -> int {
    let mut i = 0;
    let mut result = 0;

    while i < max {
        i = i + 1;

        // Skip multiples of 3
        if (i % 3) == 0 {
            continue;
        }

        // Skip numbers greater than 15
        if i > 15 {
            continue;
        }

        // Skip even numbers greater than 10
        if (i > 10) && ((i % 2) == 0) {
            continue;
        }

        result = result + i;  // Only add numbers that pass all conditions
    }

    return result;
}

// Break in nested loops - affects only inner loop
fn nested_break_demo(outer_limit: int, inner_limit: int) -> int {
    let mut outer = 0;
    let mut total = 0;

    while outer < outer_limit {
        let mut inner = 0;

        while inner < inner_limit {
            if inner == 3 {
                break;  // Only breaks inner loop
            }

            total = total + (outer * 10 + inner);
            inner = inner + 1;
        }

        outer = outer + 1;
    }

    return total;
}

// Continue in nested loops
fn nested_continue_demo(size: int) -> int {
    let mut i = 0;
    let mut total = 0;

    while i < size {
        let mut j = 0;

        while j < size {
            // Skip when i equals j (diagonal)
            if i == j {
                j = j + 1;
                continue;
            }

            // Skip when sum is even
            if ((i + j) % 2) == 0 {
                j = j + 1;
                continue;
            }

            total = total + (i * j);
            j = j + 1;
        }

        i = i + 1;
    }

    return total;
}

// Multiple break conditions
fn multi_break_demo(start: int) -> int {
    let mut n = start;
    let mut count = 0;

    while n > 0 {
        count = count + 1;

        // Break if we've done too many iterations
        if count > 20 {
            break;
        }

        // Break if number becomes very small
        if n == 1 {
            break;
        }

        // Break if we find a specific pattern
        if (n % 7) == 0 {
            break;
        }

        n = n / 2;  // Integer division
    }

    return count;
}

// Continue with accumulator pattern
fn selective_accumulation(max: int) -> int {
    let mut i = 0;
    let mut positive_sum = 0;
    let mut negative_sum = 0;

    while i < max {
        let mut value = i - 5;  // Create some negative values

        if value == 0 {
            i = i + 1;
            continue;  // Skip zero
        }

        if value > 0 {
            positive_sum = positive_sum + value;
        } else {
            negative_sum = negative_sum + value;
        }

        i = i + 1;
    }

    return positive_sum + negative_sum;  // Net sum
}

// Search with early termination
fn search_with_break(target: int, max_search: int) -> int {
    let mut current = 1;
    let mut found_position = -1;

    while current <= max_search {
        let mut square = current * current;

        if square == target {
            found_position = current;
            break;  // Found exact match
        }

        if square > target {
            break;  // Overshot, won't find exact match
        }

        current = current + 1;
    }

    return found_position;
}

// Complex skip pattern with continue
fn complex_skip_pattern(limit: int) -> int {
    let mut i = 1;
    let mut result = 0;

    while i <= limit {
        // Skip fibonacci numbers (simplified check)
        if (i == 1) || (i == 2) || (i == 3) || (i == 5) || (i == 8) || (i == 13) {
            i = i + 1;
            continue;
        }

        // Skip perfect squares
        let mut is_square = 0;
        let mut j = 1;
        while (j * j) <= i {
            if (j * j) == i {
                is_square = 1;
            }
            j = j + 1;
        }

        if is_square == 1 {
            i = i + 1;
            continue;
        }

        // Add the number if it passed all skip conditions
        result = result + i;
        i = i + 1;
    }

    return result;
}

// Infinite loop with break (controlled infinite loop)
fn controlled_infinite_loop(start: int) -> int {
    let mut value = start;
    let mut iterations = 0;

    while true {  // Infinite loop
        iterations = iterations + 1;

        // Safety break - prevent actual infinite loop
        if iterations > 50 {
            break;
        }

        // Logic break conditions
        if value <= 1 {
            break;
        }

        if (value % 2) == 0 {
            value = value / 2;
        } else {
            value = (value * 3) + 1;  // Collatz sequence
        }
    }

    return iterations;
}

fn main() -> int {
    // Test basic break
    let divisor_15 = find_first_divisor(15);  // First divisor of 15 is 3
    let divisor_17 = find_first_divisor(17);  // 17 is prime, should return -1
    print(divisor_15);  // Output: 3
    print(divisor_17);  // Output: -1

    // Test basic continue
    let even_sum = sum_even_with_continue(10);  // 2+4+6+8+10 = 30
    print(even_sum);  // Output: 30

    // Test break with target
    let target_pos = find_target_sum(20);  // Find position where sum >= 20
    print(target_pos);  // Output: depends on calculation

    // Test multiple continue conditions
    let processed = process_numbers(20);  // Sum with multiple skip conditions
    print(processed);  // Output: sum of filtered numbers

    // Test nested break
    let nested_break_result = nested_break_demo(3, 5);  // Inner loops break at 3
    print(nested_break_result);  // Output: calculated sum

    // Test nested continue
    let nested_continue_result = nested_continue_demo(4);  // Skip diagonal and even sums
    print(nested_continue_result);  // Output: calculated sum

    // Test multiple break conditions
    let multi_break_result = multi_break_demo(100);  // Break on various conditions
    print(multi_break_result);  // Output: iteration count

    // Test selective accumulation
    let selective_sum = selective_accumulation(12);  // Skip zero, sum others
    print(selective_sum);  // Output: net sum

    // Test search with break
    let search_result = search_with_break(25, 10);  // Search for perfect square
    print(search_result);  // Output: 5 (since 5^2 = 25)

    // Test complex skip pattern
    let complex_result = complex_skip_pattern(15);  // Skip fibonacci and squares
    print(complex_result);  // Output: sum of remaining numbers

    // Test controlled infinite loop
    let collatz_iterations = controlled_infinite_loop(27);  // Collatz sequence
    print(collatz_iterations);  // Output: number of iterations

    // Demonstrate break vs continue difference
    let mut break_count = 0;
    let mut continue_count = 0;

    // Count with break (stops early)
    let mut i = 0;
    while i < 10 {
        if i == 5 {
            break;  // Stops counting here
        }
        break_count = break_count + 1;
        i = i + 1;
    }

    // Count with continue (skips but continues)
    i = 0;
    while i < 10 {
        if i == 5 {
            i = i + 1;
            continue;  // Skips increment but continues loop
        }
        continue_count = continue_count + 1;
        i = i + 1;
    }

    print(break_count);     // Output: 5 (stopped at 5)
    print(continue_count);  // Output: 9 (counted all except 5)

    return 0;
}