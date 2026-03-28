# diagnostics

## Purpose
Provide structured diagnostic message infrastructure — severity levels, error codes, builder API, and a fail-slow collector — for use across all compiler slices.

## Entry Point
- Type: Library (no entry function — pure data and utilities)
- Key types: `Diagnostic`, `DiagnosticCode`, `DiagnosticCollector`, `Severity`

## Data Ownership
- Tables: none
- Events Published: none
- Events Consumed: none
- Public Read Model: none

## Shared Kernel
- shared-types — `Span` embedded in every `Diagnostic` for source-location tagging

## Notes
Pure infrastructure with no compiler business logic. The `DiagnosticCollector` enables fail-slow error strategies: slices accumulate all diagnostics in a single pass and return them together rather than aborting on the first error. Severity levels: `Error`, `Warning`, `Info`, `Hint`.
