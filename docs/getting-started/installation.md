# Installation Guide

This guide covers installation of the Neuro compiler on Linux, macOS, and Windows:
the three platforms CI builds, tests, and ships release binaries for.

## Prerequisites

| Requirement | Version | Notes |
|---|---|---|
| Rust | 1.98.1+ | Install via rustup |
| LLVM | 23 | Development package required (headers + `llvm-config` + link libraries) |
| MLIR | 23 | Installed into the same prefix as LLVM; see [MLIR](#mlir) |
| C linker | any | `clang`, `gcc`, or the MSVC linker from Visual Studio Build Tools |

**Optional:**
- An NVIDIA GPU and its driver, to run a program with `@gpu` functions (Linux only). Compiling one needs no CUDA toolkit.
- For an AMD GPU instead: ROCm to compile with `--gpu-arch gfxNNN`, and the HIP runtime to run the result. See [Choosing a GPU](../guides/cli-usage.md#choosing-a-gpu).

---

## Arch Linux / CachyOS

Arch ships no MLIR package that fits. `aur/mlir` installs next to the stock `llvm` but
provides no `libMLIR-C.so`, and with no static LLVM libraries in `/usr`, `mlir-sys` links
the shared `MLIR` and `MLIR-C` libraries. Build LLVM 23 with MLIR from source into its own
prefix instead. The system libclang is 23 already, so `LIBCLANG_PATH` is not needed.

```bash
# 1. Install the build tools
sudo pacman -S base-devel cmake ninja clang lld

# 2. Build LLVM + MLIR 23 from the llvm-project-23.1.2.src tarball
cmake -S llvm-project-23.1.2.src/llvm -B build -G Ninja -DCMAKE_BUILD_TYPE=Release \
  -DLLVM_ENABLE_PROJECTS=mlir -DLLVM_TARGETS_TO_BUILD="X86;NVPTX;AMDGPU" \
  -DCMAKE_C_COMPILER=clang -DCMAKE_CXX_COMPILER=clang++ -DLLVM_USE_LINKER=lld \
  -DLLVM_INCLUDE_TESTS=OFF -DMLIR_INCLUDE_TESTS=OFF \
  -DCMAKE_INSTALL_PREFIX=/opt/llvm-mlir-23
cmake --build build && sudo cmake --install build

# 3. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
rustup component add clippy rustfmt rust-analyzer

# 4. Point all three prefixes at it (add to ~/.bashrc or ~/.zshrc for permanence)
export LLVM_SYS_231_PREFIX=/opt/llvm-mlir-23
export MLIR_SYS_230_PREFIX=/opt/llvm-mlir-23
export TABLEGEN_230_PREFIX=/opt/llvm-mlir-23

# 5. Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# 6. Run tests
cargo test --workspace

# 7. Install the compiler (optional)
cargo install --path compiler/neurc
```

---

## Ubuntu / Debian

```bash
# 1. Install LLVM 23
wget https://apt.llvm.org/llvm.sh
chmod +x llvm.sh
sudo ./llvm.sh 23

# 2. Install the LLVM and MLIR development libraries and build dependencies
sudo apt-get install -y llvm-23-dev libpolly-23-dev libmlir-23-dev mlir-23-tools \
  libclang-23-dev libclang-common-23-dev libzstd-dev zlib1g-dev build-essential git

# 3. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# 4. Set the prefixes (the same lines in ~/.bashrc make them permanent)
export LLVM_SYS_231_PREFIX=/usr/lib/llvm-23
export MLIR_SYS_230_PREFIX=/usr/lib/llvm-23
export TABLEGEN_230_PREFIX=/usr/lib/llvm-23
export LIBCLANG_PATH=/usr/lib/llvm-23/lib

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
# 1. Install LLVM 23 (Homebrew builds MLIR into the same formula)
brew install llvm

# 2. Set the prefixes (the same lines in ~/.zshrc make them permanent)
export LLVM_SYS_231_PREFIX=$(brew --prefix llvm)
export MLIR_SYS_230_PREFIX=$(brew --prefix llvm)
export TABLEGEN_230_PREFIX=$(brew --prefix llvm)

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

> **Note (Apple Silicon):** The LLVM prefix is usually `/opt/homebrew/opt/llvm`. On Intel Macs it is `/usr/local/opt/llvm`. `brew --prefix llvm` returns the correct path automatically.

---

## Windows (MSVC)

No Windows LLVM release carries MLIR, so Windows builds LLVM and MLIR 23 from source into
one prefix, as CI does. Expect the build to take hours on a four-core machine.

```powershell
# 1. Install the MSVC toolchain (C++ build tools + Windows SDK), CMake, Ninja, and
#    LLVM 23 itself for its libclang.dll, which bindgen loads (another major version
#    lays the MLIR-C structs out wrong)
winget install --id Microsoft.VisualStudio.2022.BuildTools
winget install --id Kitware.CMake
winget install --id Ninja-build.Ninja
winget install --id LLVM.LLVM --version 23.1.2

# 2. Install Rust (MSVC toolchain)
winget install --id Rustlang.Rustup

# 3. From an "x64 Native Tools" prompt, in the llvm-project-23.1.2.src tree, build a
#    static LLVM + MLIR prefix against the static CRT (see below)
cmake -S llvm -B build -G Ninja -DCMAKE_BUILD_TYPE=Release `
  -DCMAKE_C_COMPILER=cl -DCMAKE_CXX_COMPILER=cl `
  -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded `
  -DLLVM_ENABLE_PROJECTS=mlir "-DLLVM_TARGETS_TO_BUILD=X86;NVPTX;AMDGPU" `
  -DLLVM_ENABLE_ZLIB=OFF -DLLVM_ENABLE_ZSTD=OFF -DLLVM_ENABLE_LIBXML2=OFF `
  -DLLVM_ENABLE_DIA_SDK=OFF -DLLVM_INCLUDE_TESTS=OFF -DMLIR_INCLUDE_TESTS=OFF `
  -DCMAKE_INSTALL_PREFIX=C:\LLVM
cmake --build build --target install

# 4. Set the prefixes (setx persists them for future sessions)
foreach ($name in 'LLVM_SYS_231_PREFIX', 'MLIR_SYS_230_PREFIX', 'TABLEGEN_230_PREFIX') {
  setx $name "C:\LLVM"
  Set-Item "env:$name" "C:\LLVM"
}
setx LIBCLANG_PATH "$env:ProgramFiles\LLVM\bin"
$env:LIBCLANG_PATH = "$env:ProgramFiles\LLVM\bin"

# 5. Let rustc find the Windows SDK's system libraries (see below)
$kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\Lib'
$sdk  = Get-ChildItem $kits -Directory |
  Where-Object { Test-Path (Join-Path $_.FullName 'um\x64\psapi.lib') } |
  Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
$um   = Join-Path $sdk.FullName 'um\x64'
"[target.x86_64-pc-windows-msvc]`nrustflags = ['-L', 'native=$um']" |
  Out-File "$env:USERPROFILE\.cargo\config.toml" -Append -Encoding utf8

# 6. Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# 7. Run tests
cargo test --workspace
```

**Static CRT.** Step 3 builds LLVM against the static CRT (`/MT`), and Rust defaults to
the dynamic one (`/MD`) on `x86_64-pc-windows-msvc`. The repository's `.cargo/config.toml`
builds Rust with `+crt-static` on that target so the two match. A `RUSTFLAGS` environment
variable replaces that setting rather than adding to it, so if you set one, include
`-C target-feature=+crt-static` in it.

**Optional libraries off.** zlib, zstd, libxml2 and the DIA SDK are switched off because
`llvm-config --system-libs` would name them, some by absolute path, and `llvm-sys` passes
every name it lists to rustc as a library to link.

**Windows SDK libraries.** With the static CRT on, `llvm-sys` links the system libraries
`llvm-config` names (`psapi`, `shell32`, `ole32`, `uuid`, `advapi32`, `ws2_32`, `ntdll`) as
static libraries, and rustc looks for each `.lib` only on its own `-L` search paths, not on
the linker's `LIB`. Without step 5 the build stops with
``could not find native static library `psapi` ``. Step 5 adds the SDK's `um\x64` directory
through your user-level Cargo config, which Cargo combines with the repository's
`+crt-static` setting instead of replacing it.

---

## MLIR

The tensor and GPU lowering in the `mlir-backend` slice is built on the `melior` Rust MLIR
bindings, and every build compiles it. MLIR 23 must sit **in the same prefix as the LLVM 23
that `LLVM_SYS_231_PREFIX` names**: `mlir-sys` finds MLIR by running
`$MLIR_SYS_230_PREFIX/bin/llvm-config`, so a separate MLIR prefix is invisible to it, and one
prefix is also what makes inkwell and melior share a single LLVM. `mlir-sys` runs `bindgen`
over the MLIR-C headers at build time, which needs a libclang; the platform sections above
say where each one comes from.

`neurc` hands straight-line tensor arithmetic to the MLIR path and links the
result into the program; every other body comes from the LLVM backend. It also builds
`@gpu` kernels, on Linux only: elsewhere `neurc` refuses a bare `@gpu` function rather than
run it on the CPU, and compiles a `@gpu(fallback: true)` one to its host copy only, with a
warning.

## Verifying the Installation

```bash
# Check syntax and types without producing a binary
cargo run -p neurc -- check examples/basics/hello.nr

# Compile to a native executable
cargo run -p neurc -- compile examples/basics/factorial.nr

# Run the compiled binary
./examples/basics/factorial            # Unix
.\examples\basics\factorial.exe       # Windows

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

`LLVM_SYS_231_PREFIX` is not set or points to the wrong directory.

```bash
# Verify it is set
echo $LLVM_SYS_231_PREFIX

# Verify it contains an LLVM installation
ls $LLVM_SYS_231_PREFIX/lib/cmake/llvm/LLVMConfig.cmake
```

Make sure the export is in your shell rc file and that you have sourced it in the current session.

On Windows the same error means the prefix has no `llvm-config.exe`, which the
`LLVM-*-win64.exe` installer does not ship; point the prefixes at your source build:

```powershell
Test-Path "$env:LLVM_SYS_231_PREFIX\bin\llvm-config.exe"   # must be True
& "$env:LLVM_SYS_231_PREFIX\bin\llvm-config.exe" --version # must start with 23.
```

### Windows: `LNK2005` / `LNK2038` CRT conflicts at link time

The Rust side was built against the dynamic CRT while LLVM uses the static one. This
happens when a `RUSTFLAGS` environment variable overrides `.cargo/config.toml`. Add
`-C target-feature=+crt-static` to it (or unset it) and rebuild from clean:

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
# Arch:   sudo rm -rf /opt/llvm-mlir-23
# Ubuntu: sudo apt-get remove llvm-23 libmlir-23-dev
# macOS:  brew uninstall llvm
```

---

Once installation is complete, continue with:
- [Quick Start Guide](quick-start.md)
- [Your First Program](first-program.md)
