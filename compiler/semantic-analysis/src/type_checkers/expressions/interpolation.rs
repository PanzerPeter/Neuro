// Type checking for interpolated string literals.

use ast_types::{Expr, InterpPart};
use shared_types::{
    FormatAlign, FormatKind, FormatSpec, MAX_FORMAT_PRECISION, MAX_FORMAT_WIDTH, Span,
};

use super::TypeChecker;
use crate::errors::TypeError;
use crate::type_checkers::derivable::DEBUG_TRAIT;
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

            let kind = spec.as_ref().map(|s| s.kind).unwrap_or(FormatKind::Default);
            // A struct gets its own diagnostic: the generic one lists the renderable
            // types, which leaves the reader no way to tell "add the derive" apart from
            // "this type will never render".
            if let Type::Struct(name) = &rendered {
                if let Some(hint) = self.struct_render_obstacle(name, kind) {
                    self.record_error(TypeError::UnrenderableStruct {
                        name: name.clone(),
                        hint,
                        span: hole_span,
                    });
                    continue;
                }
            } else if kind == FormatKind::Debug && self.debugs_by_trait(&rendered) {
                // Rendered by its `Debug` impl, or per instance under a `T: Debug` bound.
            } else if !self.is_formattable(&rendered, kind) {
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

    /// The types interpolation knows how to render under `kind`. Structs answer through
    /// [`Self::struct_render_obstacle`], which also says *why* one cannot render.
    fn is_formattable(&self, ty: &Type, kind: FormatKind) -> bool {
        if let Type::Struct(name) = ty {
            return self.struct_render_obstacle(name, kind).is_none();
        }
        ty.is_numeric()
            || ty.is_half_float()
            || matches!(ty, Type::Bool | Type::Char | Type::String)
    }

    /// Whether a non-struct `ty` renders under `{x:?}` through `Debug` rather than as a
    /// builtin: an enum or newtype with a hand-written impl, or a type parameter bounded by
    /// `Debug`, whose instances each render their own way.
    fn debugs_by_trait(&self, ty: &Type) -> bool {
        matches!(ty, Type::Enum(_) | Type::Newtype(_) | Type::Generic(_))
            && self.satisfies_bound(ty, DEBUG_TRAIT)
    }

    /// What stops struct `name` rendering under `kind`, or `None` when nothing does.
    ///
    /// A struct has no `Display` form, so it renders under `{x:?}` and only when it
    /// implements `Debug`, by derive or by hand. Both refusals point at the missing half
    /// rather than at the list of renderable primitives, which a struct can never join.
    fn struct_render_obstacle(&self, name: &str, kind: FormatKind) -> Option<String> {
        if !self.satisfies_bound(&Type::Struct(name.to_string()), DEBUG_TRAIT) {
            return Some(format!(
                "a struct renders only through its debug form; add `@derive(Debug)` to `{}` \
                 or implement `Debug` for it",
                name
            ));
        }
        if kind != FormatKind::Debug {
            return Some(
                "a struct has no display form; add the `:?` specifier to the hole to use \
                 its debug rendering"
                    .to_string(),
            );
        }
        None
    }

    /// Reject spec/type pairs no rendering can satisfy, and specs whose written
    /// width or precision is past the sanity ceiling.
    fn check_format_spec(&mut self, spec: &FormatSpec, ty: &Type, span: Span) {
        let kind_hint = match spec.kind {
            FormatKind::Fixed | FormatKind::Scientific if !renders_as_float(ty) => {
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
            if !ty.is_signed_int() && !renders_as_float(ty) {
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

/// Whether a hole of `ty` renders through the float path: a half-precision value is
/// widened to `f64` and printed exactly, as an `f32` is.
fn renders_as_float(ty: &Type) -> bool {
    ty.is_float() || ty.is_half_float()
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
