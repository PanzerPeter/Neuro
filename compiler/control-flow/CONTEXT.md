# control-flow

## Purpose
Hold the Control Flow Graph data structures. The slice reserves the boundary a future CFG-consuming pass will need; it does not participate in the compiler pipeline today.

## Entry Point
- Type: Library function
- Input: none
- Output: `Result<ControlFlowGraph, ControlFlowError>`

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
None. The slice owns its own `ControlFlowError` and touches no infrastructure crate.

## Notes
**This slice has no caller.** `neurc` does not depend on it and no other crate imports it, so
nothing it computes reaches a compiled program. `build_cfg()` takes no input and returns an
empty graph — a placeholder, not an analysis.

The two things the slice's name suggests it does are both implemented elsewhere and are not
waiting on it:
- **Return-path analysis** — `semantic-analysis` (`type_checkers/declarations/functions.rs`),
  which rejects a non-void function whose body can fall off the end.
- **Divergence / dead-arm reasoning** — `semantic-analysis`
  (`type_checkers/val_else.rs::stmts_diverge`) plus the per-arm basic-block chain
  `llvm-backend` emits.

What remains is `BasicBlock` / `ControlFlowGraph` / `ControlFlowError`: a graph representation
kept for the pass that will need one. Whoever wires that pass up owns replacing `build_cfg()`
with a real traversal and giving this file an accurate Purpose.
