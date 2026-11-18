//! Item declarations: the reserved-name pass, the generic scope the checker
//! enters per declaration, and the generic unification and substitution shared by
//! every declaration kind. Each declaration kind lives in a sibling module here;
//! all of them add methods to the same `impl TypeChecker` block.

mod consts;
mod enums;
mod functions;
mod impl_returns;
mod impls;
mod newtypes;
mod structs;
pub(crate) mod traits;

use super::derivable::{CLONE_TRAIT, COPY_TRAIT, DEBUG_TRAIT, HASHABLE_TRAIT, PARTIAL_EQ_TRAIT};
use super::{BoundInfo, GenericScope, TypeChecker};
use crate::errors::TypeError;
use crate::extents::Atom;
use crate::types::{
    ArrayLen, CollectionKind, ExtentExpr, TensorAxis, Type, shape_argument, shape_count,
};
use ast_types::{Attribute, Item};
use shared_types::{Identifier, Span};
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
/// The traits `@derive` generates, in the order the diagnostic lists them.
const DERIVABLE_TRAITS: &[&str] = &[
    COPY_TRAIT,
    CLONE_TRAIT,
    DEBUG_TRAIT,
    PARTIAL_EQ_TRAIT,
    HASHABLE_TRAIT,
];

/// The attributes some pass acts on. The set is fixed by the language, so any other name is an error:
/// ignoring it would let a misspelled `@no_grad` leave a call differentiated.
const IMPLEMENTED_ATTRIBUTES: &[&str] = &[
    DERIVE_ATTRIBUTE,
    "grad",
    "no_grad",
    "gpu",
    "kernel",
    "test",
    ALLOW,
];

/// The lint opt-out, the one attribute a `const` takes.
const ALLOW: &str = "allow";

/// Attributes the spec names that no pass acts on yet. Accepting one would be the silent
/// no-op the fixed set exists to prevent.
const PENDING_ATTRIBUTES: &[&str] = &[
    "derivative",
    "model",
    "export",
    "inline",
    "cfg",
    "deprecated",
];
/// The compiler-known `Drop` lang-item trait name.
const DROP_TRAIT: &str = "Drop";
/// The destructor method name required inside an `impl Drop` block.
const DROP_METHOD: &str = "drop";

impl TypeChecker {
    /// Reject any declared name containing the reserved `__` separator.
    ///
    /// `__` joins a receiver to its method (`Point__translate`) in the flat function
    /// table, and the LLVM backend recovers the receiver by splitting a method symbol on
    /// it. Compiler-generated names are built to hold exactly one `__` (monomorphized
    /// instances use a `$g` marker for that reason), but a user name
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
                // Module resolution consumes every import, inline `module` block, and
                // `@no_prelude` marker before the checker runs, so none introduces a name
                // of its own here.
                Item::Import(_) | Item::Module(_) | Item::NoPrelude(_) => {}
            }
        }
    }

    /// Refuse a parameter or field shape that splices more than one shape parameter: a
    /// call or a literal splits its argument's axes among them, and two splices leave the
    /// split open.
    pub(crate) fn check_parameter_splices(&mut self, items: &[Item]) {
        for item in items {
            let types: Vec<&ast_types::Type> = match item {
                Item::Function(def) => def.params.iter().map(|p| &p.ty).collect(),
                Item::Impl(def) => def
                    .methods
                    .iter()
                    .flat_map(|m| m.params.iter().map(|p| &p.ty))
                    .collect(),
                Item::Struct(def) => def.fields.iter().map(|f| &f.ty).collect(),
                Item::Module(def) => {
                    self.check_parameter_splices(&def.items);
                    continue;
                }
                _ => continue,
            };
            for ty in types {
                self.check_type_splices(ty);
            }
        }
    }

    fn check_type_splices(&mut self, ty: &ast_types::Type) {
        match ty {
            ast_types::Type::Tensor { shape, span, .. } => {
                let count = shape
                    .iter()
                    .filter(|dim| matches!(dim.extent, ast_types::TensorExtent::Splice(_)))
                    .count();
                if count > 1 {
                    self.record_error(TypeError::ShapeSplicedTwice { count, span: *span });
                }
            }
            ast_types::Type::Reference { inner, .. } => self.check_type_splices(inner),
            ast_types::Type::Tuple { elements, .. } => {
                for element in elements {
                    self.check_type_splices(element);
                }
            }
            _ => {}
        }
    }

    /// Reject every attribute outside the fixed set on a function, struct or method.
    pub(crate) fn check_attribute_names(&mut self, items: &[Item]) {
        for item in items {
            let attributes: Vec<&Attribute> = match item {
                Item::Function(def) => def.attributes.iter().collect(),
                Item::Struct(def) => def.attributes.iter().collect(),
                Item::Impl(def) => def.methods.iter().flat_map(|m| &m.attributes).collect(),
                Item::Const(def) => {
                    // A constant has no body for any other attribute to act on.
                    for attr in def.attributes.iter().filter(|a| a.name.name != ALLOW) {
                        self.record_error(TypeError::AttributeOnConst {
                            name: attr.name.name.clone(),
                            span: attr.span,
                        });
                    }
                    continue;
                }
                _ => continue,
            };
            for attr in attributes {
                self.check_attribute_name(attr);
            }
        }
    }

    fn check_attribute_name(&mut self, attr: &Attribute) {
        let name = attr.name.name.as_str();
        if IMPLEMENTED_ATTRIBUTES.contains(&name) {
            return;
        }
        if PENDING_ATTRIBUTES.contains(&name) {
            self.record_error(TypeError::UnimplementedAttribute {
                name: name.to_string(),
                span: attr.span,
            });
            return;
        }
        self.record_error(TypeError::UnknownAttribute {
            name: name.to_string(),
            known: IMPLEMENTED_ATTRIBUTES
                .iter()
                .map(|a| format!("@{a}"))
                .collect::<Vec<_>>()
                .join(", "),
            span: attr.span,
        });
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
        self.value_consts.clear();
        self.lifetime_scope.clear();
        self.generic_bounds.clear();
        self.add_generic_params(generics, lifetimes);
    }

    /// Put a generic method's own parameters in scope on top of its impl's, returning the
    /// scope to restore once its signature or body is done. `None` for a method that
    /// declares none, which leaves the scope untouched.
    pub(crate) fn enter_method_generics(
        &mut self,
        generics: &[ast_types::GenericParam],
        lifetimes: &[shared_types::Identifier],
    ) -> Option<GenericScope> {
        if generics.is_empty() && lifetimes.is_empty() {
            return None;
        }
        let saved = GenericScope {
            types: self.generic_scope.clone(),
            consts: self.const_scope.clone(),
            value_consts: self.value_consts.clone(),
            lifetimes: self.lifetime_scope.clone(),
            bounds: self.generic_bounds.clone(),
        };
        self.add_generic_params(generics, lifetimes);
        Some(saved)
    }

    /// Reject a method type parameter named like one of its impl's, which must be in
    /// scope. One name, one parameter: inside the body a reused `T` could mean either,
    /// and substitution, which is by name, would silently pick the method's.
    pub(crate) fn check_method_generic_names(&mut self, method: &ast_types::MethodDef) {
        for gp in &method.generics {
            let name = &gp.name.name;
            if self.generic_scope.contains(name) || self.const_scope.contains_key(name) {
                self.record_error(TypeError::MethodGenericShadowsImpl {
                    name: name.clone(),
                    span: gp.name.span,
                });
            }
        }
    }

    /// Restore the scope [`Self::enter_method_generics`] replaced.
    pub(crate) fn exit_method_generics(&mut self, saved: Option<GenericScope>) {
        let Some(saved) = saved else { return };
        self.generic_scope = saved.types;
        self.const_scope = saved.consts;
        self.value_consts = saved.value_consts;
        self.lifetime_scope = saved.lifetimes;
        self.generic_bounds = saved.bounds;
    }

    /// Add generic and lifetime parameters to whatever scope is already in place.
    fn add_generic_params(
        &mut self,
        generics: &[ast_types::GenericParam],
        lifetimes: &[shared_types::Identifier],
    ) {
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
            match &gp.kind {
                ast_types::GenericParamKind::Type => {
                    self.generic_scope.insert(gp.name.name.clone());
                }
                ast_types::GenericParamKind::Extent => {
                    self.const_scope.insert(gp.name.name.clone(), Type::U64);
                }
                // A shape parameter's value is the shape it stands for, `[..S]`.
                ast_types::GenericParamKind::Shape => {
                    let own = TensorAxis {
                        name: None,
                        extent: ArrayLen::Splice(gp.name.name.clone()),
                    };
                    self.const_scope
                        .insert(gp.name.name.clone(), Type::Shape(vec![own]));
                }
                ast_types::GenericParamKind::Const(ty) => {
                    self.value_consts.insert(gp.name.name.clone());
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

        // Bounds resolve in a second pass: `T: Source<Item = U>` may constrain an
        // associated type to another parameter of the same list, which is only in scope
        // once every name above has been registered.
        for gp in generics {
            if gp.bounds.is_empty() {
                continue;
            }
            let bounds = self.resolve_bounds(&gp.bounds);
            self.generic_bounds.insert(gp.name.name.clone(), bounds);
        }
    }

    /// Resolve a parameter's written bounds, validating each `Trait<Assoc = T>` binding
    /// against the trait's declaration: a binding must name an associated type that
    /// trait declares, since nothing else is constrainable.
    pub(crate) fn resolve_bounds(&mut self, bounds: &[ast_types::TraitBound]) -> Vec<BoundInfo> {
        let mut resolved = Vec::new();
        for bound in bounds {
            let declared = self
                .traits
                .get(&bound.trait_name.name)
                .map(|info| info.assoc_types.clone());
            let mut assoc = Vec::new();
            for (name, ty) in &bound.assoc_bindings {
                if let Some(declared) = &declared
                    && !declared.contains(&name.name)
                {
                    self.record_error(TypeError::UnknownAssociatedType {
                        trait_name: bound.trait_name.name.clone(),
                        name: name.name.clone(),
                        span: name.span,
                    });
                    continue;
                }
                if let Some(resolved_ty) = self.resolve_type(ty) {
                    assoc.push((name.name.clone(), resolved_ty));
                }
            }
            resolved.push(BoundInfo {
                trait_name: bound.trait_name.name.clone(),
                assoc,
            });
        }
        resolved
    }

    /// Clear the generic type + const parameter scopes on leaving a generic definition.
    fn exit_generic_scope(&mut self) {
        self.generic_scope.clear();
        self.const_scope.clear();
        self.value_consts.clear();
        self.lifetime_scope.clear();
        self.generic_bounds.clear();
    }
}

impl TypeChecker {
    /// Unify a (possibly generic) parameter type against a concrete argument type,
    /// recording each type parameter's binding in `subst`. Returns `false` when the
    /// structures do not align or a previously-bound parameter is contradicted, so the
    /// caller can report a type mismatch. A concrete leaf must match by the usual rules.
    pub(crate) fn unify_generic(
        &self,
        param: &Type,
        arg: &Type,
        subst: &mut HashMap<String, Type>,
    ) -> bool {
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
            ) => {
                pm == am
                    && match pi.as_ref() {
                        // `&T` to `&dyn Trait` is a coercion, as at a non-generic call.
                        Type::DynObject(_) => self.assignable(arg, param),
                        _ => self.unify_generic(pi, ai, subst),
                    }
            }
            (
                Type::Array {
                    element: pe,
                    size: ps,
                },
                Type::Array {
                    element: ae,
                    size: asz,
                },
            ) => unify_array_len(ps, asz, subst) && self.unify_generic(pe, ae, subst),
            // A slice reached through a reference: `&[T]` takes a slice, an array or a
            // `Vec` of the element, as the unsizing rule of a non-generic call does.
            (Type::Slice(pe), Type::Slice(ae) | Type::Array { element: ae, .. }) => {
                self.unify_generic(pe, ae, subst)
            }
            (
                Type::Slice(pe),
                Type::Collection {
                    kind: CollectionKind::Vec,
                    args,
                },
            ) => matches!(args.as_slice(), [ae] if self.unify_generic(pe, ae, subst)),
            (
                Type::Tensor {
                    element: pe,
                    shape: pshape,
                },
                Type::Tensor {
                    element: ae,
                    shape: ashape,
                },
            ) => unify_tensor_shape(pshape, ashape, subst) && self.unify_generic(pe, ae, subst),
            (Type::Tuple(pe), Type::Tuple(ae)) => {
                pe.len() == ae.len()
                    && pe
                        .iter()
                        .zip(ae)
                        .all(|(p, a)| self.unify_generic(p, a, subst))
            }
            // A function value fits a slot asking for its own call trait or a weaker one,
            // the same direction `is_compatible_with` takes.
            (
                Type::Function {
                    params: pp,
                    ret: pr,
                    mode: pm,
                },
                Type::Function {
                    params: ap,
                    ret: ar,
                    mode: am,
                },
            ) => {
                am <= pm
                    && pp.len() == ap.len()
                    && pp
                        .iter()
                        .zip(ap)
                        .all(|(p, a)| self.unify_generic(p, a, subst))
                    && self.unify_generic(pr, ar, subst)
            }
            (Type::Collection { kind: pk, args: pa }, Type::Collection { kind: ak, args: aa }) => {
                pk == ak
                    && pa.len() == aa.len()
                    && pa
                        .iter()
                        .zip(aa)
                        .all(|(p, a)| self.unify_generic(p, a, subst))
            }
            // `Option<T>` against `Option<i32>`: two instances of one template unify
            // through their arguments, since the names alone never match. The same name on
            // both sides still binds: a caller's `Option<T>` hands the callee's `T` its own.
            (Type::Struct(p), Type::Struct(a)) | (Type::Enum(p), Type::Enum(a)) => {
                match (self.instance_parts(p), self.instance_parts(a)) {
                    (Some((pbase, pargs)), Some((abase, aargs))) if pbase == abase => pargs
                        .iter()
                        .zip(&aargs)
                        .all(|(p, a)| self.unify_generic(p, a, subst)),
                    _ => param.is_compatible_with(arg),
                }
            }
            // A concrete (non-generic) parameter position: fall back to ordinary compatibility.
            _ => param.is_compatible_with(arg),
        }
    }

    /// Substitute every generic parameter in `ty` with its inferred concrete type from
    /// `subst`. An unbound parameter is left as-is (the caller reports the failure).
    ///
    /// An instance whose arguments name a parameter is rebuilt with the substituted
    /// arguments, so `Option<T>` becomes `Option<i32>`; `span` is where a rebuilt
    /// instance reports a type argument it cannot take.
    pub(crate) fn substitute_generic(
        &mut self,
        ty: &Type,
        subst: &HashMap<String, Type>,
        span: Span,
    ) -> Type {
        match ty {
            Type::Generic(name) => subst.get(name).cloned().unwrap_or_else(|| ty.clone()),
            Type::Reference { inner, mutable } => Type::Reference {
                inner: Box::new(self.substitute_generic(inner, subst, span)),
                mutable: *mutable,
            },
            Type::Array { element, size } => Type::Array {
                element: Box::new(self.substitute_generic(element, subst, span)),
                size: substitute_array_len(size, subst),
            },
            Type::Slice(element) => {
                Type::Slice(Box::new(self.substitute_generic(element, subst, span)))
            }
            Type::Tensor { element, shape } => Type::Tensor {
                element: Box::new(self.substitute_tensor_element(element, subst, span)),
                shape: substitute_shape(shape, subst),
            },
            Type::Shape(shape) => shape_argument(substitute_shape(shape, subst)),
            Type::Tuple(elements) => Type::Tuple(
                elements
                    .iter()
                    .map(|e| self.substitute_generic(e, subst, span))
                    .collect(),
            ),
            Type::Collection { kind, args } => Type::Collection {
                kind: *kind,
                args: args
                    .iter()
                    .map(|a| self.substitute_generic(a, subst, span))
                    .collect(),
            },
            Type::Function { params, ret, mode } => Type::Function {
                params: params
                    .iter()
                    .map(|p| self.substitute_generic(p, subst, span))
                    .collect(),
                ret: Box::new(self.substitute_generic(ret, subst, span)),
                mode: *mode,
            },
            Type::Struct(name) | Type::Enum(name) => {
                let Some((base, args)) = self.instance_parts(name) else {
                    return ty.clone();
                };
                let substituted: Vec<Type> = args
                    .iter()
                    .map(|a| self.substitute_generic(a, subst, span))
                    .collect();
                if substituted == args {
                    return ty.clone();
                }
                self.instantiate_generic(&base, &substituted, span)
            }
            other => other.clone(),
        }
    }
}

impl TypeChecker {
    /// A tensor's element under `subst`. An unbounded `T` may be a tensor element in a
    /// generic signature, so the type argument it receives has to be one.
    fn substitute_tensor_element(
        &mut self,
        element: &Type,
        subst: &HashMap<String, Type>,
        span: Span,
    ) -> Type {
        let concrete = self.substitute_generic(element, subst, span);
        if matches!(element, Type::Generic(_))
            && !matches!(concrete, Type::Generic(_) | Type::Unknown)
            && !super::resolution::is_tensor_element(&concrete)
        {
            self.record_error(TypeError::NonScalarTensorElement { ty: concrete, span });
            return Type::Unknown;
        }
        concrete
    }
}

/// The distinct nominal name of a monomorphized generic-struct instance,
/// e.g. `Pair<i32, f64>`. This name is internal to the checker (it never reaches a
/// backend), so it is chosen for readable diagnostics rather than symbol safety.
pub(super) fn mangle_struct_instance(base: &str, args: &[Type]) -> String {
    let parts: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    format!("{}<{}>", base, parts.join(", "))
}

/// Unify a template array length against an argument's. A const-parameter length
/// binds that parameter to the argument's length: a concrete one as a [`Type::ConstValue`],
/// and inside another template the caller's own parameter, as the rename
/// `Type::Generic`, which is how a size parameter is passed on. Two fixed lengths must be
/// equal.
pub(super) fn unify_array_len(
    param: &ArrayLen,
    arg: &ArrayLen,
    subst: &mut HashMap<String, Type>,
) -> bool {
    // An extent expression binds nothing, since `N + 1` against 5 would need solving:
    // it is compared once every name it reads is bound, and until then it accepts.
    if let ArrayLen::Expr(expr) = param {
        if expr.names().iter().any(|name| !subst.contains_key(name)) {
            return true;
        }
        return substitute_array_len(param, subst) == *arg;
    }
    let bound = match arg {
        ArrayLen::Fixed(v) => Type::ConstValue(*v as u64),
        ArrayLen::Param(name) => Type::Generic(name.clone()),
        ArrayLen::Expr(expr) => Type::Extent(expr.clone()),
        ArrayLen::Dynamic | ArrayLen::Splice(_) => return param == arg,
    };
    match param {
        ArrayLen::Param(name) => match subst.get(name) {
            Some(existing) => *existing == bound,
            None => {
                subst.insert(name.clone(), bound);
                true
            }
        },
        ArrayLen::Fixed(_) | ArrayLen::Dynamic | ArrayLen::Splice(_) | ArrayLen::Expr(_) => {
            param == arg
        }
    }
}

/// Whether `ty` writes an extent expression, which [`unify_array_len`] accepts until the
/// names it reads are bound: a caller that unified it early checks it again once they are.
pub(crate) fn writes_extent_expr(ty: &Type) -> bool {
    match ty {
        Type::Tensor { element, shape } => {
            shape
                .iter()
                .any(|axis| matches!(axis.extent, ArrayLen::Expr(_)))
                || writes_extent_expr(element)
        }
        Type::Reference { inner, .. } => writes_extent_expr(inner),
        Type::Array { element, .. } | Type::Slice(element) => writes_extent_expr(element),
        Type::Tuple(elements) | Type::Collection { args: elements, .. } => {
            elements.iter().any(writes_extent_expr)
        }
        _ => false,
    }
}

/// Unify a template tensor shape against an argument's, axis by axis.
///
/// Every axis is unified even once one has failed, so a shape parameter the rest of the
/// shape does bind is not reported as uninferable on top of the extent mismatch that is
/// the real error.
///
/// A dimension name must also agree where both sides carry one: that is what
/// rejects a transposed argument whose extents happen to match the template's.
///
/// A splice in the template takes the argument's axes the entries around it leave over,
/// possibly none: `[..S, N]` against `[2, 3, 4]` binds `N` to 4 and `S` to `[2, 3]`. A
/// template shape splices at most once (checked at its declaration), so the split is
/// never ambiguous. A `?` axis has no value to bind, so a splice refuses one, as an
/// extent parameter does.
fn unify_tensor_shape(
    param: &[TensorAxis],
    arg: &[TensorAxis],
    subst: &mut HashMap<String, Type>,
) -> bool {
    let Some(at) = param
        .iter()
        .position(|axis| matches!(axis.extent, ArrayLen::Splice(_)))
    else {
        return unify_axes(param, arg, subst);
    };
    let after = param.len() - at - 1;
    if arg.len() < at + after {
        return false;
    }
    let taken = &arg[at..arg.len() - after];
    let mut agreed = unify_axes(&param[..at], &arg[..at], subst)
        & unify_axes(&param[at + 1..], &arg[arg.len() - after..], subst);
    let ArrayLen::Splice(name) = &param[at].extent else {
        return false;
    };
    if taken.iter().any(|axis| axis.extent == ArrayLen::Dynamic) {
        // Bound all the same, so the mismatch is the one error reported, not also an
        // uninferable parameter.
        subst.entry(name.clone()).or_insert(Type::Unknown);
        return false;
    }
    let bound = shape_argument(taken.to_vec());
    agreed &= match subst.get(name) {
        Some(existing) => existing.is_compatible_with(&bound) || *existing == bound,
        None => {
            subst.insert(name.clone(), bound);
            true
        }
    };
    agreed
}

/// Unify two runs of single axes of equal length, position by position.
fn unify_axes(param: &[TensorAxis], arg: &[TensorAxis], subst: &mut HashMap<String, Type>) -> bool {
    if param.len() != arg.len() {
        return false;
    }
    let mut agreed = true;
    for (axis, found) in param.iter().zip(arg) {
        agreed &=
            unify_array_len(&axis.extent, &found.extent, subst) && axis.names_agree_with(found);
    }
    agreed
}

/// A template shape under `subst`: an extent parameter becomes its value and a splice
/// expands to the axes its shape parameter stands for, keeping their names.
pub(super) fn substitute_shape(
    shape: &[TensorAxis],
    subst: &HashMap<String, Type>,
) -> Vec<TensorAxis> {
    let mut out = Vec::with_capacity(shape.len());
    for axis in shape {
        let ArrayLen::Splice(name) = &axis.extent else {
            out.push(axis.with_extent(substitute_array_len(&axis.extent, subst)));
            continue;
        };
        match subst.get(name) {
            Some(Type::Shape(axes)) => out.extend(axes.iter().cloned()),
            Some(Type::Generic(renamed)) => {
                out.push(axis.with_extent(ArrayLen::Splice(renamed.clone())))
            }
            _ => out.push(axis.clone()),
        }
    }
    out
}

/// The first axis whose dimension name the argument contradicts: its position, the name
/// the parameter writes, and the name the argument writes.
///
/// A transposition is the mismatch a printed expected/found pair explains worst, because
/// the two types differ only in the order of two names and may carry identical extents,
/// so the axis is named instead.
pub(crate) fn mismatched_axis_name(param: &Type, arg: &Type) -> Option<(usize, String, String)> {
    match (param, arg) {
        (Type::Reference { inner: pi, .. }, Type::Reference { inner: ai, .. }) => {
            mismatched_axis_name(pi, ai)
        }
        (
            Type::Tensor { shape: pshape, .. },
            Type::Tensor {
                shape: ashape,
                element: _,
            },
        ) => pshape
            .iter()
            .zip(ashape)
            .enumerate()
            .find(|(_, (p, a))| !p.names_agree_with(a))
            .and_then(|(axis, (p, a))| Some((axis, p.name.clone()?, a.name.clone()?))),
        _ => None,
    }
}

/// The first shape parameter this argument contradicts: its name, the extent already
/// inferred for it, and the extent this argument writes.
///
/// A repeated shape parameter (the `K` in `matmul<M, N, K>`) is the one mismatch a plain
/// expected/found pair explains badly, because the expected type it prints was itself
/// inferred from an earlier argument, so the parameter and both extents are named instead.
pub(crate) fn conflicting_shape_param(
    param: &Type,
    arg: &Type,
    subst: &HashMap<String, Type>,
) -> Option<(String, u64, u64)> {
    match (param, arg) {
        (
            Type::Reference { inner: pi, .. },
            Type::Reference {
                inner: ai,
                mutable: _,
            },
        ) => conflicting_shape_param(pi, ai, subst),
        (
            Type::Tensor { shape: pshape, .. },
            Type::Tensor {
                shape: ashape,
                element: _,
            },
        ) => pshape.iter().zip(ashape).find_map(|(p, a)| {
            match (
                &p.extent,
                &a.extent,
                subst.get(extent_param_name(&p.extent)?),
            ) {
                (ArrayLen::Param(name), ArrayLen::Fixed(found), Some(Type::ConstValue(bound)))
                    if *bound != *found as u64 =>
                {
                    Some((name.clone(), *bound, *found as u64))
                }
                _ => None,
            }
        }),
        _ => None,
    }
}

/// The parameter name an extent is written with, or `None` for a concrete one.
fn extent_param_name(extent: &ArrayLen) -> Option<&str> {
    match extent {
        ArrayLen::Param(name) => Some(name),
        ArrayLen::Fixed(_) | ArrayLen::Dynamic | ArrayLen::Splice(_) | ArrayLen::Expr(_) => None,
    }
}

/// Substitute a template array length using an inferred substitution: a const
/// parameter bound to a [`Type::ConstValue`] becomes a concrete `Fixed` length; anything
/// else is left as-is.
pub(super) fn substitute_array_len(size: &ArrayLen, subst: &HashMap<String, Type>) -> ArrayLen {
    match size {
        ArrayLen::Param(name) => match subst.get(name) {
            Some(Type::ConstValue(v)) => ArrayLen::Fixed(*v as usize),
            // A rename: conformance maps a trait method's const parameter to the impl's.
            Some(Type::Generic(renamed)) => ArrayLen::Param(renamed.clone()),
            Some(Type::Extent(extent)) => ArrayLen::Expr(extent.clone()),
            _ => size.clone(),
        },
        // An extent with no value at these arguments is left as written: lowering
        // reports it at the call, with the arguments that left it none.
        ArrayLen::Expr(extent) => extent
            .substitute(&|atom| substituted_atom(atom, subst))
            .ok()
            .and_then(ArrayLen::from_extent)
            .unwrap_or_else(|| size.clone()),
        ArrayLen::Fixed(_) | ArrayLen::Dynamic | ArrayLen::Splice(_) => size.clone(),
    }
}

/// What a parameter or a shape's count stands for under `subst`, `None` when unbound.
fn substituted_atom(atom: &Atom, subst: &HashMap<String, Type>) -> Option<ExtentExpr> {
    match (atom, subst.get(atom.name()?)?) {
        (Atom::Param(_), Type::ConstValue(v)) => Some(ExtentExpr::constant(i128::from(*v))),
        (Atom::Param(_), Type::Generic(renamed)) => Some(ExtentExpr::param(renamed)),
        (Atom::Param(_), Type::Extent(extent)) => Some(extent.clone()),
        (Atom::Count(_), Type::Shape(axes)) => shape_count(axes),
        (Atom::Count(_), Type::Generic(renamed)) => Some(ExtentExpr::count(renamed)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{substitute_shape, unify_tensor_shape};
    use crate::types::{ArrayLen, TensorAxis, Type};
    use std::collections::HashMap;

    fn axis(extent: ArrayLen) -> TensorAxis {
        TensorAxis { name: None, extent }
    }

    fn fixed(extents: &[usize]) -> Vec<TensorAxis> {
        extents.iter().map(|e| axis(ArrayLen::Fixed(*e))).collect()
    }

    fn splice(name: &str) -> TensorAxis {
        axis(ArrayLen::Splice(name.to_string()))
    }

    fn param(name: &str) -> TensorAxis {
        axis(ArrayLen::Param(name.to_string()))
    }

    #[test]
    fn a_splice_takes_the_axes_the_entries_around_it_leave() {
        let mut subst = HashMap::new();
        let template = [param("B"), splice("S"), param("N")];
        assert!(unify_tensor_shape(
            &template,
            &fixed(&[2, 3, 4, 5]),
            &mut subst
        ));
        assert_eq!(subst["B"], Type::ConstValue(2));
        assert_eq!(subst["N"], Type::ConstValue(5));
        assert_eq!(subst["S"], Type::Shape(fixed(&[3, 4])));
        assert_eq!(substitute_shape(&template, &subst), fixed(&[2, 3, 4, 5]));
    }

    #[test]
    fn a_splice_may_take_no_axes_but_not_fewer() {
        let mut subst = HashMap::new();
        assert!(unify_tensor_shape(
            &[splice("S"), param("N")],
            &fixed(&[7]),
            &mut subst
        ));
        assert_eq!(subst["S"], Type::Shape(Vec::new()));
        let mut subst = HashMap::new();
        assert!(!unify_tensor_shape(
            &[param("M"), splice("S"), param("N")],
            &fixed(&[7]),
            &mut subst
        ));
    }

    #[test]
    fn a_callers_own_splice_is_passed_on_by_name() {
        let mut subst = HashMap::new();
        let caller = [splice("R"), axis(ArrayLen::Fixed(3))];
        assert!(unify_tensor_shape(&[splice("S")], &caller, &mut subst));
        assert_eq!(subst["S"], Type::Shape(caller.to_vec()));
        let mut subst = HashMap::new();
        assert!(unify_tensor_shape(
            &[splice("S")],
            &[splice("R")],
            &mut subst
        ));
        assert_eq!(subst["S"], Type::Generic("R".to_string()));
        assert_eq!(
            substitute_shape(&[splice("S"), param("N")], &subst)[0],
            splice("R")
        );
    }

    #[test]
    fn a_dynamic_axis_binds_no_splice() {
        let mut subst = HashMap::new();
        let found = [axis(ArrayLen::Dynamic), axis(ArrayLen::Fixed(2))];
        assert!(!unify_tensor_shape(
            &[splice("S"), param("N")],
            &found,
            &mut subst
        ));
    }

    #[test]
    fn an_extent_parameter_passes_on_the_callers_own() {
        let mut subst = HashMap::new();
        assert!(unify_tensor_shape(&[param("N")], &[param("M")], &mut subst));
        assert_eq!(subst["N"], Type::Generic("M".to_string()));
    }
}
