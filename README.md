# NEURO Programming Language

[![CI](https://github.com/PanzerPeter/Neuro/workflows/CI/badge.svg)](https://github.com/PanzerPeter/Neuro/actions/workflows/ci.yml)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)

> **✅ Phase 1 Major Progress**: NEURO has successfully completed Phase 0 and major Phase 1 features. Current capabilities include complete compilation pipeline (lexer → parser → semantic analyzer → LLVM IR generator), working CLI compiler with type checking, LLVM backend, and comprehensive error reporting. The language specification and API are subject to change.

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

- **Complete Compilation Pipeline**: Lexical analysis → Syntax parsing → Semantic analysis → LLVM IR generation
- **Working CLI Compiler (neurc)**: Full-featured command-line interface with all major subcommands
- **LLVM Backend**: Text-based LLVM IR generator producing valid SSA-form IR from NEURO source code
- **Expression Evaluator**: Direct expression evaluation with `neurc eval` command for interactive testing
- **Semantic Analysis**: Type checking, symbol resolution, scope management, comprehensive error reporting
- **Type System**: Type inference engine with comprehensive type checking for expressions and function calls
- **Code Generation**: Complete expression and statement compilation (arithmetic, comparisons, function calls, variables)
- **Syntax Support**: Functions, variables, control flow (if/else, while), assignments, imports, structs
- **Optimization Framework**: LLVM optimization levels (O0-O3) with pass management infrastructure
- **Developer Experience**: Multiple output formats, verbose mode, detailed analysis commands, LLVM IR output
- **Error Reporting**: Comprehensive error messages with precise source location information
- **Testing Infrastructure**: 160+ tests covering all compiler phases with comprehensive edge case testing
- **Functions**: Function definitions with parameters and return types, fully compiled to LLVM IR
- **Variables**: Let statements with mutable/immutable bindings, proper stack allocation in IR
- **Module System**: Import/export functionality with dependency resolution and circular dependency detection

### 🔄 Currently Working On

- Memory management (ARC runtime implementation)
- Advanced type system features (generics, tensor types)
- Native binary generation from LLVM IR

### 📊 Test Coverage

- **160+ passing tests** with comprehensive coverage
- Lexical analysis: 49 tests (including edge cases and error conditions)
- Syntax parsing: 56+ tests (including expression parsing and error recovery)
- Module system: 42 tests (module registry, import resolution, dependency graph)
- Semantic analysis: 24 tests (complete type checking and symbol resolution)
- LLVM Backend: 9 tests (code generation and optimization)
- CLI Integration: 17 tests (end-to-end command testing)

### 🧪 Try It Now

```bash
# Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# Run tests to see current functionality
cargo test

# Interactive expression evaluation (NEW!)
./target/release/neurc eval "2 + 3 * 4"        # Returns: 14
./target/release/neurc eval "42 == 42"         # Returns: true
./target/release/neurc eval "\"Hello\" + \" World\""  # Returns: "Hello World"

# Compile a NEURO program with full pipeline
./target/release/neurc compile examples/basic_expressions.nr

# Generate LLVM IR from NEURO source
./target/release/neurc llvm examples/basic_expressions.nr

# Generate optimized LLVM IR with output file  
./target/release/neurc llvm examples/basic_expressions.nr -O2 -o output.ll

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

### 🚧 Phase 1: Minimal MVP (NEARLY COMPLETE - ~85% Complete)
- [x] Complete lexer/parser implementation
- [x] Core language features (control flow, functions, modules)
- [x] Basic type system with inference
- [x] LLVM backend integration with IR generation
- [x] Complete compilation pipeline (source → LLVM IR)
- [x] Optimization framework (O0-O3 levels)
- [ ] Memory management (ARC + pools)
- [ ] Tensor primitives and basic operations  
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