#!/usr/bin/env sh
# Regenerate compiler/llvm-backend/src/codegen/gpu_runtime.ll from gpu_runtime.c with the
# repo's LLVM (22). Keeps the .ll's leading comment block, drops what the backend's
# parser should not see: target datalayout and triple, attribute groups, metadata, and
# the `; Function Attrs:` comments that would describe the stripped groups.
set -eu
dir="$(dirname "$0")/../compiler/llvm-backend/src/codegen"
out="$dir/gpu_runtime.ll"
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
sed -n '/^;/!q;p' "$out" > "$tmp"
clang -O2 -S -emit-llvm -fno-stack-protector -fno-unwind-tables \
    -fno-asynchronous-unwind-tables "$dir/gpu_runtime.c" -o - |
    sed -E -e '/^(; ModuleID|; Function Attrs:|source_filename|target (datalayout|triple)|attributes #|!)/d' \
        -e 's/ #[0-9]+//g' -e 's/,? !(tbaa|llvm\.loop) ![0-9]+//g' |
    cat -s >> "$tmp"
cat "$tmp" > "$out"
