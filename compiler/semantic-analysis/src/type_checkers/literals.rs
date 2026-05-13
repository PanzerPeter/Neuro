use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;
use shared_types::{IntSuffix, Span};

impl TypeChecker {
    /// Check if an integer literal fits within the range of a target type
    pub(crate) fn check_integer_range(&self, value: i64, target_ty: &Type) -> bool {
        match target_ty {
            Type::I8 => value >= i8::MIN as i64 && value <= i8::MAX as i64,
            Type::I16 => value >= i16::MIN as i64 && value <= i16::MAX as i64,
            Type::I32 => value >= i32::MIN as i64 && value <= i32::MAX as i64,
            Type::I64 => true, // All i64 values fit in i64
            Type::U8 => value >= 0 && value <= u8::MAX as i64,
            Type::U16 => value >= 0 && value <= u16::MAX as i64,
            Type::U32 => value >= 0 && value <= u32::MAX as i64,
            Type::U64 => value >= 0, // Positive i64 values fit in u64
            _ => false,              // Not an integer type
        }
    }

    /// Infer the type of an integer literal based on expected type
    /// Returns the inferred type and whether it's valid
    pub(crate) fn infer_integer_type(
        &mut self,
        value: i64,
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
        value: i64,
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
