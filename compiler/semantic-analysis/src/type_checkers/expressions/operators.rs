// Binary and unary operators and the `as` cast.
//
// Reached from the `check_expr` dispatch in this module's `mod.rs`. Every file
// here adds methods to the same `impl TypeChecker` block.

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::{BinaryOp, Expr, UnaryOp};
use shared_types::Span;

impl TypeChecker {
    pub(super) fn check_binary_expr(
        &mut self,
        left: &Expr,
        op: &BinaryOp,
        right: &Expr,
        span: &Span,
        _expected: Option<&Type>,
    ) -> Option<Type> {
        if op.is_comparison() {
            if let Expr::Binary { op: inner_op, .. } = left {
                if inner_op.is_comparison() {
                    self.record_error(TypeError::ComparisonChain { span: *span });
                    return Some(Type::Unknown);
                }
            }
        }

        // Check both operands even if one fails, for better error reporting.
        // Left is checked bare to get its natural type, then right uses it
        // as the expected type for symmetric inference.
        let left_ty = self.check_expr(left, None).unwrap_or(Type::Unknown);
        let right_ty = self
            .check_expr(right, Some(&left_ty))
            .unwrap_or(Type::Unknown);

        // If either operand is Unknown (error), propagate Unknown
        if matches!(left_ty, Type::Unknown) || matches!(right_ty, Type::Unknown) {
            return Some(Type::Unknown);
        }

        // Operator-trait dispatch on a user type: when the left operand is
        // a struct that implements the operator's trait, the operator lowers to
        // that impl's method and takes its result type. Checked before the
        // built-in numeric/bitwise/comparison paths, which reject struct operands.
        if let Type::Struct(name) = left_ty.referent() {
            if let Some(dispatch) = self
                .operator_binary_impls
                .get(&(name.clone(), *op))
                .cloned()
            {
                if !right_ty.referent().is_compatible_with(&dispatch.rhs) {
                    self.record_error(TypeError::InvalidBinaryOperator {
                        op: op.to_string(),
                        left: left_ty.clone(),
                        right: right_ty,
                        span: *span,
                    });
                    return Some(Type::Unknown);
                }
                return Some(dispatch.result);
            }
        }

        match op {
            // Arithmetic operators: require numeric types, return same type
            BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::Modulo => {
                // String concatenation: `+` joins two strings into a new
                // owned, immutable `string`. A `&string` slice participates too, so
                // a single string reference is peeled exactly as equality does. The
                // other arithmetic operators have no string meaning. Checked before
                // the numeric path, which would reject a non-numeric operand.
                let left_cat = left_ty.peel_string_ref();
                let right_cat = right_ty.peel_string_ref();
                if matches!(left_cat, Type::String) || matches!(right_cat, Type::String) {
                    if matches!(op, BinaryOp::Add)
                        && matches!(left_cat, Type::String)
                        && matches!(right_cat, Type::String)
                    {
                        return Some(Type::String);
                    }
                    self.record_error(TypeError::InvalidBinaryOperator {
                        op: op.to_string(),
                        left: left_ty.clone(),
                        right: right_ty.clone(),
                        span: *span,
                    });
                    return Some(Type::Unknown);
                }

                // Half-precision scalars have no arithmetic: point the
                // programmer at the `f32` workaround rather than a generic error.
                if let Some(half) = [&left_ty, &right_ty]
                    .into_iter()
                    .find(|t| t.is_half_float())
                {
                    self.record_error(TypeError::HalfFloatArithmetic {
                        op: op.to_string(),
                        ty: half.clone(),
                        span: *span,
                    });
                    return Some(Type::Unknown);
                }

                if !left_ty.is_numeric() {
                    self.record_error(TypeError::InvalidBinaryOperator {
                        op: op.to_string(),
                        left: left_ty.clone(),
                        right: right_ty.clone(),
                        span: *span,
                    });
                    return Some(Type::Unknown);
                }

                if !left_ty.is_compatible_with(&right_ty) {
                    self.record_error(TypeError::Mismatch {
                        expected: left_ty.clone(),
                        found: right_ty,
                        span: *span,
                    });
                    return Some(Type::Unknown);
                }

                Some(left_ty)
            }

            // Comparison operators: require compatible types, return bool.
            // `&string` is a borrowed string slice, so an owned `string`
            // and a `&string` slice compare equal byte-wise in any combination.
            BinaryOp::Equal | BinaryOp::NotEqual => {
                let left_cmp = left_ty.peel_string_ref();
                let right_cmp = right_ty.peel_string_ref();
                if !left_cmp.is_compatible_with(&right_cmp) {
                    self.record_error(TypeError::Mismatch {
                        expected: left_ty,
                        found: right_ty,
                        span: *span,
                    });
                    return Some(Type::Unknown);
                }
                Some(Type::Bool)
            }

            // Ordering operators: require numeric or `char` operands (this gives
            // `char` a built-in total order), return bool.
            BinaryOp::Less | BinaryOp::Greater | BinaryOp::LessEqual | BinaryOp::GreaterEqual => {
                if !left_ty.is_compatible_with(&right_ty) {
                    self.record_error(TypeError::Mismatch {
                        expected: left_ty,
                        found: right_ty,
                        span: *span,
                    });
                    return Some(Type::Unknown);
                }

                if !left_ty.is_numeric() && !left_ty.is_char() {
                    self.record_error(TypeError::InvalidBinaryOperator {
                        op: op.to_string(),
                        left: left_ty.clone(),
                        right: right_ty.clone(),
                        span: *span,
                    });
                    return Some(Type::Unknown);
                }

                Some(Type::Bool)
            }

            // Bitwise operators: require integer types, return same type
            BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor | BinaryOp::Shl => {
                if !left_ty.is_integer() {
                    self.record_error(TypeError::InvalidBinaryOperator {
                        op: op.to_string(),
                        left: left_ty.clone(),
                        right: right_ty.clone(),
                        span: *span,
                    });
                    return Some(Type::Unknown);
                }

                if !left_ty.is_compatible_with(&right_ty) {
                    self.record_error(TypeError::Mismatch {
                        expected: left_ty.clone(),
                        found: right_ty,
                        span: *span,
                    });
                    return Some(Type::Unknown);
                }

                Some(left_ty)
            }

            // `??` is parsed (R-to-L per Appendix B) but unwrapping Option/Result
            // arrives in Phase 2; reject here so codegen never sees it.
            BinaryOp::NullCoalesce => {
                self.record_error(TypeError::OperatorNotYetSupported {
                    op: op.to_string(),
                    hint: "requires Option<T> / Result<T, E> — available in Phase 2".to_string(),
                    span: *span,
                });
                Some(Type::Unknown)
            }

            // Logical operators: require bool types, return bool
            BinaryOp::And | BinaryOp::Or => {
                let mut has_error = false;

                if !left_ty.is_bool() {
                    self.record_error(TypeError::InvalidBinaryOperator {
                        op: op.to_string(),
                        left: left_ty,
                        right: right_ty.clone(),
                        span: *span,
                    });
                    has_error = true;
                }

                if !right_ty.is_bool() {
                    self.record_error(TypeError::InvalidBinaryOperator {
                        op: op.to_string(),
                        left: Type::Bool,
                        right: right_ty,
                        span: *span,
                    });
                    has_error = true;
                }

                if has_error {
                    Some(Type::Unknown)
                } else {
                    Some(Type::Bool)
                }
            }
        }
    }

    pub(super) fn check_unary_expr(
        &mut self,
        op: &UnaryOp,
        operand: &Expr,
        span: &Span,
        expected: Option<&Type>,
    ) -> Option<Type> {
        // For unary operations, propagate expected type to operand if appropriate
        let expected_operand = match op {
            UnaryOp::Negate => expected.filter(|t| t.is_numeric()),
            UnaryOp::Not => None,
            UnaryOp::BitNot => expected.filter(|t| t.is_integer()),
        };

        let operand_ty = self
            .check_expr(operand, expected_operand)
            .unwrap_or(Type::Unknown);

        if matches!(operand_ty, Type::Unknown) {
            return Some(Type::Unknown);
        }

        // Operator-trait dispatch on a user type: `-a` via `Neg`, `~a` via
        // `Not`. The boolean `!a` (`UnaryOp::Not`) is never overloadable.
        if let Type::Struct(name) = operand_ty.referent() {
            if let Some(result) = self.operator_unary_impls.get(&(name.clone(), *op)).cloned() {
                return Some(result);
            }
        }

        match op {
            UnaryOp::Negate => {
                if !operand_ty.is_numeric() {
                    self.record_error(TypeError::InvalidOperator {
                        op: op.to_string(),
                        ty: operand_ty,
                        span: *span,
                    });
                    return Some(Type::Unknown);
                }
                Some(operand_ty)
            }
            UnaryOp::Not => {
                if !operand_ty.is_bool() {
                    self.record_error(TypeError::InvalidOperator {
                        op: op.to_string(),
                        ty: operand_ty,
                        span: *span,
                    });
                    return Some(Type::Unknown);
                }
                Some(Type::Bool)
            }
            UnaryOp::BitNot => {
                if !operand_ty.is_integer() {
                    self.record_error(TypeError::InvalidOperator {
                        op: op.to_string(),
                        ty: operand_ty,
                        span: *span,
                    });
                    return Some(Type::Unknown);
                }
                Some(operand_ty)
            }
        }
    }

    pub(super) fn check_cast_expr(
        &mut self,
        expr: &Expr,
        target_type: &ast_types::Type,
        span: &Span,
    ) -> Option<Type> {
        let from_type = self.check_expr(expr, None)?;
        if matches!(from_type, Type::Unknown) {
            return Some(Type::Unknown);
        }

        let to_type = self.resolve_type(target_type)?;
        if matches!(to_type, Type::Unknown) {
            return Some(Type::Unknown);
        }

        if to_type.is_valid_cast(&from_type) {
            Some(to_type)
        } else {
            self.record_error(TypeError::Mismatch {
                expected: to_type.clone(),
                found: from_type,
                span: *span,
            });
            Some(Type::Unknown)
        }
    }
}
