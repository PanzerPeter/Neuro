//! Semantic analysis for the NEURO compiler
//! 
//! This module provides type checking, scope resolution, and semantic validation
//! for NEURO programs following VSA principles.

use shared_types::*;
use std::collections::HashMap;
use thiserror::Error;
use serde::{Deserialize, Serialize};

pub mod analyzer;
pub mod scope;
pub mod type_checker;
pub mod errors;

pub use analyzer::SemanticAnalyzer;
pub use scope::{Scope, Symbol};
pub use type_checker::TypeChecker;
pub use errors::SemanticError;

/// Analyze a program and return semantic information
pub fn analyze_program(program: &Program) -> Result<SemanticInfo, SemanticError> {
    let mut analyzer = SemanticAnalyzer::new();
    analyzer.analyze(program)
}

/// Information gathered during semantic analysis
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticInfo {
    pub symbols: HashMap<String, Symbol>,
    pub type_info: HashMap<String, Type>,
    pub errors: Vec<SemanticError>,
}
