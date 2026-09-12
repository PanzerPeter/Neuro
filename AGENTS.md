# AGENTS.md

Instructions for AI coding agents working in the Neuro repository. Human-facing
detail lives in [CONTRIBUTING.md](CONTRIBUTING.md), [VSA.md](VSA.md) and
[DESIGN.md](DESIGN.md); this file is the short operational contract.

## Project Overview

Neuro is an AOT-compiled language for high-performance AI development. Source
files (`.nr`) compile to native binaries through LLVM 20. The compiler is
written in Rust and laid out as a Cargo workspace.

Pipeline:

```
source (.nr) -> lexical analysis -> syntax parsing -> semantic analysis
             -> HIR lowering -> LLVM backend -> linker -> native binary
```

Workspace layout (`compiler/`):

| Crate | Role |
| --- | --- |
| `infrastructure/shared-types` | `Span`, `Identifier`, `Literal`: no business logic |
| `infrastructure/source-location` | Source mapping |
| `infrastructure/ast-types` | AST node definitions (owned here, not in the parser) |
| `infrastructure/neuro-hir` | Typed HIR: the frontend/backend contract |
| `lexical-analysis` | Tokenizer (logos + unicode-ident) |
| `syntax-parsing` | Pratt expression parser + statement parser |
| `module-resolution` | Multi-file program loading |
| `argument-binding` | Named/positional argument binding |
| `semantic-analysis` | Type checking, scope resolution |
| `hir-lowering` | AST → typed HIR |
| `llvm-backend` | Codegen via inkwell 0.10.0 (LLVM 20) |
| `mlir-backend` | MLIR/melior codegen, behind the off-by-default `mlir` feature |
| `neurc` | CLI driver; the only crate allowed to depend on every slice |

**Architecture: Vertical Slice Architecture (VSA).** Crates are organized by
language feature, not by technical layer. The rules an agent must not break:

- A feature slice depends only on `infrastructure/` crates. Never on another
  feature slice.
- One exception is allowlisted, and only one: `syntax-parsing` depends on
  `lexical-analysis`, so `parse()` calls `tokenize()` internally instead of
  making `neurc` orchestrate the pair. It is named in
  `compiler/syntax-parsing/CONTEXT.md` and enforced by
  `compiler/neurc/tests/architecture_tests.rs`. A second exception requires
  changing that test first. Do not add one casually.
- Feature slices in each other's `[dev-dependencies]` are fine; the
  architecture test reads `[dependencies]` only.
- No business logic in infrastructure crates.
- Duplication across slices is preferred over coupling.

Status (current phase, per-feature progress) lives in the
[Quick Roadmap](README.md#quick-roadmap) and [CHANGELOG.md](CHANGELOG.md).
Never restate it elsewhere. It goes stale.

## Build and Test Commands

LLVM 20 must be installed and `LLVM_SYS_201_PREFIX` exported before anything
builds:

```bash
export LLVM_SYS_201_PREFIX=/usr/lib/llvm20            # Arch / CachyOS
export LLVM_SYS_201_PREFIX=/usr/lib/llvm-20           # Ubuntu / Debian
export LLVM_SYS_201_PREFIX=$(brew --prefix llvm@20)   # macOS
```

```bash
cargo build --workspace                # build everything
cargo build --profile dev-optimized    # faster iteration (opt-level 1)
cargo build --release                  # release build

cargo test --workspace                 # full test suite
cargo test -p lexical-analysis         # one crate
cargo test <test_name>                 # one test by name
cargo test -- --nocapture              # show test output

cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
python tools/check_docs_hygiene.py     # documentation hygiene gate
python tools/clean_stale_target.py     # garbage-collect stale target/ artifacts
```

Running the compiler:

```bash
cargo run -p neurc -- check   examples/basics/hello.nr   # type-check only
cargo run -p neurc -- compile examples/basics/hello.nr   # compile to a binary
cargo run -p neurc -- compile examples/basics/hello.nr -O2 -o hello
```

The MLIR backend is optional and off by default. It needs an LLVM 20 install
that includes MLIR plus a matching libclang 20
(`MLIR_SYS_200_PREFIX`, `TABLEGEN_200_PREFIX`, `LIBCLANG_PATH`); see
[installation](docs/getting-started/installation.md#optional-mlir-backend).
Then: `cargo test -p mlir-backend --features mlir`.

**Quality gates.** Every change must leave these green before it is called
done. Run them; do not assume:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p neurc --test architecture_tests
python tools/check_docs_hygiene.py
```

## Code Style Guidelines

Rust, edition 2021, MSRV 1.85. Formatting is whatever `cargo fmt` produces:
never hand-format around it.

- **No `unwrap()` / `expect()` in production code paths.** Return
  `Result<T, E>` with an actionable message. Tests may use them.
- **`pub(crate)` is the default visibility.** `pub` is reserved for a slice's
  single entry point. Do not widen a surface with permissive `pub use`.
- **Explicit integer widths** on public-facing APIs (`u32`, `i64`), not
  `usize` for domain values.
- **Borrow rather than clone.** A `.clone()` in a hot compiler path needs a
  reason.
- **Document every `unsafe` block** with a safety rationale comment. The
  backend is where these concentrate.
- **Doc comments (`///`) on public APIs.** Describe the contract, not the
  implementation.
- Match the surrounding code's naming and comment density. New code should be
  indistinguishable in style from the slice it lives in.

Backend-specific invariants (see `compiler/llvm-backend/CONTEXT.md` for the
authoritative version):

- String ABI is a `{ ptr, i64 }` fat pointer.
- Struct ABI is an anonymous LLVM struct in declaration order.
- `semantic-analysis` is not a production dependency of the backend; `neurc`
  owns the pipeline ordering.

**`CONTEXT.md` is a blocker, not a nicety.** Every slice has one, documenting
its purpose, entry point, data ownership, and shared-kernel dependencies.
Update the affected slice's `CONTEXT.md` in the *same commit* as any change to
its entry point, public surface, or dependencies.

**Commits.** Format: `scope: short summary` (50 chars or less), with an
optional body explaining what and why. Valid scopes: `lexer`, `parser`,
`semantic`, `codegen`, `infra`, `tests`, `docs`, `build`, `ci`. Every commit
carries a `Signed-off-by:` trailer (`git commit -s`) per the DCO in
CONTRIBUTING.md. No co-author trailers.

**Docs ride with the code.** CHANGELOG entry, README rows, and `CONTEXT.md`
updates ship in the same commit as the change they describe.

## Testing Instructions

Every logical change ships with tests. Bug fixes ship with a regression test
that fails before the fix.

Where tests go:

- **Unit tests**: `#[cfg(test)] mod tests { ... }` in the source file, for
  internal functions.
- **Slice integration tests**: the crate's `tests/` directory, exercising the
  slice's public entry point.
- **End-to-end tests**: `compiler/neurc/tests/`, one file per language
  feature. These compile real `.nr` source with the built `neurc` binary and
  run it. Use the `CompileTest` helper in
  `compiler/neurc/tests/common/mod.rs`; it writes sources to a temp dir and
  resolves the binary through `CARGO_BIN_EXE_neurc`.
- **Example programs**: `examples/`, grouped by topic. Each program is pinned
  two ways: its `main` return value is its process exit code, registered in
  `examples/expected.txt`, and its stdout is fixed byte for byte in a sibling
  `.out` file. `compiler/neurc/tests/examples.rs` checks both. Adding an
  example means adding both pins.
- **Architecture tests**: `compiler/neurc/tests/architecture_tests.rs`
  enforces the VSA dependency rules. If it fails, the fix is the dependency,
  not the test.

Cover error paths and boundary conditions, not just the happy path. Known
open defects are registered in [docs/BUGS.md](docs/BUGS.md) as `BUG-NNN`;
check there before reporting a new one, and remove the entry when it is fixed.

Never write a test count into any document. The CI badge is the only honest
source, and `tools/check_docs_hygiene.py` fails the build on hard-coded
counts, on prose copies of the workspace version, and on references to
local-only paths.

CI additionally runs the suite on Linux/macOS/Windows against stable and
nightly, `cargo audit`, benchmark regression budgets, release smoke tests
(`tools/run_release_smoke_tests.py`), and coverage.

## Security Considerations

Neuro is an alpha-stage compiler. Security-relevant work concentrates in three
places, and agent changes should be read with them in mind:

- **Compiler integrity**: malformed or adversarial `.nr` input must not
  panic, crash, read out of bounds, or hang the compiler. Parser and analysis
  code must handle untrusted input defensively; this is the main reason the
  no-`unwrap()` rule exists.
- **Generated code safety**: a backend bug that emits incorrect IR is a
  security bug (uninitialized reads, bad pointer arithmetic, missing overflow
  or bounds guards). Codegen changes need runtime tests that execute the
  produced binary, not just IR inspection.
- **Dependency vulnerabilities**: run `cargo audit` when touching
  `Cargo.toml`; CI enforces it. Do not add a dependency without a clear reason,
  and never pin to an unmaintained crate.

Rules for agents:

- Never commit secrets, tokens, or credentials. Check `.gitignore` before
  referencing any path; if a file you are about to link, `include`, `mod`, or
  cite matches an ignored pattern, remove the reference rather than committing
  it. Build output, local assistant configuration, generated caches, and
  private working notes are all ignored and must not appear in tracked files.
- Do not weaken or delete a runtime guard (overflow check, bounds check,
  division guard, null check) to make a test pass.
- Do not disable, `#[ignore]`, or loosen a failing test, lint, or architecture
  check to get green. Fix the cause.
- Report vulnerabilities privately through GitHub's security advisory system,
  never in a public issue. See [SECURITY.md](SECURITY.md) for the process and
  response timelines.
