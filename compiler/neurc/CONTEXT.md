# neurc

## Purpose
Orchestrate the full Neuro compiler pipeline and expose it as a CLI tool.

## Entry Point
- Type: CLI
- Input: `neurc check <file.nr>` | `neurc compile <file.nr> [-O<0-3>] [-o <output>]`
- Output: Executable binary on success; diagnostic errors and non-fatal lint warnings to stderr

## Data Ownership
- Tables: none
- Events Published: none
- Events Consumed: none
- Public Read Model: none

## Shared Kernel
- diagnostics — pipeline error formatting
- project-config — reads `neurc.toml` workspace configuration
- source-location — source span resolution for error display

## Notes
- 2026-08-23: Implicit prelude. `prelude.rs` became a `Prelude` value: `prelude::load()` parses
  `prelude.nr` once and answers both `variants()` — every variant of every enum it declares,
  handed to `module_resolution::resolve_program` so each module may write `Some` / `Ok` bare —
  and `prepend()`, the declarations themselves. Reading the variant list off the parsed prelude
  is what keeps `prelude.nr` the single place the prelude's contents are stated; module
  resolution is *told* them for the same reason it is handed the parser. `resolve_modules` became
  `load_program`, which also decides whether to prepend at all: `ResolvedProgram.no_prelude`
  reports the root file's `@no_prelude`, and the merged namespace is flat, so the prelude's
  declarations are either in the program or absent from all of it.
- 2026-08-22: Prelude visibility. `with_prelude` stamps every prelude declaration with
  `PRELUDE_MODULE` (`ModuleId::MAX`), an id no loaded file can hold: the prelude is prepended
  *after* module resolution has numbered the program's files, and leaving it at 0 would make its
  internals private to whichever file happens to be the root rather than to the prelude itself.
  `prelude.nr` marks its own surface with `export` for the same reason it now has to —
  `OrderedF32` / `OrderedF64` and their `value` field are reachable from any module.
- 2026-08-16: Multi-file compilation. `check_file` and `compile_file` no longer parse the input
  themselves: both call the module-resolution entry point (today's `load_program`), which hands `syntax_parsing::parse` to
  `module_resolution::resolve_program` and gets back the merged item list of every module the
  root reaches through a qualified path. The parser is passed in rather than depended on because
  `module-resolution` may not import a feature slice — neurc is the single place the two meet.
  The prelude still rides on top of the merged program, so a local declaration in any module
  shadows a prelude item exactly as before. The backend is still handed the *root* file's source
  for panic-location rendering; merged modules share one span space, the same approximation the
  prepended prelude has always had.
- 2026-07-27: Standard collections. `prelude.nr` gains the `OrderedF32` / `OrderedF64` validating
  wrapper structs — `@derive(Copy, Clone)`, a `new` constructor that panics on NaN, and `PartialEq` +
  `Comparable` impls. They exist so an ordered map can be keyed on a float: IEEE-754 `<` is a partial
  order, so a raw float key could be inserted and never found again. They deliberately do not
  implement `Hashable`.
- 2026-07-26: Implicit prelude. New `src/prelude.rs` + `src/prelude.nr`: after parsing, both
  `check_file` and `compile_file` call `prelude::with_prelude`, which parses the built-in
  `prelude.nr` source (currently `enum Option<T>` and `enum Result<T, E>`) and prepends its items to
  the program's own. A prelude item whose name the program already declares is dropped, so a local
  declaration shadows the prelude. The items are ordinary declarations: nothing downstream
  special-cases `Option` or `Result`. Unqualified names (`Some(x)` without `Option::`) and
  `@no_prelude` are the module-system item's work and are not part of this (both landed
  2026-08-23, above).

neurc is the only component permitted to depend on all feature slices. It holds no
business logic of its own; every decision is delegated to the owning slice.
The two-step linker strategy (clang on Unix; lld-link / cl.exe on Windows) is
required because LLVM object files need a platform linker driver to attach the C
runtime startup code — neurc cannot ship its own linker.

Lint warnings emitted by `semantic_analysis::type_check` are forwarded to stderr by
`print_warnings` in both `check_file` and `compile_file`. Warnings never cause a
non-zero exit; they are informational guidance and may be silenced with `@allow(...)`
on the enclosing function.

After a successful `type_check`, both `check_file` and `compile_file` lower the AST to
typed HIR via `hir_lowering::lower_program` (1D). `check` reports the lowered item
count; `compile` hands the HIR directly to `llvm_backend::compile`, which lowers native
object code from the typed HIR (the backend no longer consumes the AST).
