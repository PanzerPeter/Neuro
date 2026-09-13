// The order-based selections `.sort()`, `.argsort()`, and `.topk()` on a tensor.
//
// Reached from the builtin-method arm of `check_call_expr`. All three order one axis of
// the receiver and differ only in what they hand back: the reordered elements, the
// ordering itself, or the leading `k` of both. The argument list always arrives complete
// and in declaration order — `axis` then `descending`, or `k` then `axis` — because the
// argument-binding pass fills an omitted one from the default the specification gives.
//
// Every argument is read as syntax rather than as a value, the way a reduction's axis is:
// an axis may be a dimension NAME that no value scope declares, and `k` and `descending`
// decide the result's shape and the comparator the backend emits, both of which are
// settled before any element exists.
//
// The receiver is READ, not consumed: the result is a fresh allocation, so ordering a
// weight must not move it out of whatever owns it.

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::{ArrayLen, TensorAxis, Type};
use ast_types::{Expr, UnaryOp};
use shared_types::{Literal, Span};

pub(crate) const SORT_METHOD: &str = "sort";
pub(crate) const ARGSORT_METHOD: &str = "argsort";
pub(crate) const TOPK_METHOD: &str = "topk";

/// The element type of the index tensors `.argsort` and `.topk` produce, which the
/// language spells `Tensor<i32, ...>`.
pub(crate) const INDEX_ELEMENT: Type = Type::I32;

/// Whether `method` names one of the three order-based selections.
pub(crate) fn is_sort_method(method: &str) -> bool {
    matches!(method, SORT_METHOD | ARGSORT_METHOD | TOPK_METHOD)
}

impl TypeChecker {
    /// Type-check one selection and return the type it produces: the receiver's own type
    /// for `.sort`, an index tensor of the same shape for `.argsort`, and a pair of
    /// tensors whose sorted axis is `k` long for `.topk`.
    pub(crate) fn check_tensor_sort(
        &mut self,
        element: &Type,
        shape: &[TensorAxis],
        method: &str,
        args: &[Expr],
        call_span: Span,
    ) -> Type {
        if !element.is_integer() && !element.is_float() {
            self.record_error(TypeError::TensorSortElementType {
                method: method.to_string(),
                element: element.clone(),
                span: call_span,
            });
            return Type::Unknown;
        }
        if let Some(symbolic) = shape
            .iter()
            .find(|axis| !matches!(axis.extent, ArrayLen::Fixed(_)))
        {
            self.record_error(TypeError::TensorShapeCastSymbolicExtent {
                method: method.to_string(),
                name: symbolic.extent.to_string(),
                span: call_span,
            });
            return Type::Unknown;
        }
        if shape.is_empty() {
            self.record_error(TypeError::TensorSortRankZero {
                method: method.to_string(),
                span: call_span,
            });
            return Type::Unknown;
        }

        if method == TOPK_METHOD {
            return self.check_topk(element, shape, args, call_span);
        }
        let Some(axis) = self.sort_axis(shape, args.first(), method, call_span) else {
            return Type::Unknown;
        };
        if !self.sorted_axis_is_populated(shape, axis, method, call_span) {
            return Type::Unknown;
        }
        if args.len() > 1 && self.const_bool(&args[1], method).is_none() {
            return Type::Unknown;
        }
        if method == ARGSORT_METHOD {
            return Type::Tensor {
                element: Box::new(INDEX_ELEMENT),
                shape: shape.to_vec(),
            };
        }
        Type::Tensor {
            element: Box::new(element.clone()),
            shape: shape.to_vec(),
        }
    }

    /// `.topk(k:, axis:)`: the leading `k` of the ordering, as a values tensor and an
    /// index tensor paired in a tuple.
    fn check_topk(
        &mut self,
        element: &Type,
        shape: &[TensorAxis],
        args: &[Expr],
        call_span: Span,
    ) -> Type {
        let Some(k) = self.const_index(args.first(), call_span) else {
            return Type::Unknown;
        };
        let Some(axis) = self.sort_axis(shape, args.get(1), TOPK_METHOD, call_span) else {
            return Type::Unknown;
        };
        let ArrayLen::Fixed(extent) = shape[axis].extent else {
            return Type::Unknown;
        };
        if k == 0 || k > extent {
            self.record_error(TypeError::TensorTopKOutOfRange {
                k,
                extent,
                span: call_span,
            });
            return Type::Unknown;
        }
        // The selected axis is `k` long rather than what the receiver declared, so a name
        // it carried no longer describes it and is dropped: `[batch, classes]` topped at
        // 5 is `[batch, 5]`, not five `classes`.
        let mut selected = shape.to_vec();
        selected[axis] = TensorAxis {
            name: None,
            extent: ArrayLen::Fixed(k),
        };
        Type::Tuple(vec![
            Type::Tensor {
                element: Box::new(element.clone()),
                shape: selected.clone(),
            },
            Type::Tensor {
                element: Box::new(INDEX_ELEMENT),
                shape: selected,
            },
        ])
    }

    /// Reject a selection over an empty axis. There is no ordering of no elements, and
    /// reporting it here keeps the backend's walk free of a zero-length special case.
    fn sorted_axis_is_populated(
        &mut self,
        shape: &[TensorAxis],
        axis: usize,
        method: &str,
        span: Span,
    ) -> bool {
        if shape[axis].extent != ArrayLen::Fixed(0) {
            return true;
        }
        self.record_error(TypeError::TensorSortEmpty {
            method: method.to_string(),
            span,
        });
        false
    }

    /// The receiver axis an `axis:` argument names: a dimension name, a non-negative
    /// index, or a negative index counting from the end, which is how the language spells the
    /// last axis.
    fn sort_axis(
        &mut self,
        shape: &[TensorAxis],
        entry: Option<&Expr>,
        method: &str,
        call_span: Span,
    ) -> Option<usize> {
        let Some(entry) = entry else {
            return Some(shape.len() - 1);
        };
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
            self.record_error(TypeError::TensorSortArgNotConstant {
                method: method.to_string(),
                label: "axis".to_string(),
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
                span: call_span,
            });
        }
        axis
    }

    /// A non-negative constant count written as an argument, which `k:` has to be: the
    /// result's shape is built from it.
    fn const_index(&mut self, entry: Option<&Expr>, call_span: Span) -> Option<usize> {
        let span = entry.map(Expr::span).unwrap_or(call_span);
        let value = entry
            .and_then(const_integer)
            .and_then(|v| usize::try_from(v).ok());
        if value.is_none() {
            self.record_error(TypeError::TensorSortArgNotConstant {
                method: TOPK_METHOD.to_string(),
                label: "k".to_string(),
                example: "k: 5".to_string(),
                span,
            });
        }
        value
    }

    /// The value of a `descending:` argument, which has to be a `true`/`false` literal:
    /// the backend picks one comparator for the whole call.
    fn const_bool(&mut self, entry: &Expr, method: &str) -> Option<bool> {
        let value = const_boolean(entry);
        if value.is_none() {
            self.record_error(TypeError::TensorSortArgNotConstant {
                method: method.to_string(),
                label: "descending".to_string(),
                example: "descending: true".to_string(),
                span: entry.span(),
            });
        }
        value
    }
}

/// The value of an integer constant expression written as an argument: a literal, or one
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

/// The value of a boolean constant expression written as an argument.
fn const_boolean(expr: &Expr) -> Option<bool> {
    match expr {
        Expr::Literal(Literal::Boolean(value), _) => Some(*value),
        Expr::Paren(inner, _) => const_boolean(inner),
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
