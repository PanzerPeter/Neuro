//! Lowering for tensor indexing and slicing `t[i, j]` / `t[1..3, ..]`.
//!
//! Reached from the two index arms of `lower_expr_uncoerced`: the multi-axis spelling
//! arrives as `Expr::TensorIndex`, and the one-argument `t[i]` as the ordinary
//! `Expr::Index` whose object lowered to a tensor.
//!
//! The type checker has already validated rank, integer positions, and the constant
//! bounds of every range, so the work here is folding each range to the pair the
//! backend walks and deciding the result type: an element when every axis is a
//! position, a tensor of the surviving extents otherwise.

use ast_types::{BinaryOp, Expr, TensorIndexArg};
use neuro_hir::{AxisNames, HirExpr, HirExprKind, HirStmt, HirTensorAxis, HirType};
use shared_types::{Literal, Span};

use crate::extents::{
    INDEX_OUT_OF_BOUNDS, binary, borrow, extent_read, guard, panic_text, u64_value, variable,
};
use crate::{Lowerer, LoweringError};

impl Lowerer {
    /// Lower `object[a0, a1, ...]` over a tensor, given the already-lowered receiver.
    ///
    /// A position or a constant range on a dynamic axis is checked at run time against the
    /// extent the object has there, before the read: the check and the read share one block,
    /// and an object that is not a place is bound first so both see the same tensor.
    pub(crate) fn lower_tensor_index(
        &mut self,
        object: HirExpr,
        indices: &[TensorIndexArg],
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let (element, shape, names) = match object.ty.referent() {
            HirType::Tensor {
                element,
                shape,
                names,
            } => ((**element).clone(), shape.clone(), names.clone()),
            other => {
                return Err(LoweringError::Malformed {
                    detail: format!("tensor index over non-tensor type '{other}'"),
                });
            }
        };
        if indices.len() != shape.len() {
            return Err(LoweringError::Malformed {
                detail: format!(
                    "tensor index names {} axes for a rank-{} tensor",
                    indices.len(),
                    shape.len()
                ),
            });
        }

        let checked = indices.iter().zip(&shape).any(|(index, extent)| {
            extent.is_none() && !matches!(index, TensorIndexArg::FullAxis(_))
        });
        let mut prelude = Vec::new();
        let object = if checked && !is_place(&object) {
            let temp = self.fresh_extent_temp();
            let ty = object.ty.clone();
            prelude.push(HirStmt::VarDecl {
                name: temp.clone(),
                ty: ty.clone(),
                init: Some(object),
                mutable: false,
                span,
            });
            variable(&temp, ty, span)
        } else {
            object
        };

        let mut axes = Vec::with_capacity(indices.len());
        let mut kept: Vec<Option<usize>> = Vec::new();
        // A surviving axis is still the axis its name documented, so a sub-range of
        // `height` keeps that name; an axis given a position disappears with it.
        let mut kept_names = Vec::new();
        let mut checks = Vec::new();
        for (position, (index, extent)) in indices.iter().zip(shape.iter()).enumerate() {
            let axis = match (index, extent) {
                // A position on a dynamic axis is bound once, so the check and the read agree.
                (TensorIndexArg::Position(expr), None) => {
                    let value = self.lower_expr(expr, None)?;
                    let ty = value.ty.clone();
                    let temp = self.fresh_extent_temp();
                    prelude.push(HirStmt::VarDecl {
                        name: temp.clone(),
                        ty: ty.clone(),
                        init: Some(value),
                        mutable: false,
                        span,
                    });
                    HirTensorAxis::Position(variable(&temp, ty, span))
                }
                _ => self.lower_tensor_axis(index, *extent)?,
            };
            if extent.is_none()
                && let Some(check) = dynamic_axis_check(&axis, &object, position, span)
            {
                checks.push(check);
            }
            match (&axis, extent) {
                (
                    HirTensorAxis::Range {
                        start, end, step, ..
                    },
                    _,
                ) => {
                    kept.push(Some((end - start).div_ceil(*step)));
                    kept_names.push(names.0.get(position).cloned().flatten());
                }
                (HirTensorAxis::Whole, _) => {
                    kept.push(None);
                    kept_names.push(names.0.get(position).cloned().flatten());
                }
                (HirTensorAxis::Position(_), _) => {}
            }
            axes.push(axis);
        }

        let ty = if kept.is_empty() {
            element.clone()
        } else {
            HirType::Tensor {
                element: Box::new(element),
                shape: kept,
                names: AxisNames(kept_names),
            }
        };
        let read = HirExpr::new(
            HirExprKind::TensorIndex {
                object: Box::new(object),
                axes,
            },
            ty.clone(),
            span,
        );
        if prelude.is_empty() && checks.is_empty() {
            return Ok(read);
        }
        let mut stmts = prelude;
        stmts.extend(checks);
        stmts.push(HirStmt::Expr(read));
        Ok(HirExpr::new(HirExprKind::Block { stmts }, ty, span))
    }

    /// One axis of an index. A static axis of `extent` is covered by its range; a dynamic
    /// one (`None`) keeps `..` as the whole axis, and takes a constant range with its bounds
    /// checked where the caller holds the object.
    pub(crate) fn lower_tensor_axis(
        &mut self,
        index: &TensorIndexArg,
        extent: Option<usize>,
    ) -> Result<HirTensorAxis, LoweringError> {
        match index {
            TensorIndexArg::FullAxis(_) => Ok(match extent {
                Some(extent) => HirTensorAxis::Range {
                    start: 0,
                    end: extent,
                    reversed: false,
                    step: 1,
                },
                None => HirTensorAxis::Whole,
            }),
            TensorIndexArg::Position(expr) => {
                Ok(HirTensorAxis::Position(self.lower_expr(expr, None)?))
            }
            TensorIndexArg::Range {
                start,
                end,
                inclusive,
                reversed,
                step,
                ..
            } => {
                let start = const_int(start)?;
                let end = const_int(end)?;
                // An inclusive range names its last position, so it stops one further on.
                let end = if *inclusive { end + 1 } else { end };
                let step = match step {
                    None => 1,
                    Some(step) => {
                        const_int(step)
                            .ok()
                            .filter(|step| *step > 0)
                            .ok_or_else(|| LoweringError::Malformed {
                                detail: "a tensor slice stride is not a positive constant"
                                    .to_string(),
                            })?
                    }
                };
                Ok(HirTensorAxis::Range {
                    start,
                    end,
                    reversed: *reversed,
                    step,
                })
            }
        }
    }
}

/// The run-time check of one axis of an index over a dynamic extent: a position must lie
/// inside the axis, and a constant range must end inside it. `object` is read as a borrow,
/// so the check consumes nothing; `axis_index` is the axis's position in the object's shape.
/// `None` for a whole axis, which needs no check.
pub(crate) fn dynamic_axis_check(
    axis: &HirTensorAxis,
    object: &HirExpr,
    axis_index: usize,
    span: Span,
) -> Option<HirStmt> {
    let bound = extent_read(borrow(object.clone()), axis_index, span);
    let inside = match axis {
        HirTensorAxis::Position(position) => {
            let widened = match &position.ty {
                HirType::U64 => position.clone(),
                _ => HirExpr::new(
                    HirExprKind::Cast {
                        value: Box::new(position.clone()),
                    },
                    HirType::U64,
                    span,
                ),
            };
            binary(BinaryOp::Less, widened, bound, HirType::Bool, span)
        }
        HirTensorAxis::Range { end, .. } => binary(
            BinaryOp::LessEqual,
            u64_value(*end as u64, span),
            bound,
            HirType::Bool,
            span,
        ),
        HirTensorAxis::Whole => return None,
    };
    Some(guard(inside, panic_text(INDEX_OUT_OF_BOUNDS, span), span))
}

/// Whether an expression names a location, so that reading it again reads the same place
/// without moving out of it.
pub(crate) fn is_place(expr: &HirExpr) -> bool {
    matches!(
        expr.kind,
        HirExprKind::Variable(_)
            | HirExprKind::FieldAccess { .. }
            | HirExprKind::TupleIndex { .. }
            | HirExprKind::NewtypeAccess { .. }
            | HirExprKind::Index { .. }
            | HirExprKind::Deref { .. }
    )
}

/// Fold a slice bound to the constant the checker already proved it to be.
fn const_int(expr: &Expr) -> Result<usize, LoweringError> {
    fold(expr)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| LoweringError::Malformed {
            detail: "a tensor slice bound is not a constant".to_string(),
        })
}

fn fold(expr: &Expr) -> Option<i128> {
    match expr {
        Expr::Literal(Literal::Integer(value, _), _) => Some(*value),
        Expr::Paren(inner, _) => fold(inner),
        Expr::Binary {
            left, op, right, ..
        } => {
            let left = fold(left)?;
            let right = fold(right)?;
            match op {
                // Checked: an overflowing bound folds to nothing, refused as no constant.
                BinaryOp::Add => left.checked_add(right),
                BinaryOp::Subtract => left.checked_sub(right),
                BinaryOp::Multiply => left.checked_mul(right),
                BinaryOp::Divide => left.checked_div(right),
                BinaryOp::Modulo => left.checked_rem(right),
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::const_int;
    use ast_types::{BinaryOp, Expr};
    use shared_types::{Literal, Span};

    fn literal(value: i128) -> Expr {
        Expr::Literal(Literal::Integer(value, None), Span::new(0, 0))
    }

    #[test]
    fn an_overflowing_slice_bound_is_refused_not_folded() {
        let product = Expr::Binary {
            left: Box::new(literal(i128::MAX)),
            op: BinaryOp::Multiply,
            right: Box::new(literal(2)),
            span: Span::new(0, 0),
        };
        assert!(const_int(&product).is_err());
    }
}
