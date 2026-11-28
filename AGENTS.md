# AGENTS.md

Instructions for AI coding agents working in the Neuro repository. Human-facing
detail lives in [CONTRIBUTING.md](CONTRIBUTING.md), [VSA.md](VSA.md) and
[DESIGN.md](DESIGN.md); this file is the short operational contract.

## Project Overview

Neuro is an AOT-compiled language for high-performance AI development. Source
files (`.nr`) compile to native binaries through LLVM 23. The compiler is
written in Rust and laid out as a Cargo workspace.

Pipeline:

```text
source (.nr) -> lexical analysis -> syntax parsing -> module resolution
             -> argument binding -> semantic analysis -> HIR lowering
             -> MLIR tensor bodies -> LLVM backend -> linker -> native binary
```

Workspace layout (`compiler/`):

| Crate | Role |
| --- | --- |
| `infrastructure/shared-types` | `Span`, `Identifier`, `Literal`: no business logic |
| `infrastructure/ast-types` | AST node definitions (owned here, not in the parser) |
| `infrastructure/neuro-hir` | Typed HIR: the frontend/backend contract |
| `lexical-analysis` | Tokenizer (logos) |
| `syntax-parsing` | Pratt expression parser + statement parser |
| `module-resolution` | Multi-file program loading |
| `argument-binding` | Named/positional argument binding |
| `semantic-analysis` | Type checking, scope resolution |
| `hir-lowering` | AST → typed HIR |
| `llvm-backend` | Codegen via inkwell (LLVM 23) |
| `mlir-backend` | MLIR/melior codegen for tensor bodies and GPU kernels |
| `neuroc` | CLI driver; the only crate allowed to depend on every slice |

**Architecture: Vertical Slice Architecture (VSA)**, rules in [VSA.md](VSA.md). The
ones an agent must not break:

- A feature slice depends only on `infrastructure/` crates. Never on another
  feature slice.
- One allowlisted exception: `syntax-parsing` depends on `lexical-analysis`
  (`parse()` calls `tokenize()`), enforced by
  `compiler/neuroc/tests/architecture_tests.rs`. A second one needs that test
  changed first.
- Feature slices in each other's `[dev-dependencies]` are fine; the
  architecture test reads `[dependencies]` only.
- No business logic in infrastructure crates.
- Duplication across slices is preferred over coupling.

Status lives in the [Quick Roadmap](README.md#quick-roadmap) and
[CHANGELOG.md](CHANGELOG.md) only.

## Build and Test Commands

LLVM 23 and MLIR 23, in one prefix, must be installed and three variables
exported before anything builds:

```bash
prefix=/opt/llvm-mlir-23                  # Arch / CachyOS (source build)
prefix=/usr/lib/llvm-23                   # Ubuntu / Debian (apt.llvm.org)
prefix=$(brew --prefix llvm)              # macOS
export LLVM_SYS_231_PREFIX=$prefix MLIR_SYS_230_PREFIX=$prefix TABLEGEN_230_PREFIX=$prefix
```

[Installation](docs/getting-started/installation.md) has each platform's
recipe, Windows' source build included.

```bash
cargo build --workspace                # build everything
cargo build --profile dev-optimized    # faster iteration (opt-level 1)
cargo build --release                  # release build

cargo test --workspace                 # full test suite

cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
python tools/check_docs_hygiene.py     # documentation hygiene gate
python tools/clean_stale_target.py     # garbage-collect stale target/ artifacts
```

Running the compiler:

```bash
cargo run -p neuroc -- check   examples/basics/hello.nr   # type-check only
cargo run -p neuroc -- compile examples/basics/hello.nr   # compile to a binary
cargo run -p neuroc -- compile examples/basics/hello.nr -O2 -o hello
```

**Quality gates.** Every change leaves these green. `tools/gates.sh` runs all
five with one exit code (`--full` adds the release build and rustdoc, `--asan`
runs every example under AddressSanitizer):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p neuroc --test architecture_tests
python tools/check_docs_hygiene.py
```

## Code Style Guidelines

Rust, edition 2024, MSRV in the root `Cargo.toml`. Formatting is whatever `cargo fmt` produces:
never hand-format around it.

- **No `unwrap()` / `expect()` in production code paths.** Return
  `Result<T, E>` with an actionable message. Tests may use them.
- **`pub(crate)` is the default visibility.** `pub` is reserved for a slice's
  entry points. Do not widen a surface with permissive `pub use`.
- **Explicit integer widths** on public-facing APIs (`u32`, `i64`), not
  `usize` for domain values.
- **Borrow rather than clone.** A `.clone()` in a hot compiler path needs a
  reason.
- **Document every `unsafe` block** with a safety rationale comment.
- **Suppress a lint with `#[expect(...)]`, never `#[allow(...)]`** (enforced by
  the workspace `allow_attributes` lint).
- **Doc comments (`///`) on public APIs.** Describe the contract, not the
  implementation.
- Match the surrounding code's naming and comment density. New code should be
  indistinguishable in style from the slice it lives in.

Backend-specific invariants (see `compiler/llvm-backend/CONTEXT.md` for the
authoritative version):

- String ABI is a `{ ptr, i64 }` fat pointer. Bit 63 of the length is the owner bit, set when
  the value owns its buffer, and every reader of the length masks it.
- Struct ABI is an anonymous LLVM struct in declaration order.
- `semantic-analysis` is not a production dependency of the backend; `neuroc`
  owns the pipeline ordering.

**`CONTEXT.md` is a blocker** (VSA AC-012): update the slice's file in the same
commit as any change to its entry point, public surface, or dependencies.

**Commits.** Format: `scope: short summary` (50 chars or less), with an
optional body explaining what and why. Valid scopes: `lexer`, `parser`,
`semantic`, `codegen`, `infra`, `tests`, `examples`, `docs`, `build`, `ci`. A
contributed commit carries a `Signed-off-by:` trailer (`git commit -s`) per the
DCO in CONTRIBUTING.md. The human committer adds it, so an agent never writes one into
a message. No co-author trailers.

**Versioning.** The number lives once, in `[workspace.package].version`, and
tracks the roadmap rather than SemVer: see
[CONTRIBUTING.md](CONTRIBUTING.md#project-status).

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
- **End-to-end tests**: `compiler/neuroc/tests/suite/`, one file per language
  feature, each a module of the single `suite` target (declare it in
  `suite/main.rs`, never a new file at `tests/` root). They compile and run real
  `.nr` source through the `CompileTest` helper and the shared functions in
  `compiler/neuroc/tests/suite/compile_harness/mod.rs` (`run`, `rejected`,
  `stdout_of`, `compile_tagged`, ...): use those before writing a local helper.
- **Example programs**: `examples/`, grouped by topic. Each program is pinned
  two ways: its `main` return value is its process exit code, registered in
  `examples/expected.txt`, and its stdout is fixed byte for byte in a sibling
  `.out` file. `compiler/neuroc/tests/examples.rs` checks both. Adding an
  example means adding both pins. Programs that combine features live under
  `examples/integration/<theme>/`. `examples/showcase/` is a curated handful
  of flagships: nothing is added there without the maintainer's say.
- **Architecture tests**: `compiler/neuroc/tests/architecture_tests.rs`
  enforces the VSA dependency rules. If it fails, the fix is the dependency,
  not the test.

Cover error paths and boundary conditions, not just the happy path. Known
open defects are registered in [docs/BUGS.md](docs/BUGS.md) as `BUG-NNN`;
check there before reporting a new one, and remove the entry when it is fixed.

Never write a test count or the version into a document:
`tools/check_docs_hygiene.py` fails the build on both, and on references to
local-only paths.

CI additionally runs the suite on Linux/macOS/Windows against stable, plus a
Linux nightly leg, OSV-Scanner over the dependency graph, benchmark regression budgets,
release smoke tests (compile and run two examples with the release `neuroc`), and coverage.
A separate release workflow builds the prebuilt Linux `neuroc` from the scripts in
`tools/release/` and attaches it to each GitHub Release once every example passes in a
container with only `cc`.

## Security Considerations

Security-relevant work concentrates in three places:

- **Compiler integrity**: malformed or adversarial `.nr` input must not
  panic, crash, read out of bounds, or hang the compiler (the reason for the
  no-`unwrap()` rule).
- **Generated code safety**: a backend bug that emits incorrect IR is a
  security bug (uninitialized reads, bad pointer arithmetic, missing overflow
  or bounds guards). Codegen changes need runtime tests that execute the
  produced binary, not just IR inspection.
- **Dependency vulnerabilities**: run `cargo audit` when touching
  `Cargo.toml`; CI runs OSV-Scanner. Do not add a dependency without a clear reason,
  and never pin to an unmaintained crate.

Rules for agents:

- Never reference a gitignored path (link, `include`, `mod`, or prose) from a
  tracked file; remove the reference instead.
- Do not weaken or delete a runtime guard (overflow check, bounds check,
  division guard, null check) to make a test pass.
- Do not disable, `#[ignore]`, or loosen a failing test, lint, or architecture
  check to get green. Fix the cause.
- Report vulnerabilities privately through GitHub's security advisory system,
  never in a public issue. See [SECURITY.md](SECURITY.md) for the process and
  response timelines.
