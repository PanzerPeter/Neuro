//! Type-annotation resolution: `ast_types::Type` → `neuro_hir::HirType`.

use neuro_hir::{AxisNames, HirType};
use shared_types::{FloatSuffix, IntSuffix};

use crate::{Lowerer, LoweringError};

impl Lowerer {
    /// Resolve a surface type annotation to its HIR type. Mirrors the checker's
    /// `resolve_type`; a struct name resolves to [`HirType::Struct`], and a generic
    /// application `Name<...>` monomorphizes on demand. Copy-element validation
    /// is the checker's job and is not repeated here. Takes `&mut self` because
    /// resolving a generic application may materialize a new struct instance.
    pub(crate) fn resolve_type(&mut self, ty: &ast_types::Type) -> Result<HirType, LoweringError> {
        self.resolve_type_ctx(ty, false)
    }

    /// Resolve one position of a trait's declared signature.
    ///
    /// A trait declaration has no implementing type, so a `Self` or associated-type
    /// position (`Self::Item`) has nothing to resolve against and stands as `void` here.
    /// Nothing reads it: this table types calls through `&dyn Trait`, and a trait naming
    /// either is not object-safe. An impl's own signatures resolve normally: the parser
    /// replaced their `Self`, and their `Self::Item` resolves through the block's binding.
    pub(crate) fn resolve_trait_sig_type(
        &mut self,
        ty: &ast_types::Type,
    ) -> Result<HirType, LoweringError> {
        if names_self(ty) {
            return Ok(HirType::Void);
        }
        self.resolve_type(ty)
    }

    /// Resolve a type annotation, tracking whether it sits directly behind a reference.
    /// The flag matters only for `dyn Trait`, which is unsized and valid solely
    /// as a reference referent; the checker has already rejected any other placement,
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
            // its concrete substitution, checked before the built-in names so a
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
                // inner type. Cycles were rejected by the checker, so this
                // recursion terminates.
                name if self.newtypes.contains_key(name) => {
                    let inner_ast = self.newtypes[name].clone();
                    HirType::Newtype {
                        name: name.to_string(),
                        inner: Box::new(self.resolve_type(&inner_ast)?),
                    }
                }
                // A nullary collection's bare name is a complete type. It is checked
                // after the user-declaration arms above, so a program's own `StringBuilder`
                // shadows the standard one exactly as it does for the prelude enums.
                name if crate::collections::nullary_collection(name).is_some() => {
                    let kind = crate::collections::nullary_collection(name).ok_or_else(|| {
                        LoweringError::UnresolvedType {
                            name: name.to_string(),
                        }
                    })?;
                    HirType::Collection {
                        kind,
                        args: Vec::new(),
                    }
                }
                name => {
                    return Err(LoweringError::UnresolvedType {
                        name: name.to_string(),
                    });
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
            // `[T]` is unsized, so it reaches here only as a reference referent; the
            // checker rejects every other placement, mirroring `dyn Trait` above.
            ast_types::Type::Slice { element, .. } if behind_ref => {
                Ok(HirType::Slice(Box::new(self.resolve_type(element)?)))
            }
            ast_types::Type::Slice { element, .. } => Err(LoweringError::UnresolvedType {
                name: format!("[{}]", self.resolve_type(element)?),
            }),
            ast_types::Type::Tuple { elements, .. } => {
                let mut resolved = Vec::with_capacity(elements.len());
                for element in elements {
                    resolved.push(self.resolve_type(element)?);
                }
                Ok(HirType::Tuple(resolved))
            }
            // Generic application `Name<...>`: resolve the arguments and
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
                        // The caller's own extent passed on, `Ring<N>` inside a template.
                        ast_types::GenericArg::Type(ast_types::Type::Named(id))
                            if self.const_subst.contains_key(&id.name) =>
                        {
                            resolved.push(crate::MonoArg::Const(self.const_subst[&id.name]));
                        }
                        // `Layer<[N]>`: the one-name shape the checker reads off a slice type.
                        ast_types::GenericArg::Type(ast_types::Type::Slice { element, .. })
                            if matches!(&**element, ast_types::Type::Named(id)
                                if self.const_subst.contains_key(&id.name)) =>
                        {
                            let ast_types::Type::Named(id) = &**element else {
                                return Err(crate::malformed("a one-name shape argument".into()));
                            };
                            let extent = Some(self.const_subst[&id.name] as usize);
                            resolved.push(crate::MonoArg::Type(shape_carrier(
                                vec![extent],
                                AxisNames(vec![None]),
                            )));
                        }
                        ast_types::GenericArg::Type(ty) => {
                            resolved.push(crate::MonoArg::Type(self.resolve_type(ty)?));
                        }
                        ast_types::GenericArg::Shape { dims, .. } => {
                            resolved.push(crate::MonoArg::Type(self.resolve_shape_carrier(dims)?));
                        }
                    }
                }
                // A standard collection is compiler-known, so it is not monomorphized
                // into a nominal instance: it carries its type arguments directly. A
                // program declaring its own generic type of that name shadows it.
                if let Some(kind) = crate::collections::collection_kind(&name.name)
                    && !self.structs.contains_key(&name.name)
                    && !self.is_generic_enum(&name.name)
                {
                    let mut collection_args = Vec::with_capacity(resolved.len());
                    for arg in resolved {
                        match arg {
                            crate::MonoArg::Type(ty) => collection_args.push(ty),
                            crate::MonoArg::Const(_) => {
                                return Err(LoweringError::UnresolvedType {
                                    name: name.name.clone(),
                                });
                            }
                        }
                    }
                    return Ok(HirType::Collection {
                        kind,
                        args: collection_args,
                    });
                }
                if self.is_generic_enum(&name.name) {
                    let mangled = self.instantiate_generic_enum(&name.name, &resolved)?;
                    return Ok(HirType::Enum(mangled));
                }
                let mangled = self.instantiate_generic_struct(&name.name, &resolved)?;
                Ok(HirType::Struct(mangled))
            }
            // Closure / function type `(T1, ...) -> R`.
            ast_types::Type::Function { params, ret, .. } => {
                let mut resolved = Vec::with_capacity(params.len());
                for param in params {
                    resolved.push(self.resolve_type(param)?);
                }
                Ok(HirType::Function {
                    params: resolved,
                    ret: Box::new(self.resolve_type(ret)?),
                })
            }
            ast_types::Type::Tensor {
                element_type,
                shape,
                ..
            } => {
                let (extents, names) = self.resolve_shape(shape)?;
                let element = self.resolve_type(element_type)?;
                if !is_tensor_scalar(&element) {
                    return Err(LoweringError::NonScalarTensorElement {
                        ty: element.to_string(),
                    });
                }
                Ok(HirType::Tensor {
                    element: Box::new(element),
                    shape: extents,
                    names: AxisNames(names),
                })
            }
        }
    }

    /// A written shape's extents and dimension names under the active substitution: an
    /// extent parameter becomes its value and a `..S` splice the axes `S` is bound to.
    pub(crate) fn resolve_shape(
        &self,
        shape: &[ast_types::TensorDim],
    ) -> Result<ResolvedShape, LoweringError> {
        let mut extents = Vec::with_capacity(shape.len());
        let mut names = Vec::with_capacity(shape.len());
        for dim in shape {
            let ast_types::TensorExtent::Splice(id) = &dim.extent else {
                extents.push(crate::resolve_tensor_dim(
                    dim,
                    &self.const_subst,
                    &|shape| self.shape_count(shape),
                )?);
                names.push(dim.name.as_ref().map(|n| n.name.clone()));
                continue;
            };
            let Some((bound, bound_names)) = self.type_subst.get(&id.name).and_then(carried_shape)
            else {
                return Err(LoweringError::UnresolvedType {
                    name: format!("shape parameter '{}'", id.name),
                });
            };
            extents.extend(bound.iter().copied());
            names.extend(bound_names.0.iter().cloned());
        }
        Ok((extents, names))
    }

    /// The element count of the shape a shape parameter is bound to, for `S.count()`.
    fn shape_count(&self, shape: &str) -> Option<i128> {
        let (extents, _) = carried_shape(self.type_subst.get(shape)?)?;
        extents.iter().try_fold(1i128, |count, extent| {
            count.checked_mul(i128::try_from((*extent)?).ok()?)
        })
    }

    /// A shape argument, `[2, 3]` in `Dense<[2, 3]>`, as the value a shape parameter is
    /// bound to.
    pub(crate) fn resolve_shape_carrier(
        &self,
        shape: &[ast_types::TensorDim],
    ) -> Result<HirType, LoweringError> {
        let (extents, names) = self.resolve_shape(shape)?;
        Ok(shape_carrier(extents, AxisNames(names)))
    }
}

/// A shape's extents and the dimension name at each axis.
type ResolvedShape = (Vec<Option<usize>>, Vec<Option<String>>);

/// The value a shape parameter is bound to while an instance lowers. It rides in the
/// type substitution, beside the type parameters, as a tensor of no element: every
/// path that carries, mangles and compares a type argument then carries a shape
/// argument too, and the `void` element keeps it apart from any real tensor type. It
/// never reaches the HIR, since every splice is expanded where a shape is resolved.
pub(crate) fn shape_carrier(shape: Vec<Option<usize>>, names: AxisNames) -> HirType {
    HirType::Tensor {
        element: Box::new(HirType::Void),
        shape,
        names,
    }
}

/// The shape a [`shape_carrier`] holds, with its dimension names.
pub(crate) fn carried_shape(ty: &HirType) -> Option<(&[Option<usize>], &AxisNames)> {
    match ty {
        HirType::Tensor {
            element,
            shape,
            names,
        } if matches!(**element, HirType::Void) => Some((shape, names)),
        _ => None,
    }
}

/// Whether `ty` may be a tensor's element: a scalar with a fixed width.
fn is_tensor_scalar(ty: &HirType) -> bool {
    matches!(
        ty,
        HirType::I8
            | HirType::I16
            | HirType::I32
            | HirType::I64
            | HirType::U8
            | HirType::U16
            | HirType::U32
            | HirType::U64
            | HirType::F16
            | HirType::BF16
            | HirType::F32
            | HirType::F64
            | HirType::Bool
    )
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

/// Whether an annotation names `Self` or an associated type in any position:
/// `Option<Self::Item>` and `&Self` name one just as a bare `Self::Item` or `Self` does.
fn names_self(ty: &ast_types::Type) -> bool {
    match ty {
        ast_types::Type::Named(ident) => ident.name == "Self" || ident.name.starts_with("Self::"),
        ast_types::Type::Reference { inner, .. } => names_self(inner),
        ast_types::Type::Array { element, .. } | ast_types::Type::Slice { element, .. } => {
            names_self(element)
        }
        ast_types::Type::Tuple { elements, .. } => elements.iter().any(names_self),
        ast_types::Type::Generic { args, .. } => args.iter().any(|arg| match arg {
            ast_types::GenericArg::Type(inner) => names_self(inner),
            ast_types::GenericArg::Const { .. } | ast_types::GenericArg::Shape { .. } => false,
        }),
        ast_types::Type::Function { params, ret, .. } => {
            params.iter().any(names_self) || names_self(ret)
        }
        ast_types::Type::ImplTrait { .. }
        | ast_types::Type::DynTrait { .. }
        | ast_types::Type::Tensor { .. } => false,
    }
}
