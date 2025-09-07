//! Tensor operation definitions

use crate::{TensorResult, TensorError};
use crate::tensor_types::TensorType;

/// Basic tensor operations
#[derive(Debug, Clone, PartialEq)]
pub enum TensorOp {
    // Arithmetic operations
    Add,
    Subtract,
    Multiply,
    Divide,
    
    // Matrix operations
    MatMul,
    Transpose,
    
    // Reduction operations
    Sum,
    Mean,
    Max,
    Min,
    
    // Shape operations
    Reshape,
    Squeeze,
    Unsqueeze,
}

impl TensorOp {
    /// Check if this operation is valid for the given tensor types
    pub fn validate(&self, inputs: &[&TensorType]) -> TensorResult<()> {
        match self {
            TensorOp::Add | TensorOp::Subtract | TensorOp::Multiply | TensorOp::Divide => {
                if inputs.len() != 2 {
                    return Err(TensorError::UnsupportedOperation {
                        op: format!("{:?}", self),
                        shape: vec![inputs.len()],
                    });
                }
                
                if !inputs[0].can_broadcast_with(inputs[1]) {
                    return Err(TensorError::BroadcastError {
                        left: inputs[0].shape.to_vec(),
                        right: inputs[1].shape.to_vec(),
                    });
                }
            },
            
            TensorOp::MatMul => {
                if inputs.len() != 2 {
                    return Err(TensorError::UnsupportedOperation {
                        op: "MatMul".to_string(),
                        shape: vec![inputs.len()],
                    });
                }
                
                let left = inputs[0];
                let right = inputs[1];
                
                // Both must be at least 2D
                if left.rank() < 2 || right.rank() < 2 {
                    return Err(TensorError::InvalidRank {
                        expected: 2,
                        actual: left.rank().min(right.rank()),
                    });
                }
                
                // Last dim of left must match second-to-last dim of right
                let left_cols = left.shape[left.rank() - 1];
                let right_rows = right.shape[right.rank() - 2];
                
                if left_cols != right_rows {
                    return Err(TensorError::ShapeMismatch {
                        expected: vec![left_cols],
                        actual: vec![right_rows],
                    });
                }
            },
            
            TensorOp::Transpose => {
                if inputs.len() != 1 {
                    return Err(TensorError::UnsupportedOperation {
                        op: "Transpose".to_string(),
                        shape: vec![inputs.len()],
                    });
                }
                
                if inputs[0].rank() != 2 {
                    return Err(TensorError::InvalidRank {
                        expected: 2,
                        actual: inputs[0].rank(),
                    });
                }
            },
            
            _ => {} // Other operations are more permissive
        }
        
        Ok(())
    }
    
    /// Compute the output type for this operation
    pub fn output_type(&self, inputs: &[&TensorType]) -> TensorResult<TensorType> {
        self.validate(inputs)?;
        
        match self {
            TensorOp::Add | TensorOp::Subtract | TensorOp::Multiply | TensorOp::Divide => {
                inputs[0].broadcast_with(inputs[1])
            },
            
            TensorOp::MatMul => {
                let left = inputs[0];
                let right = inputs[1];
                let data_type = left.data_type.promote_with(&right.data_type)?;
                
                // Result shape: [...left_batch, left_rows, right_cols]
                let mut result_shape = Vec::new();
                
                // Add batch dimensions from left
                for &dim in &left.shape[..left.rank() - 2] {
                    result_shape.push(dim);
                }
                
                // Add matrix dimensions
                result_shape.push(left.shape[left.rank() - 2]); // rows
                result_shape.push(right.shape[right.rank() - 1]); // cols
                
                Ok(TensorType::new(data_type, &result_shape))
            },
            
            TensorOp::Transpose => {
                inputs[0].transpose()
            },
            
            TensorOp::Sum | TensorOp::Mean => {
                Ok(TensorType::scalar(inputs[0].data_type.clone()))
            },
            
            TensorOp::Max | TensorOp::Min => {
                Ok(TensorType::scalar(inputs[0].data_type.clone()))
            },
            
            _ => Err(TensorError::UnsupportedOperation {
                op: format!("{:?}", self),
                shape: vec![],
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TensorDataType;

    #[test]
    fn test_add_operation() {
        let t1 = TensorType::matrix(TensorDataType::Float32, 2, 3);
        let t2 = TensorType::matrix(TensorDataType::Float32, 2, 3);
        
        let op = TensorOp::Add;
        assert!(op.validate(&[&t1, &t2]).is_ok());
        
        let result = op.output_type(&[&t1, &t2]).unwrap();
        assert_eq!(result.shape, t1.shape);
    }
    
    #[test]
    fn test_matmul_operation() {
        let t1 = TensorType::matrix(TensorDataType::Float32, 2, 3);
        let t2 = TensorType::matrix(TensorDataType::Float32, 3, 4);
        
        let op = TensorOp::MatMul;
        assert!(op.validate(&[&t1, &t2]).is_ok());
        
        let result = op.output_type(&[&t1, &t2]).unwrap();
        assert_eq!(result.shape.as_slice(), &[2, 4]);
    }
}