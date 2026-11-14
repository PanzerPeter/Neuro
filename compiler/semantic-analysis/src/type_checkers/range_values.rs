//! A range `start..end` or `start..=end` written as a value.
//!
//! Outside a `for` head and a slice argument, a range is the prelude's `Range<T>` over
//! its integer element type, an ordinary struct value that binds, passes and returns,
//! and an `Iterator` through its own `impl`. `.rev()` and `.step(n)` are its prelude
//! methods, answering `Rev<T>` and `Step<T>`.
//!
//! The walk is written in the prelude against two intrinsics a generic body could not
//! otherwise express, typed here for the prelude alone.

use ast_types::{Expr, PRELUDE_MODULE};
use shared_types::Span;

use super::TypeChecker;
use super::iteration::constant_stride;
use crate::errors::TypeError;
use crate::types::Type;

const RANGE_STRUCT: &str = "Range";
const REV_STRUCT: &str = "Rev";
const REV_METHOD: &str = "rev";
const STEP_METHOD: &str = "step";
/// `x.__to_u64()`: `x as u64`, sign-extended.
const TO_U64_INTRINSIC: &str = "__to_u64";
/// `x.__offset(k)`: `x` plus the `u64` `k`, modulo the width of `x`'s type.
const OFFSET_INTRINSIC: &str = "__offset";

/// The diagnostic for a method `struct_name` does not have. `.rev()` and `.step(n)` are
/// defined on ranges alone, so on anything else the error names the range
/// spelling rather than a method the language never gives that type.
pub(crate) fn missing_method(struct_name: String, method: &str, span: Span) -> TypeError {
    match method {
        REV_METHOD => TypeError::RevOnNonRange { span },
        STEP_METHOD => TypeError::StepOnNonRange { span },
        _ => TypeError::MethodNotFound {
            struct_name,
            method_name: method.to_string(),
            span,
        },
    }
}

impl TypeChecker {
    /// Type a range written as a value: `Range<T>` over the bounds' integer type.
    ///
    /// A `Range<T>` the value is expected to be types an unsuffixed literal bound. A
    /// program checked without the standard library's `Range` has no range value, and the
    /// expression is the error it always was.
    pub(crate) fn check_range_value(
        &mut self,
        start: &Expr,
        end: &Expr,
        span: Span,
        expected: Option<&Type>,
    ) -> Option<Type> {
        if self.struct_modules.get(RANGE_STRUCT) != Some(&PRELUDE_MODULE) {
            let _ = self.check_expr(start, None);
            let _ = self.check_expr(end, None);
            self.record_error(TypeError::RangeNotAllowed { span });
            return Some(Type::Unknown);
        }
        let hint = match expected {
            Some(Type::Struct(name)) => self
                .instance_parts(name)
                .filter(|(base, _)| base == RANGE_STRUCT)
                .and_then(|(_, args)| args.into_iter().next()),
            _ => None,
        };
        let element = self.check_range_bounds(start, end, hint.as_ref());
        if !element.is_integer() {
            return Some(Type::Unknown);
        }
        Some(self.instantiate_generic(RANGE_STRUCT, &[element], span))
    }

    /// Refuse a constant stride below one in `.step(n)` on a range value, as the `for`-head
    /// form does. The prelude's check stays for a stride known only at run time.
    pub(crate) fn check_range_value_stride(&mut self, receiver: &str, method: &str, args: &[Expr]) {
        let [stride] = args else {
            return;
        };
        let is_range = self.instance_parts(receiver).is_some_and(|(base, _)| {
            matches!(base.as_str(), RANGE_STRUCT | REV_STRUCT)
                && self.struct_modules.get(&base) == Some(&PRELUDE_MODULE)
        });
        if method != STEP_METHOD || !is_range {
            return;
        }
        if let Some(step) = constant_stride(stride).filter(|value| *value <= 0) {
            self.record_error(TypeError::RangeStepNotPositive {
                step,
                span: stride.span(),
            });
        }
    }

    /// Type a call of one of the prelude's range intrinsics on a value of type parameter
    /// `param`, or answer `None` when `method` is not one or the call is outside the
    /// prelude. Only the prelude builds a `Range`, and only over integers, so every
    /// instance the body is lowered for has an integer `param`.
    pub(crate) fn check_range_intrinsic(
        &mut self,
        param: &str,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> Option<Type> {
        if !self.in_prelude() {
            return None;
        }
        match method {
            TO_U64_INTRINSIC => {
                self.check_call_args(args, &[], span);
                Some(Type::U64)
            }
            OFFSET_INTRINSIC => {
                self.check_call_args(args, &[Type::U64], span);
                Some(Type::Generic(param.to_string()))
            }
            _ => None,
        }
    }
}
