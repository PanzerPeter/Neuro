//! The order-based tensor selections `.sort()`, `.argsort()`, and `.topk()`.
//!
//! Reached from the method-call arm of `lower_expr`, before the arguments are lowered:
//! an `axis:` argument may be a dimension NAME, which resolves against the receiver's own
//! shape and would be an unresolved variable if it reached `lower_expr`; `k:` and
//! `descending:` are compile-time choices that never become runtime values at all.
//!
//! The type checker has already validated the call, so an argument that does not resolve
//! here is a divergence between the two surfaces and becomes a `LoweringError`, as this
//! slice's CONTEXT.md requires.

use ast_types::{Expr, UnaryOp};
use neuro_hir::{AxisNames, HirExpr, HirExprKind, HirSortKind, HirType};
use shared_types::{Literal, Span};

use crate::{Lowerer, LoweringError};

pub(crate) const SORT_METHOD: &str = "sort";
pub(crate) const ARGSORT_METHOD: &str = "argsort";
pub(crate) const TOPK_METHOD: &str = "topk";

/// The element type of the index tensors `.argsort` and `.topk` produce.
const INDEX_ELEMENT: HirType = HirType::I32;

/// Whether `method` names one of the three order-based selections.
pub(crate) fn is_sort_method(method: &str) -> bool {
    matches!(method, SORT_METHOD | ARGSORT_METHOD | TOPK_METHOD)
}

fn malformed(detail: String) -> LoweringError {
    LoweringError::Malformed { detail }
}

impl Lowerer {
    /// Lower one selection into a [`HirExprKind::TensorSort`].
    ///
    /// The receiver is left as it is: a selection reads the buffer it orders, so unlike a
    /// shape cast it moves nothing and the receiver keeps its binding.
    pub(crate) fn lower_tensor_sort(
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
        if shape.is_empty() {
            return Err(malformed(format!(
                "`.{method}` reached lowering on a rank-0 tensor"
            )));
        }

        if method == TOPK_METHOD {
            return self.lower_topk(receiver, *element, &shape, &names, args, span);
        }
        let axis = resolve_axis(&shape, &names, args.first(), method)?;
        let descending = match args.get(1) {
            Some(entry) => const_boolean(entry).ok_or_else(|| {
                malformed(format!(
                    "`.{method}` reached lowering with a non-constant `descending:`"
                ))
            })?,
            None => false,
        };
        let (kind, element) = if method == ARGSORT_METHOD {
            (HirSortKind::Indices, Box::new(INDEX_ELEMENT))
        } else {
            (HirSortKind::Values, element)
        };
        Ok(HirExpr::new(
            HirExprKind::TensorSort {
                receiver: Box::new(receiver),
                kind,
                axis,
                descending,
            },
            HirType::Tensor {
                element,
                shape: neuro_hir::static_shape(&shape),
                names,
            },
            span,
        ))
    }

    /// `.topk(k:, axis:)`, whose type is the values/indices pair the language gives it.
    fn lower_topk(
        &mut self,
        receiver: HirExpr,
        element: HirType,
        shape: &[usize],
        names: &AxisNames,
        args: &[Expr],
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let k = args
            .first()
            .and_then(const_integer)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| malformed("`.topk` reached lowering with a non-constant `k:`".into()))?;
        let axis = resolve_axis(shape, names, args.get(1), TOPK_METHOD)?;

        let mut selected = shape.to_vec();
        selected[axis] = k;
        let mut selected_names = names.0.clone();
        selected_names[axis] = None;
        let tensor = |element: HirType| HirType::Tensor {
            element: Box::new(element),
            shape: neuro_hir::static_shape(&selected),
            names: AxisNames(selected_names.clone()),
        };
        Ok(HirExpr::new(
            HirExprKind::TensorSort {
                receiver: Box::new(receiver),
                kind: HirSortKind::TopK(k),
                axis,
                // Top-k is the k GREATEST, so the ordering it takes the head of is the
                // descending one. It takes no `descending:` label for that reason.
                descending: true,
            },
            HirType::Tuple(vec![tensor(element), tensor(INDEX_ELEMENT)]),
            span,
        ))
    }
}

/// The receiver axis an `axis:` argument names: a dimension name, a non-negative index,
/// or a negative index counting from the end. The last axis when the call names none.
fn resolve_axis(
    shape: &[usize],
    names: &AxisNames,
    entry: Option<&Expr>,
    method: &str,
) -> Result<usize, LoweringError> {
    let Some(entry) = entry else {
        return Ok(shape.len() - 1);
    };
    if let Expr::Identifier(ident) = entry {
        return names.position_of(&ident.name).ok_or_else(|| {
            malformed(format!(
                "`.{method}` reached lowering naming dimension '{}', which its receiver's \
                 type does not declare",
                ident.name
            ))
        });
    }
    let value = const_integer(entry).ok_or_else(|| {
        malformed(format!(
            "`.{method}` reached lowering with a non-constant axis"
        ))
    })?;
    let rank = i128::try_from(shape.len()).map_err(|_| {
        malformed(format!(
            "`.{method}` reached lowering on an unrepresentable rank"
        ))
    })?;
    let resolved = if value < 0 { value + rank } else { value };
    usize::try_from(resolved)
        .ok()
        .filter(|axis| *axis < shape.len())
        .ok_or_else(|| {
            malformed(format!(
                "`.{method}` reached lowering with axis {value} out of range"
            ))
        })
}

/// The value of an integer constant expression written as an argument.
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
