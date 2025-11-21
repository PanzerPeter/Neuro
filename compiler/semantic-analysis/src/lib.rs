// NEURO Programming Language - Semantic Analysis
// Feature slice for type checking and semantic validation

use thiserror::Error;

/// Type representation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    I32,
    I64,
    F32,
    F64,
    Bool,
    String,
    Function {
        params: Vec<Type>,
        ret: Box<Type>,
    },
    Tensor {
        element: Box<Type>,
        shape: Vec<usize>,
    },
    Unknown,
}

/// Type checking errors
#[derive(Debug, Error, PartialEq)]
pub enum TypeError {
    #[error("type mismatch: expected {expected:?}, found {found:?}")]
    Mismatch { expected: Type, found: Type },

    #[error("undefined variable: {0}")]
    UndefinedVariable(String),

    #[error("undefined function: {0}")]
    UndefinedFunction(String),

    #[error("invalid tensor shape")]
    InvalidTensorShape,
}

/// Type environment for tracking variable types
#[derive(Debug, Default)]
pub struct TypeEnvironment {
    bindings: std::collections::HashMap<String, Type>,
}

impl TypeEnvironment {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: String, ty: Type) {
        self.bindings.insert(name, ty);
    }

    pub fn get(&self, name: &str) -> Option<&Type> {
        self.bindings.get(name)
    }
}

/// Type check the program
pub fn type_check() -> Result<(), TypeError> {
    // Phase 1: Simple stub implementation
    // TODO: Implement full type checking
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_environment_basic() {
        let mut env = TypeEnvironment::new();
        env.insert("x".to_string(), Type::I32);
        assert_eq!(env.get("x"), Some(&Type::I32));
    }
}
