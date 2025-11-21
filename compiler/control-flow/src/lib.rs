// NEURO Programming Language - Control Flow
// Feature slice for control flow analysis and compilation

use thiserror::Error;

/// Basic block in control flow graph
#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: usize,
    pub predecessors: Vec<usize>,
    pub successors: Vec<usize>,
}

impl BasicBlock {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            predecessors: Vec::new(),
            successors: Vec::new(),
        }
    }
}

/// Control Flow Graph
#[derive(Debug, Default)]
pub struct ControlFlowGraph {
    pub blocks: Vec<BasicBlock>,
}

impl ControlFlowGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_block(&mut self) -> usize {
        let id = self.blocks.len();
        self.blocks.push(BasicBlock::new(id));
        id
    }

    pub fn add_edge(&mut self, from: usize, to: usize) {
        if let Some(from_block) = self.blocks.get_mut(from) {
            from_block.successors.push(to);
        }
        if let Some(to_block) = self.blocks.get_mut(to) {
            to_block.predecessors.push(from);
        }
    }
}

/// Control flow errors
#[derive(Debug, Error, PartialEq)]
pub enum ControlFlowError {
    #[error("unreachable code detected")]
    UnreachableCode,

    #[error("missing return statement")]
    MissingReturn,
}

/// Build control flow graph
pub fn build_cfg() -> Result<ControlFlowGraph, ControlFlowError> {
    // Phase 1: Simple stub implementation
    // TODO: Implement CFG construction
    Ok(ControlFlowGraph::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cfg_add_block() {
        let mut cfg = ControlFlowGraph::new();
        let id = cfg.add_block();
        assert_eq!(id, 0);
        assert_eq!(cfg.blocks.len(), 1);
    }
}
