use ast_types::{ArraySize, GenericArg};

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::{ArrayLen, Type};

impl TypeChecker {
    /// Convert syntax-parsing type to semantic type.
    /// Returns None if the type is unknown (error is recorded).
    pub(crate) fn resolve_type(&mut self, ty: &ast_types::Type) -> Option<Type> {
        self.resolve_type_ctx(ty, false)
    }

    /// Resolve a type annotation, tracking whether it appears directly behind a
    /// reference (`behind_ref`). The flag is consulted only for `dyn Trait`,
    /// which is unsized and therefore valid solely as a reference referent.
    fn resolve_type_ctx(&mut self, ty: &ast_types::Type, behind_ref: bool) -> Option<Type> {
        match ty {
            // Dynamic-dispatch trait object `dyn Trait`: valid only behind a
            // reference, and only for an object-safe, declared trait.
            ast_types::Type::DynTrait { trait_name, span } => {
                if !behind_ref {
                    self.record_error(TypeError::DynTraitNotBehindReference {
                        trait_name: trait_name.name.clone(),
                        span: *span,
                    });
                    return None;
                }
                if !self.traits.contains_key(&trait_name.name) {
                    self.record_error(TypeError::UnknownTrait {
                        trait_name: trait_name.name.clone(),
                        span: *span,
                    });
                    return None;
                }
                if let Err(reason) = self.trait_object_safety(&trait_name.name) {
                    self.record_error(TypeError::TraitNotObjectSafe {
                        trait_name: trait_name.name.clone(),
                        reason,
                        span: *span,
                    });
                    return None;
                }
                Some(Type::DynObject(trait_name.name.clone()))
            }
            // `impl Trait` reaching resolution is always in a disallowed position:
            // argument position was rewritten to a generic by the parser, and return
            // position is intercepted before resolution.
            ast_types::Type::ImplTrait { span, .. } => {
                self.record_error(TypeError::ImplTraitNotAllowedHere { span: *span });
                None
            }
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
                    // A name matching an in-scope generic parameter resolves to a
                    // generic placeholder rather than erroring as unknown.
                    if self.generic_scope.contains(name) {
                        Some(Type::Generic(name.to_string()))
                    } else if self.is_generic_struct(name) {
                        // A generic struct is only usable with type arguments;
                        // its bare name (also kept in `struct_defs` as the placeholder
                        // template) must not resolve to a concrete type.
                        self.record_error(TypeError::GenericStructNeedsArgs {
                            name: name.to_string(),
                            span: ident.span,
                        });
                        None
                    } else if self.struct_defs.contains_key(name) {
                        Some(Type::Struct(name.to_string()))
                    } else if self.enum_defs.contains_key(name) {
                        Some(Type::Enum(name.to_string()))
                    } else if self.newtype_defs.contains_key(name) {
                        Some(Type::Newtype(name.to_string()))
                    } else {
                        self.record_error(TypeError::UnknownTypeName {
                            name: name.to_string(),
                            span: ident.span,
                        });
                        None
                    }
                }
            },
            // Borrow `&T` / `&mut T`: resolve the referent recursively,
            // preserving mutability. An explicit lifetime `&'a T` is validated for
            // well-formedness against the in-scope lifetime parameters, then erased — a
            // reference type's identity does not depend on its lifetime.
            ast_types::Type::Reference {
                inner,
                mutable,
                lifetime,
                ..
            } => {
                if let Some(lt) = lifetime {
                    if !self.lifetime_scope.contains(&lt.name) {
                        self.record_error(TypeError::UndeclaredLifetime {
                            name: lt.name.clone(),
                            span: lt.span,
                        });
                    }
                }
                self.resolve_type_ctx(inner, true).map(|t| Type::Reference {
                    inner: Box::new(t),
                    mutable: *mutable,
                })
            }
            // Fixed-size array `[T; N]`. The element must be a `Copy` scalar
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
                let len = self.resolve_array_size(size, *span)?;
                Some(Type::Array {
                    element: Box::new(element_ty),
                    size: len,
                })
            }
            // Tuple `(T1, T2, ...)`. Each element must be `Copy` in this phase
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
            // Generic type application `Name<T1, ...>`: resolve the arguments
            // and monomorphize the generic struct into a distinct nominal instance.
            ast_types::Type::Generic { name, args, span } => {
                let mut resolved = Vec::with_capacity(args.len());
                for arg in args {
                    match arg {
                        // A const (value) argument `Ring<i32, 4>` becomes an internal
                        // `ConstValue` marker consumed by monomorphization.
                        GenericArg::Const { value, .. } => {
                            resolved.push(Type::ConstValue(*value as u64));
                        }
                        GenericArg::Type(ty) => resolved.push(self.resolve_type(ty)?),
                    }
                }
                if !self.is_generic_struct(&name.name) {
                    self.record_error(TypeError::NotAGenericType {
                        name: name.name.clone(),
                        span: *span,
                    });
                    return None;
                }
                // A type argument that is itself an unresolved type parameter means a
                // nested generic instantiated with the enclosing parameter (e.g. a
                // `Wrapper<T>` field inside a generic struct). Monomorphizing the outer
                // type cannot substitute into an opaque nested instance, so this is a
                // documented limitation deferred with broader generic support.
                if resolved.iter().any(|t| matches!(t, Type::Generic(_))) {
                    self.record_error(TypeError::NestedGenericTypeArg { span: *span });
                    return None;
                }
                Some(self.instantiate_generic_struct(&name.name, &resolved, *span))
            }
            // Closure / function type `(T1, ...) -> R`: the type of a callable value.
            ast_types::Type::Function { params, ret, .. } => {
                let mut resolved = Vec::with_capacity(params.len());
                for param in params {
                    resolved.push(self.resolve_type(param)?);
                }
                let ret_ty = self.resolve_type(ret)?;
                Some(Type::Function {
                    params: resolved,
                    ret: Box::new(ret_ty),
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

    /// Resolve an array length annotation to a semantic [`ArrayLen`]. A
    /// literal becomes `Fixed`; a name is accepted only when it is an in-scope const
    /// generic parameter (becoming `Param`), else it is an unknown length.
    fn resolve_array_size(
        &mut self,
        size: &ArraySize,
        span: shared_types::Span,
    ) -> Option<ArrayLen> {
        match size {
            ArraySize::Literal(n) => Some(ArrayLen::Fixed(*n as usize)),
            ArraySize::Const(ident) => {
                if self.const_scope.contains_key(&ident.name) {
                    Some(ArrayLen::Param(ident.name.clone()))
                } else {
                    self.record_error(TypeError::UnknownArrayLength {
                        name: ident.name.clone(),
                        span,
                    });
                    None
                }
            }
        }
    }

    /// Whether `name` is a registered generic struct template — a type that is
    /// usable only with type arguments.
    pub(crate) fn is_generic_struct(&self, name: &str) -> bool {
        self.generic_structs.contains_key(name)
    }

    /// Whether a trait is object-safe: every method must dispatch on a `&self`
    /// or `&mut self` receiver. A method with no receiver (associated function) or one
    /// that consumes `self` by value cannot be placed behind a fixed-layout vtable.
    /// Returns `Ok(())` when safe, or `Err(reason)` naming the first offending method.
    pub(crate) fn trait_object_safety(&self, trait_name: &str) -> Result<(), String> {
        let Some(info) = self.traits.get(trait_name) else {
            return Ok(());
        };
        for (name, sig) in &info.methods {
            match sig.self_param {
                Some(ast_types::SelfParam::Ref) | Some(ast_types::SelfParam::RefMut) => {}
                Some(ast_types::SelfParam::Owned) => {
                    return Err(format!("method '{}' takes `self` by value", name));
                }
                None => {
                    return Err(format!("method '{}' has no `self` receiver", name));
                }
            }
        }
        Ok(())
    }

    /// Whether a value of type `found` may be supplied where `expected` is required.
    ///
    /// This is ordinary type compatibility plus the one implicit conversion the language
    /// has: the unsizing coercion `&T` → `&dyn Trait`, permitted when `T`
    /// implements `Trait` and the reference mutabilities agree. There is no `&mut T` →
    /// `&T` weakening, so the mutability match is exact.
    pub(crate) fn assignable(&self, found: &Type, expected: &Type) -> bool {
        if found.is_compatible_with(expected) {
            return true;
        }
        let (
            Type::Reference {
                inner: found_inner,
                mutable: found_mut,
            },
            Type::Reference {
                inner: expected_inner,
                mutable: expected_mut,
            },
        ) = (found, expected)
        else {
            return false;
        };
        let Type::DynObject(trait_name) = expected_inner.as_ref() else {
            return false;
        };
        found_mut == expected_mut && self.type_implements_trait(found_inner, trait_name)
    }

    /// Whether a concrete type satisfies a trait bound: a nominal type
    /// (struct / enum / newtype) satisfies `Trait` when an `impl Trait for T` exists; a
    /// generic parameter satisfies it when it carries the bound. Used for `impl Trait`
    /// return conformance and `&T` → `&dyn Trait` coercion.
    pub(crate) fn type_implements_trait(&self, ty: &Type, trait_name: &str) -> bool {
        let name = match ty {
            Type::Struct(n) | Type::Enum(n) | Type::Newtype(n) => n,
            Type::Generic(p) => {
                return self
                    .generic_bounds
                    .get(p)
                    .is_some_and(|b| b.iter().any(|t| t == trait_name));
            }
            _ => return false,
        };
        self.trait_impls
            .contains(&(trait_name.to_string(), name.clone()))
    }
}
