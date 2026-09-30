#!/usr/bin/env sh
# Regenerate compiler/llvm-backend/src/codegen/gpu_runtime.ll (CUDA) and
# gpu_runtime_hip.ll (HIP) from gpu_runtime.c with the repo's LLVM (22). Keeps each .ll's
# leading comment block, drops what the backend's parser should not see: target
# datalayout and triple, attribute groups, metadata, and the `; Function Attrs:` comments
# that would describe the stripped groups.
set -eu
dir="$(dirname "$0")/../compiler/llvm-backend/src/codegen"

# regen <output> [clang flag...]
regen() {
    out="$dir/$1"
    shift
    tmp="$(mktemp)"
    sed -n '/^;/!q;p' "$out" > "$tmp"
    clang -O2 -S -emit-llvm -fno-stack-protector -fno-unwind-tables \
        -fno-asynchronous-unwind-tables "$@" "$dir/gpu_runtime.c" -o - |
        sed -E -e '/^(; ModuleID|; Function Attrs:|source_filename|target (datalayout|triple)|attributes #|!)/d' \
            -e 's/ #[0-9]+//g' -e 's/,? !(tbaa|llvm\.loop) ![0-9]+//g' |
        cat -s >> "$tmp"
    cat "$tmp" > "$out"
    rm -f "$tmp"
}

regen gpu_runtime.ll
regen gpu_runtime_hip.ll -DNEURO_HIP
