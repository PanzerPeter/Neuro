# Changelog

All notable changes to the NEURO programming language compiler will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added

- **parser/semantic/codegen**: Struct types — definition, instantiation, field access, and field mutation
  - `struct Name { field: Type, ... }` declarations parsed as `Item::Struct`
  - Struct literal expressions `Name { field: value, ... }` with full type checking
  - Field read via `.field` infix (`Expr::FieldAccess`), codegen via LLVM `build_struct_gep` + `build_load`
  - Field write `obj.field = value` on `mut` bindings (`Stmt::FieldAssignment`), codegen via `build_struct_gep` + `build_store`
  - Two-pass semantic analysis: structs pre-registered before function bodies are checked, so definition order doesn't matter
  - Nominal typing: `Type::Struct(name)` matched by name; struct-literal fields validated for presence, uniqueness, and type
  - Immutability enforced: mutating a field of a `val` binding is a compile error (`AssignToImmutableField`)
  - `no_struct_lit` parser flag prevents `{ }` after bare identifiers from being parsed as struct literals inside `if`/`while`/`for` conditions
  - 10 integration tests in `compiler/neurc/tests/structs.rs` covering all acceptance criteria
  - `examples/structs.nr` runnable example

- **codegen**: String equality operators `==` and `!=`
  - Lowered to length check (fast path) + `memcmp` call via external libc symbol
  - `select` keeps `memcmp` safe when lengths differ (passes `n=0`)
  - 4 integration tests added to `neurc/tests/string_type.rs`
  - `docs/language-reference/operators.md` and `README.md` updated

### Fixed

- **codegen**: `codegen_if` branch check used the stale `then_bb`/`else_bb` reference
  after nested control flow moved the builder to an inner merge block — replaced with
  `builder.get_insert_block()` check (mirrors the existing pattern in `codegen_while`)
- **codegen**: `codegen_binary` read `span.start` (result type, e.g. `Bool`) instead of
  `span.start + 1` (left-operand type) — this silently broke float comparisons and
  prevented string equality dispatch

### Changed

- **codegen**: String type now uses fat pointer `{ ptr, i64 }` ABI instead of bare `ptr`
  - `type_mapping.rs`: `Type::String` maps to anonymous LLVM struct `{ ptr, i64 }`
  - `codegen.rs`: string literals built via `insertvalue` instructions; field 0 = pointer to
    null-terminated UTF-8 bytes in `.rodata`, field 1 = byte count excluding null terminator
  - `lib.rs`: target machine relocation model changed from `RelocMode::Default` to
    `RelocMode::PIC` so emitted object files are linkable into PIE executables on modern Linux
  - `llvm-backend/CONTEXT.md`: String ABI section added documenting the fat pointer layout
- **infra**: `OptimizationLevel` default impl replaced with `#[derive(Default)]` + `#[default]`
  on `O0` (clippy `derivable_impls`)
- **llvm-backend**: Upgraded inkwell `0.6.0` (LLVM 18) → `0.8.0` (LLVM 20)
  - Updated `[workspace.dependencies]` inkwell feature flag to `llvm20-1`
  - Raised minimum Rust version (`rust-version`) from `1.70` to `1.85`
  - `LLVM_SYS_200_PREFIX` is now the required build-time env var (e.g. `/usr/lib/llvm20`)
  - Fixed `codegen.rs`: `try_as_basic_value().left()` → `.basic()` (inkwell 0.8 `ValueKind` API)
  - Updated `compiler/llvm-backend/CONTEXT.md` with LLVM 20 reference and future MLIR plan
  - Updated `.idea/roadmap.md` (v4.1) and `.idea/idea.md` with accurate backend stack,
    MLIR lowering strategy, Enzyme MLIR dialect plan, and GPU dialect paths
- **architecture-tests**: Renamed `test_all_slices_have_readme` → `test_all_slices_have_context_md`
  — README.md files replaced by CONTEXT.md across all slices; required sections updated to
  `Purpose`, `Entry Point`, `Data Ownership`, `Shared Kernel`
- **workspace**: Repository and homepage URLs updated to `github.com/PanzerPeter/Neuro`
- **workspace**: `Cargo.lock` format upgraded to version 4 (Cargo 1.85+)
- **neurc**: Improved linker detection with detailed error messages

### Added

- **control-flow**: Exclusive range `for` loops (`for i in 0..n`) end-to-end
  - `Stmt::ForRange` AST node in `ast-types`
  - Parser support for `for <ident> in <expr>..<expr> { ... }`
  - Semantic validation for integer range bounds and loop-scoped iterator binding
  - LLVM codegen with dedicated step block so `continue` advances the iterator correctly
  - Parser, semantic, and neurc integration tests

- **control-flow**: `break` and `continue` for `while` loops
  - `Stmt::Break` and `Stmt::Continue` AST nodes in `ast-types`
  - Semantic validation: `BreakOutsideLoop` / `ContinueOutsideLoop` errors
  - LLVM codegen loop-target stack for `break`/`continue` control transfer

- **control-flow**: `while` loops end-to-end
  - `Stmt::While` AST node; `while <condition> { ... }` syntax
  - Boolean loop condition enforcement in type checker
  - LLVM IR: `while.cond` / `while.body` / `while.exit` basic blocks

- **neurc**: `neurc compile <file.nr>` produces executables on Linux, macOS, Windows
  - Multi-stage linker fallback: clang → lld-link → MSVC/cc
  - Platform-specific object file handling
  - 16 end-to-end integration tests

- **neurc**: CLI contract integration test suite (`tests/cli_contract.rs`)
  - `neurc check` success path writes to stdout, empty stderr
  - `neurc check` type errors return non-zero exit and print diagnostics to stderr
  - `neurc compile` type errors return non-zero exit with failure diagnostics

- **semantic-analysis**: Contextual type inference for numeric literals
  - Integers and floats infer type from declaration/parameter/return context
  - Range validation: `300` cannot be assigned to `i8`
  - Defaults: integers → `i32`, floats → `f64`; large integers auto-promote to `i64`
  - `IntegerLiteralOutOfRange` error type

- **semantic-analysis**: Full type checking
  - Types: i32, i64, f32, f64, bool
  - Function signature validation and lexical scoping with variable shadowing
  - Multiple-error collection (fail-slow strategy)
  - 24 tests

- **semantic-analysis / lexical-analysis / llvm-backend**: String type (Phase 1)
  - `Type::String` in the type system; string literal checking and propagation
  - LLVM IR: string literals as global constants, opaque pointer mapping (LLVM 15+ style)
  - C-style null-terminated implementation (fat-pointer refactor planned for Phase 1.5)
  - Full escape sequence support: `\n`, `\r`, `\t`, `\\`, `\"`, `\0`, `\xNN`, `\u{NNNN}`

- **syntax-parsing / semantic-analysis / llvm-backend**: Mutable variable reassignment
  - `Stmt::Assign` AST node; `mut x: i32 = 0; x = 10` syntax
  - `SymbolInfo` tracks mutability; `AssignToImmutable` error for `val` targets
  - LLVM `store` instruction for assignment codegen

- **semantic-analysis / llvm-backend**: Extended primitive types
  - Signed: i8, i16 (complementing i32, i64)
  - Unsigned: u8, u16, u32, u64
  - Signedness-aware codegen: `sdiv`/`udiv`, `srem`/`urem`, `icmp s*/u*`

- **llvm-backend**: Complete LLVM code generation
  - Function codegen with parameters and return values
  - Expression codegen (arithmetic, comparison, logical)
  - Statement codegen (variables, return, if/else)
  - Object code emission for the native target
  - 4 tests

- **syntax-parsing**: Statement and function parsing
  - Variable declarations, return statements, expression statements
  - Function definitions with parameters and return types
  - If/else with multiple else-if clauses
  - 39 additional tests (65 total)

- **syntax-parsing**: Comprehensive test suite (117 tests)
  - `expression_tests.rs` (34), `statement_tests.rs` (20), `function_tests.rs` (16),
    `error_tests.rs` (31), `integration_tests.rs` (16)

### Fixed

- **neurc**: Object file linking race condition with tempfile cleanup
- **llvm-backend**: Type inference for identifiers and function calls
- **lexical-analysis**: `InvalidEscape` and `UnterminatedString` no longer masked as `UnexpectedChar`
- **syntax-parsing**: Hardcoded `Span::new(0, 0)` in `token_to_binary_op` error path replaced with the token's actual span
- **syntax-parsing**: Added maximum expression nesting depth (256) to prevent stack overflow
- **syntax-parsing**: Duplicate parameter names in function definitions now produce a compile error
- **semantic-analysis**: Symbol table correctly tracks mutability for all declaration forms

### Architecture

- Extracted AST types from `syntax-parsing` into new `compiler/infrastructure/ast-types` crate
  - `semantic-analysis` and `llvm-backend` now depend on `ast-types`, not `syntax-parsing`
  - VSA cross-slice dependency eliminated; `syntax-parsing` maintains backward-compatible re-exports
- Replaced per-slice `README.md` with `CONTEXT.md` (AI-contract files) across all feature slices
  - Sections: Purpose, Entry Point, Data Ownership, Shared Kernel, Notes
  - Architecture test `test_all_slices_have_context_md` enforces compliance
- Removed direct `llvm-backend` → `semantic-analysis` production dependency
  - `neurc` remains the single orchestration boundary for parse → type-check → codegen
  - `llvm-backend` uses a backend-local type model for codegen decisions

### Infrastructure

- CI: dedicated `Architecture` gate runs `cargo test -p neurc --test architecture_tests`
- CI: docs-consistency gate (`tools/check_docs_consistency.py`) on every push/PR
- CI: benchmark regression gate (`tools/check_benchmark_regression.py`) for `llvm-backend`
- CI: cross-platform release smoke gate — builds `neurc` on Linux, macOS, Windows
  and executes representative examples via `tools/run_release_smoke_tests.py`

---

## [0.1.0] - 2025-01-21

### Initial Release — Lexer and Expression Parser

### Added

- **lexical-analysis**: Complete tokenizer
  - Phase 1 keywords, number literals (binary/octal/hex/decimal/float), string literals,
    line and block comments, source span tracking
  - 28 tests

- **syntax-parsing**: Expression parser with Pratt precedence climbing
  - Literals, identifiers, function calls, binary and unary operators, parenthesized expressions
  - 26 tests

- **infrastructure**: Workspace setup with Vertical Slice Architecture (VSA)
  - inkwell 0.6.0 (LLVM 18 bindings) — replaced by LLVM 20 in Unreleased

### Fixed

- String error reporting (unterminated strings, invalid escapes)
- Redundant closure warnings in Unicode validation
