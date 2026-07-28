//! Type derivation shared by the expression dispatch: the trait-object unsizing
//! coercion, contextual literal typing, and the result type of a binary operator.

use neuro_hir::{HirExpr, HirExprKind, HirType};
use shared_types::Literal;

use crate::types::{float_suffix_type, int_suffix_type};
use crate::{is_full_float, is_integer, peels_to_string, LoweringError};
use ast_types::BinaryOp;

/// Wrap a concrete reference in the unsizing coercion `&T` → `&dyn Trait` when
/// the context calls for a trait object and the value is not already one.
///
/// This is the sole implicit conversion in the language, so it is applied at exactly one
/// place: every context that supplies an expected type routes through here. The checker
/// has already verified that `T` implements the trait, so no impl lookup is repeated.
pub(super) fn apply_dyn_coercion(expr: HirExpr, expected: Option<&HirType>) -> HirExpr {
    let Some(HirType::Reference {
        inner: expected_inner,
        mutable,
    }) = expected
    else {
        return expr;
    };
    if !matches!(expected_inner.as_ref(), HirType::DynObject(_)) {
        return expr;
    }
    // A value that is already a trait object needs no coercion; anything else must be a
    // concrete `&T` for the checker to have accepted it here.
    match &expr.ty {
        HirType::Reference { inner, .. } if matches!(inner.as_ref(), HirType::DynObject(_)) => expr,
        HirType::Reference { .. } => {
            let span = expr.span;
            HirExpr::new(
                HirExprKind::DynCoerce {
                    value: Box::new(expr),
                },
                HirType::Reference {
                    inner: expected_inner.clone(),
                    mutable: *mutable,
                },
                span,
            )
        }
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
        Literal::Integer(n, _) => Ok(*n),
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
        // `??` is rejected by the checker (Option/Result arrive in Phase 2), so it
        // never reaches a well-typed program's HIR.
        BinaryOp::NullCoalesce => {
            return Err(LoweringError::Malformed {
                detail: "`??` operator is not supported until Phase 2".to_string(),
            })
        }
    })
}
