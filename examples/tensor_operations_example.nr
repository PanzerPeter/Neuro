// NEURO Tensor Operations Example
// Demonstrates the complete tensor type system with broadcasting and operations

import std::tensor;
import std::linalg;

// Tensor type declarations with compile-time shape checking
fn tensor_types_example() -> void {
    // Basic tensor creation with explicit shapes
    let vector: Tensor<f32, [5]> = tensor([1.0, 2.0, 3.0, 4.0, 5.0]);
    let matrix: Tensor<f32, [3, 4]> = zeros([3, 4]);
    let tensor_3d: Tensor<f32, [2, 3, 4]> = ones([2, 3, 4]);
    
    // Shape information is compile-time validated
    println("Vector shape: {}", vector.shape());  // [5]
    println("Matrix shape: {}", matrix.shape());  // [3, 4]
    
    // Type promotion works automatically
    let mixed: Tensor<f64, [3, 4]> = matrix.cast::<f64>();
}

// Broadcasting operations (NumPy-compatible)
fn broadcasting_example() -> void {
    let a: Tensor<f32, [3, 1]> = tensor([[1.0], [2.0], [3.0]]);
    let b: Tensor<f32, [1, 4]> = tensor([[10.0, 20.0, 30.0, 40.0]]);
    
    // Broadcasting: [3, 1] + [1, 4] -> [3, 4]
    let result = a + b;  // Compile-time shape inference
    
    println("Broadcasted result shape: {}", result.shape());  // [3, 4]
    
    // More complex broadcasting
    let x: Tensor<f32, [2, 3, 1]> = ones([2, 3, 1]);
    let y: Tensor<f32, [1, 1, 4]> = ones([1, 1, 4]);
    let z = x * y;  // Result: [2, 3, 4]
}

// Tensor operations and linear algebra
fn tensor_operations_example() -> void {
    let a: Tensor<f32, [3, 4]> = random_normal([3, 4]);
    let b: Tensor<f32, [4, 5]> = random_normal([4, 5]);
    
    // Matrix multiplication with shape checking
    let c = matmul(a, b);  // Result: [3, 5]
    
    // Element-wise operations
    let squared = a * a;  // Element-wise multiplication
    let activated = relu(a);  // ReLU activation
    
    // Reduction operations  
    let sum_all = a.sum();          // Scalar sum
    let sum_axis0 = a.sum(axis=0);  // Sum along axis 0: [4]
    let mean = a.mean();            // Mean value
    
    // Reshape operations (total elements must match)
    let reshaped = a.reshape([12, 1]);  // [3, 4] -> [12, 1]
    let transposed = a.transpose();     // [3, 4] -> [4, 3]
    
    // Indexing and slicing
    let row = a[1, :];      // Get row 1: [4]
    let col = a[:, 2];      // Get column 2: [3] 
    let submat = a[1:3, :]; // Slice rows 1-2: [2, 4]
}

// Advanced tensor manipulations
fn advanced_tensor_ops() -> void {
    let tensor: Tensor<f32, [2, 3, 4]> = random_uniform([2, 3, 4]);
    
    // Squeeze/unsqueeze operations
    let with_ones: Tensor<f32, [2, 1, 3, 1, 4]> = tensor.unsqueeze(1).unsqueeze(3);
    let squeezed = with_ones.squeeze();  // Back to [2, 3, 4]
    
    // Concatenation along different axes
    let a: Tensor<f32, [2, 3]> = ones([2, 3]);
    let b: Tensor<f32, [2, 3]> = zeros([2, 3]);
    
    let concat_rows = concat([a, b], axis=0);  // [4, 3]
    let concat_cols = concat([a, b], axis=1);  // [2, 6]
    
    // Stack operations
    let stacked = stack([a, b], axis=0);  // [2, 2, 3]
}

// Neural network tensor patterns
fn neural_network_patterns() -> void {
    let batch_size = 32;
    let input_dim = 784;   // MNIST flattened
    let hidden_dim = 256;
    let output_dim = 10;
    
    // Input batch: [batch_size, input_dim]
    let inputs: Tensor<f32, [32, 784]> = random_normal([batch_size, input_dim]);
    
    // Weight matrices
    let w1: Tensor<f32, [784, 256]> = xavier_init([input_dim, hidden_dim]);
    let b1: Tensor<f32, [256]> = zeros([hidden_dim]);
    
    let w2: Tensor<f32, [256, 10]> = xavier_init([hidden_dim, output_dim]);
    let b2: Tensor<f32, [10]> = zeros([output_dim]);
    
    // Forward pass
    let hidden = relu(matmul(inputs, w1) + b1);  // Broadcasting bias
    let logits = matmul(hidden, w2) + b2;        // [32, 10]
    
    // Softmax for classification
    let probs = softmax(logits, axis=1);
    
    println("Output probabilities shape: {}", probs.shape());  // [32, 10]
}

// Tensor memory optimization
#[pool("tensor_ops")]
fn optimized_tensor_compute() -> void {
    // Use memory pool for tensor operations
    let large_tensor: Tensor<f32, [1024, 1024]> = zeros([1024, 1024]);
    
    // In-place operations to minimize memory allocation
    large_tensor.add_inplace(1.0);  // Add scalar in-place
    large_tensor.mul_inplace(2.0);  // Multiply by scalar in-place
    
    // Fused operations for efficiency
    let result = fused_linear(large_tensor, 
                             weights: random_normal([1024, 512]),
                             bias: zeros([512]),
                             activation: "relu");
}

fn main() -> int {
    // Initialize tensor backend
    tensor::initialize();
    
    // Run examples
    tensor_types_example();
    broadcasting_example();
    tensor_operations_example();
    advanced_tensor_ops();
    neural_network_patterns();
    optimized_tensor_compute();
    
    return 0;
}