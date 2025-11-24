# Changelog

All notable changes to the NEURO programming language compiler will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Expression-Based Returns**: Functions can now use implicit returns (Phase 1 feature complete)
  - **syntax-parsing**: AST already supports expression statements (no changes needed)
  - **semantic-analysis**: Type validation for trailing expressions matching return types
  - **semantic-analysis**: ReturnTypeMismatch error for incompatible trailing expression types
  - **llvm-backend**: Code generation for implicit returns from trailing expressions
  - **examples**: Added expression_returns.nr demonstrating the feature
  - **tests**: 11 new semantic analysis tests + 7 new end-to-end integration tests (18 total)
  - Syntax: `func add(a: i32, b: i32) -> i32 { a + b }` (no explicit return needed)
  - Type safety: Trailing expression type must match function return type
  - Backward compatible: Explicit `return` statements still work
  - Ergonomic: Eliminates verbose `return` keywords in most functions

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

- **examples**:
  - Added assignment_test.nr demonstrating mutable variable reassignment
  - Added extended_types.nr demonstrating all new integer types

### Changed
- **Roadmap**: Updated Phase 1 progress to ~88% complete (2 of 4 extended features done)
- **semantic-analysis**: Type enum now includes 8 integer types (was 2)
- **semantic-analysis**: Updated is_numeric() to include all integer types
- **semantic-analysis**: Symbol table now stores both type and mutability information

### Fixed
- **semantic-analysis**: Type checker now properly tracks mutability for variable declarations

### Changed
- **Roadmap**: Updated Phase 1 progress to ~92% complete (3 of 4 extended features done)

### Pending for Phase 1 Completion
- Type inference for numeric literals (integers default to i32, floats to f64)
- String type with UTF-8 support

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
