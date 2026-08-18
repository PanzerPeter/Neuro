//! Item declarations: the reserved-name pass, the generic scope the checker
//! enters per declaration, and the generic unification and substitution shared by
//! every declaration kind. Each declaration kind lives in a sibling module here;
//! all of them add methods to the same `impl TypeChecker` block.

mod consts;
mod enums;
mod functions;
mod impls;
mod newtypes;
mod structs;
mod traits;

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::{ArrayLen, Type};
use ast_types::Item;
use shared_types::Identifier;
use std::collections::HashMap;

/// Built-in type names a newtype may not shadow.
const BUILTIN_TYPE_NAMES: &[&str] = &[
    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f16", "bf16", "f32", "f64", "bool",
    "char", "string", "void",
];

/// Whether `name` is a built-in primitive type name.
fn is_builtin_type_name(name: &str) -> bool {
    BUILTIN_TYPE_NAMES.contains(&name)
}

/// The attribute name carrying trait derivations (`@derive(...)`).
const DERIVE_ATTRIBUTE: &str = "derive";
/// Derive argument requesting the `Copy` trait.
const COPY_TRAIT: &str = "Copy";
/// Derive argument requesting the `Clone` trait.
const CLONE_TRAIT: &str = "Clone";
/// The compiler-known `Drop` lang-item trait name.
const DROP_TRAIT: &str = "Drop";
/// The destructor method name required inside an `impl Drop` block.
const DROP_METHOD: &str = "drop";

impl TypeChecker {
    /// Reject any declared name containing the reserved `__` separator.
    ///
    /// `__` joins a receiver to its method (`Point__translate`) in the flat function
    /// table, and the LLVM backend recovers the receiver by splitting a method symbol on
    /// it. Compiler-generated names are built to hold exactly one `__` — monomorphized
    /// instances use a single-underscore `_g_` marker for that reason — but a user name
    /// carrying its own `__` would break the property from the other side: a method
    /// `a__b` on struct `S` and a method `b` on a struct named `S__a` produce the same
    /// symbol. Reserving the separator closes the collision instead of ranking one
    /// meaning over the other, and keeps generic methods on generic structs unambiguous
    /// as that work lands.
    pub(crate) fn check_reserved_names(&mut self, items: &[Item]) {
        for item in items {
            match item {
                Item::Function(def) => {
                    self.reject_reserved(&def.name);
                    for param in &def.params {
                        self.reject_reserved(&param.name);
                    }
                }
                Item::Struct(def) => {
                    self.reject_reserved(&def.name);
                    for field in &def.fields {
                        self.reject_reserved(&field.name);
                    }
                }
                Item::Enum(def) => {
                    self.reject_reserved(&def.name);
                    for variant in &def.variants {
                        self.reject_reserved(&variant.name);
                    }
                }
                Item::Trait(def) => {
                    self.reject_reserved(&def.name);
                    for method in &def.methods {
                        self.reject_reserved(&method.name);
                    }
                }
                Item::Impl(def) => {
                    // `type_name` is checked at its own declaration; only the method
                    // names are introduced here.
                    for method in &def.methods {
                        self.reject_reserved(&method.name);
                    }
                }
                Item::Const(def) => self.reject_reserved(&def.name),
                Item::Newtype(def) => self.reject_reserved(&def.name),
                // Module resolution consumes every import before the checker runs, so an
                // import introduces no name of its own here.
                Item::Import(_) => {}
            }
        }
    }

    /// Record a [`TypeError::ReservedNameSeparator`] if `ident` contains `__`.
    fn reject_reserved(&mut self, ident: &Identifier) {
        if ident.name.contains("__") {
            self.record_error(TypeError::ReservedNameSeparator {
                name: ident.name.clone(),
                span: ident.span,
            });
        }
    }

    /// Put a definition's generic parameters in scope for signature and body resolution
    /// Type parameters as [`Type::Generic`] placeholders, const parameters as
    /// in-scope values of their declared integer type. Replaces any previous scope.
    fn enter_generic_scope(
        &mut self,
        generics: &[ast_types::GenericParam],
        lifetimes: &[shared_types::Identifier],
    ) {
        self.generic_scope.clear();
        self.const_scope.clear();
        self.lifetime_scope.clear();
        self.generic_bounds.clear();
        for lt in lifetimes {
            self.lifetime_scope.insert(lt.name.clone());
        }
        for gp in generics {
            if is_builtin_type_name(&gp.name.name) {
                self.record_error(TypeError::GenericParamShadowsBuiltin {
                    name: gp.name.name.clone(),
                    span: gp.name.span,
                });
            }
            if !gp.bounds.is_empty() {
                self.generic_bounds.insert(
                    gp.name.name.clone(),
                    gp.bounds.iter().map(|b| b.name.clone()).collect(),
                );
            }
            match &gp.kind {
                ast_types::GenericParamKind::Type => {
                    self.generic_scope.insert(gp.name.name.clone());
                }
                ast_types::GenericParamKind::Const(ty) => {
                    let ity = self.resolve_type(ty).unwrap_or(Type::Unknown);
                    if !matches!(ity, Type::Unknown) && !ity.is_integer() {
                        self.record_error(TypeError::ConstParamNotInteger {
                            name: gp.name.name.clone(),
                            ty: ity.clone(),
                            span: gp.name.span,
                        });
                    }
                    self.const_scope.insert(gp.name.name.clone(), ity);
                }
            }
        }
    }

    /// Clear the generic type + const parameter scopes on leaving a generic definition.
    fn exit_generic_scope(&mut self) {
        self.generic_scope.clear();
        self.const_scope.clear();
        self.lifetime_scope.clear();
        self.generic_bounds.clear();
    }
}

/// Unify a (possibly generic) parameter type against a concrete argument type,
/// recording each type parameter's binding in `subst`. Returns `false` when the
/// structures do not align or a previously-bound parameter is contradicted, so the
/// caller can report a type mismatch. A concrete leaf must match by the usual rules.
pub(crate) fn unify_generic(param: &Type, arg: &Type, subst: &mut HashMap<String, Type>) -> bool {
    match (param, arg) {
        (Type::Generic(name), _) => match subst.get(name) {
            Some(bound) => bound.is_compatible_with(arg),
            None => {
                subst.insert(name.clone(), arg.clone());
                true
            }
        },
        (
            Type::Reference {
                inner: pi,
                mutable: pm,
            },
            Type::Reference {
                inner: ai,
                mutable: am,
            },
        ) => pm == am && unify_generic(pi, ai, subst),
        (
            Type::Array {
                element: pe,
                size: ps,
            },
            Type::Array {
                element: ae,
                size: asz,
            },
        ) => unify_array_len(ps, asz, subst) && unify_generic(pe, ae, subst),
        (Type::Tuple(pe), Type::Tuple(ae)) => {
            pe.len() == ae.len() && pe.iter().zip(ae).all(|(p, a)| unify_generic(p, a, subst))
        }
        // A concrete (non-generic) parameter position: fall back to ordinary compatibility.
        _ => param.is_compatible_with(arg),
    }
}

/// Substitute every generic parameter in `ty` with its inferred concrete type from
/// `subst`. An unbound parameter is left as-is (the caller reports the failure).
pub(crate) fn substitute_generic(ty: &Type, subst: &HashMap<String, Type>) -> Type {
    match ty {
        Type::Generic(name) => subst.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Type::Reference { inner, mutable } => Type::Reference {
            inner: Box::new(substitute_generic(inner, subst)),
            mutable: *mutable,
        },
        Type::Array { element, size } => Type::Array {
            element: Box::new(substitute_generic(element, subst)),
            size: substitute_array_len(size, subst),
        },
        Type::Tuple(elements) => Type::Tuple(
            elements
                .iter()
                .map(|e| substitute_generic(e, subst))
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|p| substitute_generic(p, subst))
                .collect(),
            ret: Box::new(substitute_generic(ret, subst)),
        },
        other => other.clone(),
    }
}

/// The distinct nominal name of a monomorphized generic-struct instance,
/// e.g. `Pair<i32, f64>`. This name is internal to the checker (it never reaches a
/// backend), so it is chosen for readable diagnostics rather than symbol safety.
pub(super) fn mangle_struct_instance(base: &str, args: &[Type]) -> String {
    let parts: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    format!("{}<{}>", base, parts.join(", "))
}

/// Rewrite a monomorphized method's signature: substitute the impl's type parameters
/// and rename the receiver's `Struct(base)` to the concrete `Struct(mangled)`.
pub(super) fn remap_method_type(
    ty: &Type,
    subst: &HashMap<String, Type>,
    base: &str,
    mangled: &str,
) -> Type {
    match ty {
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|p| remap_type(p, subst, base, mangled))
                .collect(),
            ret: Box::new(remap_type(ret, subst, base, mangled)),
        },
        other => remap_type(other, subst, base, mangled),
    }
}

/// Substitute type parameters and rename the base struct to its concrete instance
/// within a single type, recursing through references, arrays, and tuples.
pub(super) fn remap_type(
    ty: &Type,
    subst: &HashMap<String, Type>,
    base: &str,
    mangled: &str,
) -> Type {
    match ty {
        Type::Generic(name) => subst.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Type::Struct(name) if name == base => Type::Struct(mangled.to_string()),
        Type::Reference { inner, mutable } => Type::Reference {
            inner: Box::new(remap_type(inner, subst, base, mangled)),
            mutable: *mutable,
        },
        Type::Array { element, size } => Type::Array {
            element: Box::new(remap_type(element, subst, base, mangled)),
            size: substitute_array_len(size, subst),
        },
        Type::Tuple(elements) => Type::Tuple(
            elements
                .iter()
                .map(|e| remap_type(e, subst, base, mangled))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Unify a template array length against an argument's. A const-parameter length
/// binds that parameter to the argument's concrete value (recorded as a [`Type::ConstValue`]
/// in `subst`); two fixed lengths must be equal; a fixed template length against a symbolic
/// argument (only inside another template) matches structurally by name.
pub(super) fn unify_array_len(
    param: &ArrayLen,
    arg: &ArrayLen,
    subst: &mut HashMap<String, Type>,
) -> bool {
    match (param, arg) {
        (ArrayLen::Fixed(a), ArrayLen::Fixed(b)) => a == b,
        (ArrayLen::Param(name), ArrayLen::Fixed(v)) => match subst.get(name) {
            Some(Type::ConstValue(existing)) => *existing as usize == *v,
            Some(_) => false,
            None => {
                subst.insert(name.clone(), Type::ConstValue(*v as u64));
                true
            }
        },
        (ArrayLen::Param(a), ArrayLen::Param(b)) => a == b,
        _ => false,
    }
}

/// Substitute a template array length using an inferred substitution: a const
/// parameter bound to a [`Type::ConstValue`] becomes a concrete `Fixed` length; anything
/// else is left as-is.
pub(super) fn substitute_array_len(size: &ArrayLen, subst: &HashMap<String, Type>) -> ArrayLen {
    match size {
        ArrayLen::Param(name) => match subst.get(name) {
            Some(Type::ConstValue(v)) => ArrayLen::Fixed(*v as usize),
            _ => size.clone(),
        },
        ArrayLen::Fixed(_) => size.clone(),
    }
}
