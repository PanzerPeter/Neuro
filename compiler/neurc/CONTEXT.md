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
