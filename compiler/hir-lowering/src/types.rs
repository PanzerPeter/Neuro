//! Type-annotation resolution: `ast_types::Type` → `neuro_hir::HirType`.

use neuro_hir::HirType;
use shared_types::{FloatSuffix, IntSuffix};

use crate::{Lowerer, LoweringError};

impl Lowerer {
    /// Resolve a surface type annotation to its HIR type. Mirrors the checker's
    /// `resolve_type`; a struct name resolves to [`HirType::Struct`], and a generic
    /// application `Name<...>` monomorphizes on demand (§3.8). Copy-element validation
    /// is the checker's job and is not repeated here. Takes `&mut self` because
    /// resolving a generic application may materialize a new struct instance.
    pub(crate) fn resolve_type(&mut self, ty: &ast_types::Type) -> Result<HirType, LoweringError> {
        self.resolve_type_ctx(ty, false)
    }

    /// Resolve a type annotation, tracking whether it sits directly behind a reference.
    /// The flag matters only for `dyn Trait` (§3.17), which is unsized and valid solely
    /// as a reference referent — the checker has already rejected any other placement,
    /// so an unreferenced `dyn` here is an internal inconsistency.
    fn resolve_type_ctx(
        &mut self,
        ty: &ast_types::Type,
        behind_ref: bool,
    ) -> Result<HirType, LoweringError> {
        match ty {
            ast_types::Type::DynTrait { trait_name, .. } if behind_ref => {
                Ok(HirType::DynObject(trait_name.name.clone()))
            }
            ast_types::Type::DynTrait { trait_name, .. } => Err(LoweringError::UnresolvedType {
                name: format!("dyn {}", trait_name.name),
            }),
            // Argument-position `impl Trait` was rewritten to a generic parameter by the
            // parser and return-position `impl Trait` is resolved to its concrete type
            // before lowering, so reaching here means the checker let through a position
            // it should have rejected.
            ast_types::Type::ImplTrait { trait_name, .. } => Err(LoweringError::UnresolvedType {
                name: format!("impl {}", trait_name.name),
            }),
            // Inside a monomorphized instance body, a type-parameter name resolves to
            // its concrete substitution (§3.8) — checked before the built-in names so a
            // parameter can never be a built-in name (the checker rejects that shadowing).
            ast_types::Type::Named(ident) if self.type_subst.contains_key(&ident.name) => {
                Ok(self.type_subst[&ident.name].clone())
            }
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
                // A newtype resolves to its nominal wrapper carrying the resolved
                // inner type (§3.15). Cycles were rejected by the checker, so this
                // recursion terminates.
                name if self.newtypes.contains_key(name) => {
                    let inner_ast = self.newtypes[name].clone();
                    HirType::Newtype {
                        name: name.to_string(),
                        inner: Box::new(self.resolve_type(&inner_ast)?),
                    }
                }
                name => {
                    return Err(LoweringError::UnresolvedType {
                        name: name.to_string(),
                    })
                }
            }),
            ast_types::Type::Reference { inner, mutable, .. } => Ok(HirType::Reference {
                inner: Box::new(self.resolve_type_ctx(inner, true)?),
                mutable: *mutable,
            }),
            ast_types::Type::Array { element, size, .. } => Ok(HirType::Array {
                element: Box::new(self.resolve_type(element)?),
                size: crate::resolve_array_size(size, &self.const_subst)?,
            }),
            ast_types::Type::Tuple { elements, .. } => {
                let mut resolved = Vec::with_capacity(elements.len());
                for element in elements {
                    resolved.push(self.resolve_type(element)?);
                }
                Ok(HirType::Tuple(resolved))
            }
            // Generic application `Name<...>` (§3.8): resolve the arguments and
            // monomorphize the generic struct into a distinct concrete instance. The
            // arguments resolve under any active type-parameter substitution, so a
            // `Wrapper<T>` inside a monomorphized body sees `T` already concrete.
            ast_types::Type::Generic { name, args, .. } => {
                let mut resolved = Vec::with_capacity(args.len());
                for arg in args {
                    match arg {
                        ast_types::GenericArg::Const { value, .. } => {
                            resolved.push(crate::MonoArg::Const(*value as u64));
                        }
                        ast_types::GenericArg::Type(ty) => {
                            resolved.push(crate::MonoArg::Type(self.resolve_type(ty)?));
                        }
                    }
                }
                let mangled = self.instantiate_generic_struct(&name.name, &resolved)?;
                Ok(HirType::Struct(mangled))
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
