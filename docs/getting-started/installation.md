# Installation Guide

This guide covers installation of the Neuro compiler on Linux, macOS, and Windows:
the three platforms CI builds, tests, and ships release binaries for.

## Prerequisites

| Requirement | Version | Notes |
|---|---|---|
| Rust | 1.98.1+ | Install via rustup |
| LLVM | 22 | Development package required (headers + `llvm-config` + link libraries) |
| C linker | any | `clang`, `gcc`, or the MSVC linker from Visual Studio Build Tools |

**Optional:**
- MLIR 22 for the experimental MLIR backend; see [MLIR Backend](#optional-mlir-backend) below. Not needed for a normal build.
- An NVIDIA GPU and its driver, to run a program with `@gpu` functions (Linux only). Compiling one needs the MLIR backend, not a CUDA toolkit.
- For an AMD GPU instead: ROCm to compile with `--gpu-arch gfxNNN`, and the HIP runtime to run the result. See [Choosing a GPU](../guides/cli-usage.md#choosing-a-gpu).

---

## Arch Linux / CachyOS

```bash
# 1. Install LLVM 22 (Arch's stock `llvm` package)
sudo pacman -S llvm

# 2. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
rustup component add clippy rustfmt rust-analyzer

# 3. Set LLVM prefix (add to ~/.bashrc or ~/.zshrc for permanence)
export LLVM_SYS_221_PREFIX=/usr

# 4. Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# 5. Run tests
cargo test --workspace

# 6. Install the compiler (optional)
cargo install --path compiler/neurc
```

Once Arch's `llvm` moves past 22, install the versioned `llvm22` package instead and
point `LLVM_SYS_221_PREFIX` at `/usr/lib/llvm22`.

---

## Ubuntu / Debian

```bash
# 1. Install LLVM 22
wget https://apt.llvm.org/llvm.sh
chmod +x llvm.sh
sudo ./llvm.sh 22

# 2. Install the LLVM development libraries and build dependencies
sudo apt-get install -y llvm-22-dev libpolly-22-dev libzstd-dev zlib1g-dev build-essential git

# 3. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# 4. Set LLVM prefix
export LLVM_SYS_221_PREFIX=/usr/lib/llvm-22
echo 'export LLVM_SYS_221_PREFIX=/usr/lib/llvm-22' >> ~/.bashrc
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
# 1. Install LLVM 22
brew install llvm@22

# 2. Set LLVM prefix
export LLVM_SYS_221_PREFIX=$(brew --prefix llvm@22)
echo "export LLVM_SYS_221_PREFIX=$(brew --prefix llvm@22)" >> ~/.zshrc
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

> **Note (Apple Silicon):** The LLVM prefix is usually `/opt/homebrew/opt/llvm@22`. On Intel Macs it is `/usr/local/opt/llvm@22`. `brew --prefix llvm@22` returns the correct path automatically.

---

## Windows (MSVC)

Windows needs a **full LLVM 22 development build**. LLVM's release page has one:
`clang+llvm-22.1.8-x86_64-pc-windows-msvc.tar.xz` carries `llvm-config.exe`, the
headers and the static libraries `llvm-sys` builds against. The `LLVM-*-win64.exe`
installer does not: it ships only Clang and `LLVM-C.dll`.

```powershell
# 1. Install the MSVC toolchain (C++ build tools + Windows SDK)
winget install --id Microsoft.VisualStudio.2022.BuildTools

# 2. Install Rust (MSVC toolchain)
winget install --id Rustlang.Rustup

# 3. Download and unpack LLVM 22 into a space-free prefix
$version = "22.1.8"
$asset   = "clang+llvm-$version-x86_64-pc-windows-msvc"
curl.exe -fsSL -o "$env:TEMP\$asset.tar.xz" `
  "https://github.com/llvm/llvm-project/releases/download/llvmorg-$version/$asset.tar.xz"
tar.exe -xf "$env:TEMP\$asset.tar.xz" -C $env:TEMP
Move-Item "$env:TEMP\$asset" C:\LLVM

# 4. Provide the libxml2 library that LLVM's llvm-config names (see below)
vcpkg install "libxml2[core]:x64-windows-static"
Get-ChildItem "<vcpkg-root>\installed\x64-windows-static\lib\*xml2*.lib" |
  Select-Object -First 1 | Copy-Item -Destination C:\LLVM\lib\xml2s.lib

# 5. Set the LLVM prefix (persists for future sessions)
setx LLVM_SYS_221_PREFIX "C:\LLVM"
$env:LLVM_SYS_221_PREFIX = "C:\LLVM"

# 6. Let rustc find the Windows SDK's system libraries (see below)
$kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\Lib'
$sdk  = Get-ChildItem $kits -Directory |
  Where-Object { Test-Path (Join-Path $_.FullName 'um\x64\psapi.lib') } |
  Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
$um   = Join-Path $sdk.FullName 'um\x64'
"[target.x86_64-pc-windows-msvc]`nrustflags = ['-L', 'native=$um']" |
  Out-File "$env:USERPROFILE\.cargo\config.toml" -Append -Encoding utf8

# 7. Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# 8. Run tests
cargo test --workspace
```

**Static CRT.** LLVM's Windows build is compiled against the static CRT (`/MT`), and
Rust defaults to the dynamic one (`/MD`) on `x86_64-pc-windows-msvc`. The repository's
`.cargo/config.toml` builds Rust with `+crt-static` on that target so the two match. A
`RUSTFLAGS` environment variable replaces that setting rather than adding to it, so if
you set one, include `-C target-feature=+crt-static` in it.

**Windows SDK libraries.** With the static CRT on, `llvm-sys` links the system libraries
`llvm-config` names (`psapi`, `shell32`, `ole32`, `uuid`, `advapi32`, `ws2_32`, `ntdll`) as
static libraries, and rustc looks for each `.lib` only on its own `-L` search paths, not on
the linker's `LIB`. Without step 6 the build stops with
``could not find native static library `psapi` ``. Step 6 adds the SDK's `um\x64` directory
through your user-level Cargo config, which Cargo combines with the repository's
`+crt-static` setting instead of replacing it.

**libxml2.** `llvm-config.exe --system-libs` lists `xml2s.lib`, because one LLVM
component (the Windows manifest merger) uses libxml2. Neuro never calls into it, so no
libxml2 code ends up in `neurc.exe`, but the linker still refuses to start when a named
library is missing. Any static libxml2 copied in under that name satisfies it. The archive
vcpkg builds changes its file name between libxml2 releases (`libxml2s.lib` up to 2.14), which
is why the command above matches `*xml2*.lib` instead of naming it.

**Backend subset.** The archive carries X86, AArch64, ARM, BPF, NVPTX, RISCV and
WebAssembly, which is why the workspace pins inkwell to `target-x86` rather than
`target-all`. Neuro only ever initializes the native target, so nothing is lost.

---

## Optional: MLIR Backend

The MLIR lowering path for tensors and GPU kernels lives in the
`mlir-backend` slice, built on the `melior` Rust MLIR bindings. It is **off by
default** behind the `mlir` cargo feature, so nothing here is required for a
normal Neuro build: the default `cargo build/test --workspace` compiles a
placeholder and needs only LLVM 22.

To build the MLIR path you need MLIR 22 installed **into the same prefix as the LLVM 22
that `LLVM_SYS_221_PREFIX` names**. `mlir-sys` finds MLIR by running
`$MLIR_SYS_220_PREFIX/bin/llvm-config`, so a separate MLIR prefix is invisible to it, and
one prefix is also what makes inkwell and melior share a single LLVM. `mlir-sys` runs
`bindgen` over the MLIR-C headers at build time, which needs a libclang of the same major
version (22).

```bash
# Ubuntu/Debian (apt.llvm.org ships MLIR and libclang 22 beside LLVM 22):
sudo apt-get install -y libmlir-22-dev mlir-22-tools libclang-22-dev libclang-common-22-dev
export MLIR_SYS_220_PREFIX=/usr/lib/llvm-22
export TABLEGEN_220_PREFIX=/usr/lib/llvm-22
export LIBCLANG_PATH=/usr/lib/llvm-22/lib
cargo test -p mlir-backend --features mlir
```

Arch/CachyOS ships no MLIR package that fits. `aur/mlir` installs next to the stock `llvm`
but provides no `libMLIR-C.so`, and with no static LLVM libraries in `/usr`, `mlir-sys`
links the shared `MLIR` and `MLIR-C` libraries. Build LLVM 22 with MLIR from source into
its own prefix instead, and point all three variables at it (the system libclang is 22
already, so `LIBCLANG_PATH` is not needed):

```bash
# In an llvm-project 22.1.8 source tree:
cmake -S llvm -B build -DCMAKE_BUILD_TYPE=Release -DLLVM_ENABLE_PROJECTS=mlir \
  -DLLVM_TARGETS_TO_BUILD="X86;NVPTX;AMDGPU" -DLLVM_INSTALL_UTILS=ON \
  -DCMAKE_C_COMPILER=clang -DCMAKE_CXX_COMPILER=clang++ -DLLVM_USE_LINKER=lld \
  -DCMAKE_INSTALL_PREFIX=/opt/llvm-mlir-22
cmake --build build -j"$(nproc)" && sudo cmake --install build

# Then, from the Neuro checkout:
export LLVM_SYS_221_PREFIX=/opt/llvm-mlir-22
export MLIR_SYS_220_PREFIX=/opt/llvm-mlir-22
export TABLEGEN_220_PREFIX=/opt/llvm-mlir-22
cargo test -p mlir-backend --features mlir
```

`mlir-sys` uses Rust 2024 let-chains in its build script, so the `mlir` feature needs
Rust 1.88 or newer.

With the same environment, `neurc` can be built with a feature of the same name. That
compiler hands straight-line `f32` / `f64` tensor arithmetic to the MLIR path and links the
result into the program; every other body still comes from the LLVM backend, and programs
behave identically either way. It is also the only compiler that builds `@gpu` kernels: a build without
the feature refuses a bare `@gpu` function rather than run it on the CPU, and compiles a
`@gpu(fallback: true)` one to its host copy only, with a warning.

```bash
cargo build -p neurc --features mlir
cargo test -p neurc --features mlir   # the whole end-to-end suite on that path
```

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

`LLVM_SYS_221_PREFIX` is not set or points to the wrong directory.

```bash
# Verify it is set
echo $LLVM_SYS_221_PREFIX

# Verify it contains an LLVM installation
ls $LLVM_SYS_221_PREFIX/lib/cmake/llvm/LLVMConfig.cmake
```

Make sure the export is in your shell rc file and that you have sourced it in the current session.

On Windows the same error means the prefix has no `llvm-config.exe`: you
installed the `.exe` installer rather than the `clang+llvm-*-windows-msvc` archive:

```powershell
Test-Path "$env:LLVM_SYS_221_PREFIX\bin\llvm-config.exe"   # must be True
& "$env:LLVM_SYS_221_PREFIX\bin\llvm-config.exe" --version # must start with 22.
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
# Arch:   sudo pacman -R llvm
# Ubuntu: sudo apt-get remove llvm-22
# macOS:  brew uninstall llvm@22
```

---

Once installation is complete, continue with:
- [Quick Start Guide](quick-start.md)
- [Your First Program](first-program.md)
