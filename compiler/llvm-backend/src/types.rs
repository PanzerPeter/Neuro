// Neuro Programming Language - LLVM Backend
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
    /// Half-precision floats (§1.2), lowered to LLVM `half` / `bfloat`. Scalar
    /// contract is storage + `==`/`!=` + `as`-cast only; no arithmetic.
    F16,
    BF16,
    F32,
    F64,
    Bool,
    /// A single Unicode scalar value (§1.2), lowered to a 32-bit integer.
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
    /// Immutable borrow `&T` (§2.4). Lowered to an opaque LLVM pointer; the
    /// referent type drives auto-deref of method/field receivers.
    Reference(Box<Type>),
    /// Fixed-size array `[T; N]` (§3.1). Lowered to an LLVM `[N x T]` aggregate.
    Array {
        element: Box<Type>,
        size: usize,
    },
}

impl Type {
    pub(crate) fn from_ast(ast_ty: &ast_types::Type) -> Self {
        match ast_ty {
            ast_types::Type::Named(ident) => match ident.name.as_str() {
                "i8" => Type::I8,
                "i16" => Type::I16,
                "i32" => Type::I32,
                "i64" => Type::I64,
                "u8" => Type::U8,
                "u16" => Type::U16,
                "u32" => Type::U32,
                "u64" => Type::U64,
                "f16" => Type::F16,
                "bf16" => Type::BF16,
                "f32" => Type::F32,
                "f64" => Type::F64,
                "bool" => Type::Bool,
                "char" => Type::Char,
                "string" => Type::String,
                name => Type::Struct(name.to_string()),
            },
            ast_types::Type::Reference { inner, .. } => {
                Type::Reference(Box::new(Type::from_ast(inner)))
            }
            ast_types::Type::Array { element, size, .. } => Type::Array {
                element: Box::new(Type::from_ast(element)),
                size: *size,
            },
            // Tensor types (Phase 3) never reach the backend: semantic analysis rejects
            // every `Tensor<...>` annotation as an unknown type before codegen runs, so
            // this arm is an invariant assertion rather than a missing-feature stub.
            ast_types::Type::Tensor { .. } => {
                unreachable!("tensor types are rejected by semantic analysis before codegen")
            }
        }
    }

    /// The referent of a reference type, or the type itself otherwise. Used to
    /// auto-deref `&T` receivers when resolving builtin methods (§2.4).
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
    /// analysis rejects it for half-precision (§1.2).
    pub(crate) fn is_float(&self) -> bool {
        matches!(self, Type::F16 | Type::BF16 | Type::F32 | Type::F64)
    }
}
