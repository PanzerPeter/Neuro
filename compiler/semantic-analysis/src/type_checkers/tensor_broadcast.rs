// The by-value tensor operators `a + b`, `&a + &b` and the scalar broadcast, plus the
// shape rule they share with in-place compound assignment.
//
// Reached from the binary-operator arm of `check_expr` and from
// `check_tensor_compound_assign`. Adds methods to the same `impl TypeChecker` block as
// the rest of `type_checkers`.

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::{ArrayLen, TensorAxis, Type};
use ast_types::{BinaryOp, Expr};
use shared_types::Span;

/// The extent an axis absent from a lower-rank operand behaves as: a missing leading
/// axis contributes one element and is stretched across the result's, which is what
/// makes `[2, 3] * [3]` the row-wise product rather than a rank error.
const IMPLICIT_LEADING_EXTENT: usize = 1;

/// Whether `op` is one of the element-wise arithmetic operators defined on tensors.
/// Comparison, bitwise and logical operators have no tensor meaning: they would each
/// have to answer with a `bool` tensor, which the language does not specify.
fn is_tensor_operator(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::Modulo
    )
}

/// Join two operand shapes under the broadcast rule, or `None` when they do not
/// broadcast.
///
/// The rule is NumPy's, which is what the ecosystem a Neuro user arrives from means by
/// the word: shapes align at the TRAILING axis, an extent of 1 stretches across a larger
/// one, and an operand of lower rank supplies the innermost axes and repeats across the
/// leading ones. A shape parameter's extent is never the axis that STRETCHES: whether a
/// shape parameter is 1 is not known until the instantiation, so an operand stretched on
/// that guess would read index 0 forever against an axis that turns out to be wider. It
/// is still stretched INTO, since the literal 1 on the other side is known here.
fn broadcast_shapes(left: &[TensorAxis], right: &[TensorAxis]) -> Option<Vec<TensorAxis>> {
    let rank = left.len().max(right.len());
    let mut result = Vec::with_capacity(rank);
    for position in 0..rank {
        // Counted from the trailing end, which is where the two shapes are aligned.
        let depth = rank - 1 - position;
        let l = left.len().checked_sub(depth + 1).map(|i| &left[i]);
        let r = right.len().checked_sub(depth + 1).map(|i| &right[i]);
        result.push(match (l, r) {
            (Some(l), Some(r)) => join_axes(l, r)?,
            (Some(present), None) | (None, Some(present)) => present.clone(),
            (None, None) => return None,
        });
    }
    Some(result)
}

/// The result axis two aligned operand axes produce, or `None` when neither stretches
/// into the other.
fn join_axes(left: &TensorAxis, right: &TensorAxis) -> Option<TensorAxis> {
    if left.extent == right.extent {
        // Both operands walk this axis, so a name either supplies is the result's and a
        // disagreement is a dimension-name mismatch rather than a silent relabel.
        if !left.names_agree_with(right) {
            return None;
        }
        let name = left.name.clone().or_else(|| right.name.clone());
        return Some(TensorAxis {
            name,
            extent: left.extent.clone(),
        });
    }
    // A stretched axis contributes no extent and no name: the axis the result keeps is
    // entirely the other operand's.
    match (&left.extent, &right.extent) {
        (ArrayLen::Fixed(IMPLICIT_LEADING_EXTENT), _) => Some(right.clone()),
        (_, ArrayLen::Fixed(IMPLICIT_LEADING_EXTENT)) => Some(left.clone()),
        _ => None,
    }
}

/// Whether two shapes describe the same element layout. Axis NAMES are deliberately left
/// out: the broadcast join already rejected a disagreement between two walked axes, and
/// an axis a stretch filled in carries the other operand's name legitimately.
fn shapes_have_equal_extents(left: &[TensorAxis], right: &[TensorAxis]) -> bool {
    left.len() == right.len() && left.iter().zip(right).all(|(l, r)| l.extent == r.extent)
}

/// How a tensor operator reads one of its operands.
enum Operand {
    /// A tensor, with its element type and shape, and whether it is owned (and so
    /// consumed by the operator) rather than borrowed.
    Tensor {
        element: Type,
        shape: Vec<TensorAxis>,
        owned: bool,
    },
    /// A scalar broadcast across every element of the other operand.
    Scalar(Type),
}

fn classify(ty: &Type) -> Option<Operand> {
    if let Type::Tensor { element, shape } = ty.referent() {
        return Some(Operand::Tensor {
            element: (**element).clone(),
            shape: shape.clone(),
            owned: !matches!(ty, Type::Reference { .. }),
        });
    }
    ty.is_numeric().then(|| Operand::Scalar(ty.clone()))
}

impl TypeChecker {
    /// The element type an operand of `ty` would broadcast against, when `ty` is a
    /// tensor.
    ///
    /// This is what types a bare literal written beside a tensor: `matrix * 2.0` is
    /// the scalar broadcast, and the literal there is the element's type rather than
    /// the `f64` the language default would otherwise pick and the operator then reject.
    pub(crate) fn tensor_element_expectation(ty: &Type) -> Option<Type> {
        match ty.referent() {
            Type::Tensor { element, .. } => Some((**element).clone()),
            _ => None,
        }
    }

    /// The element type a scalar written to the LEFT of a tensor operand should take, so
    /// `0.5 * matrix` is the scalar broadcast it reads as.
    ///
    /// The right operand has not been checked yet at this point — an operator types its
    /// left side first — so the tensor is recognized from the binding rather than from a
    /// type. That is what keeps the lookahead free of side effects: a speculative
    /// `check_expr` would record any diagnostic the discarded attempt produced. A scalar
    /// beside anything else needs its suffix, exactly as it does today.
    pub(crate) fn scalar_broadcast_expectation(&self, right: &Expr) -> Option<Type> {
        let Expr::Identifier(name) = right else {
            return None;
        };
        let ty = self
            .symbols
            .lookup(&name.name)
            .map(|info| info.ty.clone())?;
        Self::tensor_element_expectation(&ty)
    }

    /// Whether `value_ty` may sit on the right of `target OP= value`, reporting why not.
    ///
    /// Compound assignment takes the by-value operator's broadcast rules, with one
    /// asymmetry the in-place update forces: the result is written back into the buffer
    /// the target already owns, so the join has to come back as the TARGET's shape. An
    /// operand may therefore be stretched up to it but never past it — `w -= row` is the
    /// row-wise update, `row -= w` has nowhere to put the wider result.
    pub(crate) fn compound_assign_operand_fits(
        &mut self,
        value_ty: &Type,
        target_ty: &Type,
        op: BinaryOp,
        span: Span,
    ) -> bool {
        let Type::Tensor {
            element: target_element,
            shape: target_shape,
        } = target_ty
        else {
            return false;
        };
        match classify(value_ty) {
            Some(Operand::Scalar(scalar)) if target_element.is_compatible_with(&scalar) => true,
            Some(Operand::Tensor { element, shape, .. })
                if target_element.is_compatible_with(&element) =>
            {
                let joined = broadcast_shapes(target_shape, &shape);
                if joined.is_some_and(|joined| shapes_have_equal_extents(&joined, target_shape)) {
                    return true;
                }
                self.record_error(TypeError::TensorBroadcastMismatch {
                    op: format!("{op}="),
                    left: target_ty.clone(),
                    right: value_ty.clone(),
                    span,
                });
                false
            }
            _ => {
                self.record_error(TypeError::Mismatch {
                    expected: target_ty.clone(),
                    found: value_ty.clone(),
                    span,
                });
                false
            }
        }
    }

    /// Check `left OP right` where at least one operand is a tensor, answering the
    /// freshly allocated result's type.
    ///
    /// A by-value operator allocates rather than updating in place, so unlike `OP=` it
    /// reads both operands and consumes neither's buffer into the other's. An owned
    /// operand is still moved — its buffer is released once the result is built — while
    /// a borrowed one is only read, which is what lets a weight stay in a training loop.
    pub(crate) fn check_tensor_binary(
        &mut self,
        left: &Expr,
        left_ty: &Type,
        op: BinaryOp,
        right: &Expr,
        right_ty: &Type,
        span: Span,
    ) -> Type {
        if !is_tensor_operator(op) {
            self.record_error(TypeError::InvalidBinaryOperator {
                op: op.to_string(),
                left: left_ty.clone(),
                right: right_ty.clone(),
                span,
            });
            return Type::Unknown;
        }
        let (Some(lhs), Some(rhs)) = (classify(left_ty), classify(right_ty)) else {
            self.record_error(TypeError::InvalidBinaryOperator {
                op: op.to_string(),
                left: left_ty.clone(),
                right: right_ty.clone(),
                span,
            });
            return Type::Unknown;
        };

        let Some(result) = self.join_operands(&lhs, op, &rhs, left_ty, right_ty, span) else {
            return Type::Unknown;
        };

        // The move is recorded only once the operands are known to combine: a rejected
        // operator leaves the bindings usable, so the one diagnostic is the shape error
        // rather than a use-after-move cascade behind it.
        if matches!(lhs, Operand::Tensor { owned: true, .. }) {
            self.record_move(left);
        }
        if matches!(rhs, Operand::Tensor { owned: true, .. }) {
            self.record_move(right);
        }
        result
    }

    /// The result type of two classified operands, or `None` once a diagnostic has been
    /// recorded for them.
    fn join_operands(
        &mut self,
        lhs: &Operand,
        op: BinaryOp,
        rhs: &Operand,
        left_ty: &Type,
        right_ty: &Type,
        span: Span,
    ) -> Option<Type> {
        let (element, shape) = match (lhs, rhs) {
            (
                Operand::Tensor { element, shape, .. },
                Operand::Tensor {
                    element: other_element,
                    shape: other_shape,
                    ..
                },
            ) => {
                if !element.is_compatible_with(other_element) {
                    self.record_error(TypeError::Mismatch {
                        expected: left_ty.clone(),
                        found: right_ty.clone(),
                        span,
                    });
                    return None;
                }
                let Some(joined) = broadcast_shapes(shape, other_shape) else {
                    self.record_error(TypeError::TensorBroadcastMismatch {
                        op: op.to_string(),
                        left: left_ty.clone(),
                        right: right_ty.clone(),
                        span,
                    });
                    return None;
                };
                (element.clone(), joined)
            }
            (Operand::Tensor { element, shape, .. }, Operand::Scalar(scalar))
            | (Operand::Scalar(scalar), Operand::Tensor { element, shape, .. }) => {
                // A scalar is broadcast, not converted: the language has no implicit numeric
                // conversion, so the scalar already carries the element's own type.
                if !element.is_compatible_with(scalar) {
                    self.record_error(TypeError::Mismatch {
                        expected: left_ty.clone(),
                        found: right_ty.clone(),
                        span,
                    });
                    return None;
                }
                (element.clone(), shape.clone())
            }
            // Two scalars never reach here: the caller routes to this rule only when an
            // operand is a tensor.
            (Operand::Scalar(_), Operand::Scalar(_)) => return None,
        };

        let result = Type::Tensor {
            element: Box::new(element.clone()),
            shape,
        };
        // The element carries the arithmetic, so the operator is defined exactly where it
        // is defined on the scalar: `bool` has none, and the half-precision scalar
        // contract stops short of it.
        if !element.is_numeric() || element.is_half_float() {
            self.record_error(TypeError::TensorElementNotArithmetic {
                op: op.to_string(),
                element,
                span,
            });
            return None;
        }
        // The result is a fresh buffer, so its element count must be a number here; the
        // operands' own extents are what it is computed from.
        if let Type::Tensor { shape, .. } = &result {
            let shape = shape.clone();
            if self.reject_dynamic_extent(&shape, &format!("`{op}`"), &result, span) {
                return None;
            }
        }
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis(extent: usize) -> TensorAxis {
        TensorAxis {
            name: None,
            extent: ArrayLen::Fixed(extent),
        }
    }

    fn named(name: &str, extent: usize) -> TensorAxis {
        TensorAxis {
            name: Some(name.to_string()),
            extent: ArrayLen::Fixed(extent),
        }
    }

    fn extents(shape: &[TensorAxis]) -> Vec<ArrayLen> {
        shape.iter().map(|a| a.extent.clone()).collect()
    }

    #[test]
    fn equal_shapes_join_to_themselves() {
        let shape = vec![axis(2), axis(3)];
        let joined = broadcast_shapes(&shape, &shape).expect("equal shapes broadcast");
        assert_eq!(extents(&joined), extents(&shape));
    }

    #[test]
    fn a_size_one_axis_stretches() {
        let joined = broadcast_shapes(&[axis(1), axis(3)], &[axis(2), axis(3)])
            .expect("a size-1 axis stretches");
        assert_eq!(
            extents(&joined),
            vec![ArrayLen::Fixed(2), ArrayLen::Fixed(3)]
        );
    }

    #[test]
    fn a_lower_rank_operand_supplies_the_innermost_axes() {
        let joined =
            broadcast_shapes(&[axis(2), axis(3)], &[axis(3)]).expect("a rank-1 operand aligns");
        assert_eq!(
            extents(&joined),
            vec![ArrayLen::Fixed(2), ArrayLen::Fixed(3)]
        );
    }

    #[test]
    fn a_rank_zero_operand_broadcasts_like_a_scalar() {
        let joined = broadcast_shapes(&[axis(2), axis(3)], &[]).expect("rank 0 broadcasts");
        assert_eq!(
            extents(&joined),
            vec![ArrayLen::Fixed(2), ArrayLen::Fixed(3)]
        );
    }

    #[test]
    fn an_extent_neither_equal_nor_one_does_not_broadcast() {
        assert!(broadcast_shapes(&[axis(2)], &[axis(3)]).is_none());
    }

    #[test]
    fn a_symbolic_extent_is_stretched_into_but_never_stretched() {
        let n = TensorAxis {
            name: None,
            extent: ArrayLen::Param("N".to_string()),
        };
        assert!(broadcast_shapes(std::slice::from_ref(&n), std::slice::from_ref(&n)).is_some());
        let stretched_into =
            broadcast_shapes(std::slice::from_ref(&n), &[axis(1)]).expect("a literal 1 stretches");
        assert_eq!(stretched_into[0].extent, ArrayLen::Param("N".to_string()));
        assert!(broadcast_shapes(std::slice::from_ref(&n), &[axis(3)]).is_none());
    }

    #[test]
    fn a_stretched_axis_contributes_no_name() {
        let joined = broadcast_shapes(&[named("batch", 1)], &[named("width", 4)])
            .expect("a size-1 axis stretches");
        assert_eq!(joined[0].name.as_deref(), Some("width"));
    }

    #[test]
    fn walked_axes_with_disagreeing_names_do_not_broadcast() {
        assert!(broadcast_shapes(&[named("height", 4)], &[named("width", 4)]).is_none());
    }
}
