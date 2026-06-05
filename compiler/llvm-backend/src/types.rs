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
    F32,
    F64,
    Bool,
    String,
    Void,
    Function {
        params: Vec<Type>,
        ret: Box<Type>,
    },
    /// User-defined struct, identified by name. Field layout is resolved via the
    /// CodegenContext struct_defs table rather than embedding it in the type.
    Struct(std::string::String),
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
                "f32" => Type::F32,
                "f64" => Type::F64,
                "bool" => Type::Bool,
                "string" => Type::String,
                name => Type::Struct(name.to_string()),
            },
            ast_types::Type::Tensor { .. } => {
                unimplemented!("Tensors not implemented in scalar backend")
            }
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

    pub(crate) fn is_float(&self) -> bool {
        matches!(self, Type::F32 | Type::F64)
    }
}
