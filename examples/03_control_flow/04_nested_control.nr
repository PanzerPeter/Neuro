//! Example: Nested Control Flow Patterns
//!
//! Purpose: Demonstrates complex combinations of if/else and while loops
//!
//! Concepts covered:
//! - If statements inside while loops
//! - While loops inside if statements
//! - Deeply nested control structures
//! - Complex conditional logic with loops
//! - Multiple break/continue points
//! - Real-world algorithmic patterns
//!
//! Expected output: Results from complex nested algorithms
//! Compilation: neurc run examples/03_control_flow/04_nested_control.nr

// Game simulation - nested loops with conditionals
fn simulate_game_rounds(rounds: int, max_score: int) -> int {
    let mut round = 1;
    let mut total_score = 0;
    let mut wins = 0;

    while round <= rounds {
        let mut current_score = 0;
        let mut attempts = 1;

        // Inner game loop
        while attempts <= 5 {  // Max 5 attempts per round
            let score_this_attempt = (round * attempts) % 10;  // Simulated score

            if score_this_attempt > 7 {
                current_score = current_score + score_this_attempt;
                break;  // Good score, end round early
            } else if score_this_attempt > 4 {
                current_score = current_score + score_this_attempt;
            } else {
                // Bad score, try again
                attempts = attempts + 1;
                continue;
            }

            attempts = attempts + 1;
        }

        total_score = total_score + current_score;

        if current_score >= max_score {
            wins = wins + 1;
        }

        round = round + 1;
    }

    return total_score * 100 + wins;  // Encoded result: score*100 + wins
}

// Matrix processing with nested conditions
fn process_matrix(size: int) -> int {
    let mut i = 0;
    let mut sum = 0;

    while i < size {
        let mut j = 0;

        while j < size {
            let value = (i * size) + j;  // Simulate matrix[i][j] = i*size + j

            // Complex nested conditions
            if i == j {  // Diagonal elements
                if value % 2 == 0 {
                    sum = sum + (value * 2);  // Double even diagonal elements
                } else {
                    sum = sum + value;        // Add odd diagonal elements normally
                }
            } else if i > j {  // Lower triangle
                if (i + j) % 3 == 0 {
                    sum = sum + (value / 2);  // Half value if sum divisible by 3
                } else {
                    // Skip this element
                }
            } else {  // Upper triangle (i < j)
                if value > 10 {
                    let mut temp = value;
                    let mut digit_sum = 0;

                    // Calculate sum of digits
                    while temp > 0 {
                        digit_sum = digit_sum + (temp % 10);
                        temp = temp / 10;
                    }

                    sum = sum + digit_sum;
                } else {
                    sum = sum + value;
                }
            }

            j = j + 1;
        }

        i = i + 1;
    }

    return sum;
}

// Search algorithm with nested loops and early termination
fn advanced_search(target: int, search_limit: int) -> int {
    let mut found_count = 0;
    let mut search_base = 1;

    while search_base <= search_limit {
        let mut multiplier = 1;
        let mut local_found = 0;

        // Search within current base range
        while multiplier <= 10 {
            let candidate = search_base * multiplier;

            if candidate == target {
                found_count = found_count + 1;
                local_found = 1;
                break;  // Found in this base, move to next
            }

            if candidate > target {
                if local_found == 0 {
                    // Check if we can find target by addition
                    let mut addend = 1;
                    while addend <= 5 {
                        if (candidate - addend) == target {
                            found_count = found_count + 1;
                            local_found = 1;
                            break;
                        }
                        addend = addend + 1;
                    }
                }
                break;  // Overshot, no point continuing with this base
            }

            multiplier = multiplier + 1;
        }

        // Additional processing if we found something
        if local_found == 1 {
            let mut bonus_search = search_base;
            while bonus_search < (search_base + 3) {
                if (bonus_search * 2) == target {
                    found_count = found_count + 1;
                }
                bonus_search = bonus_search + 1;
            }
        }

        search_base = search_base + 1;
    }

    return found_count;
}

// Nested loop with complex break/continue patterns
fn pattern_analyzer(max_iterations: int) -> int {
    let mut outer = 1;
    let mut pattern_count = 0;

    while outer <= max_iterations {
        let mut inner = 1;
        let mut inner_pattern_found = 0;

        while inner <= outer {
            // Skip certain patterns
            if (inner % outer) == 0 {
                if inner == outer {
                    // Special case: self-divisible
                    pattern_count = pattern_count + 1;
                    inner_pattern_found = 1;
                }
                inner = inner + 1;
                continue;
            }

            // Look for specific relationships
            if (inner + outer) % 5 == 0 {
                let mut sub_count = 0;
                let mut validator = 1;

                // Validate the pattern with a nested check
                while validator <= 3 {
                    if ((inner * validator) + outer) % 7 == 0 {
                        sub_count = sub_count + 1;
                    }
                    validator = validator + 1;
                }

                if sub_count > 1 {
                    pattern_count = pattern_count + sub_count;
                    inner_pattern_found = 1;
                    break;  // Found significant pattern, move to next outer
                }
            }

            // Check for prime-like relationships
            if (inner > 1) && (outer > 1) {
                let mut is_coprime = 1;  // Assume coprime until proven otherwise
                let mut divisor = 2;

                while (divisor <= inner) && (divisor <= outer) {
                    if ((inner % divisor) == 0) && ((outer % divisor) == 0) {
                        is_coprime = 0;  // Found common divisor
                        break;
                    }
                    divisor = divisor + 1;
                }

                if is_coprime == 1 {
                    pattern_count = pattern_count + 1;
                }
            }

            inner = inner + 1;
        }

        // Post-processing based on whether we found patterns
        if inner_pattern_found == 0 {
            // No patterns found, skip some numbers
            outer = outer + 2;
        } else {
            outer = outer + 1;
        }
    }

    return pattern_count;
}

// Complex decision tree with nested loops
fn decision_tree_processor(input: int) -> int {
    let mut result = 0;
    let mut phase = 1;

    while phase <= 3 {
        if phase == 1 {
            // Phase 1: Basic processing
            let mut i = 1;
            while i <= input {
                if i % 2 == 0 {
                    result = result + i;
                } else {
                    if i > 5 {
                        result = result + (i / 2);
                    }
                }
                i = i + 1;
            }
        } else if phase == 2 {
            // Phase 2: Complex processing
            if result > 20 {
                let mut j = 1;
                while j <= 5 {
                    let mut k = 1;
                    while k <= j {
                        if (j * k) > (result / 10) {
                            result = result + (j * k);
                            break;
                        }
                        k = k + 1;
                    }
                    j = j + 1;
                }
            } else {
                // Alternative processing for smaller results
                let mut alt = result;
                while alt > 0 {
                    result = result + (alt % 3);
                    alt = alt / 3;
                }
            }
        } else {
            // Phase 3: Final processing
            if result % 10 > 5 {
                let mut final_loop = 1;
                while final_loop <= 3 {
                    if (result + final_loop) % 7 == 0 {
                        result = result * 2;
                        break;
                    }
                    final_loop = final_loop + 1;
                }
            }
        }

        phase = phase + 1;
    }

    return result;
}

fn main() -> int {
    // Test game simulation
    let game_result = simulate_game_rounds(5, 15);  // 5 rounds, max score 15
    print(game_result);  // Output: encoded score and wins

    // Test matrix processing
    let matrix_sum = process_matrix(4);  // 4x4 matrix processing
    print(matrix_sum);  // Output: processed matrix sum

    // Test advanced search
    let search_count = advanced_search(12, 5);  // Search for 12 in range
    print(search_count);  // Output: number of ways found

    // Test pattern analyzer
    let patterns = pattern_analyzer(8);  // Analyze patterns up to 8
    print(patterns);  // Output: pattern count

    // Test decision tree processor
    let tree_result = decision_tree_processor(10);  // Process input 10
    print(tree_result);  // Output: final processed result

    // Demonstrate nested control complexity
    let mut complexity_demo = 0;
    let mut outer_level = 1;

    while outer_level <= 3 {
        if outer_level == 1 {
            let mut inner_level = 1;
            while inner_level <= 4 {
                if inner_level % 2 == 0 {
                    complexity_demo = complexity_demo + inner_level;
                } else {
                    let mut deep_level = 1;
                    while deep_level <= 2 {
                        complexity_demo = complexity_demo + deep_level;
                        if deep_level == 2 {
                            break;
                        }
                        deep_level = deep_level + 1;
                    }
                }
                inner_level = inner_level + 1;
            }
        } else if outer_level == 2 {
            let mut alt_inner = 1;
            while alt_inner <= 3 {
                if complexity_demo > 10 {
                    complexity_demo = complexity_demo - 2;
                    alt_inner = alt_inner + 1;
                    continue;
                }
                complexity_demo = complexity_demo + alt_inner;
                alt_inner = alt_inner + 1;
            }
        }

        outer_level = outer_level + 1;
    }

    print(complexity_demo);  // Output: result of complex nested operations

    // Compare different processing approaches
    let approach_1 = decision_tree_processor(5);
    let approach_2 = pattern_analyzer(5);
    let difference = approach_1 - approach_2;

    print(approach_1);  // First approach result
    print(approach_2);  // Second approach result
    print(difference);  // Difference between approaches

    return 0;
}