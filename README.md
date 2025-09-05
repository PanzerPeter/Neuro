# NEURO Programming Language

[![CI](https://github.com/PanzerPeter/Neuro/workflows/CI/badge.svg)](https://github.com/PanzerPeter/Neuro/actions/workflows/ci.yml)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)

> **✅ Phase 1 Progress**: NEURO has successfully completed Phase 0 and is actively implementing Phase 1 features. Current capabilities include lexical analysis, syntax parsing, and basic expression evaluation. The language specification and API are subject to change.

**NEURO** is a high-performance, AI-first programming language designed specifically for machine learning and neural network development. Built with Rust and LLVM, NEURO combines the performance of compiled languages with AI/ML-optimized features like automatic differentiation, tensor operations, and GPU acceleration.

## 🚀 Key Features

### AI-First Design
- **Tensor Types**: Built-in tensor types with compile-time shape verification
- **Automatic Differentiation**: `#[grad]` attribute for seamless gradient computation
- **GPU Acceleration**: Native CUDA and Vulkan kernel generation with `#[kernel]`
- **Neural Network DSL**: Declarative model definition with automatic optimization

### Performance & Safety
- **LLVM Backend**: Compiles to optimized native machine code
- **Zero-Cost Abstractions**: High-level constructs with low-level performance
- **Memory Management**: Default ARC with explicit memory pools for high-performance scenarios
- **Static Type System**: Type inference with compile-time optimization opportunities

### Developer Experience
- **Clean Syntax**: Familiar syntax with ML-specific enhancements
- **VSA Architecture**: Modular, maintainable codebase using Vertical Slice Architecture
- **Comprehensive Tooling**: LSP, debugger, package manager, and IDE integration

## 📋 Quick Example

```neuro
// Simple neural network with automatic differentiation
#[grad]
fn neural_network(input: Tensor<f32, [784]>) -> Tensor<f32, [10]> {
    let hidden = relu(linear(input, weights1));
    linear(hidden, weights2)
}

// GPU-accelerated matrix multiplication  
#[kernel(cuda)]
fn fast_matmul(a: Tensor<f32, [M, K]>, b: Tensor<f32, [K, N]>) -> Tensor<f32, [M, N]> {
    // CUDA kernel implementation
}

// Memory-efficient training
#[pool("training")]
fn train_model(model: &mut Model, data: Dataset) {
    for batch in data.batches(32) {
        let loss = compute_loss(model.forward(batch.inputs), batch.targets);
        loss.backward();  // Automatic gradient computation
        optimizer.step();
    }
}
```

## 🛠️ Installation & Setup

### Prerequisites
- Rust 1.70+ 
- LLVM 18
- (Optional) CUDA Toolkit for NVIDIA GPU support
- (Optional) Vulkan SDK for cross-platform GPU support

### From Source
```bash
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# Add to PATH
export PATH=$PWD/target/release:$PATH
```

### Verify Installation
```bash
neurc --version
# NEURO Compiler (neurc) v0.1.0
```

## 🎯 Current Status (Phase 1)

### ✅ Implemented Features

- **Lexical Analysis**: Complete tokenization of NEURO syntax including keywords, operators, literals, and comments
- **Syntax Parsing**: Full AST generation for expressions, statements, and function definitions  
- **Expression Evaluation**: Runtime evaluation of arithmetic, logical, and string operations
- **Control Flow**: if/else statements, while loops, break/continue statements
- **Data Types**: Integers, floats, booleans, strings with type coercion
- **Operators**: All arithmetic (+, -, *, /, %), comparison (==, !=, <, <=, >, >=), and unary (-) operators
- **Functions**: Function definitions with parameters and return types
- **Variables**: Let statements with mutable/immutable bindings
- **Module System**: Import/export functionality with dependency resolution and circular dependency detection

### 🔄 Currently Working On

- Type system with inference engine
- Memory management (ARC runtime)
- LLVM backend integration

### 📊 Test Coverage

- **108 passing tests** with comprehensive coverage
- Lexical analysis: 24 tests
- Parser functionality: 20 tests  
- Expression evaluation: 24 tests
- Integration testing: 16 tests
- Module system: 30 tests (module registry, import resolution, dependency graph)

### 🧪 Try It Now

```bash
# Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build

# Run tests to see current functionality
cargo test

# Check example parsing
cargo run --bin neurc -- check examples/basic_expressions.nr

# Try module system example
cargo run --bin neurc -- check examples/modules/calculator.nr
```

## 📚 Documentation

- **[Language Specification](docs/specification/)** - Complete language reference
- **[Module System](docs/modules.md)** - Import/export and dependency management
- **[Architecture Guide](docs/architecture/)** - VSA principles and compiler design
- **[API Documentation](https://panzerPeter.github.io/Neuro/api-docs/)** - Generated API docs
- **[Examples](examples/)** - Comprehensive code examples

## 🎯 Project Status

### ✅ Phase 0: Project Foundations (COMPLETE)
- [x] Repository scaffolding and CI/CD pipeline
- [x] VSA-compliant project structure (15 feature slices + 5 infrastructure)
- [x] CONTRIBUTING guide with detailed VSA guidelines
- [x] GNU GPL 3.0 license with alpha notice
- [x] Infrastructure components (diagnostics, source-location, shared-types)
- [x] Basic lexical analysis framework
- [x] Compiler driver (neurc) with CLI interface
- [x] Testing framework with unit, integration, and property-based tests
- [x] Benchmarking infrastructure with criterion
- [x] Documentation system and examples

### 🚧 Phase 1: Minimal MVP (IN PROGRESS)
- [x] Complete lexer/parser implementation
- [x] Core language features (control flow, functions, modules)
- [ ] Basic type system with inference
- [ ] Memory management (ARC + pools)
- [ ] Tensor primitives and basic operations
- [ ] LLVM backend integration
- [ ] Package manager (neurpm)

### 📋 Phase 2: AI Optimization (PLANNED)
- [ ] Dual GPU backend (CUDA + Vulkan)
- [ ] Advanced automatic differentiation
- [ ] Neural network DSL
- [ ] Performance optimizations
- [ ] Standard ML libraries

### 📋 Phase 3: Developer Experience (PLANNED)
- [ ] Enhanced LSP server
- [ ] Debugger with tensor inspection
- [ ] VS Code extension
- [ ] Model zoo and examples

## 🏗️ Architecture

NEURO uses **Vertical Slice Architecture (VSA)** to organize code by business capabilities:

```
compiler/
├── infrastructure/          # Shared components
│   ├── shared-types/       # Common types across slices
│   ├── diagnostics/        # Error reporting
│   └── source-location/    # Span tracking
├── lexical-analysis/       # Tokenization (Phase 0 ✅)
├── syntax-parsing/         # AST generation (Phase 1 🚧)
├── semantic-analysis/      # Semantic validation (Phase 1 🚧)
├── type-system/            # Type inference (Phase 1 📋)
├── automatic-differentiation/ # Gradient computation (Phase 2 📋)
├── gpu-compilation/        # CUDA/Vulkan kernels (Phase 2 📋)
└── neural-networks/        # Model DSL (Phase 2 📋)
```

Each slice is:
- **Independent**: Can be developed and tested separately
- **Focused**: Single business capability
- **Clean**: Clear boundaries and minimal dependencies

See [VSA Principles](docs/architecture/vsa_principles.md) for detailed architecture information.

## 🧪 Development

### Building
```bash
cargo build --workspace       # Debug build
cargo build --release         # Release build
```

### Testing
```bash
cargo test --workspace --lib  # Unit tests
cargo test --workspace        # All tests  
cargo bench                   # Benchmarks
```

### Code Quality
```bash
cargo fmt                     # Format code
cargo clippy --workspace      # Lint
```

## 🤝 Contributing

We welcome contributions! Please read our [Contributing Guide](CONTRIBUTING.md) for:

- **Development Workflow**: TDD process with VSA principles
- **Feature Implementation**: 8-step process from design to deployment
- **Testing Strategy**: Unit, integration, and property-based testing
- **Quality Standards**: Code formatting, linting, and performance requirements

### Quick Start for Contributors
1. Fork the repository
2. Pick a task from Phase 1 priorities in [`idea/todo.txt`](idea/todo.txt)
3. Follow the TDD workflow in [`CONTRIBUTING.md`](CONTRIBUTING.md)
4. Submit a pull request

## 📊 Performance Targets

| Metric | Target | Status |
|--------|--------|--------|
| Compilation Time | <5s for typical ML projects | 📋 Phase 1 |
| Training Performance | Within 20% of PyTorch | 📋 Phase 2 |
| Inference Performance | Match/exceed PyTorch | 📋 Phase 2 |
| GPU Utilization | >90% for compute-bound workloads | 📋 Phase 2 |

## 📜 License

NEURO is licensed under the [GNU General Public License v3.0](LICENSE).

**Alpha Development Notice**: This project is in early development. APIs and language features are subject to change without notice. Not recommended for production use.

## 🌟 Roadmap

- **2025 Q1**: Phase 1 MVP with basic compilation
- **2025 Q2**: Phase 2 AI features and GPU support  
- **2025 Q3**: Phase 3 developer tooling
- **2025 Q4**: Phase 4 production deployment features

See [`idea/roadmap.txt`](idea/roadmap.txt) for detailed development timeline.

## 💬 Community

- **Issues**: [GitHub Issues](https://github.com/PanzerPeter/Neuro/issues)
- **Discussions**: [GitHub Discussions](https://github.com/PanzerPeter/Neuro/discussions)
- **Contributing**: See [CONTRIBUTING.md](CONTRIBUTING.md)

---

**Built with ❤️ for the AI/ML community**