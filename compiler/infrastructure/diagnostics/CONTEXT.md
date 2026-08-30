# diagnostics

## Purpose
Provide structured diagnostic infrastructure — severity levels, error codes, a builder API, and a fail-slow collector — shared by every compiler slice.

## Entry Point
- Type: Library (no entry function — pure data and utilities)
- Key types: `Diagnostic`, `DiagnosticCode`, `DiagnosticCollector`, `Severity`

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- shared-types — `Span`, embedded in every `Diagnostic` for source-location tagging

## Notes
Pure infrastructure with no compiler business logic. `DiagnosticCollector` is what makes the
fail-slow strategy possible: a slice accumulates every diagnostic in one pass and returns them
together, so the developer sees the complete error set per compilation rather than the first
one. Severities: `Error`, `Warning`, `Info`, `Hint`.
