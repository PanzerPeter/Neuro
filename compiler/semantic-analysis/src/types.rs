// Neuro Programming Language - Semantic Analysis
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
    /// A single Unicode scalar value (§1.2). 32-bit, `Copy`, ordered, and
    /// `as`-castable to/from integer types. Does not participate in arithmetic.
    Char,
    String,
    Void,
    Function {
        params: Vec<Type>,
        ret: Box<Type>,
    },
    /// User-defined struct type, identified by name (nominal typing).
    Struct(std::string::String),
    /// Borrow `&T` (§2.4) / `&mut T` (§2.5): a non-owning reference to `inner`.
    /// References are `Copy` and never move the borrowed value. `mutable`
    /// distinguishes a write-capable `&mut T` from a read-only `&T`.
    Reference {
        inner: Box<Type>,
        mutable: bool,
    },
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
            | (Type::Char, Type::Char)
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

            // Struct types match by name (nominal typing)
            (Type::Struct(a), Type::Struct(b)) => a == b,

            // References match when their referents match and their mutability
            // agrees (§2.4, §2.5). There is no implicit `&mut T` → `&T` coercion —
            // the language is explicit over implicit (§Design Principles).
            (
                Type::Reference {
                    inner: a,
                    mutable: am,
                },
                Type::Reference {
                    inner: b,
                    mutable: bm,
                },
            ) => am == bm && a.is_compatible_with(b),

            // Unknown type for error recovery
            (Type::Unknown, _) | (_, Type::Unknown) => true,

            _ => false,
        }
    }

    /// The referent of a reference type, or the type itself when it is not a reference.
    /// Used to auto-deref `&T` receivers in method/field resolution (§2.4).
    pub(crate) fn referent(&self) -> &Type {
        match self {
            Type::Reference { inner, .. } => inner,
            other => other,
        }
    }

    /// Normalize a string operand for equality: a `&string` slice and an owned
    /// `string` compare the same UTF-8 bytes (§2.7), so a single string reference
    /// is peeled to `string`. Other `&T` are left intact — reading them through
    /// `==` needs the deref operator (`*`).
    pub(crate) fn peel_string_ref(&self) -> Type {
        match self {
            Type::Reference { inner, .. } if matches!(**inner, Type::String) => Type::String,
            other => other.clone(),
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

    /// Check if this type can be explicitly cast from `from_type` using `as`.
    pub fn is_valid_cast(&self, from: &Type) -> bool {
        match (from, self) {
            // Integer to integer
            (t1, t2) if t1.is_integer() && t2.is_integer() => true,
            // Integer to float
            (t1, t2) if t1.is_integer() && (t2 == &Type::F32 || t2 == &Type::F64) => true,
            // Float to integer
            (t1, t2) if (t1 == &Type::F32 || t1 == &Type::F64) && t2.is_integer() => true,
            // Float to float
            (t1, t2)
                if (t1 == &Type::F32 || t1 == &Type::F64)
                    && (t2 == &Type::F32 || t2 == &Type::F64) =>
            {
                true
            }
            // Bool to integer
            (Type::Bool, t2) if t2.is_integer() => true,
            // char to/from integer, and char to char (§1.2). char is not castable
            // to/from float or bool — the only conversions are integer-valued.
            (Type::Char, t2) if t2.is_integer() => true,
            (t1, Type::Char) if t1.is_integer() => true,
            (Type::Char, Type::Char) => true,
            _ => false,
        }
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

    /// Check if this is the `char` type (§1.2).
    pub(crate) fn is_char(&self) -> bool {
        matches!(self, Type::Char)
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
            Type::Char => write!(f, "char"),
            Type::String => write!(f, "string"),
            Type::Void => write!(f, "void"),
            Type::Unknown => write!(f, "<error>"),
            Type::Struct(name) => write!(f, "{}", name),
            Type::Reference { inner, mutable } => {
                if *mutable {
                    write!(f, "&mut {}", inner)
                } else {
                    write!(f, "&{}", inner)
                }
            }
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

    fn ref_to(inner: Type) -> Type {
        Type::Reference {
            inner: Box::new(inner),
            mutable: false,
        }
    }

    fn mut_ref_to(inner: Type) -> Type {
        Type::Reference {
            inner: Box::new(inner),
            mutable: true,
        }
    }

    #[test]
    fn reference_type_compatibility_and_display() {
        let ref_str = ref_to(Type::String);
        let ref_str2 = ref_to(Type::String);
        let ref_i32 = ref_to(Type::I32);

        // References match iff their referents match (§2.4).
        assert!(ref_str.is_compatible_with(&ref_str2));
        assert!(!ref_str.is_compatible_with(&ref_i32));
        // A reference is not compatible with the bare referent type.
        assert!(!ref_str.is_compatible_with(&Type::String));

        assert_eq!(ref_str.to_string(), "&string");
        assert_eq!(ref_i32.to_string(), "&i32");

        // `referent` peels exactly one layer; a non-reference is returned unchanged.
        assert_eq!(ref_str.referent(), &Type::String);
        assert_eq!(Type::I32.referent(), &Type::I32);
    }

    #[test]
    fn mutable_and_immutable_references_are_distinct() {
        // §2.5: `&mut T` and `&T` are distinct types — no implicit coercion.
        let mut_ref = mut_ref_to(Type::I32);
        let imm_ref = ref_to(Type::I32);
        assert!(!mut_ref.is_compatible_with(&imm_ref));
        assert!(!imm_ref.is_compatible_with(&mut_ref));
        assert!(mut_ref.is_compatible_with(&mut_ref_to(Type::I32)));
        assert_eq!(mut_ref.to_string(), "&mut i32");
    }

    #[test]
    fn peel_string_ref_normalizes_string_slice_only() {
        // §2.7: `&string` (a string slice) and owned `string` are equality-comparable.
        let ref_str = ref_to(Type::String);
        assert_eq!(ref_str.peel_string_ref(), Type::String);
        assert_eq!(Type::String.peel_string_ref(), Type::String);
        // After peeling, a slice and an owned string compare compatible either way.
        assert!(ref_str
            .peel_string_ref()
            .is_compatible_with(&Type::String.peel_string_ref()));

        // Non-string references are left intact — reading them through `==` needs
        // the deref operator (`*`), so `&i32` stays incompatible.
        let ref_i32 = ref_to(Type::I32);
        assert_eq!(ref_i32.peel_string_ref(), ref_i32);
        assert!(!ref_i32
            .peel_string_ref()
            .is_compatible_with(&Type::I32.peel_string_ref()));
    }

    #[test]
    fn char_type_compatibility_cast_and_display() {
        // §1.2: char is its own type, Copy, ordered, and castable to/from integers only.
        assert!(Type::Char.is_compatible_with(&Type::Char));
        assert!(!Type::Char.is_compatible_with(&Type::I32));
        assert!(!Type::Char.is_compatible_with(&Type::String));
        assert!(Type::Char.is_char());
        assert!(!Type::I32.is_char());
        assert_eq!(Type::Char.to_string(), "char");

        // char is not numeric (no arithmetic) but is not an integer/float/bool either.
        assert!(!Type::Char.is_numeric());
        assert!(!Type::Char.is_integer());

        // Valid casts: char <-> integer, char -> char.
        assert!(Type::I32.is_valid_cast(&Type::Char)); // char as i32
        assert!(Type::U8.is_valid_cast(&Type::Char)); // char as u8
        assert!(Type::Char.is_valid_cast(&Type::I32)); // i32 as char
        assert!(Type::Char.is_valid_cast(&Type::U8)); // u8  as char
        assert!(Type::Char.is_valid_cast(&Type::Char));

        // Invalid casts: char <-> float / bool.
        assert!(!Type::F64.is_valid_cast(&Type::Char));
        assert!(!Type::Char.is_valid_cast(&Type::F64));
        assert!(!Type::Bool.is_valid_cast(&Type::Char));
        assert!(!Type::Char.is_valid_cast(&Type::Bool));
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
