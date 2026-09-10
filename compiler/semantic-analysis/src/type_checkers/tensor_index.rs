// Type-checking for tensor indexing and slicing, `t[i, j]` / `t[1..3, ..]`.
//
// Reached from the index arms of `check_expr`: the multi-axis spelling arrives as
// `Expr::TensorIndex`, and the one-argument `t[i]` arrives as the ordinary
// `Expr::Index` whose object happens to be a tensor.
//
// An axis given a position is DROPPED from the result and an axis given a range or `..`
// survives at its new extent, so `m[1, 2]` reads an element, `m[0, ..]` is a row, and
// `m[1..3, 2..5]` is a sub-matrix. A slice's extents are part of the result's TYPE,
// which is why a range bound must fold to a constant here while a position may be any
// run-time integer.

use super::expressions::const_predicates::eval_literal_int;
use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::{Expr, TensorIndexArg};
use shared_types::Span;

/// One axis of an index, resolved against the tensor's shape.
enum ResolvedAxis {
    /// A position, dropped from the result. Carried as the expression it was written
    /// as, since it may be a run-time value.
    Position,
    /// A half-open sub-range `[start, end)` of the axis, folded to constants.
    Range { start: usize, end: usize },
}

impl TypeChecker {
    /// Check `object[a0, a1, ...]` over a tensor and answer the type it reads: the
    /// element type when every axis is given a position, otherwise the sliced tensor.
    ///
    /// `element` and `shape` come from the receiver's already-checked type, so this is
    /// only reached once the object is known to be a tensor.
    pub(crate) fn check_tensor_index(
        &mut self,
        element: &Type,
        shape: &[usize],
        indices: &[TensorIndexArg],
        span: Span,
    ) -> Type {
        if indices.len() != shape.len() {
            self.record_error(TypeError::TensorIndexRankMismatch {
                expected: shape.len(),
                found: indices.len(),
                span,
            });
            // The written axes are still checked: an index that named the wrong number
            // of axes may also hold an error of its own worth reporting.
            for (axis, index) in indices.iter().enumerate() {
                self.resolve_axis(index, axis, shape.get(axis).copied().unwrap_or(0));
            }
            return Type::Unknown;
        }

        let mut kept = Vec::new();
        for (axis, (index, extent)) in indices.iter().zip(shape.iter()).enumerate() {
            match self.resolve_axis(index, axis, *extent) {
                Some(ResolvedAxis::Range { start, end }) => kept.push(end - start),
                Some(ResolvedAxis::Position) => {}
                None => return Type::Unknown,
            }
        }

        if kept.is_empty() {
            return element.clone();
        }
        Type::Tensor {
            element: Box::new(element.clone()),
            shape: kept,
        }
    }

    /// Check one axis argument against that axis's extent.
    ///
    /// `None` means the argument was rejected, so the whole index has no type; a
    /// reported axis does not stop the remaining axes from being checked.
    fn resolve_axis(
        &mut self,
        index: &TensorIndexArg,
        axis: usize,
        extent: usize,
    ) -> Option<ResolvedAxis> {
        match index {
            TensorIndexArg::FullAxis(_) => Some(ResolvedAxis::Range {
                start: 0,
                end: extent,
            }),
            TensorIndexArg::Position(expr) => self.resolve_position(expr, axis, extent),
            TensorIndexArg::Range {
                start,
                end,
                inclusive,
                span,
            } => self.resolve_range(start, end, *inclusive, axis, extent, *span),
        }
    }

    /// A position along one axis: any integer expression. A constant one is bounds-
    /// checked here; a run-time one is checked by the backend's debug-tier guard, the
    /// same tier an array index sits on.
    fn resolve_position(
        &mut self,
        expr: &Expr,
        axis: usize,
        extent: usize,
    ) -> Option<ResolvedAxis> {
        let ty = self.check_expr(expr, None).unwrap_or(Type::Unknown);
        if !matches!(ty, Type::Unknown) && !ty.is_integer() {
            self.record_error(TypeError::IndexNotInteger {
                found: ty,
                span: expr.span(),
            });
            return None;
        }
        if let Some(value) = eval_literal_int(expr) {
            if value < 0 || value >= extent as i128 {
                self.record_error(TypeError::TensorIndexOutOfBounds {
                    index: value,
                    axis,
                    extent,
                    span: expr.span(),
                });
                return None;
            }
        }
        Some(ResolvedAxis::Position)
    }

    /// A sub-range of one axis. Both bounds must fold to constants: the resulting
    /// extent is part of the sliced tensor's type, and a type cannot wait for a value.
    fn resolve_range(
        &mut self,
        start: &Expr,
        end: &Expr,
        inclusive: bool,
        axis: usize,
        extent: usize,
        span: Span,
    ) -> Option<ResolvedAxis> {
        self.check_expr(start, None);
        self.check_expr(end, None);
        let (Some(start_value), Some(end_value)) = (eval_literal_int(start), eval_literal_int(end))
        else {
            self.record_error(TypeError::TensorSliceBoundNotConstant { span });
            return None;
        };
        // An inclusive range names its last position, so it stops one further on.
        let last = if inclusive { end_value + 1 } else { end_value };
        if start_value < 0 || last < start_value || last > extent as i128 {
            self.record_error(TypeError::TensorSliceOutOfRange {
                start: start_value,
                end: last,
                axis,
                extent,
                span,
            });
            return None;
        }
        Some(ResolvedAxis::Range {
            start: start_value as usize,
            end: last as usize,
        })
    }

    /// Check the multi-axis index expression itself: the receiver must be a tensor, or
    /// a borrow of one.
    pub(crate) fn check_tensor_index_expr(
        &mut self,
        object: &Expr,
        indices: &[TensorIndexArg],
        span: Span,
    ) -> Option<Type> {
        let obj_ty = self.check_expr(object, None).unwrap_or(Type::Unknown);
        if matches!(obj_ty, Type::Unknown) {
            return Some(Type::Unknown);
        }
        let Type::Tensor { element, shape } = obj_ty.referent().clone() else {
            self.record_error(TypeError::TensorIndexOnNonTensor {
                found: obj_ty.referent().clone(),
                span,
            });
            return Some(Type::Unknown);
        };
        Some(self.check_tensor_index(&element, &shape, indices, span))
    }
}
