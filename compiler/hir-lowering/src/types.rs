//! Type-annotation resolution: `ast_types::Type` → `neuro_hir::HirType`.

use neuro_hir::HirType;
use shared_types::{FloatSuffix, IntSuffix};

use crate::{Lowerer, LoweringError};

impl Lowerer {
    /// Resolve a surface type annotation to its HIR type. Mirrors the checker's
    /// `resolve_type`; a struct name resolves to [`HirType::Struct`]. Copy-element
    /// validation is the checker's job and is not repeated here.
    pub(crate) fn resolve_type(&self, ty: &ast_types::Type) -> Result<HirType, LoweringError> {
        match ty {
            ast_types::Type::Named(ident) => Ok(match ident.name.as_str() {
                "i8" => HirType::I8,
                "i16" => HirType::I16,
                "i32" => HirType::I32,
                "i64" => HirType::I64,
                "u8" => HirType::U8,
                "u16" => HirType::U16,
                "u32" => HirType::U32,
                "u64" => HirType::U64,
                "f16" => HirType::F16,
                "bf16" => HirType::BF16,
                "f32" => HirType::F32,
                "f64" => HirType::F64,
                "bool" => HirType::Bool,
                "char" => HirType::Char,
                "string" => HirType::String,
                "void" => HirType::Void,
                name if self.structs.contains_key(name) => HirType::Struct(name.to_string()),
                name if self.enums.contains_key(name) => HirType::Enum(name.to_string()),
                name => {
                    return Err(LoweringError::UnresolvedType {
                        name: name.to_string(),
                    })
                }
            }),
            ast_types::Type::Reference { inner, mutable, .. } => Ok(HirType::Reference {
                inner: Box::new(self.resolve_type(inner)?),
                mutable: *mutable,
            }),
            ast_types::Type::Array { element, size, .. } => Ok(HirType::Array {
                element: Box::new(self.resolve_type(element)?),
                size: *size,
            }),
            ast_types::Type::Tuple { elements, .. } => {
                let mut resolved = Vec::with_capacity(elements.len());
                for element in elements {
                    resolved.push(self.resolve_type(element)?);
                }
                Ok(HirType::Tuple(resolved))
            }
            ast_types::Type::Tensor { .. } => Err(LoweringError::UnresolvedType {
                name: "Tensor".to_string(),
            }),
        }
    }
}

/// The HIR type denoted by an integer-literal suffix.
pub(crate) fn int_suffix_type(suffix: &IntSuffix) -> HirType {
    match suffix {
        IntSuffix::I8 => HirType::I8,
        IntSuffix::I16 => HirType::I16,
        IntSuffix::I32 => HirType::I32,
        IntSuffix::I64 => HirType::I64,
        IntSuffix::U8 => HirType::U8,
        IntSuffix::U16 => HirType::U16,
        IntSuffix::U32 => HirType::U32,
        IntSuffix::U64 => HirType::U64,
    }
}

/// The HIR type denoted by a float-literal suffix.
pub(crate) fn float_suffix_type(suffix: &FloatSuffix) -> HirType {
    match suffix {
        FloatSuffix::F16 => HirType::F16,
        FloatSuffix::BF16 => HirType::BF16,
        FloatSuffix::F32 => HirType::F32,
        FloatSuffix::F64 => HirType::F64,
    }
}
