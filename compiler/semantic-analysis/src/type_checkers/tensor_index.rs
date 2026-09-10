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
use crate::types::{ArrayLen, Type};
use ast_types::{Expr, TensorIndexArg};
use shared_types::Span;

/// One axis of an index, resolved against the tensor's shape.
enum ResolvedAxis {
    /// A position, dropped from the result. Carried as the expression it was written
    /// as, since it may be a run-time value.
    Position,
    /// A surviving axis and the extent it survives at. A sub-range's extent is the
    /// constant length of the range; a whole axis keeps the source's extent, which is
    /// symbolic when the source's is.
    Kept(ArrayLen),
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
        shape: &[ArrayLen],
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
                let extent = shape.get(axis).cloned().unwrap_or(ArrayLen::Fixed(0));
                self.resolve_axis(index, axis, &extent);
            }
            return Type::Unknown;
        }

        let mut kept = Vec::new();
        for (axis, (index, extent)) in indices.iter().zip(shape.iter()).enumerate() {
            match self.resolve_axis(index, axis, extent) {
                Some(ResolvedAxis::Kept(extent)) => kept.push(extent),
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
        extent: &ArrayLen,
    ) -> Option<ResolvedAxis> {
        match index {
            TensorIndexArg::FullAxis(_) => Some(ResolvedAxis::Kept(extent.clone())),
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
    /// checked here against a known extent; a run-time one, or one along a shape
    /// parameter's axis, is checked by the backend's debug-tier guard, the same tier an
    /// array index sits on.
    fn resolve_position(
        &mut self,
        expr: &Expr,
        axis: usize,
        extent: &ArrayLen,
    ) -> Option<ResolvedAxis> {
        let ty = self.check_expr(expr, None).unwrap_or(Type::Unknown);
        if !matches!(ty, Type::Unknown) && !ty.is_integer() {
            self.record_error(TypeError::IndexNotInteger {
                found: ty,
                span: expr.span(),
            });
            return None;
        }
        if let (Some(value), ArrayLen::Fixed(extent)) = (eval_literal_int(expr), extent) {
            if value < 0 || value >= *extent as i128 {
                self.record_error(TypeError::TensorIndexOutOfBounds {
                    index: value,
                    axis,
                    extent: *extent,
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
        extent: &ArrayLen,
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
        // A shape parameter's axis has no extent to stop at until the instantiation, so
        // only the range's own well-formedness is checked there.
        let past_extent = matches!(extent, ArrayLen::Fixed(e) if last > *e as i128);
        if start_value < 0 || last < start_value || past_extent {
            self.record_error(TypeError::TensorSliceOutOfRange {
                start: start_value,
                end: last,
                axis,
                extent: extent.to_string(),
                span,
            });
            return None;
        }
        Some(ResolvedAxis::Kept(ArrayLen::Fixed(
            (last - start_value) as usize,
        )))
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
