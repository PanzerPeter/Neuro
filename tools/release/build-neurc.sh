#!/usr/bin/env bash
# Builds the release `neurc` against the prefix build-prefix.sh made.
#
#   build-neurc.sh <llvm-prefix> <source-dir>
#
# Runs inside quay.io/pypa/manylinux_2_28_x86_64 and leaves the binary at
# <source-dir>/target/release/neurc.
set -euo pipefail

prefix=${1:?usage: build-neurc.sh <llvm-prefix> <source-dir>}
src=${2:?usage: build-neurc.sh <llvm-prefix> <source-dir>}

# bindgen needs a libclang, and only for parsing the MLIR-C and TableGen C headers. The
# image's own (AppStream) generates the same bindings for them as libclang 23, and
# building clang into the prefix would add about an hour to a cold cache.
#
# inkwell asks the linker for libffi whatever LLVM it builds against. The prefix is built
# without libffi, so --as-needed drops it and the binary does not need it; the library only
# has to exist for the link to resolve. package.sh fails the build if it ever stays.
dnf -q install -y clang-devel libffi-devel

curl -fsSL --retry 5 --retry-connrefused https://sh.rustup.rs \
  | sh -s -- -y --profile minimal --default-toolchain stable
# shellcheck source=/dev/null
. "$HOME/.cargo/env"

# llvm-sys, mlir-sys and tblgen each ask for a bare `-lstdc++`, which the toolset answers
# with a linker script naming the shared libstdc++.so.6. With that script gone the linker
# takes libstdc++.a from the same directory, so the binary needs no libstdc++ at all. The
# container is thrown away after this build.
rm "$(gcc -print-file-name=libstdc++.so)"

export LLVM_SYS_231_PREFIX=$prefix MLIR_SYS_230_PREFIX=$prefix TABLEGEN_230_PREFIX=$prefix
export LIBCLANG_PATH=/usr/lib64
cargo build --manifest-path "$src/Cargo.toml" --release --locked -p neurc
