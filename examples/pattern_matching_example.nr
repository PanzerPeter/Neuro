// NEURO Pattern Matching Example
// Demonstrates advanced pattern matching with ML-specific patterns

import std::pattern;

// Enum for ML model types
enum ModelType {
    LinearRegression { features: int },
    LogisticRegression { features: int, regularization: f32 },
    NeuralNetwork { 
        layers: [int], 
        activation: String,
        dropout: Option<f32>
    },
    ConvNet {
        channels: [int],
        kernel_sizes: [int],
        pooling: PoolingType
    }
}

enum PoolingType {
    Max,
    Average,
    Adaptive { output_size: [int] }
}

// Tensor shape pattern matching
fn classify_tensor_by_shape(tensor: Tensor<f32, [?, ?, ?]>) -> String {
    match tensor.shape() {
        // Image batch patterns
        [batch, 224, 224] => "ImageNet batch",
        [batch, 28, 28] => "MNIST batch", 
        [batch, height, width] if height == width => "Square image batch",
        
        // Sequence patterns
        [batch, seq_len, embed_dim] if embed_dim == 512 => "Transformer embeddings",
        [batch, seq_len, embed_dim] => "General sequence data",
        
        // Tensor shape wildcards
        [_, _, dim] if dim > 1000 => "High dimensional data",
        
        // Fallback
        _ => "Unknown tensor format"
    }
}

// Pattern matching on ML data structures
fn process_model(model: ModelType) -> f32 {
    match model {
        // Simple patterns
        LinearRegression { features } => {
            println("Linear model with {} features", features);
            features as f32 * 0.1
        },
        
        // Guard patterns
        LogisticRegression { features, regularization } if regularization > 0.0 => {
            println("Regularized logistic regression");
            features as f32 * regularization
        },
        
        LogisticRegression { features, regularization: 0.0 } => {
            println("Unregularized logistic regression");
            features as f32
        },
        
        // Nested patterns
        NeuralNetwork { 
            layers: [input, ..hidden, output], 
            activation,
            dropout: Some(rate)
        } if rate < 0.5 => {
            println("NN: {} -> {:?} -> {}, {}, dropout={}", 
                   input, hidden, output, activation, rate);
            (input + output) as f32
        },
        
        NeuralNetwork { layers, activation, dropout: None } => {
            let total_params = estimate_params(layers);
            println("NN without dropout: {} params, activation: {}", 
                   total_params, activation);
            total_params
        },
        
        // Nested enum patterns
        ConvNet { channels, kernel_sizes, pooling: Max } => {
            println("ConvNet with max pooling");
            channels.len() as f32
        },
        
        ConvNet { 
            channels, 
            kernel_sizes,
            pooling: Adaptive { output_size: [h, w] }
        } => {
            println("ConvNet with adaptive pooling {}x{}", h, w);
            (h * w) as f32
        },
        
        // Catch-all with variable binding
        other => {
            println("Other model type: {:?}", other);
            0.0
        }
    }
}

// Array/slice pattern matching
fn analyze_layer_config(layers: [int]) -> String {
    match layers {
        // Empty network
        [] => "No layers",
        
        // Single layer
        [single] => format("Single layer: {}", single),
        
        // Two layers
        [input, output] => format("Simple: {} -> {}", input, output),
        
        // Three layers with specific sizes
        [784, 256, 10] => "MNIST classifier",
        [784, 512, 256, 10] => "Deep MNIST classifier",
        
        // Variable patterns
        [input, ..hidden, output] if hidden.len() > 2 => 
            format("Deep network: {} -> {} hidden -> {}", 
                   input, hidden.len(), output),
        
        [input, hidden, output] => 
            format("Simple NN: {} -> {} -> {}", input, hidden, output),
        
        // Pattern with conditions
        [first, ..rest] if first > 1000 => "High-dimensional input",
        
        _ => "Complex architecture"
    }
}

// Tuple pattern matching for coordinate data
fn classify_point(point: (f32, f32, Option<String>)) -> String {
    match point {
        // Specific coordinates
        (0.0, 0.0, _) => "Origin",
        (x, 0.0, _) if x > 0.0 => "Positive X-axis",
        (0.0, y, _) if y > 0.0 => "Positive Y-axis",
        
        // Quadrant detection
        (x, y, None) if x > 0.0 && y > 0.0 => "First quadrant",
        (x, y, None) if x < 0.0 && y > 0.0 => "Second quadrant",
        (x, y, None) if x < 0.0 && y < 0.0 => "Third quadrant", 
        (x, y, None) if x > 0.0 && y < 0.0 => "Fourth quadrant",
        
        // With labels
        (_, _, Some(label)) => format("Labeled point: {}", label),
        
        _ => "Other point"
    }
}

// Or-patterns for multiple matching
fn classify_activation(name: String) -> String {
    match name {
        "relu" | "ReLU" | "RELU" => "ReLU activation",
        "sigmoid" | "Sigmoid" => "Sigmoid activation", 
        "tanh" | "Tanh" => "Tanh activation",
        "gelu" | "GELU" | "swish" => "Advanced activation",
        _ => "Unknown activation"
    }
}

// Tensor shape pattern matching for ML operations
fn get_operation_type(input_shape: [int], weight_shape: [int]) -> String {
    match (input_shape, weight_shape) {
        // Linear/Dense layer patterns
        ([batch, in_features], [in_features2, out_features]) 
            if in_features == in_features2 => 
            format("Linear: {}x{} -> {}x{}", batch, in_features, batch, out_features),
        
        // Convolution patterns  
        ([batch, channels, height, width], [out_channels, in_channels, kh, kw])
            if channels == in_channels =>
            format("Conv2d: {}x{}x{}x{} with {}x{} kernel", 
                   batch, channels, height, width, kh, kw),
        
        // Matrix multiplication
        ([m, k], [k2, n]) if k == k2 => 
            format("MatMul: {}x{} @ {}x{} -> {}x{}", m, k, k2, n, m, n),
        
        // Broadcasting patterns
        ([batch, seq_len, d_model], [d_model2]) if d_model == d_model2 =>
            "Bias addition (broadcasting)",
        
        _ => "Incompatible shapes"
    }
}

// Range patterns for hyperparameter validation
fn validate_hyperparameter(name: String, value: f32) -> bool {
    match (name, value) {
        ("learning_rate", 0.0001..=0.1) => true,
        ("dropout", 0.0..=0.9) => true,
        ("momentum", 0.0..=1.0) => true,
        ("weight_decay", 0.0..=0.01) => true,
        ("temperature", 0.1..=10.0) => true,
        _ => false
    }
}

fn main() -> int {
    // Test tensor shape classification
    let img_batch: Tensor<f32, [32, 224, 224]> = zeros([32, 224, 224]);
    println(classify_tensor_by_shape(img_batch));
    
    // Test model pattern matching
    let model1 = LinearRegression { features: 784 };
    let model2 = NeuralNetwork { 
        layers: [784, 512, 256, 10],
        activation: "relu",
        dropout: Some(0.2)
    };
    
    println("Model 1 complexity: {}", process_model(model1));
    println("Model 2 complexity: {}", process_model(model2));
    
    // Test layer configuration analysis
    println(analyze_layer_config([784, 512, 256, 10]));
    println(analyze_layer_config([784, 10]));
    
    // Test coordinate classification
    println(classify_point((3.0, 4.0, Some("training_point"))));
    println(classify_point((-1.0, 2.0, None)));
    
    // Test activation function classification
    println(classify_activation("ReLU"));
    println(classify_activation("gelu"));
    
    // Test shape compatibility
    println(get_operation_type([32, 784], [784, 256]));
    println(get_operation_type([1, 3, 224, 224], [64, 3, 7, 7]));
    
    // Test hyperparameter validation
    println("Learning rate 0.01 valid: {}", 
           validate_hyperparameter("learning_rate", 0.01));
    println("Dropout 1.5 valid: {}", 
           validate_hyperparameter("dropout", 1.5));
    
    return 0;
}

// Helper function for estimating parameters
fn estimate_params(layers: [int]) -> f32 {
    let mut total = 0.0;
    for i in 0..(layers.len() - 1) {
        total += (layers[i] * layers[i + 1]) as f32;
    }
    total
}