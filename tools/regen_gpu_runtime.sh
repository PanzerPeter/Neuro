#!/usr/bin/env sh
# Regenerate the backend's embedded runtimes under compiler/llvm-backend/src/codegen with
# the repo's LLVM: gpu_runtime.ll (CUDA) and gpu_runtime_hip.ll (HIP) from gpu_runtime.c,
# and text_parse.ll from text_parse.c. Keeps each .ll's leading comment block, drops what
# the backend's parser should not see: target datalayout and triple, attribute groups,
# metadata, and the `; Function Attrs:` comments that would describe the stripped groups.
set -eu
dir="$(dirname "$0")/../compiler/llvm-backend/src/codegen"

# regen <output> <source> [clang flag...]
regen() {
    out="$dir/$1"
    source="$dir/$2"
    shift 2
    tmp="$(mktemp)"
    sed -n '/^;/!q;p' "$out" > "$tmp"
    clang -O2 -S -emit-llvm -fno-stack-protector -fno-unwind-tables \
        -fno-asynchronous-unwind-tables "$@" "$source" -o - |
        sed -E -e '/^(; ModuleID|; Function Attrs:|source_filename|target (datalayout|triple)|attributes #|!)/d' \
            -e 's/ #[0-9]+//g' -e 's/,? !(tbaa|llvm\.loop|inline_history) ![0-9]+//g' |
        cat -s >> "$tmp"
    cat "$tmp" > "$out"
    rm -f "$tmp"
}

regen gpu_runtime.ll gpu_runtime.c
regen gpu_runtime_hip.ll gpu_runtime.c -DNEURO_HIP
regen text_parse.ll text_parse.c -fno-vectorize -fno-slp-vectorize
