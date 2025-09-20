//! End-to-end tests for ML workload simulations
//! Tests basic ML algorithms and neural network concepts using Phase 1 features

use std::process::Command;
use std::fs;
use tempfile::NamedTempFile;

/// Test basic linear algebra operations
#[test]
fn test_basic_linear_algebra() {
    let source = r#"
fn vector_dot_product(size: int) -> int {
    // Simulate dot product of two vectors: [1,2,3,4] · [5,6,7,8]
    let mut result = 0;
    let mut i = 0;
    while i < size {
        let a = i + 1;
        let b = i + 5;
        result = result + (a * b);
        i = i + 1;
    }
    return result; // Should be 1*5 + 2*6 + 3*7 + 4*8 = 70
}

fn matrix_vector_multiply(rows: int, cols: int) -> int {
    // Simulate matrix-vector multiplication
    let mut result = 0;
    let mut row = 0;
    while row < rows {
        let mut row_sum = 0;
        let mut col = 0;
        while col < cols {
            let matrix_elem = row * cols + col + 1;
            let vector_elem = col + 1;
            row_sum = row_sum + (matrix_elem * vector_elem);
            col = col + 1;
        }
        result = result + row_sum;
        row = row + 1;
    }
    return result;
}

fn main() -> int {
    let dot_result = vector_dot_product(4);
    let matvec_result = matrix_vector_multiply(3, 4);
    return dot_result + matvec_result;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Linear algebra operations should compile\nstderr: {}",
            String::from_utf8_lossy(&output.stderr));

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

/// Test activation functions common in neural networks
#[test]
fn test_activation_functions() {
    let source = r#"
fn relu(x: int) -> int {
    if x > 0 {
        return x;
    }
    return 0;
}

fn step_function(x: int) -> int {
    if x >= 0 {
        return 1;
    }
    return 0;
}

fn leaky_relu(x: int) -> int {
    if x > 0 {
        return x;
    }
    return x / 10; // Simplified leaky ReLU with integer division
}

fn apply_activation_to_layer(size: int, activation_type: int) -> int {
    let mut result = 0;
    let mut i = 0;
    while i < size {
        let input = i - 2; // Some inputs will be negative
        let activated = 0;

        if activation_type == 1 {
            activated = relu(input);
        } else if activation_type == 2 {
            activated = step_function(input);
        } else {
            activated = leaky_relu(input);
        }

        result = result + activated;
        i = i + 1;
    }
    return result;
}

fn main() -> int {
    let relu_layer = apply_activation_to_layer(5, 1);
    let step_layer = apply_activation_to_layer(5, 2);
    let leaky_layer = apply_activation_to_layer(5, 3);

    return relu_layer + step_layer + leaky_layer;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Activation functions should compile");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

/// Test simple neural network forward pass simulation
#[test]
fn test_neural_network_forward_pass() {
    let source = r#"
fn dense_layer_forward(input_size: int, output_size: int, input_sum: int) -> int {
    // Simulate dense layer: output = weights * input + bias
    let mut output_sum = 0;
    let mut output_idx = 0;

    while output_idx < output_size {
        let mut neuron_sum = 0;
        let mut input_idx = 0;

        while input_idx < input_size {
            let weight = (output_idx * input_size + input_idx + 1); // Simulated weight
            let input_val = input_sum / input_size; // Simplified input value
            neuron_sum = neuron_sum + (weight * input_val);
            input_idx = input_idx + 1;
        }

        let bias = output_idx + 1; // Simulated bias
        neuron_sum = neuron_sum + bias;

        // Apply ReLU activation
        if neuron_sum > 0 {
            output_sum = output_sum + neuron_sum;
        }

        output_idx = output_idx + 1;
    }

    return output_sum;
}

fn neural_network_forward(input_data: int) -> int {
    // Simulate a simple 3-layer neural network
    let layer1_output = dense_layer_forward(4, 8, input_data);  // 4 -> 8 neurons
    let layer2_output = dense_layer_forward(8, 4, layer1_output); // 8 -> 4 neurons
    let layer3_output = dense_layer_forward(4, 1, layer2_output); // 4 -> 1 output

    return layer3_output;
}

fn main() -> int {
    let input_data = 10;
    return neural_network_forward(input_data);
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Neural network forward pass should compile");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

/// Test gradient computation simulation (simplified for Phase 1)
#[test]
fn test_gradient_computation_simulation() {
    let source = r#"
fn compute_loss_gradient(predicted: int, actual: int) -> int {
    // Simplified gradient computation: derivative of MSE loss
    let error = predicted - actual;
    return 2 * error; // Derivative of (predicted - actual)^2
}

fn update_weights_simulation(weight: int, gradient: int, learning_rate: int) -> int {
    // Simplified weight update: weight = weight - learning_rate * gradient
    let update = (learning_rate * gradient) / 100; // Scale down for integer math
    return weight - update;
}

fn backpropagation_simulation(num_layers: int, initial_gradient: int) -> int {
    // Simulate backpropagation through layers
    let mut gradient = initial_gradient;
    let mut layer = num_layers;
    let mut total_weight_updates = 0;

    while layer > 0 {
        // Simulate gradient flowing backward through layer
        let layer_gradient = gradient * layer; // Simplified chain rule

        // Update weights in this layer
        let weight_update = update_weights_simulation(10, layer_gradient, 1);
        total_weight_updates = total_weight_updates + weight_update;

        // Propagate gradient to previous layer
        gradient = layer_gradient / 2; // Simplified gradient propagation
        layer = layer - 1;
    }

    return total_weight_updates;
}

fn main() -> int {
    let predicted = 8;
    let actual = 5;
    let loss_gradient = compute_loss_gradient(predicted, actual);
    let weight_updates = backpropagation_simulation(3, loss_gradient);

    return weight_updates;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Gradient computation simulation should compile");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

/// Test optimization algorithms simulation
#[test]
fn test_optimization_algorithms() {
    let source = r#"
fn sgd_optimizer(weight: int, gradient: int, learning_rate: int) -> int {
    // Stochastic Gradient Descent
    return weight - (learning_rate * gradient) / 100;
}

fn momentum_optimizer(weight: int, gradient: int, momentum: int, learning_rate: int) -> int {
    // Simplified momentum optimizer
    let velocity = (momentum * 9) / 10 + gradient; // 0.9 * momentum + gradient
    return weight - (learning_rate * velocity) / 100;
}

fn adam_optimizer_simulation(weight: int, gradient: int, step: int) -> int {
    // Extremely simplified Adam optimizer simulation
    let beta1 = 9; // 0.9 in integer form
    let beta2 = 99; // 0.99 in integer form

    let m = (beta1 * 0 + (10 - beta1) * gradient) / 10; // Moving average of gradients
    let v = (beta2 * 0 + (100 - beta2) * gradient * gradient) / 100; // Moving average of squared gradients

    let learning_rate = 1;
    let update = (learning_rate * m) / (v + 1); // Simplified division

    return weight - update;
}

fn optimize_network(num_weights: int, optimizer_type: int) -> int {
    let mut total_updated_weights = 0;
    let mut weight_idx = 0;

    while weight_idx < num_weights {
        let weight = weight_idx + 5; // Initial weight value
        let gradient = weight_idx + 1; // Simulated gradient
        let updated_weight = 0;

        if optimizer_type == 1 {
            updated_weight = sgd_optimizer(weight, gradient, 1);
        } else if optimizer_type == 2 {
            updated_weight = momentum_optimizer(weight, gradient, 2, 1);
        } else {
            updated_weight = adam_optimizer_simulation(weight, gradient, weight_idx);
        }

        total_updated_weights = total_updated_weights + updated_weight;
        weight_idx = weight_idx + 1;
    }

    return total_updated_weights;
}

fn main() -> int {
    let sgd_result = optimize_network(5, 1);
    let momentum_result = optimize_network(5, 2);
    let adam_result = optimize_network(5, 3);

    return sgd_result + momentum_result + adam_result;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Optimization algorithms should compile");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

/// Test convolutional operations simulation
#[test]
fn test_convolution_simulation() {
    let source = r#"
fn apply_convolution_filter(input_height: int, input_width: int, filter_size: int) -> int {
    // Simulate 2D convolution operation
    let output_height = input_height - filter_size + 1;
    let output_width = input_width - filter_size + 1;
    let mut total_output = 0;

    let mut out_row = 0;
    while out_row < output_height {
        let mut out_col = 0;
        while out_col < output_width {
            let mut conv_sum = 0;

            // Apply filter
            let mut filter_row = 0;
            while filter_row < filter_size {
                let mut filter_col = 0;
                while filter_col < filter_size {
                    let input_row = out_row + filter_row;
                    let input_col = out_col + filter_col;

                    let input_val = input_row * input_width + input_col + 1; // Simulated input
                    let filter_val = filter_row * filter_size + filter_col + 1; // Simulated filter weight

                    conv_sum = conv_sum + (input_val * filter_val);
                    filter_col = filter_col + 1;
                }
                filter_row = filter_row + 1;
            }

            total_output = total_output + conv_sum;
            out_col = out_col + 1;
        }
        out_row = out_row + 1;
    }

    return total_output;
}

fn max_pooling_simulation(input_size: int, pool_size: int) -> int {
    // Simulate max pooling operation
    let output_size = input_size / pool_size;
    let mut pooled_sum = 0;

    let mut out_idx = 0;
    while out_idx < output_size {
        let mut max_val = 0;
        let mut pool_idx = 0;

        while pool_idx < pool_size {
            let input_idx = out_idx * pool_size + pool_idx;
            let input_val = input_idx + 1; // Simulated input value

            if input_val > max_val {
                max_val = input_val;
            }
            pool_idx = pool_idx + 1;
        }

        pooled_sum = pooled_sum + max_val;
        out_idx = out_idx + 1;
    }

    return pooled_sum;
}

fn main() -> int {
    let conv_result = apply_convolution_filter(5, 5, 3); // 5x5 input, 3x3 filter
    let pool_result = max_pooling_simulation(8, 2); // Pool size 2

    return conv_result + pool_result;
}
"#;

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path().with_extension("nr");
    fs::write(&temp_path, source).expect("Failed to write temp file");

    let output = Command::new("./target/release/neurc")
        .args(&["build", temp_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute neurc build");

    assert!(output.status.success(), "Convolution simulation should compile");

    // Clean up
    let _ = fs::remove_file(&temp_path);
    let exe_path = temp_path.with_extension("exe");
    let _ = fs::remove_file(&exe_path);
}

/// Test the neural network demo from debug directory
#[test]
fn test_neural_network_demo_integration() {
    // Test that the existing neural network demo compiles and runs
    let output = Command::new("./target/release/neurc")
        .args(&["build", "debug/neural_network_demo.nr"])
        .output()
        .expect("Failed to execute neurc build on neural_network_demo.nr");

    assert!(output.status.success(),
            "Neural network demo should compile\nstderr: {}",
            String::from_utf8_lossy(&output.stderr));

    // Test execution
    let exe_path = "debug/neural_network_demo.exe";
    if std::path::Path::new(exe_path).exists() {
        let exec_output = Command::new(exe_path)
            .output()
            .expect("Failed to execute neural network demo");

        assert!(exec_output.status.success(), "Neural network demo should execute successfully");

        // Clean up
        let _ = fs::remove_file(exe_path);
    }
}

#[cfg(test)]
mod ml_validation_tests {
    use super::*;

    /// Test numerical stability in ML computations
    #[test]
    fn test_numerical_stability() {
        let source = r#"
fn safe_division(numerator: int, denominator: int) -> int {
    if denominator == 0 {
        return 0; // Avoid division by zero
    }
    return numerator / denominator;
}

fn clamp_value(value: int, min_val: int, max_val: int) -> int {
    if value < min_val {
        return min_val;
    }
    if value > max_val {
        return max_val;
    }
    return value;
}

fn stable_computation(input: int) -> int {
    let intermediate = input * 1000; // Large computation
    let clamped = clamp_value(intermediate, -10000, 10000);
    let result = safe_division(clamped, 100);
    return result;
}

fn main() -> int {
    let test1 = stable_computation(5);
    let test2 = stable_computation(-3);
    let test3 = stable_computation(20); // Should be clamped

    return test1 + test2 + test3;
}
"#;

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let temp_path = temp_file.path().with_extension("nr");
        fs::write(&temp_path, source).expect("Failed to write temp file");

        let output = Command::new("./target/release/neurc")
            .args(&["build", temp_path.to_str().unwrap()])
            .output()
            .expect("Failed to execute neurc build");

        assert!(output.status.success(), "Numerical stability tests should compile");

        // Clean up
        let _ = fs::remove_file(&temp_path);
        let exe_path = temp_path.with_extension("exe");
        let _ = fs::remove_file(&exe_path);
    }
}