#!/usr/bin/env bash
# Checks that a Linux `neurc` runs on any glibc 2.28 system and packs it for release.
#
#   package.sh <neurc> <llvm-prefix> <source-dir> <version> <dist-dir>
#
# Writes <dist-dir>/neurc-x86_64-unknown-linux-gnu.tar.gz and its .sha256. The names carry
# no version, so `releases/latest/download/<name>` always fetches the newest release.
# <source-dir> is the tree the binary was built from, which may be an older tag than this
# script's. Needs readelf, objdump and cargo-about on PATH.
set -euo pipefail

usage="usage: package.sh <neurc> <llvm-prefix> <source-dir> <version> <dist-dir>"
bin=${1:?$usage}
prefix=${2:?$usage}
src=${3:?$usage}
version=${4:?$usage}
dist=${5:?$usage}
here=$(dirname "$0")
glibc_floor=2.28
name=neurc-x86_64-unknown-linux-gnu

fail() {
  echo "error: $*" >&2
  exit 1
}

# Anything outside glibc and libgcc_s is a library a user may not have.
for lib in $(readelf -d "$bin" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p'); do
  case $lib in
    libc.so.6 | libm.so.6 | libdl.so.2 | libpthread.so.0 | librt.so.1 | libgcc_s.so.1 | ld-linux-x86-64.so.2) ;;
    *) fail "$bin needs $lib, which a glibc $glibc_floor system may lack" ;;
  esac
done

symbols=$(objdump -T "$bin")
newest=$(grep -o 'GLIBC_[0-9.]*' <<<"$symbols" | sed 's/GLIBC_//' | sort -uV | tail -n1)
[ "$(printf '%s\n' "$newest" "$glibc_floor" | sort -V | tail -n1)" = "$glibc_floor" ] \
  || fail "$bin uses glibc $newest symbols; the floor is $glibc_floor"
if grep -qE 'GLIBCXX_|CXXABI_' <<<"$symbols"; then
  fail "$bin imports libstdc++ symbols; it must link libstdc++ statically"
fi

reported=$("$bin" --version)
[ "$reported" = "neurc $version" ] || fail "$bin reports '$reported', expected 'neurc $version'"

stage=$dist/$name
rm -rf "$stage"
mkdir -p "$stage/licenses"
install -m755 "$bin" "$stage/neurc"
install -m644 "$src/LICENSE" "$stage/LICENSE"
install -m644 "$prefix"/licenses/* "$stage/licenses/"
cargo about generate --fail -c "$here/about.toml" -m "$src/compiler/neurc/Cargo.toml" \
  -o "$stage/licenses/RUST-CRATES.txt" "$here/about.hbs"

tar -C "$dist" --sort=name --owner=0 --group=0 --numeric-owner -czf "$dist/$name.tar.gz" "$name"
(cd "$dist" && sha256sum "$name.tar.gz" >"$name.tar.gz.sha256")
cat "$dist/$name.tar.gz.sha256"
