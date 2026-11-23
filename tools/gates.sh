#!/usr/bin/env bash
# Quality gates, one command and one exit code.
#
#   tools/gates.sh           the five gates every change must pass
#   tools/gates.sh --full    also the release build and rustdoc (debug and audit passes)
#   tools/gates.sh --asan    every example under AddressSanitizer and MALLOC_PERTURB_ (Linux)
#
# The LLVM/MLIR prefix comes from LLVM_SYS_231_PREFIX (see AGENTS.md); the MLIR and
# TableGen variables default to it. On a systemd machine, outside CI, cargo runs in a
# memory-capped scope (NEURO_GATES_MEMORY, default 14G) so a runaway build or test cannot
# take the desktop session with it.
set -uo pipefail
cd "$(dirname "$0")/.."

mode=${1:-}
case "$mode" in "" | --full | --asan) ;; *) echo "usage: $0 [--full|--asan]" >&2; exit 2 ;; esac

: "${LLVM_SYS_231_PREFIX:?export LLVM_SYS_231_PREFIX (the LLVM+MLIR 23 prefix, see AGENTS.md)}"
export LLVM_SYS_231_PREFIX
export MLIR_SYS_230_PREFIX=${MLIR_SYS_230_PREFIX:-$LLVM_SYS_231_PREFIX}
export TABLEGEN_230_PREFIX=${TABLEGEN_230_PREFIX:-$LLVM_SYS_231_PREFIX}

scoped() {
    if [ -z "${CI:-}" ] && command -v systemd-run >/dev/null 2>&1; then
        systemd-run --user --scope -q -p MemoryMax="${NEURO_GATES_MEMORY:-14G}" -p MemorySwapMax=0 \
            --working-directory="$PWD" env "${extra_env[@]}" \
            LLVM_SYS_231_PREFIX="$LLVM_SYS_231_PREFIX" MLIR_SYS_230_PREFIX="$MLIR_SYS_230_PREFIX" \
            TABLEGEN_230_PREFIX="$TABLEGEN_230_PREFIX" "$@"
    else
        env "${extra_env[@]}" "$@"
    fi
}

failed=()
gate() {
    echo "==> $*"
    scoped "$@" || failed+=("$*")
}

extra_env=()
if [ "$mode" = --asan ]; then
    asan=$(gcc -print-file-name=libasan.so)
    [ -f "$asan" ] || { echo "libasan.so not found through gcc" >&2; exit 2; }
    supp=$(mktemp)
    trap 'rm -f "$supp"' EXIT
    echo "leak:libcuda.so" >"$supp"
    extra_env=(NEURO_EXAMPLE_PRELOAD="$asan" MALLOC_PERTURB_=165
        ASAN_OPTIONS=detect_leaks=1:protect_shadow_gap=0 LSAN_OPTIONS=suppressions="$supp")
    gate cargo test -p neuroc --test examples
else
    gate cargo fmt --all -- --check
    gate cargo clippy --workspace --all-targets -- -D warnings
    gate cargo test --workspace
    gate cargo test -p neuroc --test architecture_tests
    gate python3 tools/check_docs_hygiene.py
    if [ "$mode" = --full ]; then
        gate cargo build --workspace --release
        gate cargo doc --no-deps --workspace
    fi
fi

if [ ${#failed[@]} -eq 0 ]; then
    echo "==> all gates passed"
    exit 0
fi
printf '==> FAILED: %s\n' "${failed[@]}"
exit 1
