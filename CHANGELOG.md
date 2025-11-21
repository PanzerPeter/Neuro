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
  - 26 comprehensive tests including error cases

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
  - 30+ comprehensive tests for all statement types
  - Example program: [examples/hello.nr](examples/hello.nr)

### Planned
- Control flow parsing (if/else statements)
- Type system basics
- LLVM code generation
- End-to-end compilation of simple arithmetic programs
