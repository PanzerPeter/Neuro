// Simplified neural network demonstration for Phase 1
// Demonstrates: functions, control flow, arithmetic, type checking

fn relu_activation(x: int) -> int {
    if x > 0 {
        return x;
    } else {
        return 0;
    }
}

fn simple_forward_pass(input: int, weight: int) -> int {
    let activated = relu_activation(input * weight);
    return activated;
}

fn neural_network_demo() -> int {
    // Simple neural network simulation
    let input_data = 5;
    let weight = 2;
    let output = simple_forward_pass(input_data, weight);
    return output;
}

fn main() -> int {
    let result = neural_network_demo();
    return result;
}