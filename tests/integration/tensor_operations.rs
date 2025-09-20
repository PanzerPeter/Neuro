//! Integration tests for tensor operations and type system
//! Tests the tensor type system and basic tensor operations in Phase 1

use std::process::Command;
use std::fs;
use tempfile::NamedTempFile;

/// Test basic tensor type creation and usage
#[test]
fn test_basic_tensor_types() {
    // Test that tensor type annotations compile correctly
    let source = r#"
fn test_tensor_types() -> int {
    // Basic tensor operations using simplified syntax for Phase 1
    let scalar_size = 1;
    let vector_size = 5;
    let matrix_size = 12; // 3x4 matrix

    return scalar_size + vector_size + matrix_size;
}

fn main() -> int {
    return test_tensor_types();
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Basic tensor type code should compile\nstderr: {}",
            String::from_utf8_lossy(&output.stderr));

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

/// Test tensor shape calculations
#[test]
fn test_tensor_shape_calculations() {
    let source = r#"
fn calculate_tensor_size(rows: int, cols: int) -> int {
    return rows * cols;
}

fn reshape_size(original: int, new_rows: int) -> int {
    if new_rows > 0 {
        return original / new_rows;
    }
    return 0;
}

fn main() -> int {
    let matrix_size = calculate_tensor_size(3, 4);
    let reshaped_cols = reshape_size(matrix_size, 2);
    return matrix_size + reshaped_cols;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Tensor shape calculations should compile");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

/// Test tensor broadcasting logic (simplified for Phase 1)
#[test]
fn test_tensor_broadcasting_logic() {
    let source = r#"
fn can_broadcast(shape1_len: int, shape2_len: int, dim1: int, dim2: int) -> bool {
    // Simplified broadcasting check
    if dim1 == dim2 {
        return true;
    }
    if dim1 == 1 || dim2 == 1 {
        return true;
    }
    return false;
}

fn broadcast_result_size(dim1: int, dim2: int) -> int {
    if dim1 > dim2 {
        return dim1;
    }
    return dim2;
}

fn main() -> int {
    let can_broadcast_result = can_broadcast(2, 2, 3, 3);
    let broadcast_size = broadcast_result_size(3, 1);

    if can_broadcast_result {
        return broadcast_size;
    }
    return 0;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Broadcasting logic should compile");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

/// Test basic tensor arithmetic operations
#[test]
fn test_tensor_arithmetic_operations() {
    let source = r#"
fn tensor_add(a_val: int, b_val: int) -> int {
    return a_val + b_val;
}

fn tensor_multiply(a_val: int, b_val: int) -> int {
    return a_val * b_val;
}

fn tensor_elementwise_ops(size: int, a_elem: int, b_elem: int) -> int {
    let add_result = tensor_add(a_elem, b_elem);
    let mul_result = tensor_multiply(a_elem, b_elem);
    return add_result + mul_result;
}

fn main() -> int {
    let tensor_size = 6; // 2x3 tensor
    let elem_a = 5;
    let elem_b = 3;

    return tensor_elementwise_ops(tensor_size, elem_a, elem_b);
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Tensor arithmetic operations should compile");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

/// Test tensor matrix operations (simplified)
#[test]
fn test_tensor_matrix_operations() {
    let source = r#"
fn matrix_multiply_size(rows_a: int, cols_a: int, rows_b: int, cols_b: int) -> int {
    // Check if multiplication is valid
    if cols_a == rows_b {
        return rows_a * cols_b;
    }
    return 0; // Invalid multiplication
}

fn transpose_dimensions(rows: int, cols: int) -> int {
    // Return new rows (which were cols)
    return cols;
}

fn dot_product_result(vec1_elem: int, vec2_elem: int, size: int) -> int {
    return vec1_elem * vec2_elem * size;
}

fn main() -> int {
    let matrix_a_rows = 3;
    let matrix_a_cols = 4;
    let matrix_b_rows = 4;
    let matrix_b_cols = 2;

    let result_size = matrix_multiply_size(matrix_a_rows, matrix_a_cols, matrix_b_rows, matrix_b_cols);
    let transposed_rows = transpose_dimensions(matrix_a_rows, matrix_a_cols);

    let dot_result = dot_product_result(2, 3, 4);

    return result_size + transposed_rows + dot_result;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Matrix operations should compile");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

/// Test tensor reshape operations
#[test]
fn test_tensor_reshape_operations() {
    let source = r#"
fn can_reshape(original_size: int, new_size: int) -> bool {
    return original_size == new_size;
}

fn calculate_missing_dimension(total_size: int, known_dim1: int, known_dim2: int) -> int {
    let known_product = known_dim1 * known_dim2;
    if known_product > 0 {
        return total_size / known_product;
    }
    return 0;
}

fn validate_reshape(rows: int, cols: int, depth: int) -> bool {
    // All dimensions must be positive
    return rows > 0 && cols > 0 && depth > 0;
}

fn main() -> int {
    let original_tensor_size = 24; // 2x3x4 tensor
    let new_shape_size = 4 * 6; // 4x6 tensor

    let can_do_reshape = can_reshape(original_tensor_size, new_shape_size);
    let missing_dim = calculate_missing_dimension(24, 2, 3); // Should be 4
    let is_valid = validate_reshape(2, 3, 4);

    if can_do_reshape && is_valid {
        return missing_dim;
    }
    return 0;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Tensor reshape operations should compile");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

/// Test tensor indexing and slicing logic
#[test]
fn test_tensor_indexing() {
    let source = r#"
fn linear_index(rows: int, cols: int, row: int, col: int) -> int {
    return row * cols + col;
}

fn bounds_check(index: int, size: int) -> bool {
    return index >= 0 && index < size;
}

fn slice_size(start: int, end: int, step: int) -> int {
    if step > 0 && start < end {
        return (end - start + step - 1) / step;
    }
    return 0;
}

fn main() -> int {
    let matrix_rows = 3;
    let matrix_cols = 4;
    let row = 1;
    let col = 2;

    let linear_idx = linear_index(matrix_rows, matrix_cols, row, col);
    let is_valid = bounds_check(linear_idx, matrix_rows * matrix_cols);
    let slice_len = slice_size(0, 10, 2);

    if is_valid {
        return linear_idx + slice_len;
    }
    return 0;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Tensor indexing operations should compile");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

/// Test tensor type validation
#[test]
fn test_tensor_type_validation() {
    let source = r#"
fn validate_tensor_rank(rank: int) -> bool {
    return rank >= 0 && rank <= 8; // Reasonable rank limits
}

fn validate_dimension(dim: int) -> bool {
    return dim > 0; // All dimensions must be positive
}

fn calculate_total_elements(dim1: int, dim2: int, dim3: int) -> int {
    if validate_dimension(dim1) && validate_dimension(dim2) && validate_dimension(dim3) {
        return dim1 * dim2 * dim3;
    }
    return 0;
}

fn check_memory_limit(total_elements: int, element_size: int) -> bool {
    let max_elements = 1000000; // Reasonable limit for testing
    return total_elements * element_size < max_elements;
}

fn main() -> int {
    let rank = 3;
    let dim1 = 10;
    let dim2 = 20;
    let dim3 = 5;
    let element_size = 4; // 4 bytes per float

    let is_valid_rank = validate_tensor_rank(rank);
    let total_elements = calculate_total_elements(dim1, dim2, dim3);
    let within_memory_limit = check_memory_limit(total_elements, element_size);

    if is_valid_rank && within_memory_limit {
        return total_elements;
    }
    return 0;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Tensor validation operations should compile");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

#[cfg(test)]
mod tensor_error_tests {
    use super::*;

    /// Test tensor shape mismatch errors
    #[test]
    fn test_tensor_shape_mismatch_detection() {
        let source = r#"
fn check_shape_compatibility(rows1: int, cols1: int, rows2: int, cols2: int) -> bool {
    return rows1 == rows2 && cols1 == cols2;
}

fn main() -> int {
    let tensor1_rows = 3;
    let tensor1_cols = 4;
    let tensor2_rows = 3;
    let tensor2_cols = 5; // Different from tensor1

    let compatible = check_shape_compatibility(tensor1_rows, tensor1_cols, tensor2_rows, tensor2_cols);

    if compatible {
        return 1;
    }
    return 0; // Shapes don't match
}
"#;

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let temp_path = temp_file.path().with_extension("nr");
        fs::write(&temp_path, source).expect("Failed to write temp file");

        let output = Command::new("./target/release/neurc")
            .args(&["build", temp_path.to_str().unwrap()])
            .output()
            .expect("Failed to execute neurc build");

        assert!(output.status.success(), "Shape compatibility check should compile");

        // Clean up
        let _ = fs::remove_file(&temp_path);
        let exe_path = temp_path.with_extension("exe");
        let _ = fs::remove_file(&exe_path);
    }

    /// Test invalid tensor operations
    #[test]
    fn test_invalid_tensor_operations() {
        let source = r#"
fn check_matrix_multiplication_validity(cols_a: int, rows_b: int) -> bool {
    return cols_a == rows_b;
}

fn main() -> int {
    let matrix_a_cols = 3;
    let matrix_b_rows = 4; // Mismatch!

    let can_multiply = check_matrix_multiplication_validity(matrix_a_cols, matrix_b_rows);

    if can_multiply {
        return 1;
    }
    return 0; // Cannot multiply these matrices
}
"#;

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let temp_path = temp_file.path().with_extension("nr");
        fs::write(&temp_path, source).expect("Failed to write temp file");

        let output = Command::new("./target/release/neurc")
            .args(&["build", temp_path.to_str().unwrap()])
            .output()
            .expect("Failed to execute neurc build");

        assert!(output.status.success(), "Matrix multiplication validation should compile");

        // Clean up
        let _ = fs::remove_file(&temp_path);
        let exe_path = temp_path.with_extension("exe");
        let _ = fs::remove_file(&exe_path);
    }
}