# mlir-backend

## Purpose
Lower the typed HIR to MLIR for the Phase 2+ tensor / autodiff / GPU path. As of
the 1D scaffold it consumes [`neuro_hir::HirProgram`] and emits a trivial,
verifier-clean MLIR module (one `func.func` declaration per function / method),
proving the HIR → `melior` → verified MLIR pipeline end-to-end. Real body lowering
(linalg / tensor dialects) is Phase 2+.

## Feature Gate
The MLIR path is opt-in behind the off-by-default `mlir` feature
(`mlir = ["dep:melior", "dep:thiserror", "dep:neuro-hir"]`). With the feature
**disabled** this crate compiles to an empty placeholder and pulls in no MLIR
toolchain (nor `neuro-hir`), so the default `cargo build/test --workspace` works
on stock LLVM 20 across all CI OSes. With the feature **enabled** it pulls in
`melior` + `neuro-hir` and exposes the entry points below. CI provisions MLIR only
on Linux, where the `--all-features` lint job and a dedicated
`cargo test -p mlir-backend --features mlir` smoke step exercise the gated code;
the Windows/macOS test legs build the placeholder.

## Entry Points (feature `mlir`)
- `lower_program(&HirProgram) -> Result<String, MlirError>` — the HIR → MLIR
  scaffold: walks the typed HIR and returns the textual form of a verified module
  of `func.func` declarations.
- `emit_smoke_module() -> Result<String, MlirError>` — the pure-`melior` wiring
  check (builds + verifies `func.func @neuro_smoke` with an `arith.addi` body),
  independent of HIR.

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- `neuro-hir` (infrastructure) — the typed HIR contract `lower_program` consumes,
  gated under the `mlir` feature. The crate adds no business logic of its own; the
  `mlir`-gated path otherwise uses only the third-party `melior` + `thiserror`.

## Notes
`melior 0.25.1` is the newest release targeting MLIR 20 (via `mlir-sys 0.5.0`);
`melior 0.26+` moved to MLIR 21/22. `mlir-sys` carries no `llvm-sys` dependency:
it discovers MLIR through `MLIR_SYS_200_PREFIX` / `TABLEGEN_200_PREFIX` and links
its own `MLIR` key, so it coexists with inkwell's `llvm-20` link with no Cargo
`links` conflict. Pointing those prefixes at the same LLVM 20 build as
`LLVM_SYS_201_PREFIX` makes both bindings share one `libLLVM-20` dylib. That
prefix must include MLIR (`mlir-c` headers + `libMLIR*`); Arch's stock `llvm20`
omits MLIR, so build LLVM 20 with `-DLLVM_ENABLE_PROJECTS=mlir`.

`lower_program` registers all dialects, then maps each top-level `HirItem`:
free functions and `impl` methods become `func.func` *declarations* (empty region,
private visibility — external symbols, not definitions); structs and constants are
skipped. HIR types map to MLIR scalars (`i8`–`i64`, `i1` for `bool`, `i32` for
`char`, `f16`/`bf16`/`f32`/`f64`), and every aggregate / reference / string type
— including the tuple type `(T1, T2, ...)` (§3.2) and the enum type (§3.5) — maps
to an opaque `!llvm.ptr` until real tensor and struct lowering lands (Phase 2+).
Enum items, like structs and constants, carry no callable surface and are skipped. `void` is the empty result list in return position and an error
(`MlirError::UnsupportedType`) anywhere else. The module is run through the MLIR
verifier before its textual form is returned.

`emit_smoke_module` is the HIR-independent wiring check: it builds
`func.func @neuro_smoke(index, index) -> index` whose body is a single
`arith.addi`, verifies it, and returns the textual form. Both functions are
exercised by the crate's unit tests under `cargo test -p mlir-backend --features
mlir`; the default `cargo test --workspace` builds the placeholder and runs
neither.
