// Binary and unary operators and the `as` cast.
//
// Reached from the `check_expr` dispatch in this module's `mod.rs`. Every file
// here adds methods to the same `impl TypeChecker` block.

use super::TypeChecker;
use crate::errors::TypeError;
use crate::type_checkers::collections::OPTION_ENUM;
use crate::types::Type;
use ast_types::{BinaryOp, Expr, UnaryOp};
use shared_types::Span;

/// The `Result<T, E>` half of the fallible pair `??` unwraps. Like `Option`, it comes
/// from the prelude rather than the compiler; `??` recognizes it by name.
pub(super) const RESULT_ENUM: &str = "Result";

/// The variant of `Option` that carries a value — the one `??` unwraps.
const OPTION_SUCCESS_VARIANT: &str = "Some";

/// The variant of `Result` that carries a value. `Err`'s payload is discarded by `??`.
const RESULT_SUCCESS_VARIANT: &str = "Ok";

/// A fallible value's resolved shape: which prelude enum it is, which monomorphized
/// instance, and what its success variant carries.
pub(super) struct FallibleKind {
    pub(super) instance: String,
    pub(super) base: String,
    pub(super) payload: Type,
}

impl TypeChecker {
    pub(super) fn check_binary_expr(
        &mut self,
        left: &Expr,
        op: &BinaryOp,
        right: &Expr,
        span: &Span,
        _expected: Option<&Type>,
    ) -> Option<Type> {
        // `??` is the one binary operator whose operands are not symmetric: the right
        // side is typed by the left's *payload*, not by the left itself. Handled before
        // the shared operand check below, which would type `fallback` as an Option.
        if matches!(op, BinaryOp::NullCoalesce) {
            return self.check_null_coalesce(left, right, span);
        }

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
                // Equality is built in for the scalars and for `string`; every other
                // operand needs a `PartialEq` impl, and the dispatch above has already
                // taken that path when one exists. Two operands of the same aggregate
                // type are compatible, so without this rejection the operator reaches
                // codegen, which asks the aggregate for its integer variant and aborts.
                if !self.has_builtin_equality(&left_cmp) {
                    match left_ty.referent() {
                        Type::Struct(name) => self.record_error(TypeError::MissingPartialEqImpl {
                            type_name: name.clone(),
                            op: op.to_string(),
                            span: *span,
                        }),
                        _ => self.record_error(TypeError::InvalidBinaryOperator {
                            op: op.to_string(),
                            left: left_ty.clone(),
                            right: right_ty,
                            span: *span,
                        }),
                    }
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

            // Handled by the guard clause at the top of this function.
            BinaryOp::NullCoalesce => Some(Type::Unknown),

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

    /// Whether `==` / `!=` are defined on `ty` without a `PartialEq` impl.
    ///
    /// The scalars the backend compares with a single instruction, `string` (compared
    /// byte-wise), and a newtype forwarding one of those. A generic parameter answers
    /// `true`: a generic body is checked once as a template, so the operand's real type
    /// is only known at the instantiation, which lowering checks instead.
    fn has_builtin_equality(&self, ty: &Type) -> bool {
        match ty {
            // Cycles are rejected when the newtype is registered, so this terminates.
            Type::Newtype(name) => self
                .lookup_newtype_inner(name)
                .cloned()
                .map(|inner| self.has_builtin_equality(&inner))
                .unwrap_or(false),
            Type::Generic(_) | Type::Unknown => true,
            Type::Bool | Type::Char | Type::String => true,
            other => other.is_numeric() || other.is_half_float(),
        }
    }

    /// Check `lhs ?? fallback`: the left side must be an `Option<T>` or `Result<T, E>`,
    /// the fallback must produce that `T`, and the expression's type is `T`.
    ///
    /// The `Result` error payload is deliberately unconstrained — `??` means "I do not
    /// care why it failed", so `E` never reaches the fallback.
    fn check_null_coalesce(&mut self, left: &Expr, right: &Expr, span: &Span) -> Option<Type> {
        let left_ty = self.check_expr(left, None).unwrap_or(Type::Unknown);
        // The fallback is still checked on every error path so a second mistake inside it
        // is reported in the same pass rather than on the next compile.
        if matches!(left_ty, Type::Unknown) {
            self.check_expr(right, None);
            return Some(Type::Unknown);
        }

        let Some(payload) = self.fallible_payload(&left_ty) else {
            self.record_error(TypeError::NullCoalesceOnNonFallible {
                found: left_ty,
                span: *span,
            });
            self.check_expr(right, None);
            return Some(Type::Unknown);
        };

        let right_ty = self
            .check_expr(right, Some(&payload))
            .unwrap_or(Type::Unknown);
        if matches!(right_ty, Type::Unknown) {
            return Some(Type::Unknown);
        }

        if !payload.is_compatible_with(&right_ty) {
            self.record_error(TypeError::Mismatch {
                expected: payload,
                found: right_ty,
                span: *span,
            });
            return Some(Type::Unknown);
        }

        Some(payload)
    }

    /// The payload type `??` unwraps out of a fallible enum: `Option`'s `Some` or
    /// `Result`'s `Ok`. `None` for every other type, which is what makes `??` reject them.
    fn fallible_payload(&self, ty: &Type) -> Option<Type> {
        self.fallible_kind(ty).map(|kind| kind.payload)
    }

    /// Resolve a type to the fallible enum it is, if any: the concrete instance, the
    /// prelude base it was monomorphized from, and the payload its success variant carries.
    ///
    /// Shared by `??` and `?` — the two operators that read a fallible value — so both
    /// recognize exactly the same set of left-hand types.
    pub(super) fn fallible_kind(&self, ty: &Type) -> Option<FallibleKind> {
        let Type::Enum(instance) = ty.referent() else {
            return None;
        };
        // A monomorphized `Option<i32>` answers with the template it came from; a program
        // that shadows the prelude with a non-generic `Option` is its own base.
        let base = self
            .enum_instance_base(instance)
            .unwrap_or(instance.as_str());
        let variant = match base {
            OPTION_ENUM => OPTION_SUCCESS_VARIANT,
            RESULT_ENUM => RESULT_SUCCESS_VARIANT,
            _ => return None,
        };
        let info = self.lookup_enum_variant(instance, variant)?;
        Some(FallibleKind {
            instance: instance.clone(),
            base: base.to_string(),
            payload: info.fields.first().map(|(_, ty)| ty.clone())?,
        })
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
