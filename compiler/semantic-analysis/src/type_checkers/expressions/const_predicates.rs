// Compile-time evaluation of `where`-clause predicates over const generic values.

use crate::type_checkers::TypeChecker;
use crate::types::{ArrayLen, Type};
use ast_types::{BinaryOp, Expr};
use shared_types::{Literal, Span};
use std::collections::HashMap;

/// Why a constant expression has no value, and the span of the operation that failed.
pub(crate) type NoValue = (String, Span);

/// Evaluate a `where`-clause value predicate to a boolean, given the const
/// parameter values in `subst`. `Ok(None)` when the predicate is not a fully
/// resolved boolean over const values (it is then deferred to the concrete instance);
/// `Err` when its arithmetic has no value at all, which no instance can repair.
pub(crate) fn eval_const_predicate(
    expr: &Expr,
    subst: &HashMap<String, Type>,
) -> Result<Option<bool>, NoValue> {
    match expr {
        Expr::Literal(Literal::Boolean(b), _) => Ok(Some(*b)),
        Expr::Paren(inner, _) => eval_const_predicate(inner, subst),
        Expr::Binary {
            left, op, right, ..
        } => match op {
            BinaryOp::And | BinaryOp::Or => {
                let l = eval_const_predicate(left, subst)?;
                let r = eval_const_predicate(right, subst)?;
                Ok(l.zip(r).map(|(l, r)| match op {
                    BinaryOp::And => l && r,
                    _ => l || r,
                }))
            }
            BinaryOp::Less
            | BinaryOp::Greater
            | BinaryOp::LessEqual
            | BinaryOp::GreaterEqual
            | BinaryOp::Equal
            | BinaryOp::NotEqual => {
                let l = eval_const_int(left, subst)?;
                let r = eval_const_int(right, subst)?;
                Ok(l.zip(r).map(|(l, r)| match op {
                    BinaryOp::Less => l < r,
                    BinaryOp::Greater => l > r,
                    BinaryOp::LessEqual => l <= r,
                    BinaryOp::GreaterEqual => l >= r,
                    BinaryOp::Equal => l == r,
                    _ => l != r,
                }))
            }
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}

/// Evaluate a const-integer expression that names no const parameter: the literal
/// arithmetic a tensor slice bound may be written as, where the folded value
/// becomes part of the result's type rather than a run-time index.
pub(crate) fn eval_literal_int(expr: &Expr) -> Option<i128> {
    eval_const_int(expr, &HashMap::new()).ok().flatten()
}

/// Evaluate a const-integer expression: an integer literal, a const parameter
/// looked up in `subst`, or an arithmetic combination of these. `Ok(None)` when it is
/// not a fully resolved const integer.
fn eval_const_int(expr: &Expr, subst: &HashMap<String, Type>) -> Result<Option<i128>, NoValue> {
    match expr {
        Expr::Literal(Literal::Integer(v, _), _) => Ok(Some(*v)),
        Expr::Paren(inner, _) => eval_const_int(inner, subst),
        Expr::Identifier(id) => Ok(match subst.get(&id.name) {
            Some(Type::ConstValue(v)) => Some(*v as i128),
            _ => None,
        }),
        Expr::Binary {
            left,
            op,
            right,
            span,
        } => {
            let l = eval_const_int(left, subst)?;
            let r = eval_const_int(right, subst)?;
            let (Some(l), Some(r)) = (l, r) else {
                return Ok(None);
            };
            if matches!(op, BinaryOp::Divide | BinaryOp::Modulo) && r == 0 {
                return Err(("it divides by zero".to_string(), *span));
            }
            // Folded in `i128`, so an overflow is past any extent a tensor can have.
            let (symbol, result) = match op {
                BinaryOp::Add => ("+", l.checked_add(r)),
                BinaryOp::Subtract => ("-", l.checked_sub(r)),
                BinaryOp::Multiply => ("*", l.checked_mul(r)),
                BinaryOp::Divide => ("/", l.checked_div(r)),
                BinaryOp::Modulo => ("%", l.checked_rem(r)),
                _ => return Ok(None),
            };
            result.map(Some).ok_or_else(|| {
                (
                    format!("`{symbol}` overflows the 128-bit range constants are folded in"),
                    *span,
                )
            })
        }
        _ => Ok(None),
    }
}

impl TypeChecker {
    /// Whether a resolved type still mentions a generic type parameter, a const-parameter
    /// array length, or an unresolved const value, i.e. it is not fully concrete. An
    /// instance mentions one through its arguments: `Option<T>` is not concrete.
    pub(crate) fn mentions_type_parameter(&self, ty: &Type) -> bool {
        match ty {
            Type::Generic(_) | Type::ConstValue(_) => true,
            Type::Reference { inner, .. } => self.mentions_type_parameter(inner),
            Type::Array { element, size } => {
                matches!(size, ArrayLen::Param(_)) || self.mentions_type_parameter(element)
            }
            Type::Tuple(elements) => elements.iter().any(|e| self.mentions_type_parameter(e)),
            Type::Function { params, ret } => {
                params.iter().any(|p| self.mentions_type_parameter(p))
                    || self.mentions_type_parameter(ret)
            }
            Type::Struct(name) | Type::Enum(name) => self
                .instance_parts(name)
                .is_some_and(|(_, args)| args.iter().any(|a| self.mentions_type_parameter(a))),
            _ => false,
        }
    }
}
