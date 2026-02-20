# Slice: neurc (Compiler Driver)

## Business Intent
Orchestrate compiler pipeline and provide CLI interface for NEURO compilation.

## Public Interface
- **Trigger:** Command-line invocation: `neurc compile <file>` or `neurc check <file>`
- **Input:**
  - CLI arguments (subcommand, file paths, flags)
  - NEURO source files (.nr)
- **Output:**
  - Compiled executables (.exe on Windows)
  - Compilation errors/diagnostics to stderr
  - Exit codes (0 = success, non-zero = error)
- **Reads:**
  - Source files from filesystem
  - Configuration from neurc.toml (if present)
- **Writes:**
  - Executable binaries
  - Diagnostic output

## Data Ownership
- **Owns:** CLI interface, pipeline orchestration, user-facing error formatting
- **Subscribes to:** None (orchestrates all other slices)

## Implementation Details
CLI commands:
- `neurc compile <file>`: Full compilation pipeline (lex → parse → type check → codegen)
- `neurc check <file>`: Syntax and type checking only (no code generation)
- `neurc --help`: Show CLI help
- `neurc --version`: Show version info

**Pipeline Execution:**
1. Parse CLI arguments (clap crate)
2. Read source file
3. Lexical analysis (lexical-analysis slice)
4. Syntax parsing (syntax-parsing slice)
5. Type checking (semantic-analysis slice)
6. Code generation (llvm-backend slice)
7. Format and display errors (diagnostics infrastructure)

**Error Handling:**
- Collect all errors from pipeline stages
- Pretty-print with source snippets using miette
- Show multiple errors when possible
- Provide actionable error messages

**Phase 1 Scope:**
- Single-file compilation only
- No project/workspace support
- No incremental compilation
- Basic optimization level

## Dependencies
- **lexical-analysis**: Tokenization
- **syntax-parsing**: AST generation
- **semantic-analysis**: Type checking
- **llvm-backend**: Code generation
- **diagnostics**: Error reporting
- **project-config**: Configuration parsing (future)
- **clap**: CLI argument parsing
- **miette**: Pretty error output
