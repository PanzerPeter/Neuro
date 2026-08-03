// Type checking for the error-propagation operator `operand?`.
//
// Reached from the `check_expr` dispatch in this module's `mod.rs`. Two rules beyond
// "the operand must be fallible": the enclosing function has to return the same fallible
// enum for the failure to go anywhere, and — since the language has no From/Into — the
// error type must already match, with no implicit conversion.

use super::operators::RESULT_ENUM;
use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::Expr;
use shared_types::Span;

/// The `Result` variant `?` propagates. `Option::None` carries no payload, so it needs
/// no name here — the failure value is rebuilt from the variant alone.
const RESULT_FAILURE_VARIANT: &str = "Err";

impl TypeChecker {
    /// Check `operand?`: unwrap a fallible value to its success payload, propagating
    /// the failure variant out of the enclosing function.
    pub(super) fn check_try_expr(&mut self, operand: &Expr, span: Span) -> Option<Type> {
        let operand_ty = self.check_expr(operand, None).unwrap_or(Type::Unknown);
        if matches!(operand_ty, Type::Unknown) {
            return Some(Type::Unknown);
        }

        let Some(kind) = self.fallible_kind(&operand_ty) else {
            self.record_error(TypeError::TryOnNonFallible {
                found: operand_ty,
                span,
            });
            return Some(Type::Unknown);
        };

        let declared_return = self
            .current_function_return_type
            .clone()
            .unwrap_or(Type::Void);
        let Some(return_instance) = self.propagation_target(&kind.base, &declared_return) else {
            self.record_error(TypeError::TryOutsideFallibleFunction {
                operand: operand_ty,
                expected: kind.base,
                found: declared_return,
                span,
            });
            return Some(Type::Unknown);
        };

        if kind.base == RESULT_ENUM {
            self.check_propagated_error(&kind.instance, &return_instance, span);
        }

        Some(kind.payload)
    }

    /// The enum instance a propagated failure is rebuilt as: the enclosing function's
    /// return type, when it is an instance of the same fallible enum `base` the operand
    /// came from. `None` means the failure has nowhere to go.
    fn propagation_target(&self, base: &str, declared_return: &Type) -> Option<String> {
        let Type::Enum(name) = declared_return else {
            return None;
        };
        let return_base = self.enum_instance_base(name).unwrap_or(name.as_str());
        (return_base == base).then(|| name.clone())
    }

    /// Verify the operand's `Err` payload is already the function's error type.
    ///
    /// `?` forwards the error as-is — there is no From/Into trait to convert through, so
    /// a mismatch is a plain type error the programmer resolves with `.map_err(...)`.
    fn check_propagated_error(
        &mut self,
        operand_instance: &str,
        return_instance: &str,
        span: Span,
    ) {
        let found = self.failure_payload(operand_instance);
        let expected = self.failure_payload(return_instance);
        let (Some(found), Some(expected)) = (found, expected) else {
            return;
        };
        if !expected.is_compatible_with(&found) {
            self.record_error(TypeError::Mismatch {
                expected,
                found,
                span,
            });
        }
    }

    /// The `Err` payload type of a `Result` instance.
    fn failure_payload(&self, instance: &str) -> Option<Type> {
        self.lookup_enum_variant(instance, RESULT_FAILURE_VARIANT)?
            .fields
            .first()
            .map(|(_, ty)| ty.clone())
    }
}
