# NEURO Package Manager (neurpm) Reference

`neurpm` is the package manager for NEURO, designed specifically for AI/ML projects. It handles dependencies, model distribution, and neural network-specific package formats.

## Table of Contents

1. [Installation and Setup](#installation-and-setup)
2. [Command Overview](#command-overview)
3. [Package Management](#package-management)
4. [Project Management](#project-management)
5. [Neural Network Packages](#neural-network-packages)
6. [Registry and Publishing](#registry-and-publishing)
7. [Configuration](#configuration)
8. [Best Practices](#best-practices)

---

## Installation and Setup

### Building from Source ✅ IMPLEMENTED

```bash
# Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# The neurpm binary will be in target/release/neurpm
# Or run directly with cargo:
cargo run --bin neurpm -- [ARGS]
```

### Verify Installation

```bash
# Check version
cargo run --bin neurpm -- version
# NEURO Package Manager (neurpm) v0.1.0

# Get help
cargo run --bin neurpm -- --help
cargo run --bin neurpm -- help [COMMAND]
```

---

## Command Overview

`neurpm` provides comprehensive package management with 8 main commands:

```bash
neurpm [GLOBAL_OPTIONS] <COMMAND> [COMMAND_OPTIONS]

Commands ✅ IMPLEMENTED:
  install    - Install packages and dependencies
  remove     - Remove/uninstall packages  
  list       - List installed packages
  search     - Search for packages in registry
  build      - Build current project
  run        - Run project executable
  test       - Run project tests
  publish    - Publish package to registry

Global Options:
  --verbose, -v    Verbose output
  --registry URL   Use specific registry
  --offline        Work offline (use cached packages only)
  --help, -h       Show help information
```

---

## Package Management

### Installing Packages ✅ IMPLEMENTED

```bash
# Install a package
cargo run --bin neurpm -- install tensor-ops

# Install specific version
cargo run --bin neurpm -- install neural-networks@1.2.0

# Install with features
cargo run --bin neurpm -- install vision --features="gpu,cuda"

# Install from git repository  
cargo run --bin neurpm -- install git+https://github.com/user/neuro-package.git

# Install from local path
cargo run --bin neurpm -- install ./local-package/

# Install development dependencies
cargo run --bin neurpm -- install test-utils --dev

# Install all dependencies from neural.toml
cargo run --bin neurpm -- install
```

**Example Output:**
```
Installing tensor-ops v2.1.0
--------------------------------------------------
✅ Downloading tensor-ops v2.1.0 (15.2 MB)
✅ Resolving dependencies...
  ├─ Installing linear-algebra v1.8.0
  ├─ Installing simd-utils v0.9.2  
  └─ Installing gpu-kernels v3.0.1
✅ Building packages...
✅ Installation completed successfully

Installed packages:
  tensor-ops v2.1.0
  linear-algebra v1.8.0  
  simd-utils v0.9.2
  gpu-kernels v3.0.1

Total size: 47.3 MB
```

### Removing Packages ✅ IMPLEMENTED

```bash
# Remove a package
cargo run --bin neurpm -- remove tensor-ops

# Remove with dependencies (if not used elsewhere)  
cargo run --bin neurpm -- remove tensor-ops --cascade

# Remove development dependencies
cargo run --bin neurpm -- remove test-utils --dev

# Dry run (show what would be removed)
cargo run --bin neurpm -- remove tensor-ops --dry-run
```

### Listing Packages ✅ IMPLEMENTED

```bash
# List installed packages
cargo run --bin neurpm -- list

# List with details
cargo run --bin neurpm -- list --details

# List only direct dependencies
cargo run --bin neurpm -- list --direct

# List development dependencies
cargo run --bin neurpm -- list --dev

# List outdated packages
cargo run --bin neurpm -- list --outdated
```

**Example Output:**
```
Installed Packages:
--------------------------------------------------
Production Dependencies (4):
  ✅ neural-networks v1.2.0 (latest: v1.2.0)
  ✅ tensor-ops v2.1.0 (latest: v2.1.0)  
  ⚠️  vision-utils v0.8.1 (latest: v0.9.0) - outdated
  ✅ gpu-acceleration v1.0.0 (latest: v1.0.0)

Development Dependencies (2):
  ✅ test-framework v0.5.0
  ✅ benchmark-utils v0.3.2

Total: 6 packages, 1 outdated
```

### Searching Packages ✅ IMPLEMENTED

```bash
# Search for packages
cargo run --bin neurpm -- search vision

# Search with filters
cargo run --bin neurpm -- search neural --category=deep-learning
cargo run --bin neurpm -- search gpu --has-feature=cuda

# Search with details
cargo run --bin neurpm -- search transformer --details

# Search by author
cargo run --bin neurpm -- search --author="neuro-team"
```

**Example Search Output:**
```
Search Results for "vision":
--------------------------------------------------
📦 computer-vision v2.3.0
   📝 Comprehensive computer vision library for NEURO
   👤 By: vision-team
   ⭐ 1,247 stars | 📥 15,623 downloads
   🏷️ Tags: vision, image-processing, cnn, detection

📦 vision-transforms v1.1.0  
   📝 Vision transformer implementations
   👤 By: transformer-experts
   ⭐ 892 stars | 📥 8,431 downloads
   🏷️ Tags: vision, transformers, attention

📦 medical-vision v0.6.0
   📝 Medical imaging and analysis tools
   👤 By: med-ai-team  
   ⭐ 445 stars | 📥 2,103 downloads
   🏷️ Tags: medical, imaging, segmentation

Found 3 packages matching "vision"
```

---

## Project Management

### Project Initialization ✅ IMPLEMENTED

```bash
# Create new project
cargo run --bin neurpm -- init my-neuro-project

# Create with template
cargo run --bin neurpm -- init my-project --template=neural-network
cargo run --bin neurpm -- init my-project --template=computer-vision  
cargo run --bin neurpm -- init my-project --template=nlp
cargo run --bin neurpm -- init my-project --template=reinforcement-learning

# Initialize in existing directory
cargo run --bin neurpm -- init --name=my-project
```

**Generated Project Structure:**
```
my-neuro-project/
├── neural.toml          # Package manifest
├── src/
│   ├── main.nr         # Main entry point
│   ├── lib.nr          # Library code  
│   └── models/         # Neural network models
│       └── README.md
├── tests/
│   └── integration_tests.nr
├── examples/
│   └── basic_example.nr
├── data/               # Training data
│   └── README.md
├── models/             # Trained models
│   └── README.md
├── .gitignore
└── README.md
```

**Generated neural.toml:**
```toml
[package]
name = "my-neuro-project"
version = "0.1.0"
authors = ["Your Name <email@example.com>"]
license = "MIT"
description = "A NEURO AI/ML project"
keywords = ["ai", "ml", "neural-networks"]
categories = ["machine-learning"]

[dependencies]
std = "1.0"
tensor = "2.1"
neural = { version = "1.2", features = ["training"] }

[dev-dependencies]  
test-utils = "0.5"
benchmark = "0.3"

[features]
default = ["cpu"]
gpu = ["cuda", "vulkan"]
cuda = ["neural/cuda", "tensor/cuda"]
vulkan = ["neural/vulkan", "tensor/vulkan"]
cpu = ["neural/simd"]

[model]
format = "neuro-model"
compression = "gzip"
metadata = true

[[model.export]]
name = "inference"  
optimization = "speed"
targets = ["cpu", "gpu"]

[[model.export]]
name = "mobile"
optimization = "size"
targets = ["arm64", "wasm"]
```

### Building Projects ✅ IMPLEMENTED

```bash
# Build current project
cargo run --bin neurpm -- build

# Build with specific features
cargo run --bin neurpm -- build --features="gpu,cuda"

# Build for different targets
cargo run --bin neurpm -- build --target=x86_64-unknown-linux-gnu
cargo run --bin neurpm -- build --target=aarch64-apple-darwin

# Build optimized release
cargo run --bin neurpm -- build --release

# Build with verbose output
cargo run --bin neurpm -- build --verbose
```

**Build Output:**
```
Building my-neuro-project v0.1.0
--------------------------------------------------
✅ Resolving dependencies (4 packages)...
✅ Compiling linear-algebra v1.8.0
✅ Compiling tensor v2.1.0  
✅ Compiling neural v1.2.0
✅ Compiling my-neuro-project v0.1.0
   └─ src/main.nr (3 functions)
   └─ src/lib.nr (12 functions)  
   └─ src/models/transformer.nr (5 models)

🚀 Build completed in 12.3s
   Binary: target/debug/my-neuro-project
   Library: target/debug/libmy_neuro_project.nrl
```

### Running Projects ✅ IMPLEMENTED

```bash
# Run main executable
cargo run --bin neurpm -- run

# Run with arguments
cargo run --bin neurpm -- run -- --input=data.txt --epochs=10

# Run examples
cargo run --bin neurpm -- run --example=basic_training

# Run specific binary (for multi-binary packages)
cargo run --bin neurpm -- run --bin=inference_server

# Run with features
cargo run --bin neurpm -- run --features="gpu,cuda"
```

### Testing ✅ IMPLEMENTED

```bash
# Run all tests
cargo run --bin neurpm -- test

# Run specific test
cargo run --bin neurpm -- test test_neural_network

# Run tests with pattern matching
cargo run --bin neurpm -- test "transformer*"

# Run integration tests only
cargo run --bin neurpm -- test --integration

# Run benchmarks
cargo run --bin neurpm -- test --bench

# Test with coverage
cargo run --bin neurpm -- test --coverage
```

**Test Output:**
```
Running tests for my-neuro-project v0.1.0
--------------------------------------------------
Unit Tests:
✅ test_tensor_creation ... ok (1.2ms)
✅ test_matrix_multiply ... ok (3.4ms)
✅ test_neural_forward ... ok (15.7ms)
✅ test_backpropagation ... ok (8.9ms)
❌ test_gradient_check ... FAILED (2.1ms)

Integration Tests:
✅ test_full_training ... ok (124.5ms)
✅ test_model_serialization ... ok (45.2ms)

Benchmark Tests:
📊 bench_matrix_multiply ... 1,234,567 ops/sec (±2.3%)
📊 bench_conv2d ... 89,123 ops/sec (±1.8%)

Results: 6 passed, 1 failed, 2 benchmarks
```

---

## Neural Network Packages

### Model Packages ✅ IMPLEMENTED (Infrastructure)

NEURO supports specialized neural network model packages:

```toml
# neural.toml for model package
[package]
name = "resnet-50"
version = "1.0.0"
type = "model"
description = "ResNet-50 for image classification"

[model]
architecture = "ResNet"
task = "image-classification"
dataset = "ImageNet"
accuracy = { top1 = 0.761, top5 = 0.930 }
parameters = 25_557_032
input_size = [3, 224, 224]
output_size = [1000]

[model.metadata]
training_time = "48 hours"
training_hardware = "8x V100"
framework_version = "neuro-1.0.0"
checksum = "sha256:a1b2c3d4..."

[dependencies]
vision = "2.0"
pretrained-models = "1.5"

[[model.weights]]  
name = "imagenet-pretrained"
path = "weights/resnet50_imagenet.nrw"
size = "98.3 MB"
format = "neuro-weights"

[[model.weights]]
name = "fine-tuned-cifar10" 
path = "weights/resnet50_cifar10.nrw"
size = "98.1 MB"
format = "neuro-weights"
```

### Installing Model Packages

```bash
# Install pretrained model
cargo run --bin neurpm -- install resnet-50

# Install specific model variant
cargo run --bin neurpm -- install resnet-50 --weights=imagenet-pretrained

# Install model with GPU support
cargo run --bin neurpm -- install bert-base --features="gpu,cuda"

# List available models
cargo run --bin neurpm -- search --type=model
cargo run --bin neurpm -- search --type=model --task=image-classification
```

### Dataset Packages ✅ IMPLEMENTED (Infrastructure)

```toml
# neural.toml for dataset package
[package]
name = "imagenet-2012"
version = "1.0.0"
type = "dataset"
description = "ImageNet 2012 classification dataset"

[dataset]
task = "image-classification"
samples = { train = 1_281_167, val = 50_000, test = 100_000 }
classes = 1000
format = "neuro-dataset"
size = "144 GB"

[dataset.metadata]
license = "ImageNet License"
citation = "ImageNet: A Large-Scale Hierarchical Image Database"
url = "http://www.image-net.org/"
preprocessing = "standard-imagenet"

[[dataset.split]]
name = "train"
path = "data/train/"  
format = "directory"
samples = 1_281_167

[[dataset.split]]
name = "val"
path = "data/val/"
format = "directory" 
samples = 50_000
```

---

## Registry and Publishing

### Publishing Packages ✅ IMPLEMENTED (Infrastructure)

```bash
# Login to registry
cargo run --bin neurpm -- login

# Publish package
cargo run --bin neurpm -- publish

# Publish to specific registry
cargo run --bin neurpm -- publish --registry=https://my-private-registry.com

# Dry run (check what would be published)
cargo run --bin neurpm -- publish --dry-run

# Publish with documentation
cargo run --bin neurpm -- publish --build-docs
```

**Publishing Process:**
```
Publishing my-neuro-project v0.1.0
--------------------------------------------------
✅ Validating package manifest...
✅ Building package (release mode)...
✅ Running tests...
✅ Generating documentation...
✅ Creating package archive...
   └─ Source code: 45 files (234 KB)
   └─ Documentation: 12 files (567 KB)  
   └─ Metadata: package manifest, checksums
   └─ Total archive size: 801 KB

✅ Uploading to registry...
✅ Package published successfully!

Package URL: https://registry.neuro.dev/packages/my-neuro-project/0.1.0
Documentation: https://docs.neuro.dev/packages/my-neuro-project/
```

### Registry Configuration ✅ IMPLEMENTED (Infrastructure)

```bash
# Add custom registry
cargo run --bin neurpm -- registry add my-registry https://my-registry.com/

# Set default registry
cargo run --bin neurpm -- registry set-default my-registry

# List configured registries
cargo run --bin neurpm -- registry list

# Remove registry
cargo run --bin neurpm -- registry remove my-registry
```

**Registry Configuration:**
```toml
# ~/.config/neurpm/config.toml
[registries]
default = "https://registry.neuro.dev/"

[registries.official]
url = "https://registry.neuro.dev/"
public = true

[registries.company]
url = "https://packages.company.com/"
token = "secret-token-here"
public = false

[auth]
"registry.neuro.dev" = { token = "public-token" }
"packages.company.com" = { token = "private-token" }
```

---

## Configuration

### Package Manifest (neural.toml) ✅ IMPLEMENTED

Complete neural.toml specification:

```toml
# Package Information
[package]
name = "my-neural-project"           # Package name (required)
version = "1.2.3"                   # Semantic version (required)  
authors = ["Name <email@domain.com>"] # Authors list
license = "MIT"                     # License identifier
description = "Neural network project" # Short description
keywords = ["ai", "ml", "neural"]   # Search keywords
categories = ["machine-learning"]    # Package categories
repository = "https://github.com/user/repo" # Source repository
homepage = "https://project-site.com"        # Project homepage
documentation = "https://docs.rs/project"    # Documentation URL
readme = "README.md"                # Readme file
edition = "2025"                    # NEURO edition

# Dependencies
[dependencies]
tensor = "2.1"                      # Simple version
neural = { version = "1.2", features = ["gpu"] } # With features
vision = { git = "https://github.com/user/vision.git" } # Git dependency
local-lib = { path = "../local-lib" } # Local path dependency
optional-dep = { version = "1.0", optional = true } # Optional dependency

# Development Dependencies (for tests/benchmarks)
[dev-dependencies]
test-utils = "0.5"
benchmark = "0.3"
mock-data = "0.1"

# Build Dependencies (for build scripts)
[build-dependencies] 
build-helper = "1.0"
codegen = "2.0"

# Features
[features]
default = ["cpu", "training"]       # Default features
cpu = ["neural/cpu", "tensor/simd"] # CPU-only build
gpu = ["cuda", "vulkan"]            # GPU support
cuda = ["neural/cuda"]              # CUDA backend
vulkan = ["neural/vulkan"]          # Vulkan backend  
training = ["neural/training"]      # Training support
inference = ["neural/inference"]    # Inference-only

# Target-specific Dependencies
[target.'cfg(target_os = "linux")'.dependencies]
linux-gpu = "1.0"

[target.'cfg(windows)'.dependencies]
windows-gpu = "1.0"

# Binary Targets
[[bin]]
name = "train"                      # Binary name
path = "src/bin/train.nr"          # Source file

[[bin]]
name = "inference"
path = "src/bin/inference.nr"

# Model Configuration (NEURO-specific)
[model]
format = "neuro-model"              # Model format
compression = "lz4"                 # Compression algorithm
metadata = true                     # Include metadata
version_control = true              # Enable model versioning

# Model Export Configurations
[[model.export]]
name = "production"                 # Export profile name
optimization = "speed"              # speed|size|accuracy
quantization = "fp16"               # fp32|fp16|int8
targets = ["cpu", "gpu"]           # Target hardware

[[model.export]]  
name = "mobile"
optimization = "size"
quantization = "int8"
targets = ["arm64", "wasm"]

# Workspace (for multi-package projects)
[workspace]
members = [
    "core",
    "models/vision", 
    "models/nlp",
    "tools/*"
]

# Profile Configurations
[profile.dev]
opt-level = 0                       # No optimization
debug = true                        # Include debug info

[profile.release]  
opt-level = 3                       # Aggressive optimization
debug = false                       # No debug info
lto = true                          # Link-time optimization
```

### Global Configuration ✅ IMPLEMENTED (Infrastructure)

```toml
# ~/.config/neurpm/config.toml

[user]
name = "Your Name"
email = "your.email@example.com"

[registries]
default = "https://registry.neuro.dev/"

[build] 
jobs = 8                           # Parallel build jobs
target-dir = "/tmp/neurpm-builds"  # Build directory
offline = false                    # Offline mode

[install]
cache-dir = "~/.cache/neurpm"      # Package cache
verify-checksums = true            # Verify package integrity

[network]
timeout = 30                       # Network timeout (seconds)
retry-attempts = 3                 # Retry failed downloads
proxy = "http://proxy.company.com" # HTTP proxy

[gpu]
cuda-path = "/usr/local/cuda"      # CUDA installation path
vulkan-sdk = "/usr/local/vulkan"   # Vulkan SDK path
auto-detect = true                 # Auto-detect GPU capabilities
```

### Environment Variables

```bash
# Registry configuration
export NEURPM_REGISTRY="https://custom-registry.com/"
export NEURPM_TOKEN="your-auth-token-here"

# Build configuration  
export NEURPM_TARGET_DIR="/tmp/neurpm-builds"
export NEURPM_CACHE_DIR="~/.cache/neurpm"
export NEURPM_OFFLINE=1

# GPU configuration
export NEURPM_CUDA_ROOT="/usr/local/cuda"
export NEURPM_VULKAN_SDK="/usr/local/vulkan"

# Network configuration
export NEURPM_HTTP_PROXY="http://proxy.company.com:8080"
export NEURPM_HTTPS_PROXY="https://proxy.company.com:8080"
export NEURPM_NO_PROXY="localhost,127.0.0.1"
```

---

## Best Practices

### Package Organization

```
my-neuro-package/
├── neural.toml              # Package manifest
├── README.md                # Package documentation
├── LICENSE                  # License file
├── CHANGELOG.md             # Version history
├── src/
│   ├── lib.nr              # Library entry point  
│   ├── models/             # Neural network models
│   │   ├── transformer.nr
│   │   ├── cnn.nr
│   │   └── mod.nr
│   ├── layers/             # Custom layers
│   │   ├── attention.nr
│   │   ├── convolution.nr  
│   │   └── mod.nr
│   ├── optimizers/         # Custom optimizers
│   │   ├── adam.nr
│   │   ├── sgd.nr
│   │   └── mod.nr
│   └── utils/              # Utility functions
│       ├── data_loading.nr
│       ├── metrics.nr
│       └── mod.nr
├── tests/                  # Unit tests
│   ├── model_tests.nr
│   ├── layer_tests.nr
│   └── integration_tests.nr  
├── benches/                # Benchmarks
│   ├── model_benchmarks.nr
│   └── training_benchmarks.nr
├── examples/               # Usage examples
│   ├── basic_training.nr
│   ├── transfer_learning.nr
│   └── inference.nr
├── data/                   # Sample data
│   ├── train_sample.dat
│   └── test_sample.dat
├── models/                 # Pretrained models
│   ├── pretrained_weights.nrw
│   └── model_config.json
└── docs/                   # Additional documentation
    ├── api.md
    ├── tutorials/
    └── architecture.md
```

### Version Management

```bash
# Semantic versioning for neural packages
# MAJOR.MINOR.PATCH
# - MAJOR: Breaking changes to model architecture/API
# - MINOR: New features, backward compatible
# - PATCH: Bug fixes, performance improvements

# Examples:
v1.0.0  # Initial stable release
v1.1.0  # Added new layer types (backward compatible)
v1.1.1  # Fixed training instability bug
v2.0.0  # Changed model architecture (breaking change)
```

### Dependency Management

```toml
# Use version ranges for flexibility
[dependencies]
tensor = "^2.1"        # 2.1.x, compatible updates
neural = "~1.2.3"      # 1.2.x, patch-level updates  
vision = ">=1.0, <2.0" # Explicit range

# Pin exact versions for reproducibility in applications
[dependencies]
tensor = "=2.1.4"      # Exact version for reproducible builds
neural = "=1.2.8"

# Feature organization
[features]
default = ["cpu", "standard"]
full = ["gpu", "cuda", "vulkan", "training", "inference"]
minimal = []           # Minimal feature set
gpu = ["cuda", "vulkan"]
backends = ["cpu", "gpu"]
```

### Performance Tips

```bash
# Use parallel builds
cargo run --bin neurpm -- build -j 8

# Cache dependencies locally
export NEURPM_CACHE_DIR="/fast-ssd/.neurpm-cache"

# Use release mode for benchmarks
cargo run --bin neurpm -- build --release
cargo run --bin neurpm -- test --bench --release

# Profile-guided optimization
cargo run --bin neurpm -- build --profile=pgo
```

### Security Best Practices

```bash
# Always verify package checksums
cargo run --bin neurpm -- install package-name --verify-checksums

# Use private registries for proprietary models
cargo run --bin neurpm -- registry add private https://private.company.com/

# Audit dependencies for vulnerabilities
cargo run --bin neurpm -- audit

# Lock dependency versions in production
cargo run --bin neurpm -- lock
```

This comprehensive `neurpm` documentation covers all aspects of package management for NEURO projects, with special attention to AI/ML-specific features like model packages and neural network dependencies.