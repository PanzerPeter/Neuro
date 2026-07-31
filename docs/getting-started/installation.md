# Installation Guide

This guide covers installation of the Neuro compiler on Linux, macOS, and Windows —
the three platforms CI builds, tests, and ships release binaries for.

## Prerequisites

| Requirement | Version | Notes |
|---|---|---|
| Rust | 1.85+ | Install via rustup |
| LLVM | 20 | Development package required (headers + `llvm-config` + link libraries) |
| C linker | any | `clang`, `gcc`, or the MSVC linker from Visual Studio Build Tools |

**Optional:**
- MLIR 20 + a matching libclang 20 for the experimental MLIR backend (1D+) — see [MLIR Backend](#optional-mlir-backend-phase-18) below. Not needed for a normal build.
- CUDA Toolkit 12+ for GPU support (Phase 4+, not yet implemented)

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

## Windows (MSVC)

Windows needs a **full LLVM 20 development build**. The official LLVM Windows
installer ships only Clang and `LLVM-C.dll` — no `llvm-config.exe`, no headers,
no static libraries — so `llvm-sys` cannot build against it. Use a packaged dev
build instead; CI uses [vovkos/llvm-package-windows](https://github.com/vovkos/llvm-package-windows).

```powershell
# 1. Install the MSVC toolchain (C++ build tools + Windows SDK)
winget install --id Microsoft.VisualStudio.2022.BuildTools

# 2. Install Rust (MSVC toolchain)
winget install --id Rustlang.Rustup

# 3. Download and unpack an LLVM 20 dev build into a space-free prefix
$version = "20.1.8"
$asset   = "llvm-$version-windows-amd64-msvc17-msvcrt"
curl.exe -fsSL -o "$env:TEMP\$asset.7z" `
  "https://github.com/vovkos/llvm-package-windows/releases/download/llvm-$version/$asset.7z"
7z.exe x "$env:TEMP\$asset.7z" "-o$env:TEMP\llvm-extract" -y
Move-Item "$env:TEMP\llvm-extract\$asset" C:\LLVM

# 4. Set the LLVM prefix (persists for future sessions)
setx LLVM_SYS_201_PREFIX "C:\LLVM"
$env:LLVM_SYS_201_PREFIX = "C:\LLVM"

# 5. Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# 6. Run tests
cargo test --workspace
```

**Choose the `msvcrt` (`/MD`) variant.** Rust's `x86_64-pc-windows-msvc` target
uses the dynamic CRT by default; the `libcmt` (`/MT`) build will fail to link.

**x86 only.** These packages are built with `LLVM_TARGETS_TO_BUILD=X86;NVPTX;AMDGPU`,
which is why the workspace pins inkwell to the `target-x86` feature rather than
`target-all`. Neuro only ever initializes the native target, so nothing is lost.

> **Note:** `.cargo/config.toml` adds `C:/vcpkg/installed/x64-windows-static/lib`
> to the MSVC link search path for LLVM's transitive dependencies (libxml2). If
> your LLVM package needs those and linking fails with unresolved `xml*` symbols,
> install them with `vcpkg install libxml2:x64-windows-static`.

---

## Optional: MLIR Backend (1D+)

The MLIR lowering path (tensor / autodiff / GPU, Phase 2+) is being built out via
the `melior` Rust MLIR bindings in the `mlir-backend` slice. It is **off by
default** behind the `mlir` cargo feature, so nothing here is required for a
normal Neuro build — the default `cargo build/test --workspace` compiles a
placeholder and needs only LLVM 20.

To build the MLIR path you need an LLVM 20 install that **includes MLIR** (the
`mlir-c` headers and `libMLIR*`) plus a libclang whose major version matches
(libclang 20). `mlir-sys` runs `bindgen` over the MLIR-C headers at build time,
and a newer libclang misparses the LLVM 20 headers.

```bash
# Ubuntu/Debian (apt.llvm.org ships matching MLIR + libclang 20):
sudo apt-get install -y libmlir-20-dev mlir-20-tools libclang-20-dev libclang-common-20-dev
export MLIR_SYS_200_PREFIX=/usr/lib/llvm-20
export TABLEGEN_200_PREFIX=/usr/lib/llvm-20
export LIBCLANG_PATH=/usr/lib/llvm-20/lib

# Point these at the SAME prefix as LLVM_SYS_201_PREFIX so inkwell and melior
# share one libLLVM-20 dylib, then build/test the feature:
cargo test -p mlir-backend --features mlir
```

On distributions whose stock `llvm20` package omits MLIR (e.g. Arch/CachyOS),
build LLVM 20 from source with `-DLLVM_ENABLE_PROJECTS=mlir` into a prefix and
set `MLIR_SYS_200_PREFIX` / `TABLEGEN_200_PREFIX` to it. If your system libclang
is newer than 20, supply a libclang 20 separately and set `LIBCLANG_PATH` (plus
`BINDGEN_EXTRA_CLANG_ARGS=-resource-dir=<libclang20>/clang/20`) so bindgen parses
the MLIR-C headers correctly.

## Verifying the Installation

```bash
# Check syntax and types without producing a binary
cargo run -p neurc -- check examples/basics/hello.nr

# Compile to a native executable
cargo run -p neurc -- compile examples/basics/factorial.nr

# Run the compiled binary
./examples/factorial            # Unix
.\examples\factorial.exe        # Windows

# After cargo install --path compiler/neurc:
neurc --version
neurc check examples/basics/hello.nr
```

All tests should pass:

```bash
cargo test --workspace
# Expected: every test passes, 0 failing
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

On Windows the same error means the prefix has no `llvm-config.exe` — you
installed the official LLVM installer rather than a development build:

```powershell
Test-Path "$env:LLVM_SYS_201_PREFIX\bin\llvm-config.exe"   # must be True
& "$env:LLVM_SYS_201_PREFIX\bin\llvm-config.exe" --version # must start with 20.
```

### Windows: unresolved `__imp___acrt_*` / `libcmt` conflicts at link time

CRT mismatch — you unpacked the `libcmt` (`/MT`) LLVM variant. Replace it with
the `msvcrt` (`/MD`) build and rebuild from clean:

```powershell
cargo clean
```

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
