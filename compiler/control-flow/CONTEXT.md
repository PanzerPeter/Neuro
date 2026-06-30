# control-flow

## Purpose
Build a Control Flow Graph from a validated Neuro program to enable unreachable-code detection and return-path analysis.

## Entry Point
- Type: Library function (a later phase; stub only in Phase 1)
- Input: `&[Item]` (planned)
- Output: `Result<ControlFlowGraph, ControlFlowError>`

## Data Ownership
- Tables: none
- Events Published: none
- Events Consumed: none
- Public Read Model: none

## Shared Kernel
- shared-types — basic type definitions
- diagnostics — error reporting infrastructure (wired in a later phase)

## Notes
`build_cfg()` is a placeholder returning an empty graph. It exists to reserve the
slice boundary and allow neurc to compile in Phase 1 without conditional compilation
flags. The data structures (`BasicBlock`, `ControlFlowGraph`) are production-grade;
only the AST-traversal logic is absent.
