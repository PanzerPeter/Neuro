// NEURO Programming Language - Semantic Analysis
// Type checking error definitions

use shared_types::Span;
use thiserror::Error;

use crate::types::Type;

/// Type checking errors with source location information
#[derive(Debug, Error, Clone, PartialEq)]
pub enum TypeError {
    #[error("type mismatch at {span:?}: expected {expected:?}, found {found:?}")]
    Mismatch {
        expected: Type,
        found: Type,
        span: Span,
    },

    #[error("undefined variable '{name}' at {span:?}")]
    UndefinedVariable { name: String, span: Span },

    #[error("undefined function '{name}' at {span:?}")]
    UndefinedFunction { name: String, span: Span },

    #[error("variable '{name}' already defined in this scope at {span:?}")]
    VariableAlreadyDefined { name: String, span: Span },

    #[error("function '{name}' already defined at {span:?}")]
    FunctionAlreadyDefined { name: String, span: Span },

    #[error("incorrect number of arguments at {span:?}: expected {expected}, found {found}")]
    ArgumentCountMismatch {
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("cannot apply operator {op} to type {ty:?} at {span:?}")]
    InvalidOperator { op: String, ty: Type, span: Span },

    #[error("cannot apply binary operator {op} to types {left:?} and {right:?} at {span:?}")]
    InvalidBinaryOperator {
        op: String,
        left: Type,
        right: Type,
        span: Span,
    },

    #[error("return type mismatch at {span:?}: expected {expected:?}, found {found:?}")]
    ReturnTypeMismatch {
        expected: Type,
        found: Type,
        span: Span,
    },

    #[error("missing return statement in function returning {expected:?} at {span:?}")]
    MissingReturn { expected: Type, span: Span },

    #[error("unknown type name '{name}' at {span:?}")]
    UnknownTypeName { name: String, span: Span },

    #[error("cannot call non-function type {ty:?} at {span:?}")]
    NotCallable { ty: Type, span: Span },

    #[error("variable '{name}' used without initialization at {span:?}")]
    UninitializedVariable { name: String, span: Span },

    #[error("cannot assign to immutable variable '{name}' at {span:?}")]
    AssignToImmutable { name: String, span: Span },
}
