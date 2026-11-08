//! Slice ranges, array literals, and tuple literals.
//!
//! Reached from the `lower_expr_uncoerced` dispatch in this module's `mod.rs`.
//! Every file here adds methods to the same `impl Lowerer` block.

use ast_types::Expr;
use neuro_hir::{HirExpr, HirExprKind, HirType};

use crate::{Lowerer, LoweringError};

impl Lowerer {
    /// Lower the `start..end` / `start..=end` argument of a slice intrinsic, which reads
    /// the bounds off the node: it carries `void`, and its bounds are `u64`-typed. A range
    /// anywhere else is a `Range` value.
    pub(super) fn lower_slice_range(&mut self, arg: &Expr) -> Result<HirExpr, LoweringError> {
        let Expr::Range {
            start,
            end,
            inclusive,
            span,
        } = arg
        else {
            return Err(LoweringError::Malformed {
                detail: "a slice intrinsic expects a range argument".to_string(),
            });
        };
        let start = self.lower_expr(start, Some(&HirType::U64))?;
        let end = self.lower_expr(end, Some(&HirType::U64))?;
        Ok(HirExpr::new(
            HirExprKind::Range {
                start: Box::new(start),
                end: Box::new(end),
                inclusive: *inclusive,
            },
            HirType::Void,
            *span,
        ))
    }

    pub(super) fn lower_array_literal(
        &mut self,
        elements: &[Expr],
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let expected_element = match expected {
            Some(HirType::Array { element, .. }) => Some((**element).clone()),
            _ => None,
        };

        if elements.is_empty() {
            let ty = match expected {
                Some(HirType::Array { element, size }) => HirType::Array {
                    element: element.clone(),
                    size: *size,
                },
                _ => {
                    return Err(LoweringError::Malformed {
                        detail: "cannot infer element type of empty array literal".to_string(),
                    });
                }
            };
            return Ok(HirExpr::new(
                HirExprKind::ArrayLiteral { elements: vec![] },
                ty,
                span,
            ));
        }

        let first = self.lower_expr(&elements[0], expected_element.as_ref())?;
        let element_ty = first.ty.clone();
        let mut lowered = Vec::with_capacity(elements.len());
        lowered.push(first);
        for el in &elements[1..] {
            lowered.push(self.lower_expr(el, Some(&element_ty))?);
        }

        Ok(HirExpr::new(
            HirExprKind::ArrayLiteral { elements: lowered },
            HirType::Array {
                element: Box::new(element_ty),
                size: elements.len(),
            },
            span,
        ))
    }

    pub(super) fn lower_tuple_literal(
        &mut self,
        elements: &[Expr],
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let expected_elems = match expected {
            Some(HirType::Tuple(es)) if es.len() == elements.len() => Some(es.clone()),
            _ => None,
        };
        let mut lowered = Vec::with_capacity(elements.len());
        let mut tys = Vec::with_capacity(elements.len());
        for (i, el) in elements.iter().enumerate() {
            let hint = expected_elems.as_ref().map(|es| &es[i]);
            let el = self.lower_expr(el, hint)?;
            tys.push(el.ty.clone());
            lowered.push(el);
        }
        Ok(HirExpr::new(
            HirExprKind::TupleLiteral { elements: lowered },
            HirType::Tuple(tys),
            span,
        ))
    }
}
