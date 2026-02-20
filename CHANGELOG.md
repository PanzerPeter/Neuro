# Changelog

All notable changes to the NEURO programming language compiler will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **neurc**: Added CLI contract integration tests for roadmap-claimed behavior
  - New test suite: [compiler/neurc/tests/cli_contract.rs](compiler/neurc/tests/cli_contract.rs)
  - Verifies `neurc check` success writes confirmation to stdout with empty stderr
  - Verifies `neurc check` type errors return non-zero exit and print diagnostics to stderr
  - Verifies `neurc compile` type errors return non-zero exit and print failure diagnostics
- **semantic-analysis**: Contextual type inference for numeric literals
  - Integer and float literals now infer types from context (variable declarations, function parameters, return expressions, assignments)
  - Range validation ensures literals fit within target type bounds (e.g., `300` cannot be assigned to `i8`)
  - Defaults: integers default to `i32`, floats to `f64` when no context is available
  - Large integers auto-promote to `i64` if they exceed `i32` range
  - New error type: `IntegerLiteralOutOfRange` for literals that don't fit in target type
  - 12 new unit tests for type inference scenarios in [type_checker.rs](compiler/semantic-analysis/src/type_checker.rs#L773-L1046)
  - 8 new integration tests in [neurc/tests/type_inference.rs](compiler/neurc/tests/type_inference.rs)
  - Example program demonstrating type inference: [type_inference_demo.nr](examples/type_inference_demo.nr)
  - **Note**: Full code generation support in LLVM backend deferred (semantic analysis validates types correctly; backend integration requires typed IR)
- **String Type (Phase 1)**: Basic string type implementation complete
  - **semantic-analysis**: Added `Type::String` variant to the type system
  - **semantic-analysis**: Type checking for string literals, variables, parameters, and returns
  - **semantic-analysis**: String type predicates (is_string()) and compatibility rules
  - **lexical-analysis**: String tokenization already supported (escape sequences: \n, \t, \", \\, \xNN, \u{NNNN})
  - **syntax-parsing**: String literal parsing already functional
  - **llvm-backend**: LLVM IR generation for string literals as global constants
  - **llvm-backend**: String type mapping to opaque pointers (LLVM 15+ compatible)
  - **examples**: Added string_test.nr demonstrating string usage patterns
  - **tests**: 12 new semantic analysis tests + 7 new end-to-end integration tests (19 total)
  - Syntax: `val msg: string = "Hello, NEURO!"`, `func greet() -> string { "Hello" }`
  - Type safety: Strict string type checking with no implicit conversions
  - C-style strings: Null-terminated for Phase 1 (Phase 2 will add length tracking)
  - Escape sequences: Full support for \n, \r, \t, \\, \", \0, \xNN, \u{NNNN}
  - Immutable: String literals are read-only (mutable strings deferred to Phase 2)
- **Variable Reassignment**: Full support for mutable variable assignment
  - **syntax-parsing**: Assignment statement AST node and parsing logic
  - **semantic-analysis**: Symbol table tracks mutability (SymbolInfo struct)
  - **semantic-analysis**: Type checking for assignments with mutability enforcement
  - **semantic-analysis**: New error type: AssignToImmutable
  - **llvm-backend**: Code generation for assignment statements (store instructions)
  - **neurc**: 3 comprehensive end-to-end integration tests
  - Syntax: `mut x: i32 = 0; x = 10; x = x + 5`
  - Type safety: Cannot assign to immutable variables (val)
  - Type checking: Value type must match variable type
- **semantic-analysis**: Extended primitive types support
  - Added signed integer types: i8, i16 (complementing existing i32, i64)
  - Added unsigned integer types: u8, u16, u32, u64
  - New type predicates: is_signed_int(), is_unsigned_int(), is_integer(), is_float()
  - Strict type compatibility: no implicit conversions between signed/unsigned or different widths
  - 10 new comprehensive tests for extended types
- **llvm-backend**: Signedness-aware code generation
  - Updated TypeMapper to handle all extended integer types
  - Signed vs unsigned division/modulo operations (build_int_signed_div vs build_int_unsigned_div)
  - Signed vs unsigned comparison predicates (SLT/SGT/SLE/SGE vs ULT/UGT/ULE/UGE)
  - Updated resolve_syntax_type to recognize new type names
- **syntax-parsing**: Comprehensive test suite with 117 tests
  - **expression_tests.rs**: 34 tests covering literals, operators, function calls, precedence, and associativity
  - **statement_tests.rs**: 20 tests for variable declarations, assignments, returns, and control flow
  - **function_tests.rs**: 16 tests for function definitions, parameters, and bodies
  - **error_tests.rs**: 31 tests for invalid syntax and edge cases
  - **integration_tests.rs**: 16 tests for complete programs
  - Test coverage exceeds 80% of parser code paths

### Changed
- Implemented real optimization level flow for compilation (`-O0..-O3`)
  - `neurc` now validates and passes optimization level through to `llvm-backend`
  - `llvm-backend` maps optimization levels to LLVM target machine settings
  - Removed stale docs claiming optimization levels were deferred
- Extracted AST types from syntax-parsing to new ast-types infrastructure crate
  - Resolved cross-slice dependency violations (semantic-analysis and llvm-backend now depend on ast-types instead of syntax-parsing)
  - syntax-parsing maintains backward compatibility via re-exports
  - VSA 4.0 compliance: Feature slices now properly isolated, only depending on infrastructure
- **syntax-parsing**: Security and robustness improvements
  - Added maximum expression nesting depth limit (256 levels) to prevent stack overflow attacks
  - Added validation for duplicate parameter names in function definitions
  - Fixed hardcoded span bug in binary operator error handling (now uses proper token span)
  - New error variants: `MaxDepthExceeded` and `DuplicateParameter`
- **infrastructure**: Code quality and robustness enhancements
  - **source-location**: Fixed potential panic in `snippet()` method - now returns `Option<&str>`
  - **diagnostics**: Added `Display` implementations for `Severity`, `DiagnosticCode`, and `Diagnostic`
  - **All infrastructure crates**: Added comprehensive rustdoc documentation (100% public API coverage)
  - **All infrastructure crates**: Improved test coverage from 4 to 32 tests (800% increase)
  - Test coverage includes edge cases, error handling, UTF-8 boundaries, and display formatting
- **lexical-analysis**: Optimized identifier validation performance
  - Simplified `is_valid_identifier()` to use char-based iteration instead of grapheme clusters
  - Removed unnecessary unicode-segmentation dependency usage
  - Improved error handling in `tokenize()` to return early on first error (faster for invalid input)
  - Retained source access in `Lexer` for precise diagnostic classification and spans

### Fixed
- **lexical-analysis**: Correctly preserve specific lexical errors in `Lexer` iterator
  - `InvalidEscape` and `UnterminatedString` are no longer masked as generic `UnexpectedChar`
  - Unterminated string diagnostics now map to `LexError::UnterminatedString` consistently
  - Tightened lexer tests to assert precise error variants
- **syntax-parsing**: Parser error handling improvements
  - Fixed hardcoded `Span::new(0, 0)` in `token_to_binary_op` error path
  - Now properly passes token span for accurate error reporting
- **semantic-analysis**: Type checker now properly tracks mutability for variable declarations

### Documentation
- Synchronized test totals in top-level docs
  - Updated [README.md](README.md) badge and capability summary to **312 tests passing**
  - Updated [docs/README.md](docs/README.md) to **312 tests passing**
- Cleaned stale optimization guidance in [cli-usage.md](docs/guides/cli-usage.md) and [troubleshooting.md](docs/guides/troubleshooting.md)
  - Removed outdated wording that treated `-O1..-O3` as future work
  - Updated recommendations to reflect current optimization behavior
### Documented
- **syntax-parsing**: Added TODO documentation for unused `Type::Tensor` variant
  - Clearly marked as Phase 2 feature with example syntax
  - Prevents confusion about incomplete implementation

### Architecture
- Removed direct feature-slice dependency from `llvm-backend` to `semantic-analysis`
  - `neurc` remains the single orchestration boundary for parse → type-check → codegen
  - `llvm-backend` now uses backend-local type modeling for code generation decisions
  - Architecture tests no longer allow this temporary exception
- Added living documentation (README.md) to all compiler slices
  - lexical-analysis, syntax-parsing, semantic-analysis, control-flow, llvm-backend, neurc
  - Each README documents: Business Intent, Public Interface, Data Ownership, Implementation Details, Dependencies
  - Follows VSA 4.0 Living Documentation requirements
- Implemented architecture tests for automated VSA boundary verification
  - Test for cross-slice dependencies (feature slices must only depend on infrastructure)
  - Test for infrastructure isolation (infrastructure must not depend on features)
  - Test for README.md presence and required sections
  - Test for VSA 4.0 compliance (ast-types in infrastructure, pub(crate) usage documented)
  - Tests run in neurc crate: `cargo test -p neurc --test architecture_tests`

### Infrastructure
- CI now has a dedicated `Architecture` gate in [ci.yml](.github/workflows/ci.yml)
  - Runs `cargo test -p neurc --test architecture_tests --verbose` on every push/PR
  - The test matrix now depends on this gate so VSA boundary regressions fail fast
- Added docs consistency gate in [ci.yml](.github/workflows/ci.yml)
  - Runs `python tools/check_docs_consistency.py` on every push/PR
  - Fails CI when test-count values drift between top-level docs
- Added benchmark regression gate in [ci.yml](.github/workflows/ci.yml)
  - Runs Criterion benchmarks for `llvm-backend` code generation
  - Enforces threshold-based performance budgets via `tools/check_benchmark_regression.py`
- Added cross-platform release smoke gate in [ci.yml](.github/workflows/ci.yml)
  - Builds `neurc` in release mode on Linux, macOS, and Windows
  - Compiles and executes representative examples via `tools/run_release_smoke_tests.py`
- Repository hygiene cleanup
  - Removed generated VSIX artifact from `neuro-language-support`
  - Updated [.gitignore](.gitignore) to keep required repository metadata trackable while ignoring packaged VSIX outputs

---

## [0.2.0] - 2025-11-22

### Phase 1 Core MVP Complete (~85%)

End-to-end compilation pipeline from source code to native executables is working!

### Added

- **neurc**: Complete compilation to native executables
  - `neurc compile <file.nr>` produces executables on Windows, Linux, macOS
  - Multi-stage linker fallback (clang → lld-link → MSVC/cc)
  - Platform-specific object file handling
  - 16 end-to-end integration tests

- **semantic-analysis**: Full type checking implementation
  - Type checking for primitives: i32, i64, f32, f64, bool
  - Function signature validation
  - Lexical scoping with variable shadowing
  - Multiple error collection (fail-slow)
  - 24 comprehensive tests

- **llvm-backend**: Complete LLVM code generation
  - Function codegen with parameters and return values
  - Expression codegen (arithmetic, comparison, logical ops)
  - Statement codegen (variables, return, if/else branching)
  - Object code emission for native target
  - 4 comprehensive tests

- **syntax-parsing**: Statement and function parsing
  - Variable declarations, return statements, expression statements
  - Function definitions with parameters and return types
  - If/else statements with multiple else-if clauses
  - 39 additional tests (65 total)

### Fixed
- **neurc**: Object file linking race condition with tempfile cleanup
- **llvm-backend**: Type inference for identifiers and function calls

### Changed
- **neurc**: Improved linker detection with detailed error messages

### Statistics
- 142 tests passing across all components
- Zero clippy warnings
- Compilation time <1s for small programs
- LLVM 18.1.8

---

## [0.1.0] - 2025-01-21

### Initial Release - Lexer and Expression Parser

### Added

- **lexical-analysis**: Complete tokenizer
  - Phase 1 keywords (func, val, mut, if, else, return, true, false)
  - Multiple number bases (binary, octal, hex, decimal, float)
  - String literals with escape sequences
  - Line and block comments
  - Source span tracking
  - 28 tests

- **syntax-parsing**: Expression parser with Pratt precedence climbing
  - Literals, identifiers, function calls
  - Binary operators with correct precedence
  - Unary operators (-, !)
  - Parenthesized expressions
  - 26 tests

- **infrastructure**: LLVM 18.1.8 support
  - inkwell 0.6.0 for LLVM 18.x compatibility
  - Windows build configuration with vcpkg
  - libxml2 dependency handling
  - Vertical Slice Architecture (VSA) setup

### Fixed
- String error reporting (unterminated strings, invalid escapes)
- Redundant closure warnings in Unicode validation
