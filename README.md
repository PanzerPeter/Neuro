# NEURO Programming Language

[![CI](https://github.com/PanzerPeter/Neuro/workflows/CI/badge.svg)](https://github.com/PanzerPeter/Neuro/actions/workflows/ci.yml)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)

> **🚧 Phase 1 IN PROGRESS**: NEURO is actively under development with a working frontend compiler (lexer → parser → semantic analyzer) and basic CLI tool. Core compilation features are functional, with LLVM backend in development. Current capabilities include expression evaluation, syntax checking, and semantic analysis. Not yet ready for production use.

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

## 🎯 Phase 1 Status: 🚧 IN PROGRESS (~40% Complete)

### ✅ Phase 1 Features - Currently Working

**🚀 Frontend Compiler Pipeline (Functional)**
- **Frontend Processing**: NEURO source → lexical analysis → parsing → semantic analysis ✅
- **CLI Tool (neurc)**: check, tokenize, parse, eval, version commands working ✅
- **Type System**: Basic type inference and checking for expressions ✅
- **Expression Evaluation**: Direct expression computation with `neurc eval` ✅
- **Syntax Validation**: Complete syntax checking and AST generation ✅
- **Error Reporting**: Source location tracking and basic error messages ✅

**🛠️ Developer Experience**
- **Interactive Development**: `neurc eval "2 + 3 * 4"` for instant expression evaluation ✅
- **Multiple Output Formats**: JSON and pretty-print for frontend phases ✅
- **Syntax Analysis**: Parse and analyze NEURO source files ✅
- **Basic Semantic Analysis**: Symbol tables and type checking for expressions ✅
- **Error Recovery**: Basic error messages with source locations ✅

**📋 Language Features Working**
- **Functions**: Basic function definition and parsing ✅
- **Variables**: Let statements with basic type inference ✅
- **Data Types**: int, float, string, bool (basic support) ✅
- **Expressions**: Arithmetic and comparison operators in expressions ✅
- **Control Flow**: if/else and while loop parsing ✅

### 🎯 Phase 1 Remaining Work

Still needed to complete Phase 1 MVP:
- **LLVM Backend**: Code generation currently has issues
- **Module System**: Import/export not fully functional
- **Advanced Types**: Tensor types, generics not yet implemented
- **Native Compilation**: LLVM IR → executable pipeline

### 📊 Test Coverage

- **Basic test suite** covering core frontend functionality
- **Frontend Tests**: Lexical analysis, parsing, basic semantic analysis
- **CLI Tests**: Core neurc commands (eval, parse, check, tokenize)
- **Expression Tests**: Arithmetic and comparison operations
- **Integration Tests**: Basic end-to-end frontend pipeline
- **Note**: Some tests currently failing; full test suite needs stabilization

### 🧪 Try It Now

```bash
# Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# Run tests to see current functionality
cargo test

# Interactive expression evaluation (Working!)
./target/release/neurc eval "2 + 3 * 4"        # Returns: 14
./target/release/neurc eval "42 == 42"         # Returns: true

# Parse and analyze NEURO programs
./target/release/neurc parse examples/basic_expressions.nr
./target/release/neurc check examples/basic_expressions.nr

# Note: LLVM IR generation currently has issues
# ./target/release/neurc llvm examples/basic_expressions.nr  # Currently fails

# Analyze semantic information
./target/release/neurc analyze examples/basic_expressions.nr

# Tokenize source files
./target/release/neurc tokenize examples/basic_expressions.nr

# Parse and show AST
./target/release/neurc parse examples/basic_expressions.nr

# Check syntax and semantics
./target/release/neurc check examples/basic_expressions.nr

# Verbose mode shows pipeline details
./target/release/neurc --verbose llvm examples/basic_expressions.nr
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

### 🚧 Phase 1: Minimal MVP (IN PROGRESS - ~40%)
- [x] Basic lexer/parser implementation
- [x] Core language features (expressions, basic functions)
- [x] Basic type system with expression type checking
- [ ] LLVM backend integration (in development, currently has issues)
- [ ] Complete compilation pipeline (frontend complete, backend partial)
- [ ] Advanced features (generics, pattern matching, macros)
- [ ] Memory management (ARC + pools)
- [ ] Tensor primitives and operations
- [ ] Module system (import/export)
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
├── syntax-parsing/         # AST generation (Phase 1 ✅)
├── semantic-analysis/      # Semantic validation (Phase 1 ✅)
├── llvm-backend/          # LLVM IR generation (Phase 1 ✅)
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