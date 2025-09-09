# NEURO Programming Language Documentation

Welcome to the comprehensive documentation for the NEURO programming language - an AI-first systems programming language designed for machine learning workloads with native tensor support and GPU acceleration.

## Quick Navigation

### 📚 Language Reference
- [**Language Overview**](language_reference/README.md) - Complete language guide
- [**Syntax Reference**](language_reference/syntax.md) - Full syntax documentation
- [**Type System**](language_reference/types.md) - Type system and inference
- [**Memory Model**](language_reference/memory.md) - Memory management and ownership
- [**Standard Library**](language_reference/stdlib.md) - Built-in functions and modules

### 🧠 ML & AI Features
- [**Tensor Programming**](ml_features/tensors.md) - Tensor types and operations
- [**Neural Networks**](ml_features/neural_networks.md) - Neural network DSL and model definition
- [**GPU Programming**](ml_features/gpu.md) - CUDA and Vulkan kernels with #[kernel] attribute
- [**Automatic Differentiation**](ml_features/autodiff.md) - Gradient computation with #[grad]
- [**Pattern Matching**](ml_features/patterns.md) - ML-optimized pattern matching

### 🛠️ Developer Tools
- [**Getting Started**](user_guide/getting_started.md) - Installation and first steps
- [**Compiler (neurc)**](tools/neurc.md) - Command-line compiler reference
- [**Package Manager (neurpm)**](tools/neurpm.md) - Package management system
- [**IDE Integration**](tools/ide.md) - Editor support and LSP
- [**Debugging**](tools/debugging.md) - Debugging NEURO programs

### 🏗️ Architecture & Internals
- [**LLVM Backend**](architecture/llvm_backend.md) - Code generation and optimization
- [**Module System**](architecture/modules.md) - Module resolution and dependency management
- [**Compiler Pipeline**](architecture/compiler_pipeline.md) - VSA-based compilation pipeline
- [**Performance**](architecture/performance.md) - Optimization strategies

### 📋 Specifications
- [**Grammar**](specification/grammar.md) - Formal EBNF grammar
- [**Attributes**](specification/attributes.md) - Attribute system (#[grad], #[kernel], etc.)
- [**Type System**](specification/type_system.md) - Complete type system specification

### 🎯 Examples & Tutorials
- [**Examples**](../examples/README.md) - Code examples and demos
- [**Tutorials**](tutorials/README.md) - Step-by-step guides
- [**Best Practices**](guides/best_practices.md) - Coding guidelines
- [**Migration Guides**](guides/migration.md) - From other ML frameworks

## Language Philosophy

NEURO is designed around these core principles:

### 🤖 AI-First Design
Built from the ground up for AI/ML workloads with tensors, neural networks, and GPU acceleration as first-class citizens.

### ⚡ Performance Without Compromise  
Compiles to optimized native code with aggressive optimizations, achieving competitive performance for ML workloads.

### 👩‍💻 Developer Productivity
Clean syntax with type inference, reducing boilerplate while maintaining memory safety and performance.

### 🎯 ML Engineering Focus
Optimized for machine learning development and deployment, with general programming capabilities as needed.

## Implementation Status

| Feature Category | Status | Documentation |
|------------------|---------|---------------|
| **Core Language** | ✅ Implemented | [Language Reference](language_reference/README.md) |
| **Type System** | ✅ Implemented | [Type System](language_reference/types.md) |
| **Memory Management** | ✅ Implemented | [Memory Model](language_reference/memory.md) |
| **Basic Compilation** | ✅ Implemented | [Compiler Guide](tools/neurc.md) |
| **Tensor Types** | ✅ Implemented | [Tensor Programming](ml_features/tensors.md) |
| **Pattern Matching** | ✅ Implemented | [Pattern Matching](ml_features/patterns.md) |
| **Module System** | ✅ Implemented | [Modules](language_reference/modules.md) |
| **GPU Programming** | 🏗️ In Progress | [GPU Programming](ml_features/gpu.md) |
| **Neural Network DSL** | 📅 Phase 2 | [Neural Networks](ml_features/neural_networks.md) |
| **Auto Differentiation** | 📅 Phase 2 | [Autodiff](ml_features/autodiff.md) |
| **ONNX Support** | 📅 Phase 3 | [Interoperability](guides/interoperability.md) |

**Legend**: ✅ Implemented | 🏗️ In Progress | 📅 Planned

## Quick Start

```bash
# Install NEURO
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# Your first NEURO program
echo 'fn main() -> int { return 42; }' > hello.nr
cargo run --bin neurc -- llvm hello.nr

# Explore examples
ls examples/
cargo run --bin neurc -- llvm examples/hello_world/hello.nr
```

## Key Features Highlight

### 🧮 Tensor-Native Programming
```neuro
import std::tensor;

#[grad]  // Enable automatic differentiation
fn neural_layer<const N: usize, const M: usize>(
    input: Tensor<f32, [N]>, 
    weights: Tensor<f32, [N, M]>
) -> Tensor<f32, [M]> {
    let output = input @ weights;  // Matrix multiplication
    return activation(output);
}
```

### 🚀 GPU Kernels
```neuro
#[kernel(gpu = "cuda,vulkan")]
fn matrix_multiply(
    a: &Tensor<f32, [M, K]>,
    b: &Tensor<f32, [K, N]>
) -> Tensor<f32, [M, N]> {
    // Compiled to CUDA/Vulkan compute shaders
    a @ b
}
```

### 🧠 Neural Network DSL
```neuro
model TransformerBlock {
    attention: MultiHeadAttention<512, 8>,
    feedforward: Sequential<[
        Dense<512, 2048>,
        ReLU,
        Dense<2048, 512>
    ]>
}
```

### 📊 Pattern Matching for ML
```neuro
match tensor_shape {
    [batch, seq_len] => process_sequence(tensor),
    [batch, height, width, channels] => process_image(tensor),
    [batch, ...] => process_general(tensor),
}
```

## Contributing to Documentation

We welcome contributions to improve this documentation! Please see [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines.

### Documentation Structure

The documentation follows a hierarchical structure:
- **Language Reference**: Core language features and syntax
- **ML Features**: AI/ML-specific functionality 
- **Developer Tools**: Practical usage guides
- **Architecture**: Internal design and implementation
- **Specifications**: Formal language specifications
- **Tutorials**: Learning-oriented content

### Building Documentation

```bash
# Generate API documentation
cargo doc --open

# Serve documentation locally  
python -m http.server 8000 -d docs/
```

## Support & Community

- 📖 **Documentation**: This comprehensive guide
- 🐛 **Issues**: [GitHub Issues](https://github.com/PanzerPeter/Neuro/issues)
- 💬 **Discussions**: [GitHub Discussions](https://github.com/PanzerPeter/Neuro/discussions)
- 📧 **Contact**: Open an issue for questions

---

**Next**: Start with [Getting Started Guide](user_guide/getting_started.md) or explore the [Language Reference](language_reference/README.md).