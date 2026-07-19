// Backend-local type model for code generation decisions

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Type {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    /// Half-precision floats, lowered to LLVM `half` / `bfloat`. Scalar
    /// contract is storage + `==`/`!=` + `as`-cast only; no arithmetic.
    F16,
    BF16,
    F32,
    F64,
    Bool,
    /// A single Unicode scalar value, lowered to a 32-bit integer.
    Char,
    String,
    Void,
    Function {
        params: Vec<Type>,
        ret: Box<Type>,
    },
    /// User-defined struct, identified by name. Field layout is resolved via the
    /// CodegenContext struct_defs table rather than embedding it in the type.
    Struct(std::string::String),
    /// User-defined enum, identified by name. Lowered to a tagged union
    /// `{ i32 tag, [W x i64] payload }`; `W` is resolved from the enum-word table
    /// the `TypeMapper` holds, so the type need not carry the layout itself.
    Enum(std::string::String),
    /// Immutable borrow `&T`. Lowered to an opaque LLVM pointer; the
    /// referent type drives auto-deref of method/field receivers. The one exception
    /// is a reference to a [`Type::DynObject`], which is a two-word fat pointer.
    Reference(Box<Type>),
    /// A dynamic-dispatch trait object `dyn Trait`, identified by trait name.
    /// Unsized on its own: it is lowered only as the referent of a [`Type::Reference`],
    /// which becomes a `{ data pointer, vtable pointer }` fat pointer.
    DynObject(std::string::String),
    /// Fixed-size array `[T; N]`. Lowered to an LLVM `[N x T]` aggregate.
    Array {
        element: Box<Type>,
        size: usize,
    },
    /// Anonymous tuple `(T1, T2, ...)`. Lowered to an anonymous LLVM struct
    /// `{ T1, T2, ... }`; element access is `extractvalue` by constant index.
    Tuple(Vec<Type>),
}

impl Type {
    /// Lower a resolved HIR type to the backend's codegen type. The HIR is already
    /// fully type-checked, so every variant maps directly with no name resolution.
    pub(crate) fn from_hir(hir_ty: &neuro_hir::HirType) -> Self {
        use neuro_hir::HirType;
        match hir_ty {
            HirType::I8 => Type::I8,
            HirType::I16 => Type::I16,
            HirType::I32 => Type::I32,
            HirType::I64 => Type::I64,
            HirType::U8 => Type::U8,
            HirType::U16 => Type::U16,
            HirType::U32 => Type::U32,
            HirType::U64 => Type::U64,
            HirType::F16 => Type::F16,
            HirType::BF16 => Type::BF16,
            HirType::F32 => Type::F32,
            HirType::F64 => Type::F64,
            HirType::Bool => Type::Bool,
            HirType::Char => Type::Char,
            HirType::String => Type::String,
            HirType::Void => Type::Void,
            HirType::Struct(name) => Type::Struct(name.clone()),
            HirType::Enum(name) => Type::Enum(name.clone()),
            // A newtype is transparent at runtime: erase it to its inner type
            // so codegen never needs to know a newtype exists.
            HirType::Newtype { inner, .. } => Type::from_hir(inner),
            HirType::DynObject(name) => Type::DynObject(name.clone()),
            HirType::Reference { inner, .. } => Type::Reference(Box::new(Type::from_hir(inner))),
            HirType::Array { element, size } => Type::Array {
                element: Box::new(Type::from_hir(element)),
                size: *size,
            },
            HirType::Tuple(elements) => Type::Tuple(elements.iter().map(Type::from_hir).collect()),
            HirType::Function { params, ret } => Type::Function {
                params: params.iter().map(Type::from_hir).collect(),
                ret: Box::new(Type::from_hir(ret)),
            },
        }
    }

    /// The referent of a reference type, or the type itself otherwise. Used to
    /// auto-deref `&T` receivers when resolving builtin methods.
    pub(crate) fn referent(&self) -> &Type {
        match self {
            Type::Reference(inner) => inner,
            other => other,
        }
    }

    pub(crate) fn is_integer(&self) -> bool {
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

    pub(crate) fn is_unsigned_int(&self) -> bool {
        matches!(self, Type::U8 | Type::U16 | Type::U32 | Type::U64)
    }

    /// Whether this type lowers to an LLVM integer and so uses the integer cast
    /// path: any integer plus `char` (a 32-bit code point). Distinct from
    /// `is_integer`, which excludes `char` for the language-level numeric rules.
    pub(crate) fn is_int_like(&self) -> bool {
        self.is_integer() || matches!(self, Type::Char)
    }

    /// Whether an int-like value zero-extends (vs sign-extends) when widened in a
    /// cast: unsigned integers and `char` (code points are non-negative).
    pub(crate) fn is_unsigned_like(&self) -> bool {
        self.is_unsigned_int() || matches!(self, Type::Char)
    }

    /// Whether this type lowers to an LLVM floating-point value. Unlike the
    /// semantic predicate, this **includes** `f16`/`bf16`: at the LLVM level they
    /// are floats (`half`/`bfloat`), so equality and `as`-cast lowering route
    /// through the float instructions. Arithmetic never reaches here — semantic
    /// analysis rejects it for half-precision.
    pub(crate) fn is_float(&self) -> bool {
        matches!(self, Type::F16 | Type::BF16 | Type::F32 | Type::F64)
    }
}
