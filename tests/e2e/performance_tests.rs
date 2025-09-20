//! Performance tests for NEURO compiler and runtime
//! Tests compilation speed and execution performance

use std::process::Command;
use std::fs;
use std::time::Instant;
use tempfile::NamedTempFile;

/// Test compilation performance for different program sizes
#[test]
fn test_compilation_speed() {
    let small_program = r#"
fn main() -> int {
    return 42;
}
"#;

    let medium_program = r#"
fn factorial(n: int) -> int {
    if n <= 1 {
        return 1;
    }
    return n * factorial(n - 1);
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

fn sum_range(start: int, end: int) -> int {
    let mut sum = 0;
    let mut i = start;
    while i <= end {
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}

fn main() -> int {
    let fact = factorial(5);
    let fib = fibonacci(8);
    let sum = sum_range(1, 10);
    return fact + fib + sum;
}
"#;

    let large_program = r#"
fn helper1(x: int) -> int { return x + 1; }
fn helper2(x: int) -> int { return x + 2; }
fn helper3(x: int) -> int { return x + 3; }
fn helper4(x: int) -> int { return x + 4; }
fn helper5(x: int) -> int { return x + 5; }
fn helper6(x: int) -> int { return x + 6; }
fn helper7(x: int) -> int { return x + 7; }
fn helper8(x: int) -> int { return x + 8; }
fn helper9(x: int) -> int { return x + 9; }
fn helper10(x: int) -> int { return x + 10; }

fn complex_computation(input: int) -> int {
    let step1 = helper1(input);
    let step2 = helper2(step1);
    let step3 = helper3(step2);
    let step4 = helper4(step3);
    let step5 = helper5(step4);
    let step6 = helper6(step5);
    let step7 = helper7(step6);
    let step8 = helper8(step7);
    let step9 = helper9(step8);
    let step10 = helper10(step9);
    return step10;
}

fn iterative_algorithm(n: int) -> int {
    let mut result = 0;
    let mut i = 0;
    while i < n {
        let mut j = 0;
        while j < n {
            result = result + (i * j);
            j = j + 1;
        }
        i = i + 1;
    }
    return result;
}

fn recursive_algorithm(n: int) -> int {
    if n <= 1 {
        return 1;
    }
    return n + recursive_algorithm(n - 1) + recursive_algorithm(n - 2);
}

fn main() -> int {
    let complex = complex_computation(5);
    let iterative = iterative_algorithm(10);
    let recursive = recursive_algorithm(8);
    return complex + iterative + recursive;
}
"#;

    let test_cases = [
        ("small", small_program),
        ("medium", medium_program),
        ("large", large_program),
    ];

    for (size, source) in &test_cases {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let temp_path = temp_file.path().with_extension("nr");
        fs::write(&temp_path, source).expect("Failed to write temp file");

        let start_time = Instant::now();

        let output = Command::new("./target/release/neurc")
            .args(&["build", temp_path.to_str().unwrap()])
            .output()
            .expect("Failed to execute neurc build");

        let compilation_time = start_time.elapsed();

        assert!(output.status.success(),
                "{} program should compile\nstderr: {}",
                size,
                String::from_utf8_lossy(&output.stderr));

        // Reasonable compilation time limits
        let max_time_secs = match *size {
            "small" => 10,
            "medium" => 30,
            "large" => 60,
            _ => 60,
        };

        assert!(compilation_time.as_secs() < max_time_secs,
                "{} program compilation took too long: {:?}",
                size, compilation_time);

        println!("{} program compiled in {:?}", size, compilation_time);

        // Clean up
        let _ = fs::remove_file(&temp_path);
        let exe_path = temp_path.with_extension("exe");
        let _ = fs::remove_file(&exe_path);
    }
}

/// Test execution performance of compiled programs
#[test]
fn test_execution_performance() {
    let source = r#"
fn compute_intensive_task(iterations: int) -> int {
    let mut result = 0;
    let mut i = 0;
    while i < iterations {
        let mut j = 0;
        while j < 100 {
            result = result + (i + j);
            j = j + 1;
        }
        i = i + 1;
    }
    return result;
}

fn recursive_fibonacci(n: int) -> int {
    if n <= 1 {
        return n;
    }
    return recursive_fibonacci(n - 1) + recursive_fibonacci(n - 2);
}

fn main() -> int {
    let compute_result = compute_intensive_task(100);
    let fib_result = recursive_fibonacci(10);
    return compute_result + fib_result;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    // Compile the program
    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Performance test should compile");

    let exe_path = temp_path.with_extension("exe");
    assert!(exe_path.exists(), "Executable should be created");

    // Measure execution time
    let start_time = Instant::now();

    let exec_output = Command::new(&exe_path)
        .output()
        .expect("Failed to execute performance test");

    let execution_time = start_time.elapsed();

    assert!(exec_output.status.success(), "Performance test should execute successfully");

    // Execution should complete in reasonable time (less than 30 seconds)
    assert!(execution_time.as_secs() < 30,
            "Execution took too long: {:?}", execution_time);

    println!("Execution completed in {:?}", execution_time);

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let _ = fs::remove_file(&exe_path);
}

/// Test memory usage patterns
#[test]
fn test_memory_usage_patterns() {
    let source = r#"
fn allocate_and_compute(size: int) -> int {
    // Simulate memory-intensive computation
    let mut total = 0;
    let mut i = 0;
    while i < size {
        let mut local_array_sum = 0;
        let mut j = 0;
        while j < 100 {
            local_array_sum = local_array_sum + (i + j);
            j = j + 1;
        }
        total = total + local_array_sum;
        i = i + 1;
    }
    return total;
}

fn recursive_memory_test(depth: int, accumulator: int) -> int {
    if depth <= 0 {
        return accumulator;
    }
    let new_accumulator = accumulator + depth;
    return recursive_memory_test(depth - 1, new_accumulator);
}

fn main() -> int {
    let alloc_result = allocate_and_compute(50);
    let recursive_result = recursive_memory_test(100, 0);
    return alloc_result + recursive_result;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Memory usage test should compile");

    let exe_path = temp_path.with_extension("exe");
    let exec_output = Command::new(&exe_path)
        .output()
        .expect("Failed to execute memory test");

    assert!(exec_output.status.success(), "Memory test should execute without crashes");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let _ = fs::remove_file(&exe_path);
}

/// Test different optimization levels (if implemented)
#[test]
fn test_optimization_levels() {
    let source = r#"
fn optimization_test_function(n: int) -> int {
    let mut result = 0;
    let mut i = 0;
    while i < n {
        // Some computation that could be optimized
        let temp = i * i + i + 1;
        result = result + temp;
        i = i + 1;
    }
    return result;
}

fn main() -> int {
    return optimization_test_function(100);
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    // Test default compilation (should work)
    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Optimization test should compile with default settings");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

/// Test scalability with large numbers of functions
#[test]
fn test_scalability() {
    // Generate a program with many functions
    let mut source = String::new();

    // Add many simple functions
    for i in 0..50 {
        source.push_str(&format!(
            "fn func_{}_{}(x: int) -> int {{ return x + {}; }}\n",
            i / 10, i % 10, i
        ));
    }

    // Add a main function that calls some of them
    source.push_str("fn main() -> int {\n");
    source.push_str("    let mut result = 0;\n");
    for i in (0..50).step_by(5) {
        source.push_str(&format!(
            "    result = result + func_{}_{}({});\n",
            i / 10, i % 10, i
        ));
    }
    source.push_str("    return result;\n");
    source.push_str("}\n");

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, &source).expect("Failed to write temp file");

    let start_time = Instant::now();

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    let compilation_time = start_time.elapsed();

    assert!(output.status.success(),
            "Scalability test should compile\nstderr: {}",
            String::from_utf8_lossy(&output.stderr));

    // Should complete in reasonable time even with many functions
    assert!(compilation_time.as_secs() < 120,
            "Scalability test took too long: {:?}", compilation_time);

    println!("Scalability test (50 functions) compiled in {:?}", compilation_time);

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

/// Test all examples compile within reasonable time
#[test]
fn test_examples_compilation_performance() {
    let example_files = [
        "examples/06_functions.nr",
        "examples/07_control_if.nr",
        "examples/08_control_while.nr",
        "examples/01_basic_arithmetic.nr",
        "examples/02_conditional_logic.nr",
        "examples/03_loops.nr",
        "examples/04_simple_example.nr",
    ];

    for example_file in &example_files {
        let start_time = Instant::now();

        let output = Command::new("./target/release/neurc")
            .args(&["build", example_file])
            .output()
            .expect(&format!("Failed to execute neurc build on {}", example_file));

        let compilation_time = start_time.elapsed();

        assert!(output.status.success(),
                "Example {} should compile\nstderr: {}",
                example_file,
                String::from_utf8_lossy(&output.stderr));

        // Each example should compile quickly
        assert!(compilation_time.as_secs() < 30,
                "Example {} took too long to compile: {:?}",
                example_file, compilation_time);

        println!("Example {} compiled in {:?}", example_file, compilation_time);

        // Clean up
        let exe_path = example_file.replace(".nr", ".exe");
        let _ = fs::remove_file(&exe_path);
    }
}

#[cfg(test)]
mod benchmarking_tests {
    use super::*;

    /// Benchmark basic arithmetic operations
    #[test]
    fn benchmark_arithmetic_operations() {
        let source = r#"
fn arithmetic_benchmark(iterations: int) -> int {
    let mut result = 0;
    let mut i = 0;
    while i < iterations {
        let a = i + 1;
        let b = i + 2;
        let c = i + 3;

        let add_result = a + b + c;
        let mul_result = a * b * c;
        let div_result = mul_result / (a + 1);
        let mod_result = mul_result % (b + 1);

        result = result + add_result + div_result + mod_result;
        i = i + 1;
    }
    return result;
}

fn main() -> int {
    return arithmetic_benchmark(1000);
}
"#;

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let temp_path = temp_file.path().with_extension("nr");
        fs::write(&temp_path, source).expect("Failed to write temp file");

        let output = Command::new("./target/release/neurc")
            .args(&["build", temp_path.to_str().unwrap()])
            .output()
            .expect("Failed to execute neurc build");

        assert!(output.status.success(), "Arithmetic benchmark should compile");

        let exe_path = temp_path.with_extension("exe");

        // Run multiple times to get average
        let mut total_time = std::time::Duration::new(0, 0);
        let runs = 3;

        for _ in 0..runs {
            let start_time = Instant::now();

            let exec_output = Command::new(&exe_path)
                .output()
                .expect("Failed to execute arithmetic benchmark");

            let execution_time = start_time.elapsed();
            total_time += execution_time;

            assert!(exec_output.status.success(), "Arithmetic benchmark should execute");
        }

        let avg_time = total_time / runs;
        println!("Arithmetic benchmark average time: {:?}", avg_time);

        // Clean up
        let _ = fs::remove_file(&temp_path);
        let _ = fs::remove_file(&exe_path);
    }
}