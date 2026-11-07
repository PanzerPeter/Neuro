# Installation Guide

This guide builds the Neuro compiler, `neurc`, from source on Linux, macOS and Windows: the
three platforms CI builds and tests. There are no prebuilt binaries yet. Every build needs
LLVM 23 and MLIR 23 installed into one prefix before Cargo runs.

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

## Quick check

Once the build for your platform finishes, run the `hello` example through the release binary
(on Windows, `target\release\neurc.exe`):

```bash
target/release/neurc run examples/basics/hello.nr
echo $?                                  # PowerShell: echo $LASTEXITCODE
```

```text
add(5, 3)    = 8
calculate(8) = 26
```

The second command prints `26`, the value `main` returned. `run` type-checks, compiles, links
and starts the program, so this one command exercises the whole toolchain. If it fails, see the
[troubleshooting guide](../guides/troubleshooting.md).

To run the full suite, use `cargo test --workspace`. Add `--no-fail-fast` to see every failing
test rather than stopping at the first.

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

## Ubuntu / Debian

```bash
# 1. Install LLVM 23 from apt.llvm.org
wget https://apt.llvm.org/llvm.sh
chmod +x llvm.sh
sudo ./llvm.sh 23

# 2. Install the LLVM and MLIR development libraries and build dependencies
sudo apt-get install -y llvm-23-dev llvm-23-tools libpolly-23-dev libmlir-23-dev \
  mlir-23-tools libclang-23-dev libclang-common-23-dev lld-23 libzstd-dev zlib1g-dev \
  build-essential git

# 3. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# 4. Set the prefixes (the same lines in ~/.bashrc make them permanent)
export LLVM_SYS_231_PREFIX=/usr/lib/llvm-23
export MLIR_SYS_230_PREFIX=/usr/lib/llvm-23
export TABLEGEN_230_PREFIX=/usr/lib/llvm-23
export LIBCLANG_PATH=/usr/lib/llvm-23/lib
export LLVM_LINK_LLVM_DYLIB=1          # CI links the shared LLVM library

# 5. Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# 6. Run tests
cargo test --workspace
```

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

## Windows (MSVC)

No Windows LLVM release carries MLIR, so Windows builds LLVM and MLIR 23 from source into
one prefix, as CI does. Expect the build to take hours on a four-core machine.

```powershell
# 1. Install the MSVC toolchain (C++ build tools + Windows SDK), CMake, Ninja, and
#    LLVM 23 itself for its libclang.dll, which bindgen loads (another major version
#    lays the MLIR-C structs out wrong). The LLVM-*-win64.exe installer ships neither
#    MLIR nor llvm-config, so it cannot serve as the prefix.
winget install --id Microsoft.VisualStudio.2022.BuildTools
winget install --id Kitware.CMake
winget install --id Ninja-build.Ninja
winget install --id LLVM.LLVM --version 23.1.2

# 2. Install Rust (MSVC toolchain)
winget install --id Rustlang.Rustup

# 3. From an "x64 Native Tools" prompt, in the llvm-project-23.1.2.src tree, build a
#    static LLVM + MLIR prefix against the static CRT (see below)
cmake -S llvm -B build -G Ninja `
  -DCMAKE_BUILD_TYPE=Release `
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
`-C target-feature=+crt-static` in it. Otherwise the link fails with `LNK2005` or `LNK2038`
and the fix is a `cargo clean` after correcting the variable.

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

## MLIR

Every build compiles the `mlir-backend` slice, which lowers tensor code through the `melior`
Rust MLIR bindings. MLIR 23 must sit **in the same prefix as the LLVM 23 that
`LLVM_SYS_231_PREFIX` names**: `mlir-sys` finds MLIR by running
`$MLIR_SYS_230_PREFIX/bin/llvm-config`, so a separate MLIR prefix is invisible to it, and one
prefix is also what makes inkwell and melior share a single LLVM. `mlir-sys` runs `bindgen`
over the MLIR-C headers at build time, which needs a libclang; the platform sections above
say where each one comes from.

For what the MLIR path does with a program, see the [MLIR backend](../compiler/components/mlir-backend.md)
and the [tensor GPU section](../language-reference/tensors.md#running-on-a-gpu-gpu).

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

## See also

- [Quick Start](quick-start.md): the `neurc` workflow
- [Your First Program](first-program.md): a step-by-step tutorial
- [Troubleshooting](../guides/troubleshooting.md): "No suitable version of LLVM", linker and `mlir-sys` build errors
- [CLI Usage](../guides/cli-usage.md): every command and flag
