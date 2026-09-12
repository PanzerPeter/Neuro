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
use crate::errors::TypeError;
use crate::types::{ArrayLen, TensorAxis, Type};
use ast_types::{Expr, UnaryOp};
use shared_types::{Literal, Span};

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
        if !element.is_integer() && !element.is_float() {
            self.record_error(TypeError::TensorReduceElementType {
                method: method.to_string(),
                element: element.clone(),
                span: call_span,
            });
            return Type::Unknown;
        }
        if method == MEAN_METHOD && !element.is_float() {
            self.record_error(TypeError::TensorReduceMeanNotFloat {
                element: element.clone(),
                span: call_span,
            });
            return Type::Unknown;
        }

        // Every extent has to be concrete here: the reduced run's length decides whether
        // the reduction has a value at all, and the backend walks the buffer at strides
        // the shape supplies. A shape parameter has neither until it is instantiated.
        if let Some(symbolic) = shape.iter().find(|axis| !is_fixed(&axis.extent)) {
            self.record_error(TypeError::TensorShapeCastSymbolicExtent {
                method: method.to_string(),
                name: symbolic.extent.to_string(),
                span: call_span,
            });
            return Type::Unknown;
        }

        if args.is_empty() {
            let count: usize = extents(shape).product();
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
        let ArrayLen::Fixed(extent) = shape[axis].extent else {
            return Type::Unknown;
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
    fn reduced_run_is_populated(&mut self, extent: usize, method: &str, span: Span) -> bool {
        if extent > 0 {
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
        axis
    }
}

/// Whether one extent is a compile-time number, which every extent of a reduced tensor
/// is by the time the traversal below runs.
fn is_fixed(extent: &ArrayLen) -> bool {
    matches!(extent, ArrayLen::Fixed(_))
}

/// The extents of a shape. Only reached once every one of them is `Fixed`.
fn extents(shape: &[TensorAxis]) -> impl Iterator<Item = usize> + '_ {
    shape.iter().map(|axis| match axis.extent {
        ArrayLen::Fixed(extent) => extent,
        _ => 0,
    })
}

/// The value of an integer constant expression written as an axis: a literal, or one
/// under a negation (`-1`) or parentheses.
fn const_integer(expr: &Expr) -> Option<i128> {
    match expr {
        Expr::Literal(Literal::Integer(value, _), _) => Some(*value),
        Expr::Paren(inner, _) => const_integer(inner),
        Expr::Unary {
            op: UnaryOp::Negate,
            operand,
            ..
        } => const_integer(operand).map(|value| -value),
        _ => None,
    }
}

/// The dimension names a shape declares, for the diagnostic that lists them.
fn declared_names(shape: &[TensorAxis]) -> String {
    let names: Vec<&str> = shape
        .iter()
        .filter_map(|axis| axis.name.as_deref())
        .collect();
    if names.is_empty() {
        return "no dimension names".to_string();
    }
    names.join(", ")
}
