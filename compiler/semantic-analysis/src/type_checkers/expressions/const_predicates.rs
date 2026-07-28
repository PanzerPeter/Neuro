// Compile-time evaluation of `where`-clause predicates over const generic values.

use crate::types::{ArrayLen, Type};
use ast_types::{BinaryOp, Expr};
use shared_types::Literal;
use std::collections::HashMap;

/// Evaluate a `where`-clause value predicate to a boolean, given the const
/// parameter values in `subst`. Returns `None` when the predicate is not a fully
/// resolved boolean over const values (it is then deferred to the concrete instance).
pub(crate) fn eval_const_predicate(expr: &Expr, subst: &HashMap<String, Type>) -> Option<bool> {
    match expr {
        Expr::Literal(Literal::Boolean(b), _) => Some(*b),
        Expr::Paren(inner, _) => eval_const_predicate(inner, subst),
        Expr::Binary {
            left, op, right, ..
        } => match op {
            BinaryOp::And => {
                Some(eval_const_predicate(left, subst)? && eval_const_predicate(right, subst)?)
            }
            BinaryOp::Or => {
                Some(eval_const_predicate(left, subst)? || eval_const_predicate(right, subst)?)
            }
            BinaryOp::Less
            | BinaryOp::Greater
            | BinaryOp::LessEqual
            | BinaryOp::GreaterEqual
            | BinaryOp::Equal
            | BinaryOp::NotEqual => {
                let l = eval_const_int(left, subst)?;
                let r = eval_const_int(right, subst)?;
                Some(match op {
                    BinaryOp::Less => l < r,
                    BinaryOp::Greater => l > r,
                    BinaryOp::LessEqual => l <= r,
                    BinaryOp::GreaterEqual => l >= r,
                    BinaryOp::Equal => l == r,
                    BinaryOp::NotEqual => l != r,
                    _ => unreachable!(),
                })
            }
            _ => None,
        },
        _ => None,
    }
}

/// Evaluate a const-integer expression: an integer literal, a const parameter
/// looked up in `subst`, or an arithmetic combination of these. `None` when it is not a
/// fully resolved const integer.
fn eval_const_int(expr: &Expr, subst: &HashMap<String, Type>) -> Option<i128> {
    match expr {
        Expr::Literal(Literal::Integer(v, _), _) => Some(*v as i128),
        Expr::Paren(inner, _) => eval_const_int(inner, subst),
        Expr::Identifier(id) => match subst.get(&id.name) {
            Some(Type::ConstValue(v)) => Some(*v as i128),
            _ => None,
        },
        Expr::Binary {
            left, op, right, ..
        } => {
            let l = eval_const_int(left, subst)?;
            let r = eval_const_int(right, subst)?;
            match op {
                BinaryOp::Add => Some(l + r),
                BinaryOp::Subtract => Some(l - r),
                BinaryOp::Multiply => Some(l * r),
                BinaryOp::Divide if r != 0 => Some(l / r),
                BinaryOp::Modulo if r != 0 => Some(l % r),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Whether a resolved type still mentions a generic type parameter, a const-parameter
/// array length, or an unresolved const value — i.e. it is not fully concrete.
pub(super) fn mentions_type_parameter(ty: &Type) -> bool {
    match ty {
        Type::Generic(_) | Type::ConstValue(_) => true,
        Type::Reference { inner, .. } => mentions_type_parameter(inner),
        Type::Array { element, size } => {
            matches!(size, ArrayLen::Param(_)) || mentions_type_parameter(element)
        }
        Type::Tuple(elements) => elements.iter().any(mentions_type_parameter),
        Type::Function { params, ret } => {
            params.iter().any(mentions_type_parameter) || mentions_type_parameter(ret)
        }
        _ => false,
    }
}
