# Installation Guide

This guide covers installation of the Neuro compiler on Linux and macOS.

## Prerequisites

| Requirement | Version | Notes |
|---|---|---|
| Rust | 1.85+ | Install via rustup |
| LLVM | 20 | Development package required |
| C linker | any | `clang`, `gcc`, or system linker |

**Optional:**
- CUDA Toolkit 12+ for GPU support (Phase 5+, not yet implemented)

---

## Arch Linux / CachyOS

```bash
# 1. Install LLVM 20
sudo pacman -S llvm20

# 2. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
rustup component add clippy rustfmt rust-analyzer

# 3. Set LLVM prefix (add to ~/.bashrc or ~/.zshrc for permanence)
export LLVM_SYS_201_PREFIX=/usr/lib/llvm20

# 4. Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# 5. Run tests
cargo test --workspace

# 6. Install the compiler (optional)
cargo install --path compiler/neurc
```

---

## Ubuntu / Debian

```bash
# 1. Install LLVM 20
wget https://apt.llvm.org/llvm.sh
chmod +x llvm.sh
sudo ./llvm.sh 20

# 2. Install build dependencies
sudo apt-get install -y build-essential git

# 3. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# 4. Set LLVM prefix
export LLVM_SYS_201_PREFIX=/usr/lib/llvm-20
echo 'export LLVM_SYS_201_PREFIX=/usr/lib/llvm-20' >> ~/.bashrc
source ~/.bashrc

# 5. Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# 6. Run tests
cargo test --workspace
```

---

## macOS (Homebrew)

```bash
# 1. Install LLVM 20
brew install llvm@20

# 2. Set LLVM prefix
export LLVM_SYS_201_PREFIX=$(brew --prefix llvm@20)
echo "export LLVM_SYS_201_PREFIX=$(brew --prefix llvm@20)" >> ~/.zshrc
source ~/.zshrc

# 3. Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# 4. Install Xcode command-line tools (provides the system linker)
xcode-select --install

# 5. Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# 6. Run tests
cargo test --workspace
```

> **Note (Apple Silicon):** The LLVM prefix is usually `/opt/homebrew/opt/llvm@20`. On Intel Macs it is `/usr/local/opt/llvm@20`. `brew --prefix llvm@20` returns the correct path automatically.

---

## Verifying the Installation

```bash
# Check syntax and types without producing a binary
cargo run -p neurc -- check examples/hello.nr

# Compile to a native executable
cargo run -p neurc -- compile examples/factorial.nr

# Run the compiled binary
./examples/factorial

# After cargo install --path compiler/neurc:
neurc --version
neurc check examples/hello.nr
```

All tests should pass:

```bash
cargo test --workspace
# Expected: 348 tests passing, 0 failing
```

---

## Troubleshooting

### "No suitable version of LLVM was found"

`LLVM_SYS_201_PREFIX` is not set or points to the wrong directory.

```bash
# Verify it is set
echo $LLVM_SYS_201_PREFIX

# Verify it contains an LLVM installation
ls $LLVM_SYS_201_PREFIX/lib/cmake/llvm/LLVMConfig.cmake
```

Make sure the export is in your shell rc file and that you have sourced it in the current session.

### "cargo: command not found"

Rust is installed but the shell has not loaded Cargo's env:

```bash
source ~/.cargo/env
```

Add `source ~/.cargo/env` to your `~/.bashrc` or `~/.zshrc`.

### Linker errors on Linux

Missing C/C++ toolchain:

```bash
# Ubuntu / Debian
sudo apt-get install build-essential

# Arch
sudo pacman -S base-devel

# macOS
xcode-select --install
```

### Tests fail after a successful build

Run with `--no-fail-fast` to see all failures at once:

```bash
cargo test --workspace --no-fail-fast
```

Check GitHub Issues if an unexpected test fails.

---

## Updating Neuro

```bash
cd Neuro
git pull origin main
cargo build --release
cargo test --workspace
cargo install --path compiler/neurc   # Re-install if using the installed binary
```

## Uninstalling

```bash
# Remove the installed binary
cargo uninstall neurc

# Remove the repository
rm -rf /path/to/Neuro

# Remove LLVM (optional)
# Arch:   sudo pacman -R llvm20
# Ubuntu: sudo apt-get remove llvm-20
# macOS:  brew uninstall llvm@20
```

---

Once installation is complete, continue with:
- [Quick Start Guide](quick-start.md)
- [Your First Program](first-program.md)
