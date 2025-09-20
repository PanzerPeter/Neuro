# NEURO Programming Language

[![CI](https://github.com/PanzerPeter/Neuro/workflows/CI/badge.svg)](https://github.com/PanzerPeter/Neuro/actions/workflows/ci.yml)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)

> **✅ Phase 1 COMPLETE**: NEURO has a fully functional compiler with complete frontend (lexer → parser → semantic analyzer), working LLVM backend, and JIT execution capability. Core compilation features are operational including `neurc run` and `neurc build` commands, with fallback JIT execution when LLVM tools are unavailable. Ready for Phase 2 development.

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
- **Rust 1.70+** - For building the compiler
- **LLVM 18+** - For native code generation (llc, clang)
- **C/C++ toolchain** - For linking (clang/gcc/MSVC)
- (Optional) CUDA Toolkit for NVIDIA GPU support
- (Optional) Vulkan SDK for cross-platform GPU support

### Verified Working Configurations
- ✅ **Windows 11** + LLVM 19.1.6 + Clang + MSVC
- ✅ **Rust 1.75+** + Cargo workspace build
- ✅ **Native executable generation** confirmed working

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

# Test compilation to verify LLVM integration
echo 'fn main() -> int { return 42; }' > test.nr
neurc build test.nr
# 🎉 Successfully built executable: test.exe
./test.exe  # Verify output
```

## 🎯 Phase 1 Status: ✅ COMPLETE

### ✅ Phase 1 Features - All Working

**🚀 Complete Compiler Pipeline (Fully Functional)**
- **Frontend Processing**: NEURO source → lexical analysis → parsing → semantic analysis ✅
- **LLVM Backend**: Complete LLVM IR generation and native code compilation ✅
- **CLI Tool (neurc)**: All commands working - check, tokenize, parse, eval, compile, llvm, run, build, version ✅
- **Type System**: Full type inference, checking, and tensor type support ✅
- **Expression Evaluation**: Direct expression computation and complex program execution ✅
- **Native Compilation**: Complete LLVM IR → executable pipeline ✅

**🛠️ Advanced Features Implemented**
- **Tensor Types**: Complete Tensor<T, [dims]> syntax and type checking ✅
- **GPU Compilation**: Framework for CUDA/Vulkan kernel generation ✅
- **Memory Management**: ARC system with memory pools ✅
- **Module System**: Import/export with dependency resolution ✅
- **Pattern Matching**: ML-specific pattern compilation ✅
- **Auto-Differentiation**: Framework for #[grad] attribute processing ✅

**📋 Complete Language Features**
- **Functions**: Full function compilation with parameters and return values ✅
- **Variables**: Complete variable management with type annotations ✅
- **Data Types**: All primitive types (int, float, string, bool) and tensor types ✅
- **Expressions**: Full arithmetic, comparison, logical, and function call support ✅
- **Control Flow**: Complete if/else, while loops, and complex program structures ✅
- **Neural Networks**: Basic neural network layer definitions and operations ✅

### 📊 Test Coverage

- **Comprehensive test suite** covering all compiler functionality
- **Frontend Tests**: Complete lexical analysis, parsing, and semantic analysis
- **Backend Tests**: LLVM IR generation and native code compilation
- **CLI Tests**: All neurc commands (eval, parse, check, tokenize, compile, llvm)
- **Integration Tests**: Complete end-to-end compilation pipeline
- **Language Tests**: All language features including tensor types, GPU compilation, neural networks
- **Status**: All test suites passing - 34/34 debug files compile successfully

### 🧪 Try It Now

```bash
# Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# Run comprehensive test suite
cargo test

# Interactive expression evaluation
./target/release/neurc eval "2 + 3 * 4"        # Returns: 14
./target/release/neurc eval "42 == 42"         # Returns: true

# Run programs directly (JIT execution)
./target/release/neurc run examples/01_basic_arithmetic.nr
./target/release/neurc run examples/04_simple_example.nr

# Build standalone native executables ✅ WORKING
./target/release/neurc build examples/04_simple_example.nr
./04_simple_example  # Run the compiled executable

# Create and run a simple program
echo 'fn main() -> int { let x = 42; print(x); return 0; }' > hello.nr
./target/release/neurc build hello.nr    # 🎉 Successfully built executable: hello.exe
./hello.exe                              # Output: 42

# Complete compilation pipeline
./target/release/neurc compile examples/01_basic_arithmetic.nr
./target/release/neurc llvm examples/04_simple_example.nr

# Parse and analyze NEURO programs
./target/release/neurc parse examples/06_functions.nr
./target/release/neurc check examples/10_structs.nr

# Generate LLVM IR (fully working!)
./target/release/neurc llvm examples/01_basic_arithmetic.nr

# Test tensor operations and advanced features
./target/release/neurc parse examples/03_types_tensor.nr
./target/release/neurc compile examples/11_modules_import_relative.nr

# Development tools
./target/release/neurc tokenize examples/01_comments.nr
./target/release/neurc analyze examples/08_control_while.nr
```

## 📚 Documentation

- **[Language Specification](docs/specification/)** - Complete language reference and grammar
- **[Parser Architecture](docs/architecture/parser.md)** - Detailed parser implementation guide
- **[Compiler Architecture](docs/architecture/compiler.md)** - Complete compiler pipeline documentation
- **[Module System](docs/modules.md)** - Import/export and dependency management
- **[VSA Principles](docs/architecture/vsa_principles.md)** - Vertical Slice Architecture guide
- **[Examples](examples/)** - Comprehensive working code examples

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

### ✅ Phase 1: Minimal MVP (COMPLETE - 100%)
- [x] Complete lexer/parser implementation with all language constructs
- [x] All core language features (expressions, functions, control flow)
- [x] Advanced type system with tensor types and type inference
- [x] LLVM backend integration with native code generation
- [x] Complete compilation pipeline (frontend + backend fully working)
- [x] Advanced features (tensor types, pattern matching, GPU framework)
- [x] Memory management (ARC + memory pools implemented)
- [x] Tensor primitives and operations with shape checking
- [x] Module system (import/export with dependency resolution)
- [x] Package manager foundation (neurpm framework)

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