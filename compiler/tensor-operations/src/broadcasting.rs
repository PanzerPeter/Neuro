//! Broadcasting rules for tensor operations

use crate::{TensorError, TensorResult};
use crate::tensor_types::TensorShape;
use smallvec::SmallVec;

/// Check if two shapes can be broadcast together
pub fn can_broadcast_shapes(left: &TensorShape, right: &TensorShape) -> bool {
    // Broadcasting rules:
    // 1. Start from the trailing dimensions
    // 2. Dimensions are compatible if they are equal or one of them is 1
    // 3. Missing dimensions are assumed to be 1

    let left_dims = left.iter().rev();
    let right_dims = right.iter().rev();

    for (l_dim, r_dim) in left_dims.zip(right_dims) {
        if *l_dim != *r_dim && *l_dim != 1 && *r_dim != 1 {
            return false;
        }
    }

    true
}

/// Compute the broadcast shape of two shapes
pub fn broadcast_shapes(left: &TensorShape, right: &TensorShape) -> TensorResult<TensorShape> {
    if !can_broadcast_shapes(left, right) {
        return Err(TensorError::BroadcastError {
            left: left.to_vec(),
            right: right.to_vec(),
        });
    }

    let max_dims = left.len().max(right.len());
    let mut result = SmallVec::with_capacity(max_dims);

    // Process from the rightmost (trailing) dimensions
    for i in 0..max_dims {
        let pos_from_end = i;
        let left_dim = if pos_from_end < left.len() { 
            left[left.len() - 1 - pos_from_end] 
        } else { 1 };
        let right_dim = if pos_from_end < right.len() { 
            right[right.len() - 1 - pos_from_end] 
        } else { 1 };

        let result_dim = if left_dim == 1 {
            right_dim
        } else if right_dim == 1 {
            left_dim
        } else if left_dim == right_dim {
            left_dim
        } else {
            return Err(TensorError::BroadcastError {
                left: left.to_vec(),
                right: right.to_vec(),
            });
        };

        result.push(result_dim);
    }
    
    // Reverse to get correct order (we built from right to left)
    result.reverse();

    Ok(result)
}

/// Check if a shape can be broadcast to a target shape
pub fn can_broadcast_to(source: &TensorShape, target: &TensorShape) -> bool {
    // Source must have equal or fewer dimensions
    if source.len() > target.len() {
        return false;
    }

    // Check compatibility from trailing dimensions
    let offset = target.len() - source.len();
    for (i, &source_dim) in source.iter().enumerate() {
        let target_dim = target[offset + i];
        if source_dim != target_dim && source_dim != 1 {
            return false;
        }
    }

    true
}

/// Compute the strides needed for broadcasting
pub fn compute_broadcast_strides(
    original_shape: &TensorShape,
    broadcast_shape: &TensorShape,
    element_size: usize,
) -> TensorResult<SmallVec<[usize; 4]>> {
    if !can_broadcast_to(original_shape, broadcast_shape) {
        return Err(TensorError::BroadcastError {
            left: original_shape.to_vec(),
            right: broadcast_shape.to_vec(),
        });
    }

    let mut strides = SmallVec::with_capacity(broadcast_shape.len());
    let mut current_stride = element_size;

    // Start from trailing dimensions
    let offset = broadcast_shape.len() - original_shape.len();
    for i in (0..broadcast_shape.len()).rev() {
        let _broadcast_dim = broadcast_shape[i];
        
        if i >= offset {
            let original_dim = original_shape[i - offset];
            if original_dim == 1 {
                // Broadcasting dimension - stride is 0
                strides.push(0);
            } else {
                // Regular dimension
                strides.push(current_stride);
                current_stride *= original_dim;
            }
        } else {
            // Prepended dimension (original had fewer dims)
            strides.push(0);
        }
    }

    strides.reverse();
    Ok(strides)
}

/// Multi-way broadcasting for more than 2 tensors
pub fn broadcast_shapes_multi(shapes: &[&TensorShape]) -> TensorResult<TensorShape> {
    if shapes.is_empty() {
        return Ok(SmallVec::new());
    }

    let mut result = shapes[0].clone();
    for &shape in &shapes[1..] {
        result = broadcast_shapes(&result, shape)?;
    }

    Ok(result)
}

/// Check if all shapes can be broadcast together
pub fn can_broadcast_multi(shapes: &[&TensorShape]) -> bool {
    shapes.is_empty() || broadcast_shapes_multi(shapes).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::smallvec;

    #[test]
    fn test_can_broadcast_shapes() {
        // Compatible shapes
        assert!(can_broadcast_shapes(&smallvec![3, 4], &smallvec![3, 4]));
        assert!(can_broadcast_shapes(&smallvec![1, 4], &smallvec![3, 4]));
        assert!(can_broadcast_shapes(&smallvec![3, 1], &smallvec![3, 4]));
        assert!(can_broadcast_shapes(&smallvec![1], &smallvec![3, 4]));
        assert!(can_broadcast_shapes(&smallvec![], &smallvec![3, 4])); // Scalar

        // Incompatible shapes
        assert!(!can_broadcast_shapes(&smallvec![2, 4], &smallvec![3, 4]));
        assert!(!can_broadcast_shapes(&smallvec![3, 2], &smallvec![3, 4]));
    }

    #[test]
    fn test_broadcast_shapes() {
        let result = broadcast_shapes(&smallvec![3, 1], &smallvec![1, 4]).unwrap();
        assert_eq!(result, TensorShape::from_slice(&[3, 4]));

        let result = broadcast_shapes(&smallvec![2, 1, 4], &smallvec![1, 5, 1]).unwrap();
        assert_eq!(result, TensorShape::from_slice(&[2, 5, 4]));

        let result = broadcast_shapes(&smallvec![], &smallvec![3, 4]).unwrap();
        assert_eq!(result, TensorShape::from_slice(&[3, 4]));
    }

    #[test]
    fn test_broadcast_shapes_error() {
        let result = broadcast_shapes(&smallvec![2, 3], &smallvec![3, 4]);
        assert!(result.is_err());
    }

    #[test]
    fn test_can_broadcast_to() {
        assert!(can_broadcast_to(&smallvec![1, 4], &smallvec![3, 4]));
        assert!(can_broadcast_to(&smallvec![4], &smallvec![3, 4]));
        assert!(can_broadcast_to(&smallvec![], &smallvec![3, 4]));
        
        assert!(!can_broadcast_to(&smallvec![2, 4], &smallvec![3, 4]));
        assert!(!can_broadcast_to(&smallvec![3, 4, 5], &smallvec![3, 4]));
    }

    #[test]
    fn test_compute_broadcast_strides() {
        let strides = compute_broadcast_strides(
            &smallvec![1, 4],
            &smallvec![3, 4],
            4 // sizeof(f32)
        ).unwrap();
        assert_eq!(strides, SmallVec::<[usize; 4]>::from_slice(&[0, 4])); // First dim broadcasted, second regular

        let strides = compute_broadcast_strides(
            &smallvec![3, 1],
            &smallvec![3, 4],
            4
        ).unwrap();
        assert_eq!(strides, SmallVec::<[usize; 4]>::from_slice(&[4, 0])); // Second dim broadcasted
    }

    #[test]
    fn test_broadcast_shapes_multi() {
        let shape1 = smallvec![2, 1, 4];
        let shape2 = smallvec![1, 3, 1];
        let shape3 = smallvec![1, 1, 1];
        let shapes = vec![&shape1, &shape2, &shape3];
        let result = broadcast_shapes_multi(&shapes).unwrap();
        assert_eq!(result, TensorShape::from_slice(&[2, 3, 4]));
    }
}