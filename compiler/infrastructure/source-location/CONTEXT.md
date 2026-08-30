# source-location

## Purpose
Map a `Span`'s byte offsets to human-readable line/column positions and extract source snippets for diagnostic display.

## Entry Point
- Type: Library (no entry function — pure utilities)
- Key types: `SourceFile`, `Position`

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- shared-types — `Span` is the input to every position-resolution operation

## Notes
`SourceFile` caches line-start byte offsets on construction, so `position_at(span)` is an
O(log n) binary search rather than a rescan. `snippet(span)` slices the source for inline
error display. Pure infrastructure: no compiler business logic.
