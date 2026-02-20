// NEURO Programming Language - LLVM Backend
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
    F32,
    F64,
    Bool,
    String,
    Void,
    Function { params: Vec<Type>, ret: Box<Type> },
}

impl Type {
    pub(crate) fn is_unsigned_int(&self) -> bool {
        matches!(self, Type::U8 | Type::U16 | Type::U32 | Type::U64)
    }

    pub(crate) fn is_float(&self) -> bool {
        matches!(self, Type::F32 | Type::F64)
    }
}
