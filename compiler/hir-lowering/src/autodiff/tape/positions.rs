//! Compile-time positions: literal indices, strides and slice sources a tensor read is
//! resolved to, the scalar type predicates, and what a refused construct is called.

use neuro_hir::{HirExpr, HirExprKind, HirStmt, HirTensorAxis, HirType};
use shared_types::{Literal, Span};

use crate::autodiff::emit::tensor_parts;
use crate::{is_full_float, is_integer};

use super::Leaf;

pub(super) fn repeats(letters: &[usize]) -> bool {
    letters
        .iter()
        .enumerate()
        .any(|(at, letter)| letters[at + 1..].contains(letter))
}

pub(super) fn is_scalar(ty: &HirType) -> bool {
    !matches!(
        ty,
        HirType::Tensor { .. }
            | HirType::Reference { .. }
            | HirType::String
            | HirType::Void
            | HirType::Struct(_)
            | HirType::Enum(_)
            | HirType::Tuple(_)
            | HirType::Array { .. }
            | HirType::Collection { .. }
    )
}

/// Whether a value of `ty` has a derivative: a float, or a tensor of floats.
pub(super) fn is_float_valued(ty: &HirType) -> bool {
    match ty.referent() {
        HirType::Tensor { element, .. } => is_full_float(element),
        other => is_full_float(other),
    }
}

/// Whether a slot can hold a value of `ty`: something the derivative function can give a
/// placeholder to and copy, which is an owned float tensor or a plain scalar.
pub(super) fn is_slot_type(ty: &HirType) -> bool {
    match ty {
        HirType::Tensor { element, shape, .. } => {
            is_full_float(element) && shape.iter().all(Option::is_some)
        }
        other => is_full_float(other) || is_integer(other) || *other == HirType::Bool,
    }
}

/// The literal position, one per axis, of the element at row-major offset `flat`.
pub(super) fn coordinates(mut flat: usize, extents: &[usize], span: Span) -> Vec<Leaf> {
    let mut positions = vec![0usize; extents.len()];
    for (position, extent) in positions.iter_mut().zip(extents).rev() {
        *position = flat % extent;
        flat /= extent;
    }
    positions
        .into_iter()
        .map(|position| {
            let literal = Literal::Integer(position as i128, None);
            Leaf::Const(HirExpr::new(
                HirExprKind::Literal(literal),
                HirType::U64,
                span,
            ))
        })
        .collect()
}

/// A `.step(n)` stride the checker has already proved positive, when it is a literal.
/// Only a literal is taken: the replay has no run-time guard to stop a zero stride.
pub(super) fn constant_stride(step: &HirExpr) -> Option<i128> {
    match &step.kind {
        HirExprKind::Literal(Literal::Integer(value, _)) if *value > 0 => Some(*value),
        _ => None,
    }
}

/// The least value of an integer type.
pub(super) fn integer_min(ty: &HirType) -> Option<i128> {
    match ty {
        HirType::I8 => Some(i8::MIN.into()),
        HirType::I16 => Some(i16::MIN.into()),
        HirType::I32 => Some(i32::MIN.into()),
        HirType::I64 => Some(i64::MIN.into()),
        HirType::U8 | HirType::U16 | HirType::U32 | HirType::U64 => Some(0),
        _ => None,
    }
}

/// The value of an integer literal array position.
pub(super) fn literal_position(index: &HirExpr) -> Option<usize> {
    match &index.kind {
        HirExprKind::Literal(Literal::Integer(value, _)) => usize::try_from(*value).ok(),
        _ => None,
    }
}

pub(super) fn literal_index(position: &Leaf) -> Option<Option<usize>> {
    let Leaf::Const(HirExpr {
        kind: HirExprKind::Literal(Literal::Integer(value, _)),
        ..
    }) = position
    else {
        return None;
    };
    Some(usize::try_from(*value).ok())
}

/// For a slice whose every axis is a literal position or a range, the offset in the
/// object of each result element, in the result's row-major order.
pub(super) fn slice_sources(object_ty: &HirType, axes: &[HirTensorAxis]) -> Option<Vec<usize>> {
    let (_, extents) = tensor_parts(object_ty)?;
    if extents.len() != axes.len() {
        return None;
    }
    // Per object axis, the coordinates the result visits along it, in result order.
    let mut visits: Vec<Vec<usize>> = Vec::with_capacity(axes.len());
    for (axis, extent) in axes.iter().zip(&extents) {
        let along = match axis {
            HirTensorAxis::Position(HirExpr {
                kind: HirExprKind::Literal(Literal::Integer(value, _)),
                ..
            }) => vec![usize::try_from(*value)
                .ok()
                .filter(|index| index < extent)?],
            HirTensorAxis::Position(_) => return None,
            HirTensorAxis::Range {
                start,
                end,
                reversed,
                step,
            } => {
                if start > end || end > extent || *step == 0 {
                    return None;
                }
                let mut along: Vec<usize> = (*start..*end).collect();
                if *reversed {
                    along.reverse();
                }
                along.into_iter().step_by(*step).collect()
            }
        };
        visits.push(along);
    }
    let mut sources = vec![0usize];
    for (along, extent) in visits.iter().zip(&extents) {
        sources = sources
            .iter()
            .flat_map(|base| along.iter().map(move |at| base * extent + at))
            .collect();
    }
    Some(sources)
}

pub(super) fn stmt_span(stmt: &HirStmt) -> Span {
    match stmt {
        HirStmt::VarDecl { span, .. }
        | HirStmt::Assign { span, .. }
        | HirStmt::TensorCompoundAssign { span, .. }
        | HirStmt::Return { span, .. }
        | HirStmt::If { span, .. }
        | HirStmt::While { span, .. }
        | HirStmt::ForRange { span, .. }
        | HirStmt::ForEach { span, .. }
        | HirStmt::Break { span, .. }
        | HirStmt::Continue { span, .. }
        | HirStmt::ValElse { span, .. }
        | HirStmt::Const { span, .. } => *span,
        HirStmt::Expr(expr) => expr.span,
    }
}

pub(super) fn describe_stmt(stmt: &HirStmt) -> &'static str {
    match stmt {
        HirStmt::VarDecl { .. } => "a binding without an initializer",
        HirStmt::Assign { .. } | HirStmt::TensorCompoundAssign { .. } => {
            "an assignment to anything but a local binding"
        }
        HirStmt::Return { .. } => "a `return` that does not end the body or an `if` arm",
        HirStmt::If { .. } | HirStmt::While { .. } => "this statement",
        HirStmt::ForRange { .. } | HirStmt::ForEach { .. } => "a `for` loop over a collection",
        HirStmt::Break { .. } | HirStmt::Continue { .. } => "a `break` or `continue`",
        HirStmt::ValElse { .. } => "a `val ... else` binding",
        HirStmt::Const { .. } => "a local `const`",
        HirStmt::Expr(_) => "an expression statement",
    }
}

pub(super) fn describe_expr(expr: &HirExpr) -> &'static str {
    match &expr.kind {
        HirExprKind::Binary { .. } | HirExprKind::Unary { .. } => {
            "an operator on a value that has no derivative"
        }
        HirExprKind::TensorReduce { .. } => "a `.max()` / `.min()` reduction",
        HirExprKind::TensorEinsum { .. } => "an `einsum` contraction",
        HirExprKind::TensorShapeCast { .. } => "a shape change",
        HirExprKind::TensorIndex { .. } => "a tensor slice at a computed position",
        HirExprKind::TensorSort { .. } => "a sort",
        HirExprKind::TensorRandomNormal { .. } => "a random tensor",
        HirExprKind::Cast { .. } => "a cast",
        HirExprKind::Math { .. } => "elementwise math on half-precision elements",
        HirExprKind::FieldAccess { .. } => "a field that is neither a number nor a tensor",
        HirExprKind::Match { .. } => "a `match`",
        HirExprKind::Loop { .. } => "a `loop`",
        HirExprKind::Unsafe { .. } | HirExprKind::Pool { .. } => "an `unsafe` or `pool` block",
        HirExprKind::Reference { .. } | HirExprKind::Deref { .. } => {
            "a borrow of anything but a binding"
        }
        _ => "this expression",
    }
}
