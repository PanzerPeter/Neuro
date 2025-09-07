//! Tensor operations and type system for NEURO
//! 
//! Provides compile-time tensor type checking, shape inference,
//! and tensor operation definitions.

pub mod tensor_types;
pub mod operations;
pub mod shape_inference;
pub mod broadcasting;
pub mod layout_optimization;
pub mod validation;

use thiserror::Error;

/// Tensor operation errors
#[derive(Error, Debug, Clone)]
pub enum TensorError {
    #[error("Shape mismatch: expected {expected:?}, got {actual:?}")]
    ShapeMismatch {
        expected: Vec<usize>,
        actual: Vec<usize>,
    },
    #[error("Invalid tensor rank: expected {expected}, got {actual}")]
    InvalidRank { expected: usize, actual: usize },
    #[error("Broadcasting error: cannot broadcast shapes {left:?} and {right:?}")]
    BroadcastError { left: Vec<usize>, right: Vec<usize> },
    #[error("Invalid dimension: {dim} for shape {shape:?}")]
    InvalidDimension { dim: usize, shape: Vec<usize> },
    #[error("Type mismatch: expected {expected}, got {actual}")]
    TypeMismatch { expected: String, actual: String },
    #[error("Unsupported operation: {op} on tensor with shape {shape:?}")]
    UnsupportedOperation { op: String, shape: Vec<usize> },
}

/// Result type for tensor operations
pub type TensorResult<T> = Result<T, TensorError>;

/// Tensor data types supported by NEURO
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TensorDataType {
    Float32,
    Float64,
    Int32,
    Int64,
    Bool,
    String,
}

impl TensorDataType {
    /// Get the size in bytes of this data type
    pub fn size_bytes(&self) -> usize {
        match self {
            TensorDataType::Float32 => 4,
            TensorDataType::Float64 => 8,
            TensorDataType::Int32 => 4,
            TensorDataType::Int64 => 8,
            TensorDataType::Bool => 1,
            TensorDataType::String => std::mem::size_of::<String>(), // Variable size, this is just the ptr size
        }
    }

    /// Check if this type supports arithmetic operations
    pub fn supports_arithmetic(&self) -> bool {
        matches!(self, TensorDataType::Float32 | TensorDataType::Float64 | TensorDataType::Int32 | TensorDataType::Int64)
    }

    /// Check if two types are compatible for operations
    pub fn is_compatible_with(&self, other: &TensorDataType) -> bool {
        match (self, other) {
            // Exact matches
            (a, b) if a == b => true,
            // Numeric promotions
            (TensorDataType::Int32, TensorDataType::Float32) => true,
            (TensorDataType::Float32, TensorDataType::Int32) => true,
            (TensorDataType::Int64, TensorDataType::Float64) => true,
            (TensorDataType::Float64, TensorDataType::Int64) => true,
            // Size promotions
            (TensorDataType::Int32, TensorDataType::Int64) => true,
            (TensorDataType::Float32, TensorDataType::Float64) => true,
            _ => false,
        }
    }

    /// Get the promoted type when combining two types
    pub fn promote_with(&self, other: &TensorDataType) -> TensorResult<TensorDataType> {
        if self == other {
            return Ok(self.clone());
        }

        match (self, other) {
            // Float takes precedence over int
            (TensorDataType::Float32, TensorDataType::Int32) => Ok(TensorDataType::Float32),
            (TensorDataType::Int32, TensorDataType::Float32) => Ok(TensorDataType::Float32),
            (TensorDataType::Float64, TensorDataType::Int64) => Ok(TensorDataType::Float64),
            (TensorDataType::Int64, TensorDataType::Float64) => Ok(TensorDataType::Float64),
            
            // Larger size takes precedence
            (TensorDataType::Int32, TensorDataType::Int64) => Ok(TensorDataType::Int64),
            (TensorDataType::Int64, TensorDataType::Int32) => Ok(TensorDataType::Int64),
            (TensorDataType::Float32, TensorDataType::Float64) => Ok(TensorDataType::Float64),
            (TensorDataType::Float64, TensorDataType::Float32) => Ok(TensorDataType::Float64),
            
            // Mixed float/int promotions
            (TensorDataType::Int32, TensorDataType::Float64) => Ok(TensorDataType::Float64),
            (TensorDataType::Float64, TensorDataType::Int32) => Ok(TensorDataType::Float64),
            (TensorDataType::Int64, TensorDataType::Float32) => Ok(TensorDataType::Float64), // Promote to highest precision
            (TensorDataType::Float32, TensorDataType::Int64) => Ok(TensorDataType::Float64),
            
            _ => Err(TensorError::TypeMismatch {
                expected: format!("{:?}", self),
                actual: format!("{:?}", other),
            }),
        }
    }
}

impl std::fmt::Display for TensorDataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TensorDataType::Float32 => write!(f, "f32"),
            TensorDataType::Float64 => write!(f, "f64"),
            TensorDataType::Int32 => write!(f, "i32"),
            TensorDataType::Int64 => write!(f, "i64"),
            TensorDataType::Bool => write!(f, "bool"),
            TensorDataType::String => write!(f, "string"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_type_sizes() {
        assert_eq!(TensorDataType::Float32.size_bytes(), 4);
        assert_eq!(TensorDataType::Float64.size_bytes(), 8);
        assert_eq!(TensorDataType::Int32.size_bytes(), 4);
        assert_eq!(TensorDataType::Int64.size_bytes(), 8);
        assert_eq!(TensorDataType::Bool.size_bytes(), 1);
    }

    #[test]
    fn test_arithmetic_support() {
        assert!(TensorDataType::Float32.supports_arithmetic());
        assert!(TensorDataType::Int32.supports_arithmetic());
        assert!(!TensorDataType::Bool.supports_arithmetic());
        assert!(!TensorDataType::String.supports_arithmetic());
    }

    #[test]
    fn test_type_compatibility() {
        assert!(TensorDataType::Float32.is_compatible_with(&TensorDataType::Float32));
        assert!(TensorDataType::Float32.is_compatible_with(&TensorDataType::Int32));
        assert!(!TensorDataType::Float32.is_compatible_with(&TensorDataType::Bool));
    }

    #[test]
    fn test_type_promotion() {
        let result = TensorDataType::Int32.promote_with(&TensorDataType::Float32).unwrap();
        assert_eq!(result, TensorDataType::Float32);
        
        let result = TensorDataType::Int32.promote_with(&TensorDataType::Int64).unwrap();
        assert_eq!(result, TensorDataType::Int64);
        
        let result = TensorDataType::Bool.promote_with(&TensorDataType::Int32);
        assert!(result.is_err());
    }
}