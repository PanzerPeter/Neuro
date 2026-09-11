//! The shape-manipulation methods `.t()`, `.reshape(...)`, `.permute(...)`, and
//! `.flatten(...)`.
//!
//! Reached from the method-call arm of `lower_expr`, before the arguments are lowered:
//! these calls read their arguments as syntax, and a `.permute` axis may be a dimension
//! NAME, which resolves against the receiver's own shape and would be an unresolved
//! variable if it reached `lower_expr`. The receiver's names ride on its
//! [`neuro_hir::HirType::Tensor`], which is why the HIR carries them at all.
//!
//! The type checker has already validated every one of these calls, so a shape that does
//! not work out here is a divergence between the two surfaces and becomes a
//! `LoweringError`, exactly as the governing rule in this slice's CONTEXT.md requires.

use ast_types::{Expr, UnaryOp};
use neuro_hir::{AxisNames, HirExpr, HirExprKind, HirType};
use shared_types::{Literal, Span};

use crate::{Lowerer, LoweringError};

pub(crate) const TRANSPOSE_METHOD: &str = "t";
pub(crate) const RESHAPE_METHOD: &str = "reshape";
pub(crate) const PERMUTE_METHOD: &str = "permute";
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

/// The result shape and, when the element order changes, where each result axis came
/// from.
struct ShapeCast {
    shape: Vec<usize>,
    names: AxisNames,
    permutation: Option<Vec<usize>>,
}

fn malformed(detail: String) -> LoweringError {
    LoweringError::Malformed { detail }
}

impl Lowerer {
    /// Lower one shape-manipulation call into a [`HirExprKind::TensorShapeCast`].
    pub(crate) fn lower_tensor_shape_cast(
        &mut self,
        receiver: HirExpr,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let HirType::Tensor {
            element,
            shape,
            names,
        } = receiver.ty.referent().clone()
        else {
            return Err(malformed(format!(
                "`.{method}` reached lowering on a non-tensor receiver"
            )));
        };

        let cast = match method {
            TRANSPOSE_METHOD => transpose(&shape, &names)?,
            RESHAPE_METHOD => reshape(&shape, args)?,
            PERMUTE_METHOD => permute(&shape, &names, args)?,
            FLATTEN_METHOD => flatten(&shape, &names, args)?,
            other => return Err(malformed(format!("`.{other}` is not a shape method"))),
        };

        Ok(HirExpr::new(
            HirExprKind::TensorShapeCast {
                receiver: Box::new(receiver),
                permutation: cast.permutation,
            },
            HirType::Tensor {
                element,
                shape: cast.shape,
                names: cast.names,
            },
            span,
        ))
    }
}

fn transpose(shape: &[usize], names: &AxisNames) -> Result<ShapeCast, LoweringError> {
    if shape.len() != 2 {
        return Err(malformed(format!(
            "`.t()` reached lowering on a rank-{} tensor",
            shape.len()
        )));
    }
    Ok(ShapeCast {
        shape: vec![shape[1], shape[0]],
        names: reorder_names(names, &[1, 0]),
        permutation: Some(vec![1, 0]),
    })
}

fn reshape(shape: &[usize], args: &[Expr]) -> Result<ShapeCast, LoweringError> {
    let total: usize = shape.iter().product();
    let entries = axis_list(args, RESHAPE_METHOD)?;

    let mut extents: Vec<Option<usize>> = Vec::with_capacity(entries.len());
    let mut inferred_at = None;
    for entry in entries {
        let value = const_integer(entry).ok_or_else(|| {
            malformed("`.reshape` reached lowering with a non-constant extent".to_string())
        })?;
        if value == INFERRED_EXTENT {
            inferred_at = Some(extents.len());
            extents.push(None);
            continue;
        }
        let extent = usize::try_from(value)
            .map_err(|_| malformed("`.reshape` reached lowering with a negative extent".into()))?;
        extents.push(Some(extent));
    }

    let known: usize = extents.iter().flatten().product();
    if let Some(position) = inferred_at {
        if known == 0 || total % known != 0 {
            return Err(malformed(
                "`.reshape` reached lowering with an extent that cannot be inferred".to_string(),
            ));
        }
        extents[position] = Some(total / known);
    }

    let resolved: Vec<usize> = extents.into_iter().flatten().collect();
    if resolved.iter().product::<usize>() != total {
        return Err(malformed(format!(
            "`.reshape` reached lowering changing {total} elements into {}",
            resolved.iter().product::<usize>()
        )));
    }
    Ok(ShapeCast {
        names: AxisNames(vec![None; resolved.len()]),
        shape: resolved,
        permutation: None,
    })
}

fn permute(shape: &[usize], names: &AxisNames, args: &[Expr]) -> Result<ShapeCast, LoweringError> {
    let entries = axis_list(args, PERMUTE_METHOD)?;
    let order = resolve_axes(shape, names, entries, PERMUTE_METHOD)?;
    if order.len() != shape.len() {
        return Err(malformed(format!(
            "`.permute` reached lowering with {} axes for a rank-{} tensor",
            order.len(),
            shape.len()
        )));
    }
    Ok(ShapeCast {
        shape: order.iter().map(|axis| shape[*axis]).collect(),
        names: reorder_names(names, &order),
        permutation: Some(order),
    })
}

fn flatten(shape: &[usize], names: &AxisNames, args: &[Expr]) -> Result<ShapeCast, LoweringError> {
    if args.is_empty() {
        return Ok(ShapeCast {
            shape: vec![shape.iter().product()],
            names: AxisNames(vec![None]),
            permutation: None,
        });
    }
    let entries = axis_list(args, FLATTEN_METHOD)?;
    let axes = resolve_axes(shape, names, entries, FLATTEN_METHOD)?;
    let first = *axes
        .first()
        .ok_or_else(|| malformed("`.flatten` reached lowering with no axes".to_string()))?;
    if axes.iter().enumerate().any(|(step, a)| *a != first + step) {
        return Err(malformed(
            "`.flatten` reached lowering with non-adjacent axes".to_string(),
        ));
    }

    let merged: usize = shape[first..first + axes.len()].iter().product();
    let mut result = shape[..first].to_vec();
    let mut result_names = names.0[..first].to_vec();
    result.push(merged);
    result_names.push(None);
    result.extend_from_slice(&shape[first + axes.len()..]);
    result_names.extend_from_slice(&names.0[first + axes.len()..]);
    Ok(ShapeCast {
        shape: result,
        names: AxisNames(result_names),
        permutation: None,
    })
}

/// The elements of the single array-literal argument these calls take.
fn axis_list<'a>(args: &'a [Expr], method: &str) -> Result<&'a [Expr], LoweringError> {
    match args {
        [Expr::ArrayLiteral { elements, .. }] => Ok(elements),
        _ => Err(malformed(format!(
            "`.{method}` reached lowering without its axis-list literal"
        ))),
    }
}

/// Each entry of an axis list as a receiver axis index: an identifier is a dimension
/// name, an integer literal a position.
fn resolve_axes(
    shape: &[usize],
    names: &AxisNames,
    entries: &[Expr],
    method: &str,
) -> Result<Vec<usize>, LoweringError> {
    let mut axes = Vec::with_capacity(entries.len());
    for entry in entries {
        let axis = match entry {
            Expr::Identifier(ident) => names.position_of(&ident.name).ok_or_else(|| {
                malformed(format!(
                    "`.{method}` reached lowering naming dimension '{}', which its receiver's \
                     type does not declare",
                    ident.name
                ))
            })?,
            other => {
                let value = const_integer(other).ok_or_else(|| {
                    malformed(format!(
                        "`.{method}` reached lowering with a non-constant axis"
                    ))
                })?;
                usize::try_from(value)
                    .ok()
                    .filter(|a| *a < shape.len())
                    .ok_or_else(|| {
                        malformed(format!(
                            "`.{method}` reached lowering with axis {value} out of range"
                        ))
                    })?
            }
        };
        axes.push(axis);
    }
    Ok(axes)
}

/// The receiver's names, gathered in the result's axis order.
fn reorder_names(names: &AxisNames, order: &[usize]) -> AxisNames {
    AxisNames(
        order
            .iter()
            .map(|axis| names.0.get(*axis).cloned().flatten())
            .collect(),
    )
}

/// The value of an integer constant expression written in an axis list.
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
