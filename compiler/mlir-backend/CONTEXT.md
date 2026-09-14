# mlir-backend

## Purpose
Lower the typed HIR to MLIR for the tensor / autodiff / GPU path. It consumes `neuro_hir::HirProgram` and emits a verifier-clean module: a `func.func` declaration per function, except where a body is element-wise tensor arithmetic or a matrix product, which becomes a definition built from the `linalg` and `tensor` dialects. The same module carries on through bufferization and the `llvm` dialect into a verified inkwell LLVM module, so a `linalg` body arrives as a real loop nest and the HIR → MLIR → llvm dialect → inkwell pipeline is proven end to end.

## Feature Gate
The whole crate is opt-in behind the off-by-default `mlir` feature
(`mlir = ["dep:melior", "dep:mlir-sys", "dep:inkwell", "dep:thiserror", "dep:neuro-hir"]`). Disabled, it compiles to an empty
placeholder pulling in no MLIR toolchain (nor `neuro-hir`), so a default
`cargo build/test --workspace` works on stock LLVM 20 on every CI OS. Enabled, it exposes the
entry points below. CI provisions MLIR only on Linux, where the `--all-features` lint job and a
`cargo test -p mlir-backend --features mlir` step exercise the gated code; the Windows/macOS
legs build the placeholder.

## Entry Points (feature `mlir`)
- `lower_program(&HirProgram) -> Result<String, MlirError>`: walks the typed HIR and returns
  the textual form of a verified module of `func.func` declarations.
- `translate_to_llvm_ir(&HirProgram) -> Result<String, MlirError>`: the same module carried on
  through a bufferization and conversion pipeline into the `llvm` dialect, translated into an
  inkwell LLVM module, LLVM-verified, and returned as textual LLVM IR.
- `emit_smoke_module() -> Result<String, MlirError>`, the HIR-independent wiring check: builds
  `func.func @neuro_smoke(index, index) -> index` with a single `arith.addi` body, verifies it,
  and returns its textual form.

## Shared Kernel
- `neuro-hir`: the typed HIR contract `lower_program` consumes, gated under `mlir`.
- `ast-types`: `BinaryOp`, which the HIR's `Binary` expression carries rather than redeclaring,
  gated under `mlir`.

The crate adds no business logic of its own beyond the lowering; it otherwise uses only
third-party `melior` + `mlir-sys` + `inkwell` + `thiserror`.

## Notes
**The MLIR → LLVM crossing.** `translate_to_llvm_ir` runs `LLVM_LOWERING_PIPELINE`, named in
text and parsed by `melior::utility::parse_pass_pipeline` because melior wraps no bufferization
pass. Its first three entries are what carry a `linalg` body: `one-shot-bufferize` (with
`bufferize-function-boundaries=true`, or a `func.func` keeps `tensor` in its signature and never
converts) rewrites tensor values into `memref` buffers, `buffer-deallocation-pipeline` gives each
one an owner, and only then does `convert-linalg-to-loops` — nested under `func.func`, which is
what it is anchored on — produce `scf` loops; run before bufferization it silently leaves the op
alone. The rest is the descent those loops land in: `convert-scf-to-cf`, `finalize-memref-to-llvm`,
then `func` / `arith` / `cf` / `index` to LLVM and `reconcile-unrealized-casts` last by necessity,
since each conversion leaves `unrealized_conversion_cast` ops at its boundary with the dialects the
others own and the translation rejects any that survive. Pass names in a textual pipeline must be in
the process-global registry, so `new_context` calls `register_all_passes` behind a `Once`.

It then calls `mlirTranslateModuleToLLVMIR`
**directly through `mlir-sys`**: `melior 0.25` does not wrap it, and `mlir-sys 0.5.0` is pinned to
the exact version melior itself depends on so both reach one crate instance and their
`MlirOperation` / `LLVMContextRef` types unify.

The `LLVMContext` the translation builds into is **inkwell's**, and the returned `LLVMModuleRef`
is wrapped by `inkwell::module::Module` (sole owner, disposes on drop) and put through LLVM's
verifier. That is the whole point of the entry point: `mlir-sys` and `llvm-sys` are independent
bindings, and a build where they resolve to different `libLLVM-20` copies fails at this handoff
rather than miscompiling later. Each binding declares its own opaque `LLVMContextRef` /
`LLVMModuleRef` alias over the same C type, so the pointers are cast across.

`register_all_llvm_translations` runs in `new_context()` for every path, not only the translating
one: the translation interfaces have to be on the context that *built* the module.

**Toolchain pinning.** `melior 0.25.1` is the newest release targeting MLIR 20 (via
`mlir-sys 0.5.0`); `melior 0.26+` moved to MLIR 21/22. `mlir-sys` carries no `llvm-sys`
dependency. It discovers MLIR through `MLIR_SYS_200_PREFIX` / `TABLEGEN_200_PREFIX` and links
its own `MLIR` key, so it coexists with inkwell's `llvm-20` link with no Cargo `links` conflict.
Pointing those prefixes at the same LLVM 20 build as `LLVM_SYS_201_PREFIX` makes both bindings
share one `libLLVM-20` dylib. That prefix must include MLIR (`mlir-c` headers + `libMLIR*`);
Arch's stock `llvm20` omits MLIR, so build LLVM 20 with `-DLLVM_ENABLE_PROJECTS=mlir`.

**Tensor arithmetic is the only body lowered here.** `tensor_arithmetic::build_body` turns a
function whose statements are `val` bindings and a final `return` over element-wise `+ - * /`
on tensors into a `func.func` definition: one `tensor.empty` destination plus one
`linalg.generic` per operator, with one indexing map per operand, all-`parallel` iterators, and
an `arith` body terminated by `linalg.yield`. `@` is the exception and is described below. Float elements use the `arith` float operations
and integer elements theirs, with division splitting on signedness.

**Broadcasting is per-operand indexing maps.** Operand shapes align at their *trailing* axis,
so an operand of lower rank supplies the innermost axes and its map simply omits the leading
result dimensions. An extent of 1 against a larger result extent is stretched: that axis maps
to the constant `0`, so the operand is read at index 0 at every point the result axis covers. A
scalar operand of the element type has no index space at all and maps to `()`, which is how
`linalg.generic` hands one value to every point. The destination always keeps the identity map;
that is what makes the operation element-wise rather than a gather. Any other mismatch — an
extent neither equal nor 1, an operand outranking the result, or a different element type — is
a shape error the frontend owns, so it answers `Ok(None)` rather than being lowered wrongly.

**A `?` extent sizes the destination from an operand.** `tensor.empty` needs one `index`
operand per dynamic axis, and those come from `tensor.dim` on an operand that walks that axis
itself. A *stretched* operand cannot supply one: it is size 1 there and says nothing about the
result. For the same reason a `?` operand extent is never stretched — nothing here can prove it
is 1 at run time, and guessing wrong would silently read the wrong element — so a result axis
no operand walks leaves the function a declaration.

It answers `Ok(None)`, meaning "leave this function a declaration", for everything else, and
that is a design decision rather than a gap to fill: scalar arithmetic and every 2B tensor
operation stay on the inkwell backend permanently, so lowering them here would be the second
copy the sub-phase's decision exists to prevent. `None` also covers `f16` / `bf16` elements,
which carry no arithmetic in the HIR contract, and a scalar *literal* operand, since the body
builder lowers variables and operators only.

**A matrix product is a contracting `linalg.generic`.** `build_matmul` reads `@` instead of the
element-wise path and emits the canonical three-operation shape: a `tensor.empty`, a
`linalg.generic` that fills it with the element's zero from an `arith.constant` scalar input, and
a second one whose index space is `(row, column, contracted)` with maps `(d0, d2)` / `(d2, d1)` /
`(d0, d1)`, iterators `parallel, parallel, reduction`, and a multiply-accumulate body. The fill is
not optional: a reduction READS its destination at every point, and `tensor.empty` is undefined
memory. Named `linalg.matmul` and `linalg.fill` are still not reachable — melior's ODS module
generates from `LinalgOps.td` only — so all three go through the one `generic_op` builder, which
takes its operand split, maps, iterators and body region as arguments. Every extent must be
static here: `tensor.dim` can recover a dynamic result axis but not the contracted one, which
appears in no operand of the destination, so a `?` anywhere answers `Ok(None)`.

**The bufferized function has MLIR's tensor ABI, not Neuro's.** A tensor parameter crosses as an
exploded `memref` descriptor — allocated pointer, aligned pointer, offset, sizes, strides — where
the LLVM backend's tensor is one pointer to a flat buffer behind a DLPack handle, and the result
buffer is the caller's to free. Nothing calls the MLIR path from a compile, so the two never meet;
the boundary layout is deliberately left at MLIR's default until something does.

**What `lower_program` emits.** It registers all dialects, then maps each top-level `HirItem`:
free functions, `impl` methods, and lifted closures become `func.func` *declarations* (empty
region, private visibility: external symbols, not definitions); structs, enums, and constants
carry no callable surface and are skipped. A lifted closure (`HirItem::Closure`, symbol
`__closure_N`) declares its captured-environment pointer as an implicit first parameter ahead
of the user-facing ones, matching the LLVM backend's calling convention. The module is run
through the MLIR verifier before its textual form is returned.

**Type mapping.** HIR scalars map to MLIR scalars (`i8`–`i64`, `i1` for `bool`, `i32` for
`char`, `f16`/`bf16`/`f32`/`f64`). Every aggregate / reference / string type, meaning tuples,
enums, and the standard collections (`Vec` / `HashMap` / `BTreeMap`), maps to an opaque `!llvm.ptr`
until real tensor and struct lowering lands. A newtype is transparent: `HirType::Newtype`
maps to its inner type's mapping. `void` is the empty result list in return position and
`MlirError::UnsupportedType` anywhere else, as are the unsized types (`dyn Trait`, `[T]`),
which reach a value position only behind the reference that already maps to a pointer.
`HirType::Tensor` maps to a ranked MLIR tensor (`tensor<2x3xf32>`), the one aggregate that is
not an opaque pointer, because it is the one the dialects below operate on. A `?` axis becomes
MLIR's dynamic sentinel, read from `mlirShapedTypeGetDynamicSize` rather than written out. The
element must map to an MLIR integer or float; an aggregate element is `UnsupportedType`, since
`tensor<...>` does not accept `!llvm.ptr`. The LLVM backend's own flat buffer layout for a
tensor is untouched and stays the representation every 2B operation uses.

`map_type` matches `HirType` exhaustively with **no wildcard**, so a new HIR variant is a
compile error here rather than a silent mis-map. Because the crate builds only under the
off-by-default feature, that error surfaces on the `--all-features` CI job, not on a default
`cargo build`, which is the one thing to remember when adding a `HirType` variant.
