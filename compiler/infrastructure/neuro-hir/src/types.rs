// Resolved type representation

use std::fmt;

/// A fully resolved Neuro type as it appears in the HIR.
///
/// Unlike the surface [`ast_types::Type`], a `HirType` is the *result* of type
/// checking: every named annotation has been resolved to a concrete type and
/// there is deliberately no `Unknown` / error-recovery variant. A program that
/// reaches the HIR has type-checked successfully, so the HIR contract is
/// allowed to assume well-typedness.
///
/// The variant set mirrors the resolved types the semantic analyzer produces
/// today (§1.2). Composite future types (tensors, generics) are intentionally
/// absent until the language gains them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirType {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    /// Half-precision floats (§1.2): narrow scalar contract, no arithmetic.
    F16,
    BF16,
    F32,
    F64,
    Bool,
    /// A single Unicode scalar value (§1.2).
    Char,
    String,
    /// The unit / no-value type; the resolved type of statements and of
    /// functions with no declared return type.
    Void,
    /// A user-defined struct, identified by name (nominal typing).
    Struct(String),
    /// A user-defined enum, identified by name (nominal typing, §3.5). A tagged
    /// union; its variant layout lives on the [`crate::HirEnum`] item.
    Enum(String),
    /// Borrow `&T` (§2.4) / `&mut T` (§2.5). `mutable` distinguishes a
    /// write-capable `&mut T` from a read-only `&T`.
    Reference {
        inner: Box<HirType>,
        mutable: bool,
    },
    /// Fixed-size array `[T; N]` (§3.1): `size` elements of `element`.
    Array {
        element: Box<HirType>,
        size: usize,
    },
    /// Anonymous tuple `(T1, T2, ...)` (§3.2): a positionally-indexed, heterogeneous
    /// aggregate with at least two elements.
    Tuple(Vec<HirType>),
    /// A function value: parameter types and return type.
    Function {
        params: Vec<HirType>,
        ret: Box<HirType>,
    },
}

impl HirType {
    /// The referent of a reference type, or the type itself when it is not a
    /// reference. Mirrors the auto-deref helper the frontend uses (§2.4).
    pub fn referent(&self) -> &HirType {
        match self {
            HirType::Reference { inner, .. } => inner,
            other => other,
        }
    }

    /// Whether this is a reference type (`&T` or `&mut T`).
    pub fn is_reference(&self) -> bool {
        matches!(self, HirType::Reference { .. })
    }
}

impl fmt::Display for HirType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HirType::I8 => write!(f, "i8"),
            HirType::I16 => write!(f, "i16"),
            HirType::I32 => write!(f, "i32"),
            HirType::I64 => write!(f, "i64"),
            HirType::U8 => write!(f, "u8"),
            HirType::U16 => write!(f, "u16"),
            HirType::U32 => write!(f, "u32"),
            HirType::U64 => write!(f, "u64"),
            HirType::F16 => write!(f, "f16"),
            HirType::BF16 => write!(f, "bf16"),
            HirType::F32 => write!(f, "f32"),
            HirType::F64 => write!(f, "f64"),
            HirType::Bool => write!(f, "bool"),
            HirType::Char => write!(f, "char"),
            HirType::String => write!(f, "string"),
            HirType::Void => write!(f, "void"),
            HirType::Struct(name) => write!(f, "{}", name),
            HirType::Enum(name) => write!(f, "{}", name),
            HirType::Reference { inner, mutable } => {
                if *mutable {
                    write!(f, "&mut {}", inner)
                } else {
                    write!(f, "&{}", inner)
                }
            }
            HirType::Array { element, size } => write!(f, "[{}; {}]", element, size),
            HirType::Tuple(elements) => {
                write!(f, "(")?;
                for (i, el) in elements.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", el)?;
                }
                write!(f, ")")
            }
            HirType::Function { params, ret } => {
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
