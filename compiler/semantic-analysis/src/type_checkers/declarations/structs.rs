//! Struct declarations: field registration, derive validation, and generic
//! struct instantiation.
//!
//! One of the declaration-kind modules under `declarations`; each adds methods
//! to the same `impl TypeChecker` block.

use super::{
    mangle_struct_instance, remap_method_type, substitute_generic, CLONE_TRAIT, COPY_TRAIT,
    DEBUG_TRAIT, DERIVE_ATTRIBUTE, IMPLEMENTED_DERIVES, PARTIAL_EQ_TRAIT, PENDING_DERIVES,
};
use crate::errors::TypeError;
use crate::type_checkers::TypeChecker;
use crate::types::Type;
use ast_types::{ImplDef, SelfParam, StructDef};
use shared_types::Span;
use std::collections::HashMap;

impl TypeChecker {
    /// Register a struct definition without checking field initializers.
    /// Called in the pre-registration pass so that structs can be referenced
    /// by functions and other structs defined later in the file.
    pub(crate) fn register_struct(&mut self, def: &StructDef) -> Option<()> {
        if self.struct_defs.contains_key(&def.name.name) {
            self.record_error(TypeError::StructAlreadyDefined {
                name: def.name.name.clone(),
                span: def.name.span,
            });
            return None;
        }

        let mut fields: Vec<(String, Type)> = Vec::new();
        for field in &def.fields {
            if let Some(ty) = self.resolve_type(&field.ty) {
                fields.push((field.name.name.clone(), ty));
            }
        }

        self.struct_defs.insert(def.name.name.clone(), fields);
        self.record_visibility(def);
        self.record_derive_intent(def);
        Some(())
    }

    /// Record which module a struct was declared in and which of its fields it keeps
    /// private, so a field reached from another module can be rejected.
    pub(super) fn record_visibility(&mut self, def: &StructDef) {
        self.struct_modules
            .insert(def.name.name.clone(), def.module);
        let private: std::collections::HashSet<String> = def
            .fields
            .iter()
            .filter(|f| !f.exported)
            .map(|f| f.name.name.clone())
            .collect();
        if !private.is_empty() {
            self.private_fields.insert(def.name.name.clone(), private);
        }
    }

    /// Record the `@derive(...)` intent declared on a struct, rejecting any argument
    /// that names no derivable trait.
    ///
    /// `Copy` implies `Clone` (a Copy type is trivially cloneable), matching Rust. A
    /// name outside the spec's derivable set is a diagnostic, and so is one the spec
    /// lists but no pass generates yet — a derive that quietly does nothing is worse
    /// than one that refuses, because the program then compiles against behavior it
    /// does not have.
    pub(super) fn record_derive_intent(&mut self, def: &StructDef) {
        let mut derives_copy = false;
        let mut derives_clone = false;
        let mut derives_debug = false;
        let mut derives_partial_eq = false;
        let mut seen: Vec<&str> = Vec::new();
        for attr in &def.attributes {
            if attr.name.name != DERIVE_ATTRIBUTE {
                continue;
            }
            for arg in &attr.args {
                let name = arg.name.as_str();
                if let Some(known) = IMPLEMENTED_DERIVES.iter().find(|d| **d == name) {
                    if seen.contains(known) {
                        self.record_error(TypeError::DuplicateDerive {
                            struct_name: def.name.name.clone(),
                            name: name.to_string(),
                            span: arg.span,
                        });
                        continue;
                    }
                    seen.push(known);
                    match name {
                        COPY_TRAIT => derives_copy = true,
                        CLONE_TRAIT => derives_clone = true,
                        DEBUG_TRAIT => derives_debug = true,
                        _ => derives_partial_eq = true,
                    }
                    continue;
                }
                if PENDING_DERIVES.contains(&name) {
                    self.record_error(TypeError::UnimplementedDerive {
                        name: name.to_string(),
                        span: arg.span,
                    });
                    continue;
                }
                self.record_error(TypeError::UnknownDerive {
                    name: name.to_string(),
                    derivable: IMPLEMENTED_DERIVES.join(", "),
                    span: arg.span,
                });
            }
        }
        if derives_copy {
            self.copy_structs.insert(def.name.name.clone());
        }
        if derives_copy || derives_clone {
            self.clone_structs.insert(def.name.name.clone());
        }
        if derives_debug {
            self.debug_structs.insert(def.name.name.clone());
        }
        if derives_partial_eq {
            self.partial_eq_structs.insert(def.name.name.clone());
        }
    }

    /// Validate `@derive(Debug)` and `@derive(PartialEq)`: every field must itself be
    /// renderable / comparable by the same derived rules.
    ///
    /// A derive generates code straight over the fields, so it can only reach a field
    /// whose type the generated code knows how to handle — a scalar, `string`, or
    /// another struct carrying the same derive. Run after all structs are registered so
    /// a field naming a struct declared later still resolves.
    pub(crate) fn validate_field_derives(&mut self, def: &StructDef) {
        let spans: HashMap<String, Span> = def
            .fields
            .iter()
            .map(|f| (f.name.name.clone(), f.span))
            .collect();
        self.validate_derived_fields_of(&def.name.name, &def.name.name, |name| {
            spans.get(name).copied().unwrap_or(def.name.span)
        });
    }

    /// The field rule behind [`Self::validate_field_derives`], applied to whichever
    /// registered field list `registered` names and reported against `reported`.
    ///
    /// The two names differ for a monomorphized instance: its fields live under the
    /// mangled key, while the diagnostic must name the struct the programmer wrote.
    fn validate_derived_fields_of(
        &mut self,
        registered: &str,
        reported: &str,
        span_of: impl Fn(&str) -> Span,
    ) {
        let debug = self.debug_structs.contains(registered);
        let partial_eq = self.partial_eq_structs.contains(registered);
        if !debug && !partial_eq {
            return;
        }
        // Collect offenders first to avoid borrowing `self` mutably while iterating fields.
        let mut offenders: Vec<(&'static str, String, Type, String, Span)> = Vec::new();
        if let Some(fields) = self.struct_defs.get(registered) {
            for (field_name, field_ty) in fields {
                let span = span_of(field_name);
                // A derive can only ever be added to a struct, so the "derive it too"
                // remedy is offered only when the offending field IS one; for any other
                // type there is nothing to put the attribute on.
                let derivable = matches!(field_ty, Type::Struct(_));
                if debug && !self.is_debug_renderable(field_ty) {
                    let reason = if derivable {
                        "renders no debug form; give it `@derive(Debug)` too"
                    } else {
                        "renders no debug form, and no derive applies to that type"
                    };
                    offenders.push((
                        DEBUG_TRAIT,
                        field_name.clone(),
                        field_ty.clone(),
                        reason.to_string(),
                        span,
                    ));
                }
                if partial_eq && !self.is_derived_comparable(field_ty) {
                    let reason = if derivable {
                        "has no field-wise equality; give it `@derive(PartialEq)` too"
                    } else {
                        "has no field-wise equality, and no derive applies to that type"
                    };
                    offenders.push((
                        PARTIAL_EQ_TRAIT,
                        field_name.clone(),
                        field_ty.clone(),
                        reason.to_string(),
                        span,
                    ));
                }
            }
        }
        for (trait_name, field_name, field_type, reason, span) in offenders {
            self.record_error(TypeError::DeriveFieldUnsupported {
                struct_name: reported.to_string(),
                trait_name: trait_name.to_string(),
                field_name,
                field_type,
                reason,
                span,
            });
        }
    }

    /// Validate that a struct deriving `Copy` has only `Copy` fields.
    ///
    /// Emits a `CopyDeriveNonCopyField` error for each offending field. Run after all
    /// structs are registered so a field whose type is another struct resolves regardless
    /// of declaration order.
    pub(crate) fn validate_copy_derive(&mut self, def: &StructDef) {
        if !self.copy_structs.contains(&def.name.name) {
            return;
        }
        // Collect offenders first to avoid borrowing `self` mutably while iterating fields.
        let mut offenders: Vec<(String, Type, Span)> = Vec::new();
        if let Some(fields) = self.struct_defs.get(&def.name.name) {
            for (field_name, field_ty) in fields {
                if !self.is_type_copy(field_ty) {
                    let span = def
                        .fields
                        .iter()
                        .find(|f| &f.name.name == field_name)
                        .map(|f| f.span)
                        .unwrap_or(def.name.span);
                    offenders.push((field_name.clone(), field_ty.clone(), span));
                }
            }
        }
        for (field_name, field_type, span) in offenders {
            self.record_error(TypeError::CopyDeriveNonCopyField {
                struct_name: def.name.name.clone(),
                field_name,
                field_type,
                span,
            });
        }
    }

    /// Register a generic struct template.
    ///
    /// A generic struct is not itself a usable type — each distinct set of type
    /// arguments is monomorphized into a distinct nominal struct on demand. The
    /// template's field types (carrying [`Type::Generic`] placeholders) are also
    /// stored in `struct_defs` under the base name so generic `impl` method bodies
    /// resolve `self.field` while being checked abstractly, mirroring how a generic
    /// function body checks once with placeholders.
    pub(crate) fn register_generic_struct(&mut self, def: &StructDef) {
        if self.struct_defs.contains_key(&def.name.name)
            || self.generic_structs.contains_key(&def.name.name)
        {
            self.record_error(TypeError::StructAlreadyDefined {
                name: def.name.name.clone(),
                span: def.name.span,
            });
            return;
        }

        self.enter_generic_scope(&def.generics, &def.lifetimes);
        let mut fields: Vec<(String, Type)> = Vec::new();
        for field in &def.fields {
            if let Some(ty) = self.resolve_type(&field.ty) {
                fields.push((field.name.name.clone(), ty));
            }
        }
        self.exit_generic_scope();

        self.struct_defs.insert(def.name.name.clone(), fields);
        self.record_visibility(def);
        self.record_derive_intent(def);
        self.generic_structs
            .insert(def.name.name.clone(), def.clone());
    }

    /// Register a generic `impl` template, e.g. `impl<T> Wrapper<T>`.
    ///
    /// The method signatures are registered under the base struct name (with the
    /// impl's type parameters in scope, so `T` resolves to a placeholder) by reusing
    /// the ordinary impl-registration path; the block is also stored so instantiating
    /// the struct can materialize each method for a concrete instance.
    pub(crate) fn register_generic_impl(&mut self, def: &ImplDef) {
        let base = def.type_name.name.clone();
        if !self.generic_structs.contains_key(&base) {
            self.record_error(TypeError::UnknownStruct {
                name: base.clone(),
                span: def.type_name.span,
            });
            return;
        }
        self.enter_generic_scope(&def.generics, &def.lifetimes);
        let _ = self.register_impl(def);
        self.exit_generic_scope();
        self.generic_impls
            .entry(base)
            .or_default()
            .push(def.clone());
    }

    /// Type-check a generic `impl` block's method bodies once, abstractly:
    /// the impl's type parameters are in scope, so a field typed `T` resolves to a
    /// placeholder — exactly the soundness contract of a bounds-free type parameter.
    pub(crate) fn check_generic_impl(&mut self, def: &ImplDef) {
        self.enter_generic_scope(&def.generics, &def.lifetimes);
        self.check_impl(def);
        self.exit_generic_scope();
    }

    /// Materialize a monomorphized instance of a generic struct with concrete type
    /// arguments, registering its concrete fields and impl methods on demand,
    /// and return its distinct nominal [`Type::Struct`]. Idempotent per instance.
    ///
    /// Type arguments are restricted to `Copy` this phase, mirroring generic
    /// functions: a bare type parameter has no move semantics, so a non-Copy argument
    /// is rejected.
    pub(crate) fn instantiate_generic_struct(
        &mut self,
        base: &str,
        args: &[Type],
        span: Span,
    ) -> Type {
        let template = match self.generic_structs.get(base) {
            Some(t) => t.clone(),
            None => {
                self.record_error(TypeError::NotAGenericType {
                    name: base.to_string(),
                    span,
                });
                return Type::Unknown;
            }
        };
        if args.len() != template.generics.len() {
            self.record_error(TypeError::GenericArgCountMismatch {
                name: base.to_string(),
                expected: template.generics.len(),
                found: args.len(),
                span,
            });
            return Type::Unknown;
        }

        let mangled = mangle_struct_instance(base, args);
        if self.instantiated_structs.insert(mangled.clone()) {
            let mut subst: HashMap<String, Type> = HashMap::new();
            for (gp, arg) in template.generics.iter().zip(args.iter()) {
                // Validate each argument's kind: a const parameter takes a `ConstValue`,
                // a type parameter takes a type. A type argument must be Copy (the
                // abstract-body soundness condition); a const value is exempt.
                let is_const = matches!(gp.kind, ast_types::GenericParamKind::Const(_));
                match arg {
                    Type::ConstValue(_) if is_const => {}
                    Type::ConstValue(_) => self.record_error(TypeError::TurbofishKindMismatch {
                        param: gp.name.name.clone(),
                        expected: "type".to_string(),
                        span,
                    }),
                    _ if is_const => self.record_error(TypeError::TurbofishKindMismatch {
                        param: gp.name.name.clone(),
                        expected: "const".to_string(),
                        span,
                    }),
                    _ if !self.is_type_copy(arg) => {
                        self.record_error(TypeError::GenericArgumentNotCopy {
                            param: gp.name.name.clone(),
                            ty: arg.clone(),
                            span,
                        })
                    }
                    _ => {}
                }
                subst.insert(gp.name.name.clone(), arg.clone());
            }

            // Value predicates (`where N > 0`) hold against the concrete const values.
            self.check_where_predicates(&template.where_predicates.clone(), &subst);

            let template_fields = self.struct_defs.get(base).cloned().unwrap_or_default();
            let concrete_fields: Vec<(String, Type)> = template_fields
                .iter()
                .map(|(n, t)| (n.clone(), substitute_generic(t, &subst)))
                .collect();
            self.struct_defs.insert(mangled.clone(), concrete_fields);

            // A monomorphized instance is the template's struct, so it inherits the
            // template's module and its private fields verbatim.
            if let Some(module) = self.struct_modules.get(base).copied() {
                self.struct_modules.insert(mangled.clone(), module);
            }
            if let Some(private) = self.private_fields.get(base).cloned() {
                self.private_fields.insert(mangled.clone(), private);
            }

            if self.copy_structs.contains(base) {
                self.copy_structs.insert(mangled.clone());
            }
            if self.clone_structs.contains(base) {
                self.clone_structs.insert(mangled.clone());
            }
            if self.debug_structs.contains(base) {
                self.debug_structs.insert(mangled.clone());
            }
            if self.partial_eq_structs.contains(base) {
                self.partial_eq_structs.insert(mangled.clone());
            }
            // The template's own fields are type parameters, which no derive rule can
            // judge; the concrete substitution is the first point at which it can.
            self.validate_derived_fields_of(&mangled, base, |_| span);

            self.instantiate_impls_for(base, &mangled, args);
        }

        Type::Struct(mangled)
    }

    /// Register the methods of every generic `impl` of `base` for the concrete
    /// instance `mangled`, substituting the impl's type parameters (mapped positionally
    /// from the impl's type arguments to the struct's concrete arguments) into each
    /// method signature and rewriting the receiver's `Struct(base)` to `Struct(mangled)`.
    pub(super) fn instantiate_impls_for(&mut self, base: &str, mangled: &str, args: &[Type]) {
        let impls = match self.generic_impls.get(base) {
            Some(v) => v.clone(),
            None => return,
        };
        for imp in &impls {
            let mut impl_subst: HashMap<String, Type> = HashMap::new();
            for (ta, arg) in imp.type_args.iter().zip(args.iter()) {
                if let ast_types::Type::Named(id) = ta {
                    if imp.generics.iter().any(|g| g.name.name == id.name) {
                        impl_subst.insert(id.name.clone(), arg.clone());
                    }
                }
            }
            for method in &imp.methods {
                if matches!(method.self_param, Some(SelfParam::Owned)) {
                    continue;
                }
                let base_key = format!("{}__{}", base, method.name.name);
                let inst_key = format!("{}__{}", mangled, method.name.name);
                if self.functions.contains_key(&inst_key) {
                    continue;
                }
                let sig = match self.functions.get(&base_key).cloned() {
                    Some(s) => s,
                    None => continue,
                };
                let inst_sig = remap_method_type(&sig, &impl_subst, base, mangled);
                self.functions.insert(inst_key.clone(), inst_sig);
                if self.mut_self_methods.contains(&base_key) {
                    self.mut_self_methods.insert(inst_key.clone());
                }
                self.impl_methods
                    .entry(mangled.to_string())
                    .or_default()
                    .insert(method.name.name.clone(), inst_key);
            }

            // The instance implements whatever the template's impl did, with the
            // template's associated bindings substituted. Without this a generic
            // iterator adapter would satisfy `Iterator` under its base name only, and
            // no `for` head over an instance of it could find its `Item`.
            let Some(trait_ident) = &imp.trait_name else {
                continue;
            };
            let trait_name = trait_ident.name.clone();
            if let Some(bindings) = self.impl_assoc.get(&(trait_name.clone(), base.to_string())) {
                let concrete: HashMap<String, Type> = bindings
                    .iter()
                    .map(|(n, t)| (n.clone(), substitute_generic(t, &impl_subst)))
                    .collect();
                self.impl_assoc
                    .insert((trait_name.clone(), mangled.to_string()), concrete);
            }
            self.trait_impls.insert((trait_name, mangled.to_string()));
        }
    }
}
