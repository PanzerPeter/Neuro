#!/usr/bin/env bash
# Compiles and runs every registered example with a given `neurc`, held to the same two
# pins as compiler/neurc/tests/examples.rs: the exit code in examples/expected.txt and the
# stdout in the sibling .out file (none means silent). A `gpu` example on a machine with no
# GPU must instead abort at startup with the runtime's diagnostic.
#
#   smoke.sh <neurc> <examples-dir>
#
# Release CI runs it in a container that holds a C compiler and nothing else, which is
# what proves the tarball needs no toolchain beside `cc`. Needs only bash, coreutils and grep.
set -uo pipefail

neurc=${1:?usage: smoke.sh <neurc> <examples-dir>}
examples=${2:?usage: smoke.sh <neurc> <examples-dir>}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

passed=0
failures=()

while read -r rel code marker _; do
  case $rel in '' | '#'*) continue ;; esac
  [ "$code" = module ] && continue

  exe=$work/example
  # Both read stdin from /dev/null, not from the manifest this loop is reading.
  if ! "$neurc" compile "$examples/$rel" -o "$exe" </dev/null >"$work/compile.log" 2>&1; then
    failures+=("$rel: compile failed: $(cat "$work/compile.log")")
    continue
  fi
  # Some examples abort by design. The outer redirect drops bash's "Aborted" report, and
  # the ulimit keeps each one from leaving a core file.
  { (ulimit -c 0 && exec "$exe" </dev/null >"$work/stdout" 2>"$work/stderr"); } 2>/dev/null
  got=$?

  # shellcheck disable=SC2016 # the backticks are part of the diagnostic text
  if [ "$marker" = gpu ] && grep -qE '^panic: `@(gpu|kernel)` needs an NVIDIA GPU, and none is usable: ' "$work/stderr"; then
    if [ -s "$work/stdout" ] || [ "$got" -eq 0 ]; then
      failures+=("$rel: reported no GPU but did not abort at startup")
    else
      passed=$((passed + 1))
    fi
    continue
  fi

  golden=$examples/${rel%.nr}.out
  [ -f "$golden" ] || golden=/dev/null
  problems=()
  [ "$got" -eq "$code" ] || problems+=("exit code $got, expected $code")
  # Checksums, not cmp: cmp is diffutils, which a bare AlmaLinux 8 image lacks.
  [ "$(sha256sum <"$golden")" = "$(sha256sum <"$work/stdout")" ] \
    || problems+=("stdout differs from ${rel%.nr}.out")
  if [ ${#problems[@]} -eq 0 ]; then
    passed=$((passed + 1))
  else
    failures+=("$rel: ${problems[*]}")
  fi
done <"$examples/expected.txt"

echo "$passed examples matched their pins"
if [ ${#failures[@]} -gt 0 ]; then
  printf 'FAIL %s\n' "${failures[@]}" >&2
  exit 1
fi
