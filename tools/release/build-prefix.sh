#!/usr/bin/env bash
# Builds the static LLVM + MLIR prefix the Linux release binary links against.
#
#   build-prefix.sh <install-prefix>
#
# Runs inside quay.io/pypa/manylinux_2_28_x86_64, so nothing in the prefix asks for a
# glibc newer than 2.28. The recipe is the Windows one in .github/actions/setup-llvm:
# zlib, zstd and libxml2 are off, so the binary has no shared library to need for them,
# and only the X86, NVPTX and AMDGPU targets are built. No tools are built, which saves
# most of the time; llvm-config, the one the Rust bindings run, is built by hand.
set -euo pipefail

prefix=${1:?usage: build-prefix.sh <install-prefix>}
version=23.1.2
sha256=c98bbef08a2b4c2613cd50e9aa9ae7b69b1fe6c16b2c40373bc0ab6116fdf78a
src=llvm-project-$version.src
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# The image's ninja-build package (1.8) is older than CMake's Ninja generator likes.
pipx install ninja

# The image's curl (7.61) predates --retry-all-errors.
curl -fsSL --retry 5 --retry-delay 5 --retry-connrefused -o "$work/$src.tar.xz" \
  "https://github.com/llvm/llvm-project/releases/download/llvmorg-$version/$src.tar.xz"
echo "$sha256  $work/$src.tar.xz" | sha256sum -c -
# Only the trees an LLVM + MLIR build reads; LLVM's configure needs libc's shared headers
# even with the libc project off.
tar -xJf "$work/$src.tar.xz" -C "$work" \
  "$src/llvm" "$src/mlir" "$src/cmake" "$src/third-party" "$src/libc"

cmake -S "$work/$src/llvm" -B "$work/build" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$prefix" \
  -DLLVM_ENABLE_PROJECTS=mlir \
  -DLLVM_TARGETS_TO_BUILD="X86;NVPTX;AMDGPU" \
  -DLLVM_ENABLE_ASSERTIONS=OFF \
  -DLLVM_ENABLE_ZLIB=OFF \
  -DLLVM_ENABLE_ZSTD=OFF \
  -DLLVM_ENABLE_LIBXML2=OFF \
  -DLLVM_ENABLE_LIBEDIT=OFF \
  -DLLVM_ENABLE_LIBPFM=OFF \
  -DLLVM_BUILD_TOOLS=OFF \
  -DLLVM_BUILD_UTILS=OFF \
  -DLLVM_INCLUDE_UTILS=OFF \
  -DLLVM_INCLUDE_TESTS=OFF \
  -DMLIR_INCLUDE_TESTS=OFF \
  -DLLVM_INCLUDE_BENCHMARKS=OFF \
  -DLLVM_INCLUDE_EXAMPLES=OFF \
  -DLLVM_INCLUDE_DOCS=OFF \
  -DLLVM_PARALLEL_LINK_JOBS=2

cmake --build "$work/build" --target install
cmake --build "$work/build" --target llvm-config
install -Dm755 "$work/build/bin/llvm-config" "$prefix/bin/llvm-config"

# The tarball must carry LLVM's notices, and the package step reads them from here rather
# than downloading the source again. Support/ holds the two components under other terms.
install -Dm644 "$work/$src/llvm/LICENSE.TXT" "$prefix/licenses/LLVM-LICENSE.txt"
install -Dm644 "$work/$src/llvm/lib/Support/COPYRIGHT.regex" "$prefix/licenses/LLVM-COPYRIGHT.regex.txt"
install -Dm644 "$work/$src/llvm/lib/Support/BLAKE3/LICENSE" "$prefix/licenses/LLVM-BLAKE3-LICENSE.txt"

"$prefix/bin/llvm-config" --version
if [ -n "$("$prefix/bin/llvm-config" --link-static --system-libs | tr ' ' '\n' | grep -E -- '-l(z|zstd|xml2)$' || true)" ]; then
  echo "error: the prefix still links zlib, zstd or libxml2" >&2
  exit 1
fi
