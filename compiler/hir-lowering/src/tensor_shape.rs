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

use std::collections::HashMap;

use ast_types::{BinaryOp, Expr, UnaryOp};
use neuro_hir::{AxisNames, HirExpr, HirExprKind, HirType};
use shared_types::{Literal, Span};

use crate::{Lowerer, LoweringError, instance_shape, malformed};

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

        let shape = crate::static_extents(&shape)?;
        let cast = match method {
            TRANSPOSE_METHOD => transpose(&shape, &names)?,
            RESHAPE_METHOD => reshape(
                &shape,
                args,
                &|expr| self.reshape_extent(expr, CONST_FOLD_DEPTH),
                span,
            )?,
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
                shape: neuro_hir::static_shape(&cast.shape),
                names: cast.names,
            },
            span,
        ))
    }
}

impl Lowerer {
    /// A `.reshape` extent's value, mirroring the checker: a literal, a constant or an
    /// extent parameter of the instance, or arithmetic over these. A local binding
    /// shadows both and is a run-time value. Checked, like the checker's fold.
    fn reshape_extent(&self, expr: &Expr, depth: u32) -> Option<i128> {
        let depth = depth.checked_sub(1)?;
        match expr {
            Expr::Literal(Literal::Integer(value, _), _) => Some(*value),
            Expr::Paren(inner, _) => self.reshape_extent(inner, depth),
            Expr::Unary {
                op: UnaryOp::Negate,
                operand,
                ..
            } => self.reshape_extent(operand, depth)?.checked_neg(),
            Expr::Identifier(ident) => {
                if self.lookup_local(&ident.name).is_some() {
                    return None;
                }
                if let Some(value) = self.const_subst.get(&ident.name) {
                    return Some(i128::from(*value));
                }
                self.reshape_extent(self.constant_values.get(&ident.name)?, depth)
            }
            Expr::Binary {
                left, op, right, ..
            } => {
                let l = self.reshape_extent(left, depth)?;
                let r = self.reshape_extent(right, depth)?;
                match op {
                    BinaryOp::Add => l.checked_add(r),
                    BinaryOp::Subtract => l.checked_sub(r),
                    BinaryOp::Multiply => l.checked_mul(r),
                    BinaryOp::Divide if l >= 0 && r > 0 => l.checked_div(r),
                    BinaryOp::Modulo if l >= 0 && r > 0 => l.checked_rem(r),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// How many constants deep an extent folds; the checker uses the same bound.
const CONST_FOLD_DEPTH: u32 = 64;

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

/// `.reshape`'s result shape. Over a concrete receiver the checker has validated every
/// count; inside a shape-generic instance this is where they are first known, so a count
/// that does not work out is the language's per-instance error, not a checker escape.
fn reshape(
    shape: &[usize],
    args: &[Expr],
    extent: &dyn Fn(&Expr) -> Option<i128>,
    span: Span,
) -> Result<ShapeCast, LoweringError> {
    let total: usize = shape.iter().product();
    let entries = axis_list(args, RESHAPE_METHOD)?;

    let mut extents: Vec<Option<usize>> = Vec::with_capacity(entries.len());
    let mut inferred_at = None;
    for entry in entries {
        let value = extent(entry);
        if value == Some(INFERRED_EXTENT) {
            inferred_at = Some(extents.len());
            extents.push(None);
            continue;
        }
        let Some(value) = value.and_then(|value| usize::try_from(value).ok()) else {
            return Err(instance_shape(
                "a `.reshape` extent has no value at this instance: it divides by zero or is \
                 negative, and an axis has a non-negative extent"
                    .to_string(),
                entry.span(),
            ));
        };
        extents.push(Some(value));
    }

    let known: usize = extents.iter().flatten().product();
    if let Some(position) = inferred_at {
        if known == 0 || !total.is_multiple_of(known) {
            return Err(instance_shape(
                format!(
                    "`-1` cannot be inferred at this instance: the other extents multiply to \
                     {known}, which does not divide the receiver's {total} elements"
                ),
                span,
            ));
        }
        extents[position] = Some(total / known);
    }

    let resolved: Vec<usize> = extents.into_iter().flatten().collect();
    let count: usize = resolved.iter().product();
    if count != total {
        return Err(instance_shape(
            format!(
                "`.reshape` would hold {count} elements at this instance but the receiver \
                 holds {total}; a reshape cannot change how many elements a tensor has"
            ),
            span,
        ));
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

/// The value of an extent expression written in a type, `D / H`, under the instance's
/// extents. `None` where it has none: a zero divisor, or a negative operand of `/` or
/// `%`, which no extent is. The caller refuses a negative result. `count` is a shape
/// parameter's element count, for `S.count()`.
pub(crate) fn extent_value(
    expr: &Expr,
    const_subst: &HashMap<String, u64>,
    count: &dyn Fn(&str) -> Option<i128>,
) -> Option<i128> {
    match expr {
        Expr::Literal(Literal::Integer(value, _), _) => Some(*value),
        Expr::Identifier(ident) => const_subst.get(&ident.name).map(|v| i128::from(*v)),
        Expr::Paren(inner, _) => extent_value(inner, const_subst, count),
        Expr::Call { func, .. } => match &**func {
            Expr::FieldAccess { object, .. } => match &**object {
                Expr::Identifier(shape) => count(&shape.name),
                _ => None,
            },
            _ => None,
        },
        Expr::Binary {
            left, op, right, ..
        } => {
            let l = extent_value(left, const_subst, count)?;
            let r = extent_value(right, const_subst, count)?;
            match op {
                BinaryOp::Add => l.checked_add(r),
                BinaryOp::Subtract => l.checked_sub(r),
                BinaryOp::Multiply => l.checked_mul(r),
                BinaryOp::Divide if l >= 0 && r > 0 => l.checked_div(r),
                BinaryOp::Modulo if l >= 0 && r > 0 => l.checked_rem(r),
                _ => None,
            }
        }
        _ => None,
    }
}

/// An extent expression as the program spells it, for a diagnostic.
pub(crate) fn extent_text(expr: &Expr) -> String {
    match expr {
        Expr::Literal(Literal::Integer(value, _), _) => value.to_string(),
        Expr::Identifier(ident) => ident.name.clone(),
        Expr::Paren(inner, _) => format!("({})", extent_text(inner)),
        Expr::Call { func, .. } => match &**func {
            Expr::FieldAccess { object, .. } => format!("{}.count()", extent_text(object)),
            _ => "?".to_string(),
        },
        Expr::Binary {
            left, op, right, ..
        } => {
            let symbol = match op {
                BinaryOp::Add => "+",
                BinaryOp::Subtract => "-",
                BinaryOp::Multiply => "*",
                BinaryOp::Divide => "/",
                _ => "%",
            };
            format!("{} {symbol} {}", extent_text(left), extent_text(right))
        }
        _ => "?".to_string(),
    }
}

/// The value of an integer constant expression written in an axis list.
pub(crate) fn const_integer(expr: &Expr) -> Option<i128> {
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
