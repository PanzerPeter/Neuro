//! Pattern matching compiler for NEURO language
//! 
//! This module compiles high-level pattern matching expressions into
//! efficient decision trees and generates corresponding LLVM IR.

pub mod decision_tree;
pub mod exhaustiveness;
pub mod pattern_compiler;
pub mod reachability;

use shared_types::Span;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum PatternError {
    #[error("Pattern is not exhaustive at {span:?}")]
    NonExhaustive { span: Span },
    
    #[error("Unreachable pattern at {span:?}")]
    Unreachable { span: Span },
    
    #[error("Pattern variables bind different types: {variable} binds {types:?}")]
    InconsistentBinding {
        variable: String,
        types: Vec<String>,
    },
    
    #[error("Or-pattern branches bind different variables at {span:?}")]
    InconsistentOrPattern { span: Span },
    
    #[error("Guard expression in pattern has wrong type at {span:?}")]
    InvalidGuard { span: Span },
    
    #[error("Invalid tensor shape pattern: {message} at {span:?}")]
    InvalidTensorPattern { message: String, span: Span },
    
    #[error("Unsupported pattern type: {pattern_type} at {span:?}")]
    UnsupportedPattern { pattern_type: String, span: Span },
}

pub type PatternResult<T> = Result<T, PatternError>;

/// Pattern compilation context
#[derive(Debug, Clone)]
pub struct PatternContext {
    /// Variables bound in current scope
    pub bound_vars: indexmap::IndexMap<String, PatternVarInfo>,
    /// Current nesting level for optimization
    pub nesting_level: usize,
}

#[derive(Debug, Clone)]
pub struct PatternVarInfo {
    pub name: String,
    pub type_hint: Option<String>,
    pub span: Span,
}

impl PatternContext {
    pub fn new() -> Self {
        Self {
            bound_vars: indexmap::IndexMap::new(),
            nesting_level: 0,
        }
    }
    
    pub fn with_nesting(mut self, level: usize) -> Self {
        self.nesting_level = level;
        self
    }
    
    pub fn bind_var(&mut self, name: String, info: PatternVarInfo) -> PatternResult<()> {
        if let Some(existing) = self.bound_vars.get(&name) {
            // Check for type consistency in or-patterns
            if existing.type_hint != info.type_hint {
                return Err(PatternError::InconsistentBinding {
                    variable: name,
                    types: vec![
                        existing.type_hint.clone().unwrap_or_else(|| "unknown".to_string()),
                        info.type_hint.clone().unwrap_or_else(|| "unknown".to_string()),
                    ],
                });
            }
        }
        self.bound_vars.insert(name, info);
        Ok(())
    }
}

impl Default for PatternContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for pattern compilation optimization
#[derive(Debug, Clone)]
pub struct CompilerConfig {
    /// Enable decision tree optimization
    pub optimize_decision_trees: bool,
    /// Enable exhaustiveness checking
    pub check_exhaustiveness: bool,
    /// Enable reachability analysis
    pub check_reachability: bool,
    /// Maximum nesting depth before compilation warning
    pub max_nesting_depth: usize,
}

impl Default for CompilerConfig {
    fn default() -> Self {
        Self {
            optimize_decision_trees: true,
            check_exhaustiveness: true,
            check_reachability: true,
            max_nesting_depth: 10,
        }
    }
}