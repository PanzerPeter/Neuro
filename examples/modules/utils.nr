// NEURO Utility Functions Module
// Demonstrates common utility functions

/// Check if a number is even
fn is_even(n: int) -> bool {
    n % 2 == 0
}

/// Find maximum of two numbers
fn max(a: int, b: int) -> int {
    if a > b {
        a
    } else {
        b
    }
}

/// Find minimum of two numbers
fn min(a: int, b: int) -> int {
    if a < b {
        a
    } else {
        b
    }
}

/// Convert number to string (simplified)
fn to_string(n: int) -> string {
    // In a real implementation, this would convert int to string
    "number"
}