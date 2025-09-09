# NEURO Programming Language - Complete VSA Directory Structure

## Root Directory Structure

```
neuro-lang/
├── README.md                           # Project overview, installation, quick start guide
├── LICENSE                            # GNU GPL 3.0 with alpha development notice
├── CONTRIBUTING.md                    # VSA guidelines, coding standards, development workflow
├── Cargo.toml                         # Rust workspace configuration with VSA feature slices
├── Cargo.lock                         # Dependency lockfile for reproducible builds
├── .gitignore                         # Git ignore patterns for Rust/LLVM artifacts
├── .github/                           # GitHub Actions CI/CD workflows
├── docs/                              # Documentation and specifications
├── tools/                             # Development and build tools
├── examples/                          # Sample NEURO programs and tutorials
├── tests/                             # Integration and end-to-end tests
├── benchmarks/                        # Performance benchmarking suite
├── compiler/                          # Core compiler implementation (VSA slices)
└── runtime/                           # Runtime libraries and support code
```

## Feature Slices (Compiler Core)

```
compiler/
├── Cargo.toml                         # Compiler workspace dependencies and features

├── lexical-analysis/                  # ✅ COMPLETE - Tokenization and lexical processing
│   ├── Cargo.toml                     # Lexer dependencies (unicode-segmentation, logos)
│   ├── src/
│   │   ├── lib.rs                     # Public API for lexical analysis
│   │   ├── tokenizer.rs               # Main tokenizer implementation
│   │   ├── tokens.rs                  # Token type definitions and utilities
│   │   ├── keywords.rs                # NEURO keyword recognition
│   │   ├── operators.rs               # Operator tokenization (#[grad], =>, etc.)
│   │   ├── literals.rs                # Number, string, tensor literal parsing
│   │   ├── identifiers.rs             # Unicode identifier validation
│   │   └── error_recovery.rs          # Lexical error handling and recovery
│   └── tests/
│       ├── tokenizer_tests.rs         # Unit tests for tokenizer
│       ├── unicode_tests.rs           # Unicode identifier tests
│       └── error_recovery_tests.rs    # Error handling tests

├── syntax-parsing/                    # ✅ COMPLETE - AST generation and syntax analysis
│   ├── Cargo.toml                     # Parser dependencies (lalrpop, chumsky)
│   ├── build.rs                       # Parser generator build script
│   ├── src/
│   │   ├── lib.rs                     # Public parsing API
│   │   ├── parser.rs                  # Main parser implementation
│   │   ├── ast/                       # Abstract Syntax Tree definitions
│   │   │   ├── mod.rs                 # AST module exports
│   │   │   ├── expressions.rs         # Expression AST nodes
│   │   │   ├── statements.rs          # Statement AST nodes
│   │   │   ├── types.rs               # Type AST nodes (Tensor<T,[R,C]>)
│   │   │   ├── patterns.rs            # Pattern matching AST
│   │   │   └── attributes.rs          # Attribute AST (#[grad], #[kernel])
│   │   ├── grammar.lalrpop            # LALRPOP grammar definition
│   │   ├── precedence.rs              # Operator precedence rules
│   │   └── error_recovery.rs          # Parse error recovery strategies
│   └── tests/
│       ├── expression_tests.rs        # Expression parsing tests
│       ├── statement_tests.rs         # Statement parsing tests
│       └── error_recovery_tests.rs    # Parse error handling tests

├── semantic-analysis/                 # ✅ COMPLETE - Type checking and semantic validation
│   ├── Cargo.toml                     # Type system dependencies
│   ├── src/
│   │   ├── lib.rs                     # Semantic analysis public API
│   │   ├── type_checker.rs            # Main type checking engine
│   │   ├── type_inference.rs          # Type inference implementation
│   │   ├── constraint_solver.rs       # Type constraint resolution
│   │   ├── tensor_shapes.rs           # Compile-time tensor shape verification
│   │   ├── trait_resolution.rs        # Trait/typeclass resolution
│   │   ├── generics.rs                # Generic type handling
│   │   └── const_evaluation.rs        # Const generic evaluation
│   └── tests/
│       ├── type_inference_tests.rs    # Type inference test suite
│       ├── tensor_shape_tests.rs      # Shape verification tests
│       └── constraint_tests.rs        # Constraint solver tests

├── name-resolution/                   # ⚠️ INTEGRATED - Symbol resolution implemented within semantic-analysis
│   ├── Cargo.toml                     # Symbol table dependencies
│   ├── src/
│   │   ├── lib.rs                     # Name resolution public API
│   │   ├── symbol_table.rs            # Symbol table implementation
│   │   ├── scope_stack.rs             # Lexical scope management
│   │   ├── module_resolver.rs         # Module import resolution
│   │   ├── visibility.rs              # Access control validation
│   │   └── binding_analysis.rs        # Variable binding analysis
│   └── tests/
│       ├── symbol_table_tests.rs      # Symbol table tests
│       ├── scope_tests.rs             # Scope resolution tests
│       └── module_resolution_tests.rs  # Module import tests

├── control-flow/                      # ☐ Control flow analysis and compilation
│   ├── Cargo.toml                     # Control flow dependencies
│   ├── src/
│   │   ├── lib.rs                     # Control flow public API
│   │   ├── cfg_builder.rs             # Control Flow Graph construction
│   │   ├── dominance.rs               # Dominance analysis for optimization
│   │   ├── loop_analysis.rs           # Loop structure analysis
│   │   ├── branch_analysis.rs         # Branch prediction and optimization
│   │   └── reachability.rs            # Dead code detection
│   └── tests/
│       ├── cfg_tests.rs               # Control Flow Graph tests
│       ├── loop_tests.rs              # Loop analysis tests
│       └── reachability_tests.rs      # Dead code detection tests

├── tensor-operations/                 # ☐ Tensor type system and operations
│   ├── Cargo.toml                     # Tensor processing dependencies (ndarray)
│   ├── src/
│   │   ├── lib.rs                     # Tensor operations public API
│   │   ├── tensor_types.rs            # Tensor<T,[R,C]> type definitions
│   │   ├── shape_inference.rs         # Automatic shape inference
│   │   ├── layout_optimization.rs     # Memory layout optimization
│   │   ├── broadcasting.rs            # Tensor broadcasting rules
│   │   ├── operations.rs              # Tensor operation definitions
│   │   └── validation.rs              # Compile-time tensor validation
│   └── tests/
│       ├── tensor_type_tests.rs       # Tensor type system tests
│       ├── shape_inference_tests.rs   # Shape inference tests
│       └── broadcasting_tests.rs      # Broadcasting rule tests

├── automatic-differentiation/         # ☐ #[grad] attribute and AD implementation
│   ├── Cargo.toml                     # AD system dependencies
│   ├── src/
│   │   ├── lib.rs                     # Automatic differentiation public API
│   │   ├── grad_attribute.rs          # #[grad] attribute processing
│   │   ├── reverse_mode.rs            # Reverse-mode AD implementation
│   │   ├── forward_mode.rs            # Forward-mode AD implementation
│   │   ├── gradient_tape.rs           # Gradient computation tape
│   │   ├── derivative_rules.rs        # Mathematical derivative rules
│   │   └── optimization.rs            # AD optimization passes
│   └── tests/
│       ├── grad_attribute_tests.rs    # Gradient attribute tests
│       ├── reverse_mode_tests.rs      # Reverse-mode AD tests
│       └── optimization_tests.rs      # AD optimization tests

├── gpu-compilation/                   # ☐ GPU kernel generation (#[kernel], #[gpu])
│   ├── Cargo.toml                     # GPU compilation dependencies
│   ├── src/
│   │   ├── lib.rs                     # GPU compilation public API
│   │   ├── kernel_attribute.rs        # #[kernel] and #[gpu] processing
│   │   ├── cuda_backend.rs            # CUDA kernel generation
│   │   ├── vulkan_backend.rs          # Vulkan compute shader generation
│   │   ├── memory_coalescing.rs       # GPU memory access optimization
│   │   ├── thread_block_analysis.rs   # GPU thread block optimization
│   │   └── kernel_fusion.rs           # GPU kernel fusion optimization
│   └── tests/
│       ├── cuda_generation_tests.rs   # CUDA kernel tests
│       ├── vulkan_generation_tests.rs # Vulkan shader tests
│       └── optimization_tests.rs      # GPU optimization tests

├── memory-management/                 # ☐ ARC and MemoryPool implementation
│   ├── Cargo.toml                     # Memory management dependencies
│   ├── src/
│   │   ├── lib.rs                     # Memory management public API
│   │   ├── arc_runtime.rs             # Automatic Reference Counting
│   │   ├── memory_pool.rs             # Explicit memory pool API
│   │   ├── gc_collector.rs            # Optional cycle collector
│   │   ├── leak_detection.rs          # Memory leak detection
│   │   ├── allocation_tracking.rs     # Memory allocation profiling
│   │   └── optimization.rs            # Memory layout optimization
│   └── tests/
│       ├── arc_tests.rs               # Reference counting tests
│       ├── memory_pool_tests.rs       # Memory pool tests
│       └── leak_detection_tests.rs    # Leak detection tests

├── llvm-backend/                      # ✅ COMPLETE - LLVM IR generation and optimization
│   ├── Cargo.toml                     # LLVM dependencies (inkwell)
│   ├── src/
│   │   ├── lib.rs                     # LLVM backend public API
│   │   ├── codegen.rs                 # Main code generation
│   │   ├── module_builder.rs          # LLVM module construction
│   │   ├── function_builder.rs        # LLVM function generation
│   │   ├── type_mapping.rs            # NEURO to LLVM type mapping
│   │   ├── intrinsics.rs              # LLVM intrinsic generation
│   │   ├── optimization_passes.rs     # LLVM optimization pipeline
│   │   └── debug_info.rs              # Debug information generation
│   └── tests/
│       ├── codegen_tests.rs           # Code generation tests
│       ├── optimization_tests.rs      # Optimization pass tests
│       └── debug_info_tests.rs        # Debug information tests

├── module-system/                     # ✅ COMPLETE - Import/export and package management
│   ├── Cargo.toml                     # Module system dependencies
│   ├── src/
│   │   ├── lib.rs                     # Module system public API
│   │   ├── import_resolver.rs         # Import statement resolution
│   │   ├── export_analyzer.rs         # Export visibility analysis
│   │   ├── package_loader.rs          # Package loading and caching
│   │   ├── dependency_graph.rs        # Module dependency analysis
│   │   ├── circular_detection.rs      # Circular dependency detection
│   │   └── version_resolution.rs      # Package version resolution
│   └── tests/
│       ├── import_tests.rs            # Import resolution tests
│       ├── export_tests.rs            # Export analysis tests
│       └── dependency_tests.rs        # Dependency resolution tests

├── pattern-matching/                  # ☐ Pattern matching compilation
│   ├── Cargo.toml                     # Pattern matching dependencies
│   ├── src/
│   │   ├── lib.rs                     # Pattern matching public API
│   │   ├── pattern_compiler.rs        # Pattern to decision tree compiler
│   │   ├── exhaustiveness.rs          # Exhaustiveness checking
│   │   ├── decision_tree.rs           # Decision tree generation
│   │   ├── guard_analysis.rs          # Pattern guard handling
│   │   └── optimization.rs            # Pattern matching optimization
│   └── tests/
│       ├── pattern_compiler_tests.rs  # Pattern compilation tests
│       ├── exhaustiveness_tests.rs    # Exhaustiveness tests
│       └── optimization_tests.rs      # Pattern optimization tests

├── macro-expansion/                   # ☐ Template and macro preprocessing
│   ├── Cargo.toml                     # Macro system dependencies
│   ├── src/
│   │   ├── lib.rs                     # Macro expansion public API
│   │   ├── template_engine.rs         # Template processing
│   │   ├── macro_expander.rs          # Macro expansion engine
│   │   ├── hygiene.rs                 # Macro hygiene system
│   │   ├── procedural.rs              # Procedural macro support
│   │   └── attribute_macros.rs        # Attribute macro processing
│   └── tests/
│       ├── template_tests.rs          # Template expansion tests
│       ├── macro_tests.rs             # Macro expansion tests
│       └── hygiene_tests.rs           # Hygiene system tests

├── error-reporting/                   # ☐ Comprehensive error diagnostics
│   ├── Cargo.toml                     # Error reporting dependencies (miette)
│   ├── src/
│   │   ├── lib.rs                     # Error reporting public API
│   │   ├── diagnostic_engine.rs       # Main diagnostic system
│   │   ├── error_formatting.rs        # Rich error message formatting
│   │   ├── span_tracking.rs           # Source span management
│   │   ├── suggestion_engine.rs       # Error correction suggestions
│   │   ├── multi_file_errors.rs       # Cross-file error reporting
│   │   └── json_output.rs             # JSON diagnostic output for IDEs
│   └── tests/
│       ├── diagnostic_tests.rs        # Diagnostic system tests
│       ├── formatting_tests.rs        # Error formatting tests
│       └── suggestion_tests.rs        # Error suggestion tests

└── optimization-passes/               # ☐ Compiler optimization pipeline
    ├── Cargo.toml                     # Optimization dependencies
    ├── src/
    │   ├── lib.rs                     # Optimization passes public API
    │   ├── pass_manager.rs            # Optimization pass management
    │   ├── dead_code_elimination.rs   # Dead code removal
    │   ├── constant_folding.rs        # Compile-time constant evaluation
    │   ├── tensor_fusion.rs           # Tensor operation fusion
    │   ├── vectorization.rs           # SIMD vectorization
    │   ├── inline_expansion.rs        # Function inlining
    │   └── profile_guided.rs          # Profile-guided optimization
    └── tests/
        ├── pass_manager_tests.rs      # Pass management tests
        ├── dce_tests.rs               # Dead code elimination tests
        └── fusion_tests.rs            # Tensor fusion tests
```

## Infrastructure Components

```
compiler/infrastructure/
├── shared-types/                      # ✅ COMPLETE - Common type definitions across slices
│   ├── Cargo.toml                     # Shared types dependencies
│   ├── src/
│   │   ├── lib.rs                     # Shared types public API
│   │   ├── span.rs                    # Source location spans
│   │   ├── identifier.rs              # Identifier representation
│   │   ├── literal.rs                 # Literal value types
│   │   └── attributes.rs              # Attribute representation
│   └── tests/
│       └── shared_types_tests.rs      # Shared type tests

├── source-location/                   # ✅ COMPLETE - Source mapping and position tracking
│   ├── Cargo.toml                     # Source location dependencies
│   ├── src/
│   │   ├── lib.rs                     # Source location public API
│   │   ├── source_map.rs              # Source file mapping
│   │   ├── position.rs                # Line/column position tracking
│   │   ├── span.rs                    # Source span utilities
│   │   └── file_cache.rs              # Source file caching
│   └── tests/
│       └── source_location_tests.rs   # Source location tests

├── diagnostics/                       # ✅ COMPLETE - Diagnostic message infrastructure
│   ├── Cargo.toml                     # Diagnostics dependencies
│   ├── src/
│   │   ├── lib.rs                     # Diagnostics public API
│   │   ├── diagnostic.rs              # Diagnostic message types
│   │   ├── severity.rs                # Error severity levels
│   │   ├── code.rs                    # Diagnostic error codes
│   │   └── collector.rs               # Diagnostic collection
│   └── tests/
│       └── diagnostics_tests.rs       # Diagnostics tests

├── project-config/                    # ☐ Project configuration management
│   ├── Cargo.toml                     # Config management dependencies (serde)
│   ├── src/
│   │   ├── lib.rs                     # Project config public API
│   │   ├── neuro_toml.rs              # neuro.toml configuration parsing
│   │   ├── target_config.rs           # Target platform configuration
│   │   ├── optimization_config.rs     # Optimization level settings
│   │   └── workspace_config.rs        # Multi-package workspace config
│   └── tests/
│       └── config_tests.rs            # Configuration parsing tests

└── build-system/                      # ☐ Incremental compilation infrastructure
    ├── Cargo.toml                     # Build system dependencies
    ├── src/
    │   ├── lib.rs                     # Build system public API
    │   ├── incremental.rs             # Incremental compilation logic
    │   ├── cache_manager.rs           # Compilation cache management
    │   ├── dependency_tracker.rs      # File dependency tracking
    │   ├── fingerprinting.rs          # Source file fingerprinting
    │   └── parallel_build.rs          # Parallel compilation coordination
    └── tests/
        └── build_system_tests.rs      # Build system tests
```

## Command Line Interface

```
compiler/neurc/                       # ✅ COMPLETE - Main compiler driver
├── Cargo.toml                         # CLI dependencies (clap, env_logger)
├── src/
│   ├── main.rs                        # Main entry point and CLI setup
│   ├── cli.rs                         # Command line argument parsing
│   ├── commands/                      # CLI subcommands
│   │   ├── mod.rs                     # Command module exports
│   │   ├── compile.rs                 # Compilation command
│   │   ├── check.rs                   # Syntax/type checking command
│   │   ├── format.rs                  # Code formatting command
│   │   ├── version.rs                 # Version information command
│   │   ├── tokens.rs                  # Token dumping command
│   │   └── lsp.rs                     # LSP server command
│   ├── driver.rs                      # Compilation pipeline orchestration
│   ├── output.rs                      # Output handling and formatting
│   └── logging.rs                     # Structured logging setup
└── tests/
    ├── integration_tests.rs           # CLI integration tests
    └── command_tests.rs               # Individual command tests
```

## Development Tools

```
tools/
├── neurpm/                            # ☐ Package manager
│   ├── Cargo.toml                     # Package manager dependencies
│   ├── src/
│   │   ├── main.rs                    # Package manager main entry
│   │   ├── cli.rs                     # Package manager CLI
│   │   ├── commands/                  # Package management commands
│   │   │   ├── install.rs             # Package installation
│   │   │   ├── publish.rs             # Package publishing
│   │   │   ├── search.rs              # Package searching
│   │   │   └── update.rs              # Package updates
│   │   ├── registry.rs                # Package registry client
│   │   ├── lockfile.rs                # Dependency lockfile management
│   │   └── resolver.rs                # Dependency resolution
│   └── tests/
│       └── neurpm_tests.rs            # Package manager tests

├── neuro-lsp/                         # ☐ Language Server Protocol implementation
│   ├── Cargo.toml                     # LSP server dependencies (tower-lsp)
│   ├── src/
│   │   ├── main.rs                    # LSP server main entry
│   │   ├── server.rs                  # LSP server implementation
│   │   ├── handlers/                  # LSP request handlers
│   │   │   ├── completion.rs          # Code completion
│   │   │   ├── hover.rs               # Hover information
│   │   │   ├── diagnostics.rs         # Real-time diagnostics
│   │   │   ├── definition.rs          # Go-to-definition
│   │   │   └── references.rs          # Find references
│   │   ├── analysis.rs                # Code analysis for LSP
│   │   └── protocol.rs                # LSP protocol utilities
│   └── tests/
│       └── lsp_tests.rs               # LSP server tests

├── neuro-fmt/                         # ☐ Code formatter
│   ├── Cargo.toml                     # Formatter dependencies
│   ├── src/
│   │   ├── main.rs                    # Formatter main entry
│   │   ├── formatter.rs               # Code formatting engine
│   │   ├── rules/                     # Formatting rules
│   │   │   ├── indentation.rs         # Indentation rules
│   │   │   ├── spacing.rs             # Whitespace rules
│   │   │   ├── line_breaks.rs         # Line breaking rules
│   │   │   └── tensor_formatting.rs   # Tensor literal formatting
│   │   ├── config.rs                  # Formatter configuration
│   │   └── cli.rs                     # Formatter CLI
│   └── tests/
│       └── formatter_tests.rs         # Code formatter tests

└── build-tools/                       # ☐ Build and deployment utilities
    ├── cross-compile.sh               # Cross-compilation script
    ├── benchmark-runner.py            # Performance benchmarking
    ├── docs-generator.rs              # Documentation generation
    └── release-builder.sh             # Release packaging script
```

## Runtime Libraries

```
runtime/
├── neuro-std/                         # ☐ Standard library implementation
│   ├── Cargo.toml                     # Standard library dependencies
│   ├── src/
│   │   ├── lib.rs                     # Standard library public API
│   │   ├── collections/               # Data structures
│   │   │   ├── tensor.rs              # Tensor implementations
│   │   │   ├── array.rs               # Dynamic arrays
│   │   │   └── hashmap.rs             # Hash tables
│   │   ├── math/                      # Mathematical functions
│   │   │   ├── linear_algebra.rs      # Linear algebra operations
│   │   │   ├── statistics.rs          # Statistical functions
│   │   │   └── activation.rs          # Neural network activations
│   │   ├── io/                        # Input/output operations
│   │   │   ├── file.rs                # File operations
│   │   │   ├── network.rs             # Network operations
│   │   │   └── serialization.rs       # Data serialization
│   │   └── memory/                    # Memory management
│   │       ├── pool.rs                # Memory pools
│   │       └── gc.rs                  # Garbage collection
│   └── tests/
│       ├── collections_tests.rs       # Collections tests
│       ├── math_tests.rs              # Math library tests
│       └── io_tests.rs                # I/O tests

├── neuro-nn/                          # ☐ Neural network library
│   ├── Cargo.toml                     # NN library dependencies
│   ├── src/
│   │   ├── lib.rs                     # Neural network public API
│   │   ├── layers/                    # Neural network layers
│   │   │   ├── dense.rs               # Dense/fully connected layers
│   │   │   ├── conv.rs                # Convolutional layers
│   │   │   ├── recurrent.rs           # RNN/LSTM/GRU layers
│   │   │   └── attention.rs           # Attention mechanisms
│   │   ├── optimizers/                # Training optimizers
│   │   │   ├── sgd.rs                 # Stochastic Gradient Descent
│   │   │   ├── adam.rs                # Adam optimizer
│   │   │   └── rmsprop.rs             # RMSprop optimizer
│   │   ├── loss/                      # Loss functions
│   │   │   ├── mse.rs                 # Mean Squared Error
│   │   │   ├── cross_entropy.rs       # Cross Entropy Loss
│   │   │   └── custom.rs              # Custom loss functions
│   │   └── metrics/                   # Training metrics
│   │       ├── accuracy.rs            # Classification accuracy
│   │       └── f1_score.rs            # F1 score calculation
│   └── tests/
│       ├── layers_tests.rs            # Layer tests
│       ├── optimizers_tests.rs        # Optimizer tests
│       └── training_tests.rs          # Training pipeline tests

└── neuro-gpu/                         # ☐ GPU runtime support
    ├── Cargo.toml                     # GPU runtime dependencies
    ├── src/
    │   ├── lib.rs                     # GPU runtime public API
    │   ├── cuda/                      # CUDA runtime support
    │   │   ├── context.rs             # CUDA context management
    │   │   ├── memory.rs              # GPU memory management
    │   │   └── kernels.rs             # CUDA kernel execution
    │   ├── vulkan/                    # Vulkan runtime support
    │   │   ├── context.rs             # Vulkan context management
    │   │   ├── compute.rs             # Compute pipeline
    │   │   └── buffers.rs             # Buffer management
    │   ├── device.rs                  # Device detection and selection
    │   └── scheduler.rs               # GPU task scheduling
    └── tests/
        ├── cuda_tests.rs              # CUDA runtime tests
        ├── vulkan_tests.rs            # Vulkan runtime tests
        └── scheduler_tests.rs         # GPU scheduling tests
```

## Testing and Benchmarking

```
tests/                                 # Integration and end-to-end tests
├── integration/                       # Cross-slice integration tests
│   ├── compilation_pipeline.rs        # Full compilation pipeline tests
│   ├── error_reporting.rs             # Error handling integration
│   ├── tensor_operations.rs           # Tensor system integration
│   └── gpu_compilation.rs             # GPU compilation integration
├── e2e/                              # End-to-end system tests
│   ├── simple_programs.rs            # Basic program compilation
│   ├── ml_workloads.rs               # Machine learning programs
│   ├── performance_tests.rs          # Performance regression tests
│   └── interop_tests.rs              # FFI and interoperability tests
└── fixtures/                         # Test data and sample programs
    ├── valid_programs/               # Valid NEURO programs for testing
    ├── invalid_programs/             # Programs with errors for testing
    └── benchmarks/                   # Benchmark programs

benchmarks/                           # Performance benchmarking suite
├── Cargo.toml                        # Benchmarking dependencies (criterion)
├── benches/                          # Criterion benchmarks
│   ├── lexer_performance.rs          # Lexical analysis performance
│   ├── parser_performance.rs         # Parsing performance
│   ├── compilation_speed.rs          # Overall compilation speed
│   ├── tensor_operations.rs          # Tensor operation benchmarks
│   └── gpu_kernels.rs               # GPU kernel performance
├── data/                             # Benchmark input data
│   ├── small_programs/               # Small test programs
│   ├── medium_programs/              # Medium-sized programs
│   └── large_programs/               # Large programs for stress testing
└── scripts/                          # Benchmarking automation
    ├── run_benchmarks.py             # Benchmark execution script
    ├── compare_results.py            # Performance comparison
    └── generate_report.py            # Benchmark reporting
```

## Documentation and Examples

```
docs/                                 # Comprehensive documentation
├── specification/                    # Language specification
│   ├── grammar.md                    # EBNF grammar specification
│   ├── type_system.md                # Type system documentation
│   ├── tensor_types.md               # Tensor type system
│   ├── attributes.md                 # Attribute system (#[grad], #[kernel])
│   └── memory_model.md               # Memory management model
├── user_guide/                       # User documentation
│   ├── installation.md               # Installation instructions
│   ├── getting_started.md            # Quick start guide
│   ├── language_tour.md              # Language feature overview
│   ├── ml_programming.md             # ML programming guide
│   └── gpu_programming.md            # GPU programming guide
├── api/                              # API documentation (generated)
│   └── index.html                    # Auto-generated API docs
└── architecture/                     # Architecture documentation
    ├── vsa_principles.md             # VSA implementation details
    ├── compiler_pipeline.md          # Compilation pipeline overview
    ├── optimization_passes.md        # Optimization strategy
    └── gpu_backend.md                # GPU compilation architecture

examples/
├── hello_world/                       # Basic examples
│   ├── hello.nr                      # Simple hello world
│   ├── variables.nr                  # Variable declarations
│   └── functions.nr                  # Function definitions
│
├── tensor_operations/                 # Tensor examples
│   ├── basic_tensors.nr              # Basic tensor operations
│   ├── broadcasting.nr               # Broadcasting examples
│   ├── linear_algebra.nr             # Linear algebra operations
│   └── gpu_tensors.nr                # GPU tensor operations
│
├── neural_networks/                   # ML examples
│   ├── linear_regression.nr          # Simple linear regression
│   ├── mnist_classifier.nr           # MNIST digit classification
│   ├── cnn_image_classification.nr   # Convolutional neural network
│   ├── transformer.nr                # Transformer architecture
│   └── lstm_language_model.nr        # LSTM language model
│
├── gpu_programming/                   # GPU examples
│   ├── custom_kernels.nr             # Custom GPU kernels
│   ├── matrix_multiplication.nr      # GPU matrix multiplication
│   └── parallel_reduction.nr         # Parallel reduction patterns
│
├── interoperability/                  # Interop examples
│   ├── numpy_integration.nr          # NumPy array integration
│   ├── pytorch_models.nr             # PyTorch model import
│   ├── onnx_export.nr                # ONNX model export
│   └── c_ffi_example.nr              # C FFI integration
│
└── advanced/                          # Advanced examples
    ├── custom_gradients.nr           # Custom gradient implementations
    ├── memory_pools.nr               # Memory pool usage
    ├── distributed_training.nr       # Multi-GPU training
    └── model_serving.nr              # Production model serving

debug/
debug files .nr format