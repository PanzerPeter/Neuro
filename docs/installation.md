NEURO Installation

Prerequisites
- Rust 1.70+ (install via rustup)
- LLVM 18 (for backend and `neurc llvm`)
- Optional: CUDA Toolkit (NVIDIA) and/or Vulkan SDK for GPU work (framework present; kernels WIP)

Windows (PowerShell)
1. Install Rust: https://rustup.rs
2. Install LLVM 18 (e.g., via `Chocolatey` or the official binaries). Ensure `llvm-config`/LLVM bin is on PATH.
3. Clone and build:
   ```powershell
   git clone https://github.com/PanzerPeter/Neuro.git
   cd Neuro
   cargo build --release
   $env:PATH = "$pwd/target/release;" + $env:PATH
   neurc --version
   ```

macOS (zsh)
1. Install Rust: https://rustup.rs
2. Install LLVM 18:
   - Homebrew: `brew install llvm@18` then add `export PATH="$(brew --prefix llvm@18)/bin:$PATH"`
3. Build:
   ```bash
   git clone https://github.com/PanzerPeter/Neuro.git
   cd Neuro
   cargo build --release
   export PATH="$PWD/target/release:$PATH"
   neurc --version
   ```

Linux (bash)
1. Install Rust: https://rustup.rs
2. Install LLVM 18 using your package manager or from source; ensure `llvm-config` matches version.
3. Build:
   ```bash
   git clone https://github.com/PanzerPeter/Neuro.git
   cd Neuro
   cargo build --release
   export PATH="$PWD/target/release:$PATH"
   neurc --version
   ```

Troubleshooting
- If `neurc --version` fails, confirm `target/release` is on PATH.
- If LLVM linking fails, verify LLVM 18 is installed and discoverable (PATH, LIB paths).
- GPU backends require vendor SDKs. The framework compiles; full kernels are planned in Phase 2.

