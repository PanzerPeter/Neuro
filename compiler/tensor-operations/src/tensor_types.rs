//! Tensor type definitions for NEURO

use crate::{TensorDataType, TensorError, TensorResult};
use smallvec::{SmallVec, smallvec};

/// A tensor shape, optimized for small dimensions
pub type TensorShape = SmallVec<[usize; 4]>; // Most tensors have d 4 dimensions

/// Tensor type with compile-time shape information
#[derive(Debug, Clone, PartialEq)]
pub struct TensorType {
    /// Data type of tensor elements
    pub data_type: TensorDataType,
    /// Shape of the tensor (dimensions)
    pub shape: TensorShape,
    /// Optional name for debugging
    pub name: Option<String>,
}

impl TensorType {
    /// Create a new tensor type
    pub fn new(data_type: TensorDataType, shape: &[usize]) -> Self {
        Self {
            data_type,
            shape: SmallVec::from_slice(shape),
            name: None,
        }
    }

    /// Create a new tensor type with a name
    pub fn named(data_type: TensorDataType, shape: &[usize], name: String) -> Self {
        Self {
            data_type,
            shape: SmallVec::from_slice(shape),
            name: Some(name),
        }
    }

    /// Create a scalar (0-dimensional tensor)
    pub fn scalar(data_type: TensorDataType) -> Self {
        Self {
            data_type,
            shape: smallvec![],
            name: None,
        }
    }

    /// Create a vector (1-dimensional tensor)
    pub fn vector(data_type: TensorDataType, size: usize) -> Self {
        Self {
            data_type,
            shape: smallvec![size],
            name: None,
        }
    }

    /// Create a matrix (2-dimensional tensor)
    pub fn matrix(data_type: TensorDataType, rows: usize, cols: usize) -> Self {
        Self {
            data_type,
            shape: smallvec![rows, cols],
            name: None,
        }
    }

    /// Get the rank (number of dimensions)
    pub fn rank(&self) -> usize {
        self.shape.len()
    }

    /// Get the total number of elements
    pub fn size(&self) -> usize {
        if self.shape.is_empty() {
            1 // Scalar has 1 element
        } else {
            self.shape.iter().product()
        }
    }

    /// Get the size in bytes
    pub fn size_bytes(&self) -> usize {
        self.size() * self.data_type.size_bytes()
    }

    /// Check if this is a scalar
    pub fn is_scalar(&self) -> bool {
        self.shape.is_empty()
    }

    /// Check if this is a vector
    pub fn is_vector(&self) -> bool {
        self.shape.len() == 1
    }

    /// Check if this is a matrix
    pub fn is_matrix(&self) -> bool {
        self.shape.len() == 2
    }

    /// Check if two tensor types are compatible for operations
    pub fn is_compatible_with(&self, other: &TensorType) -> bool {
        self.data_type.is_compatible_with(&other.data_type) &&
        self.shape == other.shape
    }

    /// Check if two tensor types can be broadcast together
    pub fn can_broadcast_with(&self, other: &TensorType) -> bool {
        use crate::broadcasting::can_broadcast_shapes;
        self.data_type.is_compatible_with(&other.data_type) &&
        can_broadcast_shapes(&self.shape, &other.shape)
    }

    /// Get the result type of broadcasting with another tensor
    pub fn broadcast_with(&self, other: &TensorType) -> TensorResult<TensorType> {
        use crate::broadcasting::broadcast_shapes;
        
        let result_dtype = self.data_type.promote_with(&other.data_type)?;
        let result_shape = broadcast_shapes(&self.shape, &other.shape)?;
        
        Ok(TensorType::new(result_dtype, &result_shape))
    }

    /// Reshape the tensor (must preserve total size)
    pub fn reshape(&self, new_shape: &[usize]) -> TensorResult<TensorType> {
        let old_size = self.size();
        let new_size: usize = new_shape.iter().product();
        
        if old_size != new_size {
            return Err(TensorError::ShapeMismatch {
                expected: vec![old_size],
                actual: vec![new_size],
            });
        }

        Ok(TensorType::new(self.data_type.clone(), new_shape))
    }

    /// Transpose a 2D tensor
    pub fn transpose(&self) -> TensorResult<TensorType> {
        if self.rank() != 2 {
            return Err(TensorError::InvalidRank {
                expected: 2,
                actual: self.rank(),
            });
        }

        let new_shape = [self.shape[1], self.shape[0]];
        Ok(TensorType::new(self.data_type.clone(), &new_shape))
    }

    /// Squeeze dimensions of size 1
    pub fn squeeze(&self) -> TensorType {
        let new_shape: Vec<usize> = self.shape.iter()
            .copied()
            .filter(|&dim| dim != 1)
            .collect();
        
        TensorType::new(self.data_type.clone(), &new_shape)
    }

    /// Add a dimension of size 1 at the specified axis
    pub fn unsqueeze(&self, axis: usize) -> TensorResult<TensorType> {
        if axis > self.rank() {
            return Err(TensorError::InvalidDimension {
                dim: axis,
                shape: self.shape.to_vec(),
            });
        }

        let mut new_shape = self.shape.clone();
        new_shape.insert(axis, 1);
        
        Ok(TensorType::new(self.data_type.clone(), &new_shape))
    }

    /// Validate that the tensor type is well-formed
    pub fn validate(&self) -> TensorResult<()> {
        // Check that all dimensions are positive
        for (i, &dim) in self.shape.iter().enumerate() {
            if dim == 0 {
                return Err(TensorError::InvalidDimension {
                    dim: i,
                    shape: self.shape.to_vec(),
                });
            }
        }

        // Check for reasonable limits
        if self.rank() > 8 {
            return Err(TensorError::InvalidRank {
                expected: 8,
                actual: self.rank(),
            });
        }

        if self.size() > usize::MAX / self.data_type.size_bytes() {
            return Err(TensorError::UnsupportedOperation {
                op: "validate".to_string(),
                shape: self.shape.to_vec(),
            });
        }

        Ok(())
    }
}

impl std::fmt::Display for TensorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Tensor<{}, [", self.data_type)?;
        for (i, dim) in self.shape.iter().enumerate() {
            if i > 0 { write!(f, ", ")?; }
            write!(f, "{}", dim)?;
        }
        write!(f, "]>")?;
        
        if let Some(ref name) = self.name {
            write!(f, " ({})", name)?;
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tensor_type_creation() {
        let tensor = TensorType::matrix(TensorDataType::Float32, 3, 4);
        assert_eq!(tensor.rank(), 2);
        assert_eq!(tensor.size(), 12);
        assert_eq!(tensor.shape, TensorShape::from_slice(&[3, 4]));
    }

    #[test]
    fn test_scalar_tensor() {
        let scalar = TensorType::scalar(TensorDataType::Int32);
        assert!(scalar.is_scalar());
        assert_eq!(scalar.rank(), 0);
        assert_eq!(scalar.size(), 1);
    }

    #[test]
    fn test_tensor_reshaping() {
        let tensor = TensorType::matrix(TensorDataType::Float32, 2, 6);
        let reshaped = tensor.reshape(&[3, 4]).unwrap();
        assert_eq!(reshaped.shape, TensorShape::from_slice(&[3, 4]));
        assert_eq!(reshaped.size(), 12);
        
        // Should fail if sizes don't match
        let result = tensor.reshape(&[2, 5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_tensor_transpose() {
        let matrix = TensorType::matrix(TensorDataType::Float32, 2, 3);
        let transposed = matrix.transpose().unwrap();
        assert_eq!(transposed.shape, TensorShape::from_slice(&[3, 2]));
        
        // Should fail on non-2D tensors
        let vector = TensorType::vector(TensorDataType::Float32, 5);
        assert!(vector.transpose().is_err());
    }

    #[test]
    fn test_squeeze_unsqueeze() {
        let tensor = TensorType::new(TensorDataType::Float32, &[1, 3, 1, 4]);
        let squeezed = tensor.squeeze();
        assert_eq!(squeezed.shape, TensorShape::from_slice(&[3, 4]));
        
        let unsqueezed = squeezed.unsqueeze(1).unwrap();
        assert_eq!(unsqueezed.shape, TensorShape::from_slice(&[3, 1, 4]));
    }

    #[test]
    fn test_tensor_validation() {
        let valid = TensorType::matrix(TensorDataType::Float32, 2, 3);
        assert!(valid.validate().is_ok());
        
        let invalid = TensorType::new(TensorDataType::Float32, &[2, 0, 3]);
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_tensor_compatibility() {
        let t1 = TensorType::matrix(TensorDataType::Float32, 2, 3);
        let t2 = TensorType::matrix(TensorDataType::Float32, 2, 3);
        let t3 = TensorType::matrix(TensorDataType::Int32, 2, 3);
        let t4 = TensorType::matrix(TensorDataType::Float32, 3, 2);
        
        assert!(t1.is_compatible_with(&t2));
        assert!(t1.is_compatible_with(&t3)); // Compatible types
        assert!(!t1.is_compatible_with(&t4)); // Different shapes
    }
}