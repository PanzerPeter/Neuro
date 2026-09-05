# mlir-backend

## Purpose
Lower the typed HIR to MLIR for the tensor / autodiff / GPU path. Today it is a scaffold: it consumes `neuro_hir::HirProgram` and emits a verifier-clean module of `func.func` declarations, proving the HIR → `melior` → verified MLIR pipeline end to end. Real body lowering (linalg / tensor dialects) is later work.

## Feature Gate
The whole crate is opt-in behind the off-by-default `mlir` feature
(`mlir = ["dep:melior", "dep:thiserror", "dep:neuro-hir"]`). Disabled, it compiles to an empty
placeholder pulling in no MLIR toolchain (nor `neuro-hir`), so a default
`cargo build/test --workspace` works on stock LLVM 20 on every CI OS. Enabled, it exposes the
entry points below. CI provisions MLIR only on Linux, where the `--all-features` lint job and a
`cargo test -p mlir-backend --features mlir` step exercise the gated code; the Windows/macOS
legs build the placeholder.

## Entry Points (feature `mlir`)
- `lower_program(&HirProgram) -> Result<String, MlirError>` — walks the typed HIR and returns
  the textual form of a verified module of `func.func` declarations.
- `emit_smoke_module() -> Result<String, MlirError>` — the HIR-independent wiring check: builds
  `func.func @neuro_smoke(index, index) -> index` with a single `arith.addi` body, verifies it,
  and returns its textual form.

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- `neuro-hir` — the typed HIR contract `lower_program` consumes, gated under `mlir`. The crate
  adds no business logic of its own; the gated path otherwise uses only third-party `melior` +
  `thiserror`.

## Notes
**Toolchain pinning.** `melior 0.25.1` is the newest release targeting MLIR 20 (via
`mlir-sys 0.5.0`); `melior 0.26+` moved to MLIR 21/22. `mlir-sys` carries no `llvm-sys`
dependency — it discovers MLIR through `MLIR_SYS_200_PREFIX` / `TABLEGEN_200_PREFIX` and links
its own `MLIR` key, so it coexists with inkwell's `llvm-20` link with no Cargo `links` conflict.
Pointing those prefixes at the same LLVM 20 build as `LLVM_SYS_201_PREFIX` makes both bindings
share one `libLLVM-20` dylib. That prefix must include MLIR (`mlir-c` headers + `libMLIR*`);
Arch's stock `llvm20` omits MLIR, so build LLVM 20 with `-DLLVM_ENABLE_PROJECTS=mlir`.

**What `lower_program` emits.** It registers all dialects, then maps each top-level `HirItem`:
free functions, `impl` methods, and lifted closures become `func.func` *declarations* (empty
region, private visibility — external symbols, not definitions); structs, enums, and constants
carry no callable surface and are skipped. A lifted closure (`HirItem::Closure`, symbol
`__closure_N`) declares its captured-environment pointer as an implicit first parameter ahead
of the user-facing ones, matching the LLVM backend's calling convention. The module is run
through the MLIR verifier before its textual form is returned.

**Type mapping.** HIR scalars map to MLIR scalars (`i8`–`i64`, `i1` for `bool`, `i32` for
`char`, `f16`/`bf16`/`f32`/`f64`). Every aggregate / reference / string type — tuples, enums,
and the standard collections (`Vec` / `HashMap` / `BTreeMap`) — maps to an opaque `!llvm.ptr`
until real tensor and struct lowering lands. A newtype is transparent: `HirType::Newtype`
maps to its inner type's mapping. `void` is the empty result list in return position and
`MlirError::UnsupportedType` anywhere else, as are the unsized types (`dyn Trait`, `[T]`),
which reach a value position only behind the reference that already maps to a pointer.
`HirType::Tensor` is `UnsupportedType` too: it is the one variant that will eventually get a
real (Linalg-backed) mapping here rather than an opaque pointer, so it is left unmapped until
2C lowers tensor arithmetic. The LLVM backend already gives a tensor a flat buffer layout;
this path deliberately waits for the dialect rather than copying that.

`map_type` matches `HirType` exhaustively with **no wildcard**, so a new HIR variant is a
compile error here rather than a silent mis-map. Because the crate builds only under the
off-by-default feature, that error surfaces on the `--all-features` CI job, not on a default
`cargo build` — which is the one thing to remember when adding a `HirType` variant.
