# Changelog

All notable changes to the NEURO programming language compiler will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2025-01-21

### Added
- **lexical-analysis**: Comprehensive lexer with Unicode support
  - Token recognition for Phase 1 keywords (func, val, mut, if, else, return, true, false)
  - Multiple number bases support (binary, octal, hex, decimal, float)
  - String literals with escape sequences (\n, \r, \t, \", \, \0, \xNN, \u{NNNN})
  - Line and block comments
  - Proper span tracking for error reporting
  - 28 comprehensive tests

- **syntax-parsing**: Expression parser with Pratt precedence climbing
  - Parse literals (integers, floats, strings, booleans)
  - Parse identifiers
  - Parse binary operators (+, -, *, /, %, ==, !=, <, >, <=, >=, &&, ||)
  - Parse unary operators (-, !)
  - Parse function calls with arguments
  - Parse parenthesized expressions
  - Correct operator precedence and associativity
  - Expression parsing: 26 comprehensive tests including error cases

### Fixed
- lexical-analysis: Improved string error reporting (unterminated strings and invalid escapes)
- lexical-analysis: Fixed redundant closure warnings in Unicode identifier validation

### Infrastructure
- Vertical Slice Architecture (VSA) setup with independent slices
- Shared infrastructure components (shared-types, source-location, diagnostics, project-config)
- Comprehensive testing framework
- Clippy and rustfmt integration

## [Unreleased]

### Added
- **syntax-parsing**: Complete statement and function parsing
  - Parse variable declarations (`val x: i32 = 42`, `mut counter: i32 = 0`)
  - Parse return statements (`return expr`, `return`)
  - Parse expression statements
  - Parse type annotations (i32, f64, bool, string)
  - Parse function definitions with parameters and return types
  - Parse statement blocks
  - Parse complete NEURO programs
  - Parse if/else statements with multiple else-if clauses
  - Statement and function parsing: 39 additional tests
  - Total syntax-parsing tests: 65 comprehensive tests
  - Example program: [examples/hello.nr](examples/hello.nr)

- **semantic-analysis**: Full type checking implementation (Phase 1)
  - Type checking for primitive types: `i32`, `i64`, `f32`, `f64`, `bool`
  - Function signature validation (parameters and return types)
  - Expression type checking (binary ops, unary ops, literals, identifiers, function calls)
  - Statement type checking (variable declarations, return statements, if/else, expressions)
  - Lexical scoping with variable shadowing support
  - Comprehensive error reporting with source spans
  - Multiple error collection (fail-slow approach)
  - 24 comprehensive tests including milestone program validation
  - Public API: `type_check(items: &[Item]) -> Result<(), Vec<TypeError>>`

- **llvm-backend**: Complete Phase 1 LLVM code generation
  - LLVM IR generation via inkwell (LLVM 16+ support)
  - Function codegen with parameters and return values
  - Expression codegen: binary ops (arithmetic, comparison, logical), unary ops, function calls, literals, identifiers
  - Statement codegen: variable declarations, return statements, if/else branching, expression statements
  - Type mapping: NEURO types (i32, i64, f32, f64, bool) to LLVM types
  - Basic block management for control flow
  - Stack-based variable allocation (alloca)
  - Object code emission for native target
  - Support for opaque pointers (LLVM 15+)
  - Public API: `compile(items: &[Item]) -> Result<Vec<u8>, CodegenError>`
  - 4 comprehensive tests (simple arithmetic, milestone program, factorial, complex expressions)
  - Comprehensive error handling with detailed error messages
  - Zero warnings (clippy clean)

### Planned
- End-to-end compilation to executable in neurc compiler driver
- Integration tests with program execution validation
- Additional codegen optimizations
