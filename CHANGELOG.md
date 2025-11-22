# Changelog

All notable changes to the NEURO programming language compiler will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed
- **Roadmap**: Major expansion with comprehensive feature planning (v3.4)
  - Phase 1: Adjusted to ~85% complete (core MVP done, 3 extended features pending)
  - All phases expanded with detailed features from syntax specification
  - Added language features: methods, traits, tuples, error handling, pipeline operators, spread operators, and more

### Pending for Phase 1 Completion
- Extended primitive types (i8, i16, u8, u16, u32, u64)
- String type with UTF-8 support
- Expression-based returns (implicit return of last expression)

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
