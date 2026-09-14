//! Type derivation shared by the expression dispatch: the unsizing coercions,
//! contextual literal typing, and the result type of a binary operator.

use neuro_hir::{AxisNames, HirExpr, HirExprKind, HirType};
use shared_types::Literal;

use crate::types::{float_suffix_type, int_suffix_type};
use crate::{is_full_float, is_integer, peels_to_string, LoweringError};
use ast_types::BinaryOp;

/// Wrap a reference in whichever unsizing coercion the expected type calls for:
/// `&T` → `&dyn Trait`, or `&[T; N]` / `&Vec<T>` → `&[T]`.
///
/// These are the language's only implicit conversions, so they are applied at exactly
/// one place: every context that supplies an expected type routes through here. The
/// checker has already verified the conversion is legal, so nothing is re-derived.
pub(super) fn apply_unsizing_coercion(expr: HirExpr, expected: Option<&HirType>) -> HirExpr {
    let Some(HirType::Reference {
        inner: expected_inner,
        mutable,
    }) = expected
    else {
        return expr;
    };
    // Only a reference can be unsized, and one that already has the target referent
    // shape is the coercion's own output; re-wrapping it would double the conversion.
    let HirType::Reference { inner: found, .. } = &expr.ty else {
        return expr;
    };
    let target = HirType::Reference {
        inner: expected_inner.clone(),
        mutable: *mutable,
    };
    let span = expr.span;
    match (expected_inner.as_ref(), found.as_ref()) {
        (HirType::DynObject(_), HirType::DynObject(_)) => expr,
        (HirType::DynObject(_), _) => HirExpr::new(
            HirExprKind::DynCoerce {
                value: Box::new(expr),
            },
            target,
            span,
        ),
        (HirType::Slice(_), HirType::Slice(_)) => expr,
        (HirType::Slice(_), _) => HirExpr::new(
            HirExprKind::SliceCoerce {
                value: Box::new(expr),
            },
            target,
            span,
        ),
        _ => expr,
    }
}

/// The resolved type of a literal under an optional contextual `expected` type,
/// mirroring the checker's literal inference (suffix wins; else the expected type
/// when it fits the literal's family; else the default `i32` / `f64`).
pub(super) fn literal_type(lit: &Literal, expected: Option<&HirType>) -> HirType {
    match lit {
        Literal::Integer(_, Some(suffix)) => int_suffix_type(suffix),
        Literal::Integer(_, None) => match expected {
            Some(t) if is_integer(t) => t.clone(),
            _ => HirType::I32,
        },
        Literal::Float(_, Some(suffix)) => float_suffix_type(suffix),
        Literal::Float(_, None) => match expected {
            Some(t) if is_full_float(t) => t.clone(),
            _ => HirType::F64,
        },
        Literal::Boolean(_) => HirType::Bool,
        Literal::Char(_) => HirType::Char,
        Literal::String(_) => HirType::String,
    }
}

/// The scalar value a match-pattern literal denotes, as the low bits of an `i64`
/// Integers as-is, `bool` as 0/1, `char` as its Unicode scalar value. Float
/// and string literals are not matchable (the checker rejects them before lowering).
pub(super) fn literal_scalar(lit: &Literal) -> Result<i64, LoweringError> {
    match lit {
        Literal::Integer(n, _) => Ok(*n as i64),
        Literal::Boolean(b) => Ok(*b as i64),
        Literal::Char(c) => Ok(*c as i64),
        Literal::Float(_, _) | Literal::String(_) => Err(LoweringError::Malformed {
            detail: "float/string literal reached a match pattern".to_string(),
        }),
    }
}

/// Whether `t` is a numeric type usable with `-` / arithmetic (integer or
/// full-precision float). Half-precision is excluded.
pub(super) fn is_numeric(t: &HirType) -> bool {
    is_integer(t) || is_full_float(t)
}

/// The result type of a binary operator given its operand types. Comparisons and
/// logical operators yield `bool`; `+` on two strings yields a new owned `string`
/// Other arithmetic and bitwise operators yield the left operand's type.
pub(super) fn binary_result_type(
    op: BinaryOp,
    left: &HirType,
    right: &HirType,
) -> Result<HirType, LoweringError> {
    // A tensor operand is element-wise and allocates a fresh result whose shape is
    // the broadcast join of the two. Handled before the scalar rule below, which reads a
    // tensor as an operand with no operator lowering.
    if is_tensor(left) || is_tensor(right) {
        return tensor_result_type(op, left, right);
    }

    // Every operator below is emitted as one scalar instruction, or (for `==`, `!=`
    // and `+` on strings) as a byte compare or a concatenation. An aggregate operand
    // has no such lowering; it reaches here only from a monomorphized generic body,
    // since the checker rejects a concrete one. Refusing it keeps the backend from
    // asking an aggregate value for its integer variant and aborting the compiler.
    if !has_operator_lowering(left) {
        return Err(LoweringError::UnsupportedOperand {
            op: op.to_string(),
            ty: left.to_string(),
        });
    }

    Ok(match op {
        BinaryOp::Equal
        | BinaryOp::NotEqual
        | BinaryOp::Less
        | BinaryOp::Greater
        | BinaryOp::LessEqual
        | BinaryOp::GreaterEqual
        | BinaryOp::And
        | BinaryOp::Or => HirType::Bool,
        BinaryOp::Add if peels_to_string(left) && peels_to_string(right) => HirType::String,
        BinaryOp::Add
        | BinaryOp::Subtract
        | BinaryOp::Multiply
        | BinaryOp::Divide
        | BinaryOp::Modulo
        | BinaryOp::BitAnd
        | BinaryOp::BitOr
        | BinaryOp::BitXor
        | BinaryOp::Shl => left.clone(),
        // `@` is defined on tensors only; the tensor rule above has already taken every
        // operand pair that reaches it, so a scalar one here is a checker escape.
        BinaryOp::MatMul => {
            return Err(LoweringError::UnsupportedOperand {
                op: op.to_string(),
                ty: left.to_string(),
            })
        }
        // `??` desugars to a `match` before any operand type is combined, so it never
        // reaches the operand-symmetric result rule.
        BinaryOp::NullCoalesce => {
            return Err(LoweringError::Malformed {
                detail: "`??` reached the binary result rule; it desugars to a match".to_string(),
            })
        }
    })
}

/// Whether the backend has an instruction sequence for a binary operator on `ty`:
/// the scalars, `string` (and a `&string` slice, which is normalized to it), and a
/// newtype forwarding one of those. Everything else needs an operator-trait impl,
/// which is dispatched to a method call before the operand types are ever combined.
fn has_operator_lowering(ty: &HirType) -> bool {
    match ty {
        HirType::Newtype { inner, .. } => has_operator_lowering(inner),
        HirType::Reference { inner, .. } => matches!(**inner, HirType::String),
        HirType::Bool | HirType::Char | HirType::String | HirType::F16 | HirType::BF16 => true,
        other => is_numeric(other),
    }
}

/// The element type a tensor operand broadcasts a scalar operand against, when `ty` is a
/// tensor or a borrow of one.
///
/// This is what types a bare literal written beside a tensor: `matrix * 2.0` is
/// the scalar broadcast, so the literal is the element's type rather than the `f64` the
/// language default would otherwise pick.
pub(crate) fn tensor_element(ty: &HirType) -> Option<&HirType> {
    match ty.referent() {
        HirType::Tensor { element, .. } => Some(element),
        _ => None,
    }
}

/// Whether `ty` is a tensor, or a borrow of one. Every tensor operator is defined on
/// borrowed operands too, so the two reach the element-wise rule together.
fn is_tensor(ty: &HirType) -> bool {
    matches!(ty.referent(), HirType::Tensor { .. })
}

/// The tensor a binary operand denotes: element type, shape, and axis names.
fn tensor_parts(ty: &HirType) -> Option<(&HirType, &[Option<usize>], &AxisNames)> {
    match ty.referent() {
        HirType::Tensor {
            element,
            shape,
            names,
        } => Some((element, shape, names)),
        _ => None,
    }
}

/// The freshly allocated result of an element-wise tensor operator.
///
/// The checker has already accepted the operands, so the join here re-derives the shape
/// it settled rather than re-deciding it: a stage carries the rule it needs over the
/// shared type instead of importing a sibling's. A pair that does not join is therefore
/// a compiler bug and answers `UnsupportedOperand`, not a diagnostic.
fn tensor_result_type(
    op: BinaryOp,
    left: &HirType,
    right: &HirType,
) -> Result<HirType, LoweringError> {
    let unsupported = || LoweringError::UnsupportedOperand {
        op: op.to_string(),
        ty: left.to_string(),
    };
    let (element, shape, names) = match (tensor_parts(left), tensor_parts(right)) {
        // `@` contracts the operands' inner axis instead of joining their shapes, so it
        // is separated before the element-wise rule rather than inside it.
        (Some(l), Some(r)) if op == BinaryOp::MatMul => {
            let (shape, names) = matmul_shape(l.1, l.2, r.1, r.2).ok_or_else(unsupported)?;
            (l.0, shape, names)
        }
        (Some(l), Some(r)) => {
            let (shape, names) = broadcast_shapes(l.1, l.2, r.1, r.2).ok_or_else(unsupported)?;
            (l.0, shape, names)
        }
        // `@` has no scalar form: a scalar has no inner axis to contract, so the arm
        // below would silently give it the element-wise operator's shape.
        _ if op == BinaryOp::MatMul => return Err(unsupported()),
        // A scalar operand is stretched across every element, so the tensor side alone
        // decides the result's shape.
        (Some(t), None) | (None, Some(t)) => (t.0, t.1.to_vec(), t.2.clone()),
        (None, None) => return Err(unsupported()),
    };
    Ok(HirType::Tensor {
        element: Box::new(element.clone()),
        shape,
        names,
    })
}

/// The contracted shape of `[M, K] @ [K, N]`: the left operand's rows and the right
/// operand's columns, with the inner axis read away. `None` where the checker would not
/// have accepted the pair.
fn matmul_shape(
    left: &[Option<usize>],
    left_names: &AxisNames,
    right: &[Option<usize>],
    right_names: &AxisNames,
) -> Option<(Vec<Option<usize>>, AxisNames)> {
    let ([rows, inner], [contracted, columns]) = (left, right) else {
        return None;
    };
    if inner != contracted {
        return None;
    }
    Some((
        vec![*rows, *columns],
        AxisNames(vec![
            left_names.0.first().cloned().flatten(),
            right_names.0.get(1).cloned().flatten(),
        ]),
    ))
}

/// The broadcast join of two shapes: align at the trailing axis, stretch an extent of 1,
/// and let a lower-rank operand supply the innermost axes. `None` where they do not join.
fn broadcast_shapes(
    left: &[Option<usize>],
    left_names: &AxisNames,
    right: &[Option<usize>],
    right_names: &AxisNames,
) -> Option<(Vec<Option<usize>>, AxisNames)> {
    let rank = left.len().max(right.len());
    let mut shape = Vec::with_capacity(rank);
    let mut names = Vec::with_capacity(rank);
    for position in 0..rank {
        let depth = rank - 1 - position;
        let l = left.len().checked_sub(depth + 1);
        let r = right.len().checked_sub(depth + 1);
        let (extent, name) = match (l, r) {
            (Some(l), Some(r)) => join_extents(
                left[l],
                left_names.0.get(l).cloned().flatten(),
                right[r],
                right_names.0.get(r).cloned().flatten(),
            )?,
            (Some(l), None) => (left[l], left_names.0.get(l).cloned().flatten()),
            (None, Some(r)) => (right[r], right_names.0.get(r).cloned().flatten()),
            (None, None) => return None,
        };
        shape.push(extent);
        names.push(name);
    }
    Some((shape, AxisNames(names)))
}

/// The result axis two aligned operand axes produce. A stretched axis contributes
/// neither its extent nor its name.
fn join_extents(
    left: Option<usize>,
    left_name: Option<String>,
    right: Option<usize>,
    right_name: Option<String>,
) -> Option<(Option<usize>, Option<String>)> {
    if left == right {
        return Some((left, left_name.or(right_name)));
    }
    match (left, right) {
        (Some(1), _) => Some((right, right_name)),
        (_, Some(1)) => Some((left, left_name)),
        _ => None,
    }
}
