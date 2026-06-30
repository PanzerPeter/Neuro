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
                "f16" => Some(Type::F16),
                "bf16" => Some(Type::BF16),
                "f32" => Some(Type::F32),
                "f64" => Some(Type::F64),
                // Other types
                "bool" => Some(Type::Bool),
                "char" => Some(Type::Char),
                "string" => Some(Type::String),
                "void" => Some(Type::Void),
                name => {
                    if self.struct_defs.contains_key(name) {
                        Some(Type::Struct(name.to_string()))
                    } else if self.enum_defs.contains_key(name) {
                        Some(Type::Enum(name.to_string()))
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
            // Fixed-size array `[T; N]` (§3.1). The element must be a `Copy` scalar
            // primitive in this phase — non-Copy element arrays (strings, structs)
            // need per-element move/Drop tracking, which is a documented follow-on.
            ast_types::Type::Array {
                element,
                size,
                span,
            } => {
                let element_ty = self.resolve_type(element)?;
                if !self.is_type_copy(&element_ty) {
                    self.record_error(TypeError::NonCopyArrayElement {
                        ty: element_ty,
                        span: *span,
                    });
                    return None;
                }
                Some(Type::Array {
                    element: Box::new(element_ty),
                    size: *size,
                })
            }
            // Tuple `(T1, T2, ...)` (§3.2). Each element must be `Copy` in this phase
            // — non-Copy element tuples (e.g. holding a `string` or a non-Copy struct)
            // need per-element move/Drop tracking, a documented follow-on (mirrors the
            // array element rule).
            ast_types::Type::Tuple { elements, span } => {
                let mut resolved = Vec::with_capacity(elements.len());
                for element in elements {
                    let element_ty = self.resolve_type(element)?;
                    if !self.is_type_copy(&element_ty) {
                        self.record_error(TypeError::NonCopyTupleElement {
                            ty: element_ty,
                            span: *span,
                        });
                        return None;
                    }
                    resolved.push(element_ty);
                }
                Some(Type::Tuple(resolved))
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
