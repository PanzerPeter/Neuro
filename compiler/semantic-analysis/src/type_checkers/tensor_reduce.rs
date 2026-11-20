// The reductions `.sum()`, `.mean()`, `.max()`, and `.min()` on a tensor.
//
// Reached from the builtin-method arm of `check_call_expr`. Each takes either nothing,
// reducing the whole buffer to one scalar, or a single axis, reducing along it and
// dropping that axis from the result. The axis is read as syntax rather than as a value,
// the way `.permute`'s is: a dimension NAME resolves against the receiver's own shape and
// no value scope declares it.
//
// A reduction READS its receiver. Unlike the shape casts it neither re-describes nor
// consumes the buffer it summarises, so nothing is moved and a borrowed receiver is
// accepted: reading a shared weight's mean must not move it out of whatever owns it.

use super::TypeChecker;
use super::tensor_shape::{const_integer, declared_names};
use crate::errors::TypeError;
use crate::types::{ArrayLen, TensorAxis, Type, shape_count};
use ast_types::Expr;
use shared_types::Span;

pub(crate) const SUM_METHOD: &str = "sum";
pub(crate) const MEAN_METHOD: &str = "mean";
pub(crate) const MAX_METHOD: &str = "max";
pub(crate) const MIN_METHOD: &str = "min";

/// Whether `method` names one of the four reductions.
pub(crate) fn is_reduce_method(method: &str) -> bool {
    matches!(method, SUM_METHOD | MEAN_METHOD | MAX_METHOD | MIN_METHOD)
}

impl TypeChecker {
    /// Type-check one reduction and return the type it produces: the element type for a
    /// whole-tensor reduction, and a tensor of the surviving axes for an axis reduction.
    pub(crate) fn check_tensor_reduce(
        &mut self,
        element: &Type,
        shape: &[TensorAxis],
        method: &str,
        args: &[Expr],
        call_span: Span,
    ) -> Type {
        if !self.is_arithmetic(element) {
            self.record_error(TypeError::TensorReduceElementType {
                method: method.to_string(),
                element: element.clone(),
                span: call_span,
            });
            return Type::Unknown;
        }
        if method == MEAN_METHOD && !self.is_floating(element) {
            self.record_error(TypeError::TensorReduceMeanNotFloat {
                element: element.clone(),
                span: call_span,
            });
            return Type::Unknown;
        }

        // A run whose length is a number here is checked for emptiness. An extent parameter,
        // a splice or a dynamic axis has none here: whether that run is empty is answered
        // at its instance or at run time, so only a concrete one is refused now.
        if args.is_empty() {
            let count = shape_count(shape).and_then(|count| count.as_constant());
            if !self.reduced_run_is_populated(count, method, call_span) {
                return Type::Unknown;
            }
            return element.clone();
        }
        if args.len() > 1 {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: 1,
                found: args.len(),
                span: call_span,
            });
            return Type::Unknown;
        }

        let Some(axis) = self.resolve_reduce_axis(shape, &args[0], method) else {
            return Type::Unknown;
        };
        let extent = match &shape[axis].extent {
            ArrayLen::Fixed(extent) => Some(*extent as i128),
            _ => None,
        };
        if !self.reduced_run_is_populated(extent, method, call_span) {
            return Type::Unknown;
        }
        let mut result = shape.to_vec();
        result.remove(axis);
        Type::Tensor {
            element: Box::new(element.clone()),
            shape: result,
        }
    }

    /// Reject a reduction with nothing to reduce. `.max()` over an empty run has no
    /// answer at all, and reporting the whole family the same way keeps one rule rather
    /// than an identity value per operation.
    fn reduced_run_is_populated(&mut self, extent: Option<i128>, method: &str, span: Span) -> bool {
        if extent.is_none_or(|extent| extent > 0) {
            return true;
        }
        self.record_error(TypeError::TensorReduceEmpty {
            method: method.to_string(),
            span,
        });
        false
    }

    /// The receiver axis one `axis:` argument names: a dimension name, a non-negative
    /// index, or a negative index counting from the end, which is how the specification
    /// spells the last axis.
    fn resolve_reduce_axis(
        &mut self,
        shape: &[TensorAxis],
        entry: &Expr,
        method: &str,
    ) -> Option<usize> {
        if let Expr::Identifier(ident) = entry {
            let found = shape
                .iter()
                .position(|axis| axis.name.as_deref() == Some(ident.name.as_str()));
            if found.is_none() {
                self.record_error(TypeError::UnknownTensorAxisName {
                    name: ident.name.clone(),
                    declared: declared_names(shape),
                    span: entry.span(),
                });
            }
            return found;
        }
        let Some(value) = const_integer(entry) else {
            self.record_error(TypeError::TensorShapeArgNotLiteral {
                method: method.to_string(),
                example: "axis: 0".to_string(),
                span: entry.span(),
            });
            return None;
        };
        let rank = i128::try_from(shape.len()).ok()?;
        let resolved = if value < 0 { value + rank } else { value };
        let axis = usize::try_from(resolved).ok().filter(|a| *a < shape.len());
        if axis.is_none() {
            self.record_error(TypeError::TensorAxisOutOfRange {
                axis: value.unsigned_abs() as usize,
                rank: shape.len(),
                span: entry.span(),
            });
        }
        // Around a splice a position names a written axis only when it counts from the
        // side the axis is written on: `-1` of `[..S, N]` is `N` at every rank, `0` is not.
        let splice = shape
            .iter()
            .position(|axis| matches!(axis.extent, ArrayLen::Splice(_)));
        if let (Some(at), Some(found)) = (splice, axis)
            && !(if value < 0 { found > at } else { found < at })
        {
            let ty = Type::Shape(shape.to_vec());
            self.reject_splice(
                shape,
                &format!("`.{method}` along axis {value}"),
                &ty,
                entry.span(),
            );
            return None;
        }
        axis
    }
}
