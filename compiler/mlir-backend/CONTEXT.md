# mlir-backend

## Purpose
Anchor the `melior` (Rust MLIR bindings) integration for the Phase 3+ tensor /
autodiff / GPU lowering path. As of Phase 1.8 item 1 it does not consume HIR or
take part in compilation — it exists to bring `melior` into the workspace
alongside inkwell and prove both bindings link the same LLVM 20 / MLIR 20
toolchain. The HIR-consuming lowering entry point arrives once typed HIR exists
(Phase 1.8 items 2–5).

## Feature Gate
The MLIR path is opt-in behind the off-by-default `mlir` feature
(`mlir = ["dep:melior", "dep:thiserror"]`). With the feature **disabled** this
crate compiles to an empty placeholder and pulls in no MLIR toolchain, so the
default `cargo build/test --workspace` works on stock LLVM 20 across all CI OSes.
With the feature **enabled** it pulls in `melior` and exposes the entry point
below. CI provisions MLIR only on Linux, where the `--all-features` lint job
exercises the gated code; the Windows/macOS test legs build the placeholder.

## Entry Point (feature `mlir`)
- Type: Library function `emit_smoke_module`
- Input: none
- Output: `Result<String, MlirError>` — textual MLIR of a verified trivial module

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- none yet (the placeholder needs no infrastructure crate; the `mlir`-gated path
  uses only the third-party `melior` + `thiserror`). A `diagnostics` dependency
  arrives with the HIR-consuming lowering entry point.

## Notes
`melior 0.25.1` is the newest release targeting MLIR 20 (via `mlir-sys 0.5.0`);
`melior 0.26+` moved to MLIR 21/22. `mlir-sys` carries no `llvm-sys` dependency:
it discovers MLIR through `MLIR_SYS_200_PREFIX` / `TABLEGEN_200_PREFIX` and links
its own `MLIR` key, so it coexists with inkwell's `llvm-20` link with no Cargo
`links` conflict. Pointing those prefixes at the same LLVM 20 build as
`LLVM_SYS_201_PREFIX` makes both bindings share one `libLLVM-20` dylib. That
prefix must include MLIR (`mlir-c` headers + `libMLIR*`); Arch's stock `llvm20`
omits MLIR, so build LLVM 20 with `-DLLVM_ENABLE_PROJECTS=mlir`.

`emit_smoke_module` registers all dialects, builds
`func.func @neuro_smoke(index, index) -> index` whose body is a single
`arith.addi`, runs the MLIR verifier, and returns the module's textual form. It
is the Phase 1.8 integration smoke test (exercised by the crate's unit test and
`cargo test --workspace`) until real HIR lowering lands.
