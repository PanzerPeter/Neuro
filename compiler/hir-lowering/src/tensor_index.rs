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
use neuro_hir::{AxisNames, HirExpr, HirExprKind, HirTensorAxis, HirType};
use shared_types::{Literal, Span};

use crate::{Lowerer, LoweringError};

impl Lowerer {
    /// Lower `object[a0, a1, ...]` over a tensor, given the already-lowered receiver.
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
                })
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

        let mut axes = Vec::with_capacity(indices.len());
        let mut kept = Vec::new();
        // A surviving axis is still the axis its name documented, so a sub-range of
        // `height` keeps that name; an axis given a position disappears with it.
        let mut kept_names = Vec::new();
        let extents = crate::static_extents(&shape)?;
        for (position, (index, extent)) in indices.iter().zip(extents.iter()).enumerate() {
            let axis = self.lower_tensor_axis(index, *extent)?;
            if let HirTensorAxis::Range { start, end } = &axis {
                kept.push(end - start);
                kept_names.push(names.0.get(position).cloned().flatten());
            }
            axes.push(axis);
        }

        let ty = if kept.is_empty() {
            element
        } else {
            HirType::Tensor {
                element: Box::new(element),
                shape: neuro_hir::static_shape(&kept),
                names: AxisNames(kept_names),
            }
        };
        Ok(HirExpr::new(
            HirExprKind::TensorIndex {
                object: Box::new(object),
                axes,
            },
            ty,
            span,
        ))
    }

    fn lower_tensor_axis(
        &mut self,
        index: &TensorIndexArg,
        extent: usize,
    ) -> Result<HirTensorAxis, LoweringError> {
        match index {
            TensorIndexArg::FullAxis(_) => Ok(HirTensorAxis::Range {
                start: 0,
                end: extent,
            }),
            TensorIndexArg::Position(expr) => {
                Ok(HirTensorAxis::Position(self.lower_expr(expr, None)?))
            }
            TensorIndexArg::Range {
                start,
                end,
                inclusive,
                ..
            } => {
                let start = const_int(start)?;
                let end = const_int(end)?;
                // An inclusive range names its last position, so it stops one further on.
                let end = if *inclusive { end + 1 } else { end };
                Ok(HirTensorAxis::Range { start, end })
            }
        }
    }
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
                BinaryOp::Add => Some(left + right),
                BinaryOp::Subtract => Some(left - right),
                BinaryOp::Multiply => Some(left * right),
                BinaryOp::Divide if right != 0 => Some(left / right),
                BinaryOp::Modulo if right != 0 => Some(left % right),
                _ => None,
            }
        }
        _ => None,
    }
}
