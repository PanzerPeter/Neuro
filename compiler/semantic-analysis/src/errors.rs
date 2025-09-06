//! Semantic error types and reporting
//!
//! This module defines errors that can occur during semantic analysis

use shared_types::Span;
use thiserror::Error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Error, PartialEq, Serialize, Deserialize)]
pub enum SemanticError {
    #[error("Undefined variable '{name}' at {span:?}")]
    UndefinedVariable { name: String, span: Span },
    
    #[error("Type mismatch: expected '{expected}', found '{found}' at {span:?}")]
    TypeMismatch { 
        expected: String, 
        found: String, 
        span: Span 
    },
    
    #[error("Function '{name}' already defined at {span:?}")]
    FunctionAlreadyDefined { name: String, span: Span },
    
    #[error("Variable '{name}' already defined in this scope at {span:?}")]
    VariableAlreadyDefined { name: String, span: Span },
    
    #[error("Function '{name}' not found at {span:?}")]
    FunctionNotFound { name: String, span: Span },
    
    #[error("Wrong number of arguments for function '{name}': expected {expected}, found {found} at {span:?}")]
    ArgumentCountMismatch { 
        name: String, 
        expected: usize, 
        found: usize, 
        span: Span 
    },
    
    #[error("Cannot assign to immutable variable '{name}' at {span:?}")]
    AssignToImmutable { name: String, span: Span },
    
    #[error("Return type mismatch in function '{function}': expected '{expected}', found '{found}' at {span:?}")]
    ReturnTypeMismatch {
        function: String,
        expected: String,
        found: String,
        span: Span,
    },
}