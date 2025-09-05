// NEURO Math Library Module
// Demonstrates basic mathematical functions

/// Add two numbers
fn add(a: int, b: int) -> int {
    a + b
}

/// Multiply two numbers  
fn multiply(a: int, b: int) -> int {
    a * b
}

/// Calculate factorial (recursive)
fn factorial(n: int) -> int {
    if n <= 1 {
        1
    } else {
        n * factorial(n - 1)
    }
}

/// Calculate power
fn power(base: int, exp: int) -> int {
    let mut result = 1;
    let mut i = 0;
    while i < exp {
        result = result * base;
        i = i + 1;
    }
    result
}