// Type checking for interpolated string literals.

use ast_types::{Expr, InterpPart};
use shared_types::{
    FormatAlign, FormatKind, FormatSpec, Span, MAX_FORMAT_PRECISION, MAX_FORMAT_WIDTH,
};

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;

impl TypeChecker {
    /// Check every hole of an interpolated literal and validate its format spec
    /// against the hole's resolved type. Always yields `string`: a spec error
    /// disqualifies the hole, not the literal, so checking continues.
    pub(crate) fn check_interp_string(&mut self, parts: &[InterpPart]) -> Option<Type> {
        for part in parts {
            let InterpPart::Formatted { expr, spec, span } = part else {
                continue;
            };
            let Some(ty) = self.check_expr(expr, None) else {
                continue;
            };
            // `{r}` on a `&T` renders the referent: a borrow has no rendering of
            // its own, and requiring `*r` at every hole would be noise.
            let rendered = ty.referent().clone();
            let hole_span = expr_span_or(expr, *span);

            if !is_formattable(&rendered) {
                self.record_error(TypeError::UnformattableType {
                    ty: rendered,
                    span: hole_span,
                });
                continue;
            }

            if let Some(spec) = spec {
                self.check_format_spec(spec, &rendered, hole_span);
            }
        }

        Some(Type::String)
    }

    /// Reject spec/type pairs no rendering can satisfy, and specs whose written
    /// width or precision is past the sanity ceiling.
    fn check_format_spec(&mut self, spec: &FormatSpec, ty: &Type, span: Span) {
        let kind_hint = match spec.kind {
            FormatKind::Fixed | FormatKind::Scientific if !ty.is_float() => {
                Some("fixed-point and scientific notation apply to floats")
            }
            FormatKind::Decimal
            | FormatKind::LowerHex
            | FormatKind::UpperHex
            | FormatKind::Binary
            | FormatKind::Octal
                if !ty.is_integer() =>
            {
                Some("radix formatting applies to integers")
            }
            _ => None,
        };
        if let Some(hint) = kind_hint {
            self.record_error(TypeError::FormatSpecMismatch {
                spec: describe_kind(spec.kind).to_string(),
                ty: ty.clone(),
                hint: hint.to_string(),
                span,
            });
        }

        if spec.plus_sign {
            // An unsigned value has no sign position to fill, so `+` would have
            // nothing to render; only signed integers and floats accept it.
            if !ty.is_signed_int() && !ty.is_float() {
                self.record_error(TypeError::FormatSpecMismatch {
                    spec: "+".to_string(),
                    ty: ty.clone(),
                    hint: "the `+` sign flag applies to signed integers and floats".to_string(),
                    span,
                });
            } else if matches!(
                spec.kind,
                FormatKind::LowerHex
                    | FormatKind::UpperHex
                    | FormatKind::Binary
                    | FormatKind::Octal
            ) {
                // Radix rendering is two's-complement over the value's own width,
                // so there is no sign position for `+` to occupy.
                self.record_error(TypeError::FormatSpecMismatch {
                    spec: "+".to_string(),
                    ty: ty.clone(),
                    hint: "the `+` sign flag does not combine with hex, binary, or octal"
                        .to_string(),
                    span,
                });
            }
        }

        if spec.zero_pad && matches!(spec.align, Some(FormatAlign::Left | FormatAlign::Center)) {
            self.record_error(TypeError::FormatSpecMismatch {
                spec: "0".to_string(),
                ty: ty.clone(),
                hint: "zero padding fills to the left, so it cannot combine with `<` or `^`"
                    .to_string(),
                span,
            });
        }

        if let Some(width) = spec.width.filter(|w| *w > MAX_FORMAT_WIDTH) {
            self.record_error(TypeError::FormatWidthTooLarge {
                width,
                max: MAX_FORMAT_WIDTH,
                span,
            });
        }

        if let Some(precision) = spec.precision.filter(|p| *p > MAX_FORMAT_PRECISION) {
            self.record_error(TypeError::FormatPrecisionTooLarge {
                precision,
                max: MAX_FORMAT_PRECISION,
                span,
            });
        }
    }
}

/// The types interpolation knows how to render. Aggregates wait on `@derive(Debug)`.
fn is_formattable(ty: &Type) -> bool {
    ty.is_numeric() || matches!(ty, Type::Bool | Type::Char | Type::String)
}

/// The spec text to quote back in a diagnostic for a format kind.
fn describe_kind(kind: FormatKind) -> &'static str {
    match kind {
        FormatKind::Default => "",
        FormatKind::Debug => "?",
        FormatKind::Fixed => ".N",
        FormatKind::Scientific => "e",
        FormatKind::Decimal => "d",
        FormatKind::LowerHex => "x",
        FormatKind::UpperHex => "X",
        FormatKind::Binary => "b",
        FormatKind::Octal => "o",
    }
}

/// Prefer the hole expression's own span; fall back to the whole hole when the
/// expression carries none more precise.
fn expr_span_or(expr: &Expr, fallback: Span) -> Span {
    let span = expr.span();
    if span.start == span.end {
        fallback
    } else {
        span
    }
}
