# source-location

## Purpose
Map byte offsets from `Span` values to human-readable line/column positions and extract source code snippets for diagnostic display.

## Entry Point
- Type: Library (no entry function — pure utilities)
- Key types: `SourceFile`, `Position`

## Data Ownership
- Tables: none
- Events Published: none
- Events Consumed: none
- Public Read Model: none

## Shared Kernel
- shared-types — `Span` is the input type for all position-resolution operations

## Notes
`SourceFile` caches line-start byte offsets on construction for O(log n) span-to-line conversion. `position_at(span)` returns a `Position { line, column }`. `snippet(span)` returns the source text slice for inline error display. Pure infrastructure with no compiler business logic.
