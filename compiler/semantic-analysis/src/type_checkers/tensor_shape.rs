// The shape-manipulation methods on a tensor: `.t()`, `.reshape([...])`,
// `.permute([...])`, and `.flatten()` / `.flatten([...])`.
//
// Reached from the builtin-method arm of `check_call_expr`. What these four have in
// common is that their arguments are read at compile time and never as values: a
// `.reshape` extent is folded here, and a `.permute` axis may be written as a dimension
// NAME, which is resolved against the receiver's own shape rather than the surrounding
// value scope. So the arguments are inspected as syntax and never handed to `check_expr`,
// which would look an axis name up as a variable and not find one.

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::{ArrayLen, TensorAxis, Type};
use ast_types::{Expr, UnaryOp};
use shared_types::{Literal, Span};

/// The rank-2 transpose `matrix.t()`.
pub(crate) const TRANSPOSE_METHOD: &str = "t";
/// `tensor.reshape([d0, ...])`, with `-1` in at most one position.
pub(crate) const RESHAPE_METHOD: &str = "reshape";
/// `tensor.permute([...])`, whose entries name every axis exactly once.
pub(crate) const PERMUTE_METHOD: &str = "permute";
/// `tensor.flatten()` / `tensor.flatten(dims: [...])`.
pub(crate) const FLATTEN_METHOD: &str = "flatten";

/// The `-1` an extent may be written as to have it inferred from the others.
const INFERRED_EXTENT: i128 = -1;

/// Whether `method` names one of the four shape-manipulation intrinsics.
pub(crate) fn is_shape_method(method: &str) -> bool {
    matches!(
        method,
        TRANSPOSE_METHOD | RESHAPE_METHOD | PERMUTE_METHOD | FLATTEN_METHOD
    )
}

impl TypeChecker {
    /// Type-check one of the four shape-manipulation methods and return the tensor type
    /// it produces. The receiver is consumed: the one buffer moves into the result.
    pub(crate) fn check_tensor_shape_method(
        &mut self,
        element: &Type,
        shape: &[TensorAxis],
        object: &Expr,
        method: &str,
        args: &[Expr],
        call_span: Span,
    ) -> Type {
        let Some(result) = self.resolve_result_shape(shape, method, args, call_span) else {
            return Type::Unknown;
        };
        self.record_move(object);
        Type::Tensor {
            element: Box::new(element.clone()),
            shape: result,
        }
    }

    /// The shape a shape-manipulation call produces, or `None` when a diagnostic was
    /// recorded instead. How the elements move is the backend's concern and is
    /// re-derived during lowering; the checker only has to name the result type.
    fn resolve_result_shape(
        &mut self,
        shape: &[TensorAxis],
        method: &str,
        args: &[Expr],
        call_span: Span,
    ) -> Option<Vec<TensorAxis>> {
        match method {
            TRANSPOSE_METHOD => self.resolve_transpose(shape, args, call_span),
            RESHAPE_METHOD => self.resolve_reshape(shape, args, call_span),
            PERMUTE_METHOD => self.resolve_permute(shape, args, call_span),
            FLATTEN_METHOD => self.resolve_flatten(shape, args, call_span),
            _ => None,
        }
    }

    /// `.t()`: the rank-2 transpose. Both axes keep their dimension names, which is what
    /// makes a transposed `[height: H, width: W]` land as `[width: W, height: H]` and
    /// still be rejected where the original was expected.
    fn resolve_transpose(
        &mut self,
        shape: &[TensorAxis],
        args: &[Expr],
        call_span: Span,
    ) -> Option<Vec<TensorAxis>> {
        self.check_shape_arity(args, 0, call_span);
        if shape.len() != 2 {
            self.record_error(TypeError::TensorTransposeRank {
                rank: shape.len(),
                span: call_span,
            });
            return None;
        }
        Some(vec![shape[1].clone(), shape[0].clone()])
    }

    /// `.reshape([...])`: the same elements, re-described. Dimension names are dropped —
    /// a new extent is not the axis the old name documented.
    fn resolve_reshape(
        &mut self,
        shape: &[TensorAxis],
        args: &[Expr],
        call_span: Span,
    ) -> Option<Vec<TensorAxis>> {
        let entries = self.shape_arg_entries(args, RESHAPE_METHOD, "-1", call_span)?;
        let total = self.static_element_count(shape, RESHAPE_METHOD, call_span)?;

        let mut extents: Vec<Option<usize>> = Vec::with_capacity(entries.len());
        let mut inferred_at: Option<usize> = None;
        for entry in entries {
            let Some(value) = const_integer(entry) else {
                self.record_error(TypeError::TensorReshapeExtentNotConstant { span: entry.span() });
                return None;
            };
            if value == INFERRED_EXTENT {
                if inferred_at.is_some() {
                    self.record_error(TypeError::TensorReshapeRepeatedInference {
                        span: entry.span(),
                    });
                    return None;
                }
                inferred_at = Some(extents.len());
                extents.push(None);
                continue;
            }
            let Ok(extent) = usize::try_from(value) else {
                self.record_error(TypeError::TensorReshapeExtentNotConstant { span: entry.span() });
                return None;
            };
            extents.push(Some(extent));
        }

        let known: usize = extents.iter().flatten().product();
        if let Some(position) = inferred_at {
            // A zero-extent axis beside a `-1` leaves nothing to divide by, and the
            // division below would trap rather than diagnose.
            if known == 0 || total % known != 0 {
                self.record_error(TypeError::TensorReshapeIndivisible {
                    known,
                    total,
                    span: call_span,
                });
                return None;
            }
            extents[position] = Some(total / known);
        } else if known != total {
            self.record_error(TypeError::TensorReshapeElementCount {
                expected: total,
                found: known,
                span: call_span,
            });
            return None;
        }

        Some(
            extents
                .into_iter()
                .flatten()
                .map(|extent| TensorAxis {
                    name: None,
                    extent: ArrayLen::Fixed(extent),
                })
                .collect(),
        )
    }

    /// `.permute([...])`: every axis named exactly once, in the order the result wants
    /// them. Each entry is a dimension name or a positional index.
    fn resolve_permute(
        &mut self,
        shape: &[TensorAxis],
        args: &[Expr],
        call_span: Span,
    ) -> Option<Vec<TensorAxis>> {
        let entries = self.shape_arg_entries(args, PERMUTE_METHOD, "1, 0", call_span)?;
        if entries.len() != shape.len() {
            self.record_error(TypeError::TensorPermuteRank {
                found: entries.len(),
                rank: shape.len(),
                span: call_span,
            });
            return None;
        }
        let order = self.resolve_axis_list(shape, entries, PERMUTE_METHOD)?;
        Some(order.iter().map(|axis| shape[*axis].clone()).collect())
    }

    /// `.flatten()` merges every axis; `.flatten(dims: [...])` merges the named run.
    ///
    /// A merged axis loses its name: it is no longer the axis either name documented.
    /// The run has to be adjacent, because merging non-neighbouring axes would move
    /// elements, and flattening is specified as a re-description of the same order.
    fn resolve_flatten(
        &mut self,
        shape: &[TensorAxis],
        args: &[Expr],
        call_span: Span,
    ) -> Option<Vec<TensorAxis>> {
        if args.is_empty() {
            let total = self.static_element_count(shape, FLATTEN_METHOD, call_span)?;
            return Some(vec![TensorAxis {
                name: None,
                extent: ArrayLen::Fixed(total),
            }]);
        }

        let entries = self.shape_arg_entries(args, FLATTEN_METHOD, "rows, cols", call_span)?;
        if entries.is_empty() {
            self.record_error(TypeError::TensorFlattenNoAxes { span: call_span });
            return None;
        }
        let axes = self.resolve_axis_list(shape, entries, FLATTEN_METHOD)?;
        let first = axes[0];
        if axes
            .iter()
            .enumerate()
            .any(|(step, axis)| *axis != first + step)
        {
            self.record_error(TypeError::TensorFlattenNotAdjacent { span: call_span });
            return None;
        }

        let mut merged = 1usize;
        for axis in &axes {
            let ArrayLen::Fixed(extent) = shape[*axis].extent else {
                self.record_error(TypeError::TensorShapeCastSymbolicExtent {
                    method: FLATTEN_METHOD.to_string(),
                    name: shape[*axis].extent.to_string(),
                    span: call_span,
                });
                return None;
            };
            merged *= extent;
        }

        let mut result: Vec<TensorAxis> = shape[..first].to_vec();
        result.push(TensorAxis {
            name: None,
            extent: ArrayLen::Fixed(merged),
        });
        result.extend_from_slice(&shape[first + axes.len()..]);
        Some(result)
    }

    /// Turn each entry of an axis list into a receiver axis index, reporting an unknown
    /// name, an out-of-range index, or a repeat.
    fn resolve_axis_list(
        &mut self,
        shape: &[TensorAxis],
        entries: &[Expr],
        method: &str,
    ) -> Option<Vec<usize>> {
        let mut axes = Vec::with_capacity(entries.len());
        for entry in entries {
            let axis = self.resolve_shape_axis(shape, entry, method)?;
            if axes.contains(&axis) {
                self.record_error(TypeError::TensorAxisRepeated {
                    method: method.to_string(),
                    axis,
                    span: entry.span(),
                });
                return None;
            }
            axes.push(axis);
        }
        Some(axes)
    }

    /// One axis-list entry: a bare identifier is a dimension name, an integer literal a
    /// positional index.
    ///
    /// An identifier here is ALWAYS a dimension name: the names live in a
    /// namespace attached to the tensor type, so a local binding of the same name
    /// neither shadows the axis nor is shadowed by it.
    fn resolve_shape_axis(
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
                example: "0, 1".to_string(),
                span: entry.span(),
            });
            return None;
        };
        let axis = usize::try_from(value).ok().filter(|a| *a < shape.len());
        if axis.is_none() {
            self.record_error(TypeError::TensorAxisOutOfRange {
                axis: value.unsigned_abs() as usize,
                rank: shape.len(),
                span: entry.span(),
            });
        }
        axis
    }

    /// The single array-literal argument these calls take, as its element expressions.
    fn shape_arg_entries<'a>(
        &mut self,
        args: &'a [Expr],
        method: &str,
        example: &str,
        call_span: Span,
    ) -> Option<&'a [Expr]> {
        if !self.check_shape_arity(args, 1, call_span) {
            return None;
        }
        match &args[0] {
            Expr::ArrayLiteral { elements, .. } => Some(elements),
            other => {
                self.record_error(TypeError::TensorShapeArgNotLiteral {
                    method: method.to_string(),
                    example: example.to_string(),
                    span: other.span(),
                });
                None
            }
        }
    }

    /// Report an argument-count mismatch, returning whether the count was right.
    fn check_shape_arity(&mut self, args: &[Expr], expected: usize, call_span: Span) -> bool {
        if args.len() == expected {
            return true;
        }
        self.record_error(TypeError::ArgumentCountMismatch {
            expected,
            found: args.len(),
            span: call_span,
        });
        false
    }

    /// The receiver's element count, which only exists once every extent is concrete.
    fn static_element_count(
        &mut self,
        shape: &[TensorAxis],
        method: &str,
        call_span: Span,
    ) -> Option<usize> {
        let mut total = 1usize;
        for axis in shape {
            let ArrayLen::Fixed(extent) = axis.extent else {
                self.record_error(TypeError::TensorShapeCastSymbolicExtent {
                    method: method.to_string(),
                    name: axis.extent.to_string(),
                    span: call_span,
                });
                return None;
            };
            total *= extent;
        }
        Some(total)
    }
}

/// The value of an integer constant expression written in an axis list: a literal, or a
/// literal under a negation (`-1`) or parentheses.
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

/// The dimension names a shape declares, for the diagnostic that names them: naming a
/// dimension the type does not declare lists the ones it does.
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
