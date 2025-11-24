# Installation Guide

This guide covers installation of the NEURO compiler on Windows, Linux, and macOS.

## Prerequisites

### Required

- **Rust**: 1.70 or later
- **LLVM**: 18.1.8 (full development package)
- **CMake**: 3.20 or later
- **C Compiler**: MSVC 2022 (Windows), GCC, or Clang (Unix)

### Optional

- **CUDA Toolkit**: 12.0+ for GPU support (Phase 5+, not yet implemented)

## Windows Installation

### 1. Install LLVM 18.1.8

Download the full development package:
- Package: `clang+llvm-18.1.8-x86_64-pc-windows-msvc.tar.xz`
- URL: https://github.com/llvm/llvm-project/releases/tag/llvmorg-18.1.8

**Important**: Do not use the .exe installer as it lacks required development files.

Extract to: `C:\LLVM-1818`

### 2. Set Environment Variables

Open PowerShell as Administrator:

```powershell
# Set LLVM prefix
[System.Environment]::SetEnvironmentVariable('LLVM_SYS_181_PREFIX', 'C:\LLVM-1818', 'Machine')

# Add LLVM to PATH
$machinePath = [System.Environment]::GetEnvironmentVariable('Path', 'Machine')
[System.Environment]::SetEnvironmentVariable('Path', "$machinePath;C:\LLVM-1818\bin", 'Machine')
```

Restart your terminal for changes to take effect.

### 3. Install vcpkg and libxml2

```powershell
cd C:\
git clone https://github.com/Microsoft/vcpkg.git
cd vcpkg
.\bootstrap-vcpkg.bat
.\vcpkg install libxml2:x64-windows-static
.\vcpkg integrate install
```

### 4. Verify LLVM Installation

```cmd
echo %LLVM_SYS_181_PREFIX%
dir %LLVM_SYS_181_PREFIX%\lib\cmake\llvm\LLVMConfig.cmake
vcpkg list | findstr libxml2
```

All commands should succeed without errors.

### 5. Clone and Build NEURO

```powershell
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release
```

### 6. Run Tests

```powershell
cargo test --all
```

All 113 tests should pass.

### 7. Test the Compiler

```powershell
cargo run -p neurc -- check examples/hello.nr
cargo run -p neurc -- compile examples/hello.nr
.\examples\hello.exe
```

### 8. Install Compiler (Optional)

```powershell
cargo install --path compiler/neurc
neurc --version
```

## Linux Installation

### Ubuntu/Debian

#### 1. Install LLVM 18

```bash
wget https://apt.llvm.org/llvm.sh
chmod +x llvm.sh
sudo ./llvm.sh 18
```

#### 2. Set Environment Variable

```bash
export LLVM_SYS_181_PREFIX=/usr/lib/llvm-18
echo 'export LLVM_SYS_181_PREFIX=/usr/lib/llvm-18' >> ~/.bashrc
source ~/.bashrc
```

#### 3. Install Build Dependencies

```bash
sudo apt-get update
sudo apt-get install build-essential cmake git
```

#### 4. Clone and Build NEURO

```bash
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release
```

#### 5. Run Tests

```bash
cargo test --all
```

#### 6. Install Compiler

```bash
cargo install --path compiler/neurc
```

### Arch Linux

```bash
sudo pacman -S llvm18 llvm18-libs cmake git base-devel
export LLVM_SYS_181_PREFIX=/usr/lib/llvm18
echo 'export LLVM_SYS_181_PREFIX=/usr/lib/llvm18' >> ~/.bashrc
```

Then follow steps 4-6 from Ubuntu instructions.

## macOS Installation

### Using Homebrew

#### 1. Install LLVM 18

```bash
brew install llvm@18
```

#### 2. Set Environment Variable

```bash
export LLVM_SYS_181_PREFIX=/opt/homebrew/opt/llvm@18
echo 'export LLVM_SYS_181_PREFIX=/opt/homebrew/opt/llvm@18' >> ~/.zshrc
source ~/.zshrc
```

Note: Path may be `/usr/local/opt/llvm@18` on Intel Macs.

#### 3. Install Build Tools

```bash
xcode-select --install
```

#### 4. Clone and Build NEURO

```bash
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release
```

#### 5. Run Tests

```bash
cargo test --all
```

#### 6. Install Compiler

```bash
cargo install --path compiler/neurc
```

## Troubleshooting

### "No suitable version of LLVM was found"

**Problem**: LLVM_SYS_181_PREFIX not set or points to wrong location.

**Solution**:
- Verify environment variable: `echo $LLVM_SYS_181_PREFIX` (Unix) or `echo %LLVM_SYS_181_PREFIX%` (Windows)
- Ensure LLVM 18.1.8 is installed at that path
- Restart terminal after setting environment variables

### "cannot open input file 'libxml2s.lib'" (Windows)

**Problem**: libxml2 not installed via vcpkg.

**Solution**:
```powershell
cd C:\vcpkg
.\vcpkg install libxml2:x64-windows-static
.\vcpkg integrate install
```

### "LLVMConfig.cmake not found"

**Problem**: Installed .exe installer instead of full tar.xz package.

**Solution**: Download and extract the full development package:
- `clang+llvm-18.1.8-x86_64-pc-windows-msvc.tar.xz` (NOT the .exe)

### Linker errors on Unix

**Problem**: Missing C/C++ standard libraries.

**Solution**:
```bash
# Ubuntu/Debian
sudo apt-get install build-essential

# Arch
sudo pacman -S base-devel

# macOS
xcode-select --install
```

### Tests fail after successful build

**Problem**: Build succeeded but some tests fail.

**Check**:
- Ensure all 113 tests pass: `cargo test --all`
- If tests fail, check GitHub issues or report the failure

## Next Steps

Once installation is complete, see:
- [Quick Start Guide](quick-start.md) - Basic usage
- [Your First Program](first-program.md) - Tutorial

## Updating NEURO

To update to the latest version:

```bash
cd Neuro
git pull origin main
cargo build --release
cargo test --all
cargo install --path compiler/neurc
```

## Uninstalling

To remove NEURO:

```bash
cargo uninstall neurc
rm -rf ~/path/to/Neuro  # Or delete the cloned directory
```

To remove LLVM (optional):
- Windows: Delete `C:\LLVM-1818` and remove from PATH
- Linux: `sudo apt-get remove llvm-18` or equivalent
- macOS: `brew uninstall llvm@18`
