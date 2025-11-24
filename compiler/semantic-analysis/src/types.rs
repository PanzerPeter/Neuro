// NEURO Programming Language - Semantic Analysis
// Type system definitions and type predicates

use std::fmt;

/// Type representation for semantic analysis
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    // Signed integers (ordered by bit width)
    I8,
    I16,
    I32,
    I64,
    // Unsigned integers (ordered by bit width)
    U8,
    U16,
    U32,
    U64,
    // Floating point
    F32,
    F64,
    // Other types
    Bool,
    String,
    Void,
    Function { params: Vec<Type>, ret: Box<Type> },
    Unknown,
}

impl Type {
    /// Check if this type is compatible with another type
    ///
    /// Type compatibility follows strict typing rules:
    /// - Signed and unsigned integers are NOT compatible (even of same width)
    /// - Different integer widths are NOT compatible
    /// - No implicit conversions allowed
    pub(crate) fn is_compatible_with(&self, other: &Type) -> bool {
        match (self, other) {
            // Exact matches for all primitive types
            (Type::I8, Type::I8)
            | (Type::I16, Type::I16)
            | (Type::I32, Type::I32)
            | (Type::I64, Type::I64)
            | (Type::U8, Type::U8)
            | (Type::U16, Type::U16)
            | (Type::U32, Type::U32)
            | (Type::U64, Type::U64)
            | (Type::F32, Type::F32)
            | (Type::F64, Type::F64)
            | (Type::Bool, Type::Bool)
            | (Type::String, Type::String)
            | (Type::Void, Type::Void) => true,

            // Function types must match exactly
            (
                Type::Function {
                    params: p1,
                    ret: r1,
                },
                Type::Function {
                    params: p2,
                    ret: r2,
                },
            ) => {
                p1.len() == p2.len()
                    && p1
                        .iter()
                        .zip(p2.iter())
                        .all(|(a, b)| a.is_compatible_with(b))
                    && r1.is_compatible_with(r2)
            }

            // Unknown type for error recovery
            (Type::Unknown, _) | (_, Type::Unknown) => true,

            _ => false,
        }
    }

    /// Check if this is a numeric type (any integer or float)
    pub(crate) fn is_numeric(&self) -> bool {
        matches!(
            self,
            Type::I8
                | Type::I16
                | Type::I32
                | Type::I64
                | Type::U8
                | Type::U16
                | Type::U32
                | Type::U64
                | Type::F32
                | Type::F64
        )
    }

    /// Check if this is an integer type (signed or unsigned)
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            Type::I8
                | Type::I16
                | Type::I32
                | Type::I64
                | Type::U8
                | Type::U16
                | Type::U32
                | Type::U64
        )
    }

    /// Check if this is a signed integer type
    pub fn is_signed_int(&self) -> bool {
        matches!(self, Type::I8 | Type::I16 | Type::I32 | Type::I64)
    }

    /// Check if this is an unsigned integer type
    pub fn is_unsigned_int(&self) -> bool {
        matches!(self, Type::U8 | Type::U16 | Type::U32 | Type::U64)
    }

    /// Check if this is a floating-point type
    pub fn is_float(&self) -> bool {
        matches!(self, Type::F32 | Type::F64)
    }

    /// Check if this is a boolean type
    pub(crate) fn is_bool(&self) -> bool {
        matches!(self, Type::Bool)
    }

    /// Check if this is a string type
    #[allow(dead_code)]
    pub(crate) fn is_string(&self) -> bool {
        matches!(self, Type::String)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::I8 => write!(f, "i8"),
            Type::I16 => write!(f, "i16"),
            Type::I32 => write!(f, "i32"),
            Type::I64 => write!(f, "i64"),
            Type::U8 => write!(f, "u8"),
            Type::U16 => write!(f, "u16"),
            Type::U32 => write!(f, "u32"),
            Type::U64 => write!(f, "u64"),
            Type::F32 => write!(f, "f32"),
            Type::F64 => write!(f, "f64"),
            Type::Bool => write!(f, "bool"),
            Type::String => write!(f, "string"),
            Type::Void => write!(f, "void"),
            Type::Unknown => write!(f, "<error>"),
            Type::Function { params, ret } => {
                write!(f, "fn(")?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", param)?;
                }
                write!(f, ") -> {}", ret)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_compatibility() {
        assert!(Type::I32.is_compatible_with(&Type::I32));
        assert!(Type::Bool.is_compatible_with(&Type::Bool));
        assert!(!Type::I32.is_compatible_with(&Type::Bool));
        assert!(!Type::F64.is_compatible_with(&Type::I32));
    }

    #[test]
    fn type_predicates() {
        assert!(Type::I32.is_numeric());
        assert!(Type::F64.is_numeric());
        assert!(!Type::Bool.is_numeric());

        assert!(Type::I32.is_integer());
        assert!(!Type::F64.is_integer());

        assert!(Type::Bool.is_bool());
        assert!(!Type::I32.is_bool());
    }

    #[test]
    fn extended_type_compatibility() {
        // All new integer types should be compatible with themselves
        assert!(Type::I8.is_compatible_with(&Type::I8));
        assert!(Type::I16.is_compatible_with(&Type::I16));
        assert!(Type::U8.is_compatible_with(&Type::U8));
        assert!(Type::U16.is_compatible_with(&Type::U16));
        assert!(Type::U32.is_compatible_with(&Type::U32));
        assert!(Type::U64.is_compatible_with(&Type::U64));

        // Signed and unsigned of same width should NOT be compatible
        assert!(!Type::I8.is_compatible_with(&Type::U8));
        assert!(!Type::I16.is_compatible_with(&Type::U16));
        assert!(!Type::I32.is_compatible_with(&Type::U32));
        assert!(!Type::I64.is_compatible_with(&Type::U64));

        // Different widths should NOT be compatible (even same signedness)
        assert!(!Type::I8.is_compatible_with(&Type::I16));
        assert!(!Type::I16.is_compatible_with(&Type::I32));
        assert!(!Type::I32.is_compatible_with(&Type::I64));
        assert!(!Type::U8.is_compatible_with(&Type::U16));
        assert!(!Type::U16.is_compatible_with(&Type::U32));
        assert!(!Type::U32.is_compatible_with(&Type::U64));

        // Integers should NOT be compatible with floats
        assert!(!Type::I8.is_compatible_with(&Type::F32));
        assert!(!Type::U32.is_compatible_with(&Type::F64));

        // Integers should NOT be compatible with bool
        assert!(!Type::I16.is_compatible_with(&Type::Bool));
        assert!(!Type::U64.is_compatible_with(&Type::Bool));
    }

    #[test]
    fn extended_type_predicates() {
        // Test is_numeric for all integer types
        assert!(Type::I8.is_numeric());
        assert!(Type::I16.is_numeric());
        assert!(Type::I32.is_numeric());
        assert!(Type::I64.is_numeric());
        assert!(Type::U8.is_numeric());
        assert!(Type::U16.is_numeric());
        assert!(Type::U32.is_numeric());
        assert!(Type::U64.is_numeric());
        assert!(Type::F32.is_numeric());
        assert!(Type::F64.is_numeric());
        assert!(!Type::Bool.is_numeric());
        assert!(!Type::Void.is_numeric());

        // Test is_integer for all integer types
        assert!(Type::I8.is_integer());
        assert!(Type::I16.is_integer());
        assert!(Type::I32.is_integer());
        assert!(Type::I64.is_integer());
        assert!(Type::U8.is_integer());
        assert!(Type::U16.is_integer());
        assert!(Type::U32.is_integer());
        assert!(Type::U64.is_integer());
        assert!(!Type::F32.is_integer());
        assert!(!Type::F64.is_integer());
        assert!(!Type::Bool.is_integer());

        // Test is_signed_int
        assert!(Type::I8.is_signed_int());
        assert!(Type::I16.is_signed_int());
        assert!(Type::I32.is_signed_int());
        assert!(Type::I64.is_signed_int());
        assert!(!Type::U8.is_signed_int());
        assert!(!Type::U16.is_signed_int());
        assert!(!Type::U32.is_signed_int());
        assert!(!Type::U64.is_signed_int());
        assert!(!Type::F32.is_signed_int());
        assert!(!Type::Bool.is_signed_int());

        // Test is_unsigned_int
        assert!(!Type::I8.is_unsigned_int());
        assert!(!Type::I16.is_unsigned_int());
        assert!(!Type::I32.is_unsigned_int());
        assert!(!Type::I64.is_unsigned_int());
        assert!(Type::U8.is_unsigned_int());
        assert!(Type::U16.is_unsigned_int());
        assert!(Type::U32.is_unsigned_int());
        assert!(Type::U64.is_unsigned_int());
        assert!(!Type::F32.is_unsigned_int());
        assert!(!Type::Bool.is_unsigned_int());

        // Test is_float
        assert!(!Type::I32.is_float());
        assert!(!Type::U32.is_float());
        assert!(Type::F32.is_float());
        assert!(Type::F64.is_float());
        assert!(!Type::Bool.is_float());
    }

    #[test]
    fn function_type_compatibility() {
        let func1 = Type::Function {
            params: vec![Type::I32, Type::Bool],
            ret: Box::new(Type::I32),
        };

        let func2 = Type::Function {
            params: vec![Type::I32, Type::Bool],
            ret: Box::new(Type::I32),
        };

        let func3 = Type::Function {
            params: vec![Type::I32],
            ret: Box::new(Type::I32),
        };

        assert!(func1.is_compatible_with(&func2));
        assert!(!func1.is_compatible_with(&func3));
    }

    #[test]
    fn string_type_compatibility() {
        // String type should only be compatible with itself
        assert!(Type::String.is_compatible_with(&Type::String));

        // String should NOT be compatible with other types
        assert!(!Type::String.is_compatible_with(&Type::I32));
        assert!(!Type::String.is_compatible_with(&Type::Bool));
        assert!(!Type::String.is_compatible_with(&Type::F64));
        assert!(!Type::String.is_compatible_with(&Type::Void));

        // Other types should NOT be compatible with String
        assert!(!Type::I32.is_compatible_with(&Type::String));
        assert!(!Type::Bool.is_compatible_with(&Type::String));
    }

    #[test]
    fn string_type_predicates() {
        // Test is_string predicate
        assert!(Type::String.is_string());

        // String is NOT numeric, integer, float, or bool
        assert!(!Type::String.is_numeric());
        assert!(!Type::String.is_integer());
        assert!(!Type::String.is_float());
        assert!(!Type::String.is_bool());
        assert!(!Type::String.is_signed_int());
        assert!(!Type::String.is_unsigned_int());

        // Other types are NOT strings
        assert!(!Type::I32.is_string());
        assert!(!Type::Bool.is_string());
        assert!(!Type::F64.is_string());
        assert!(!Type::Void.is_string());
    }
}
