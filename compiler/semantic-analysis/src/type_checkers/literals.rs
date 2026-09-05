use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;
use shared_types::{FloatSuffix, IntSuffix, Span};

impl TypeChecker {
    /// Whether an integer literal's value fits the range of a target type.
    ///
    /// The value is an `i128` so both ends of every type are expressible: `u64::MAX`
    /// exceeds an `i64`, and the most negative value of a signed type is written as a
    /// negation whose magnitude is one past that type's maximum.
    pub(crate) fn check_integer_range(&self, value: i128, target_ty: &Type) -> bool {
        let (min, max) = match target_ty {
            Type::I8 => (i8::MIN as i128, i8::MAX as i128),
            Type::I16 => (i16::MIN as i128, i16::MAX as i128),
            Type::I32 => (i32::MIN as i128, i32::MAX as i128),
            Type::I64 => (i64::MIN as i128, i64::MAX as i128),
            Type::U8 => (0, u8::MAX as i128),
            Type::U16 => (0, u16::MAX as i128),
            Type::U32 => (0, u32::MAX as i128),
            Type::U64 => (0, u64::MAX as i128),
            _ => return false, // Not an integer type
        };
        value >= min && value <= max
    }

    /// Whether `ty` is a signed integer type, and so has a most negative value one
    /// past its maximum in magnitude.
    pub(crate) fn is_signed_integer(ty: &Type) -> bool {
        matches!(ty, Type::I8 | Type::I16 | Type::I32 | Type::I64)
    }

    /// Infer the type of an integer literal based on expected type
    /// Returns the inferred type and whether it's valid
    pub(crate) fn infer_integer_type(
        &mut self,
        value: i128,
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        if let Some(exp_ty) = expected {
            // If expected type is an integer type, try to use it
            if exp_ty.is_integer() {
                if self.check_integer_range(value, exp_ty) {
                    return exp_ty.clone();
                } else {
                    // Value doesn't fit in expected type
                    self.record_error(TypeError::IntegerLiteralOutOfRange {
                        value,
                        ty: exp_ty.clone(),
                        span,
                    });
                    return Type::Unknown;
                }
            }
        }

        // No expected type or expected type is not integer: default to i32
        // Also validate that the value fits in i32
        if self.check_integer_range(value, &Type::I32) {
            Type::I32
        } else {
            // Value doesn't fit in default i32, report an error
            self.record_error(TypeError::IntegerLiteralOutOfRange {
                value,
                ty: Type::I32,
                span,
            });
            Type::Unknown
        }
    }

    /// Resolve the type for a suffix-annotated integer literal, range-checking
    /// the value against the suffix type.
    pub(crate) fn infer_suffixed_integer_type(
        &mut self,
        value: i128,
        suffix: &IntSuffix,
        span: Span,
    ) -> Type {
        let ty = suffix_to_type(suffix);
        if self.check_integer_range(value, &ty) {
            ty
        } else {
            self.record_error(TypeError::IntegerLiteralOutOfRange {
                value,
                ty: ty.clone(),
                span,
            });
            Type::Unknown
        }
    }

    /// Infer the type of a float literal based on expected type
    pub(crate) fn infer_float_type(&self, expected: Option<&Type>) -> Type {
        if let Some(exp_ty) = expected {
            // If expected type is a float type, use it
            if exp_ty.is_float() {
                return exp_ty.clone();
            }
        }

        // Default to f64
        Type::F64
    }

    /// Resolve the type for a suffix-annotated float literal. The suffix
    /// overrides contextual inference; mismatches with an explicit annotation
    /// surface through the normal assignment type-check path.
    pub(crate) fn infer_suffixed_float_type(&self, suffix: &FloatSuffix) -> Type {
        float_suffix_to_type(suffix)
    }
}

pub(crate) fn float_suffix_to_type(suffix: &FloatSuffix) -> Type {
    match suffix {
        FloatSuffix::F16 => Type::F16,
        FloatSuffix::BF16 => Type::BF16,
        FloatSuffix::F32 => Type::F32,
        FloatSuffix::F64 => Type::F64,
    }
}

pub(crate) fn suffix_to_type(suffix: &IntSuffix) -> Type {
    match suffix {
        IntSuffix::I8 => Type::I8,
        IntSuffix::I16 => Type::I16,
        IntSuffix::I32 => Type::I32,
        IntSuffix::I64 => Type::I64,
        IntSuffix::U8 => Type::U8,
        IntSuffix::U16 => Type::U16,
        IntSuffix::U32 => Type::U32,
        IntSuffix::U64 => Type::U64,
    }
}
