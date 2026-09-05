//! Type derivation shared by the expression dispatch: the unsizing coercions,
//! contextual literal typing, and the result type of a binary operator.

use neuro_hir::{HirExpr, HirExprKind, HirType};
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
    // shape is the coercion's own output — re-wrapping it would double the conversion.
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
    // Every operator below is emitted as one scalar instruction, or — for `==`, `!=`
    // and `+` on strings — as a byte compare or a concatenation. An aggregate operand
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
