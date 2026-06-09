use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;

impl TypeChecker {
    /// Convert syntax-parsing type to semantic type.
    /// Returns None if the type is unknown (error is recorded).
    pub(crate) fn resolve_type(&mut self, ty: &ast_types::Type) -> Option<Type> {
        match ty {
            ast_types::Type::Named(ident) => match ident.name.as_str() {
                // Signed integers
                "i8" => Some(Type::I8),
                "i16" => Some(Type::I16),
                "i32" => Some(Type::I32),
                "i64" => Some(Type::I64),
                // Unsigned integers
                "u8" => Some(Type::U8),
                "u16" => Some(Type::U16),
                "u32" => Some(Type::U32),
                "u64" => Some(Type::U64),
                // Floating point
                "f32" => Some(Type::F32),
                "f64" => Some(Type::F64),
                // Other types
                "bool" => Some(Type::Bool),
                "string" => Some(Type::String),
                "void" => Some(Type::Void),
                name => {
                    if self.struct_defs.contains_key(name) {
                        Some(Type::Struct(name.to_string()))
                    } else {
                        self.record_error(TypeError::UnknownTypeName {
                            name: name.to_string(),
                            span: ident.span,
                        });
                        None
                    }
                }
            },
            // Borrow `&T` (§2.4) / `&mut T` (§2.5): resolve the referent recursively,
            // preserving mutability.
            ast_types::Type::Reference { inner, mutable, .. } => {
                self.resolve_type(inner).map(|t| Type::Reference {
                    inner: Box::new(t),
                    mutable: *mutable,
                })
            }
            ast_types::Type::Tensor { span, .. } => {
                // Tensor types are Phase 3, not supported in Phase 1
                self.record_error(TypeError::UnknownTypeName {
                    name: "Tensor".to_string(),
                    span: *span,
                });
                None
            }
        }
    }
}
