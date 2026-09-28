//! Evaluation of integer `const` initializers, so a constant that has no value is a
//! diagnostic on its initializer rather than a failure in the backend's folder.
//!
//! Only the arithmetic that can fail is evaluated: `+`, `-`, `*`, `/`, `%` and unary
//! `-` over integer literals and other integer constants of the same type. Anything
//! else (a cast, a bitwise operator, a constant of another type) is left to the
//! backend, which folds it without a failure mode.

use ast_types::{BinaryOp, Expr, UnaryOp};
use shared_types::{Literal, Span};

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;

/// How deep a chain of constants naming constants is followed. A cycle cannot reach
/// here as a value, but a bound keeps the walk finite whatever the table holds.
const MAX_CONST_DEPTH: u32 = 64;

impl TypeChecker {
    /// Report an integer constant whose initializer overflows `ty` or divides by zero.
    pub(crate) fn check_const_value(&mut self, value: &Expr, ty: &Type) {
        if !ty.is_integer() {
            return;
        }
        if let Err((reason, span)) = self.fold_const_int(value, ty, 0) {
            self.record_error(TypeError::ConstHasNoValue { reason, span });
        }
    }

    /// The value of `expr` as a `ty` integer: `Ok(None)` when it is not one this pass
    /// evaluates, `Err` with the reason and the failing operation's span when it has
    /// no value at all.
    fn fold_const_int(
        &self,
        expr: &Expr,
        ty: &Type,
        depth: u32,
    ) -> Result<Option<i128>, (String, Span)> {
        let overflow = |op: &str, span: Span| {
            (
                format!("`{op}` overflows {ty}, whose values do not include the result"),
                span,
            )
        };
        match expr {
            // A literal is range-checked where it is typed; `-2147483648` is the one
            // literal that is only in range once negated, so it is not checked here.
            Expr::Literal(Literal::Integer(v, _), _) => Ok(Some(*v)),
            Expr::Paren(inner, _) => self.fold_const_int(inner, ty, depth),
            Expr::Identifier(ident) => {
                if depth >= MAX_CONST_DEPTH || self.constants.get(&ident.name) != Some(ty) {
                    return Ok(None);
                }
                match self.constant_values.get(&ident.name) {
                    Some(value) => self.fold_const_int(value, ty, depth + 1),
                    None => Ok(None),
                }
            }
            Expr::Unary {
                op: UnaryOp::Negate,
                operand,
                span,
            } => match self.fold_const_int(operand, ty, depth)? {
                Some(v) => match v.checked_neg().filter(|n| self.check_integer_range(*n, ty)) {
                    Some(n) => Ok(Some(n)),
                    None => Err(overflow("-", *span)),
                },
                None => Ok(None),
            },
            Expr::Binary {
                left,
                op,
                right,
                span,
            } => {
                let r = self.fold_const_int(right, ty, depth)?;
                if matches!(op, BinaryOp::Divide | BinaryOp::Modulo) && r == Some(0) {
                    return Err(("it divides by zero".to_string(), *span));
                }
                let l = self.fold_const_int(left, ty, depth)?;
                let (Some(l), Some(r)) = (l, r) else {
                    return Ok(None);
                };
                let (symbol, result) = match op {
                    BinaryOp::Add => ("+", l.checked_add(r)),
                    BinaryOp::Subtract => ("-", l.checked_sub(r)),
                    BinaryOp::Multiply => ("*", l.checked_mul(r)),
                    BinaryOp::Divide => ("/", l.checked_div(r)),
                    BinaryOp::Modulo => ("%", l.checked_rem(r)),
                    _ => return Ok(None),
                };
                match result.filter(|v| self.check_integer_range(*v, ty)) {
                    Some(v) => Ok(Some(v)),
                    None => Err(overflow(symbol, *span)),
                }
            }
            _ => Ok(None),
        }
    }
}
