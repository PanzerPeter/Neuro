# NEURO Installation Guide

## Prerequisites

### System Requirements

- **Operating System**: Windows 10+, Linux (Ubuntu 18.04+), macOS 10.15+
- **Memory**: Minimum 4GB RAM, recommended 8GB+ for ML workloads
- **Storage**: 500MB for compiler, additional space for projects
- **CPU**: x86_64 architecture with SSE4.2 support

### Required Dependencies

- **LLVM 17+**: For code generation backend
- **Rust 1.70+**: For building from source
- **Git**: For version control and package management

### Optional Dependencies

- **CUDA Toolkit 12.0+**: For NVIDIA GPU support
- **Vulkan SDK**: For cross-platform GPU compute
- **Intel MKL** or **OpenBLAS**: For optimized linear algebra

## Installation Methods

### Method 1: Binary Installation (Recommended)

#### Windows

Download and run the installer:

```powershell
# Download latest release
curl -L https://github.com/neuro-lang/neuro/releases/latest/download/neuro-windows-x64.exe -o neuro-installer.exe

# Run installer as administrator
./neuro-installer.exe
```

#### Linux

```bash
# Download and install
curl -L https://github.com/neuro-lang/neuro/releases/latest/download/neuro-linux-x64.tar.gz | tar -xz
sudo mv neuro-linux-x64/bin/* /usr/local/bin/
sudo mv neuro-linux-x64/lib/* /usr/local/lib/

# Or use package manager
sudo apt update
sudo apt install neuro-lang  # Ubuntu/Debian
sudo yum install neuro-lang   # RHEL/CentOS
```

#### macOS

```bash
# Using Homebrew
brew tap neuro-lang/tap
brew install neuro

# Or download directly
curl -L https://github.com/neuro-lang/neuro/releases/latest/download/neuro-macos-x64.tar.gz | tar -xz
sudo mv neuro-macos-x64/bin/* /usr/local/bin/
```

### Method 2: Build from Source

#### Clone Repository

```bash
git clone https://github.com/neuro-lang/neuro.git
cd neuro
```

#### Install LLVM

**Linux (Ubuntu/Debian):**
```bash
sudo apt update
sudo apt install llvm-17 llvm-17-dev clang-17
```

**Linux (RHEL/CentOS):**
```bash
sudo yum install llvm-devel clang-devel
```

**macOS:**
```bash
brew install llvm@17
export LLVM_SYS_170_PREFIX=$(brew --prefix llvm@17)
```

**Windows:**
Download LLVM 17.0+ from https://releases.llvm.org/ and install to `C:\LLVM`

#### Build Compiler

```bash
# Install Rust if not already installed
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Build NEURO compiler
cargo build --release

# Install to system
cargo install --path .
```

## GPU Support Setup

### NVIDIA CUDA Setup

1. **Install CUDA Toolkit**:
   - Download from https://developer.nvidia.com/cuda-downloads
   - Install CUDA 12.0 or later
   - Verify: `nvcc --version`

2. **Configure Environment**:
```bash
export CUDA_HOME=/usr/local/cuda
export PATH=$CUDA_HOME/bin:$PATH
export LD_LIBRARY_PATH=$CUDA_HOME/lib64:$LD_LIBRARY_PATH
```

3. **Test GPU Support**:
```bash
neurc --version --gpu-info
```

### Vulkan Setup

1. **Install Vulkan SDK**:
   - Download from https://vulkan.lunarg.com/
   - Follow platform-specific installation

2. **Verify Installation**:
```bash
vulkaninfo --summary
```

## Package Manager Setup

### Initialize neurpm

```bash
# Create neurpm configuration
neurpm init

# Configure registry (optional, defaults to official registry)
neurpm config registry https://registry.neuro-lang.org

# Login for publishing packages (optional)
neurpm login
```

### Configure Cache

```bash
# Set cache directory (optional)
neurpm config cache-dir ~/.neurpm/cache

# Set maximum cache size
neurpm config cache-max-size 5GB
```

## IDE Integration

### VS Code Extension

1. Install the NEURO Language Extension:
```bash
code --install-extension neuro-lang.neuro-vscode
```

2. Configure settings in `settings.json`:
```json
{
  "neuro.compiler.path": "/usr/local/bin/neurc",
  "neuro.lsp.enable": true,
  "neuro.format.onSave": true,
  "neuro.gpu.enableCuda": true
}
```

### Language Server

Start the language server manually:
```bash
neuro-lsp --stdio
```

## Verification

### Test Installation

Create a simple test file `hello.nr`:

```neuro
fn main() {
    println!("Hello, NEURO!");
}
```

Compile and run:
```bash
neurc compile hello.nr -o hello
./hello
```

### Test Tensor Operations

Create `tensor_test.nr`:

```neuro
fn main() {
    let a: Tensor<f32, [3, 3]> = tensor![
        [1.0, 2.0, 3.0],
        [4.0, 5.0, 6.0],
        [7.0, 8.0, 9.0]
    ];
    
    let b = a.transpose();
    let c = a @ b;
    
    println!("Matrix multiplication result: {}", c);
}
```

Compile and run:
```bash
neurc compile tensor_test.nr -o tensor_test
./tensor_test
```

### Test GPU Support (if available)

Create `gpu_test.nr`:

```neuro
#[kernel]
fn vector_add(a: Tensor<f32, [N]>, b: Tensor<f32, [N]>) -> Tensor<f32, [N]> {
    a + b
}

fn main() {
    let a: Tensor<f32, [1000]> = Tensor::ones();
    let b: Tensor<f32, [1000]> = Tensor::ones();
    
    let result = vector_add(a, b);
    println!("GPU vector addition complete: sum = {}", result.sum());
}
```

Compile with GPU support:
```bash
neurc compile gpu_test.nr --gpu --target cuda -o gpu_test
./gpu_test
```

## Performance Tuning

### Compiler Optimizations

```bash
# Maximum optimization
neurc compile -O3 --target native program.nr

# Profile-guided optimization
neurc compile --pgo --pgo-profile training_data program.nr

# Link-time optimization
neurc compile --lto program.nr
```

### Memory Pool Configuration

Set environment variables:
```bash
export NEURO_POOL_SIZE=2GB
export NEURO_ALIGNMENT=32
export NEURO_GPU_MEMORY=4GB
```

### CPU-Specific Optimizations

```bash
# Detect and use best CPU features
neurc compile --march=native program.nr

# Specific CPU targets
neurc compile --march=skylake program.nr
neurc compile --march=zen3 program.nr
```

## Troubleshooting

### Common Issues

1. **LLVM not found**:
   - Ensure LLVM 17+ is installed
   - Set `LLVM_SYS_170_PREFIX` environment variable

2. **GPU compilation fails**:
   - Verify CUDA/Vulkan installation
   - Check GPU driver compatibility
   - Use `neurc --gpu-info` to diagnose

3. **Tensor operations fail**:
   - Check memory availability
   - Verify tensor shapes match
   - Enable bounds checking for debugging

### Debug Mode

Enable verbose debugging:
```bash
neurc compile --verbose --debug program.nr
export NEURO_LOG=debug
export RUST_BACKTRACE=1
```

### Getting Help

- **Documentation**: https://docs.neuro-lang.org
- **Issues**: https://github.com/neuro-lang/neuro/issues
- **Discussions**: https://github.com/neuro-lang/neuro/discussions
- **Discord**: https://discord.gg/neuro-lang

## Uninstallation

### Binary Installation

**Windows**: Use "Add or Remove Programs"

**Linux/macOS**:
```bash
sudo rm /usr/local/bin/neurc
sudo rm /usr/local/bin/neurpm
sudo rm /usr/local/bin/neuro-lsp
sudo rm -rf /usr/local/lib/neuro
```

### Source Installation

```bash
cargo uninstall neuro
rm -rf ~/.neurpm
```

## Next Steps

1. Read the [Getting Started Guide](getting_started.md)
2. Follow the [Language Tour](language_tour.md)
3. Explore [ML Programming Examples](ml_programming.md)
4. Try [GPU Programming](gpu_programming.md)

Your NEURO installation is now ready for AI/ML development!