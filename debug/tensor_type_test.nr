// Test tensor type parsing
fn test_tensor_types() -> int {
    // Basic tensor types  
    let vector: Tensor<float, [5]> = create_vector();
    let matrix: Tensor<int, [3, 4]> = create_matrix();
    let tensor_3d: Tensor<bool, [2, 3, 4]> = create_tensor();
    
    return 42;
}

fn create_vector() -> Tensor<float, [5]> {
    // Placeholder implementation
    return dummy_tensor();
}

fn create_matrix() -> Tensor<int, [3, 4]> {
    return dummy_tensor();
}

fn create_tensor() -> Tensor<bool, [2, 3, 4]> {
    return dummy_tensor();
}

fn dummy_tensor() -> int {
    return 0;
}

fn main() -> int {
    return test_tensor_types();
}