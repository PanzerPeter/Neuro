// Calls: free functions, methods, associated functions, and generic instantiation.
//
// Reached from the `check_expr` dispatch in this module's `mod.rs`. Every file
// here adds methods to the same `impl TypeChecker` block.

use super::{declarations, eval_const_predicate, TypeChecker, CLONE_METHOD, COLLECTION_CTOR};
use crate::errors::TypeError;
use crate::types::{CollectionKind, Type};
use ast_types::{Expr, GenericArg};
use shared_types::{Identifier, Span};
use std::collections::HashMap;

impl TypeChecker {
    /// Type-check a plain identifier call (free function or previously registered
    /// method with a mangled name). Extracted so the `Call` arm can delegate here.
    pub(crate) fn check_plain_call(
        &mut self,
        func_name: &str,
        type_args: &[ast_types::GenericArg],
        args: &[ast_types::Expr],
        span: shared_types::Span,
    ) -> Option<Type> {
        // A call to a generic function: unify its parameters against the call
        // arguments (and any explicit turbofish), then yield the substituted return type.
        if self.generic_funcs.contains_key(func_name) {
            return Some(self.check_generic_call(func_name, type_args, args, span));
        }
        // A turbofish on a non-generic callee has nothing to bind.
        if !type_args.is_empty() {
            self.record_error(TypeError::TurbofishCountMismatch {
                name: func_name.to_string(),
                expected: 0,
                found: type_args.len(),
                span,
            });
        }

        // Newtype construction `Name(value)`: a call whose callee names a
        // newtype builds a value of that newtype from a single inner-typed argument.
        if let Some(inner) = self.lookup_newtype_inner(func_name).cloned() {
            return Some(self.check_newtype_construction(func_name, &inner, args, span));
        }

        // A user-defined function of the same name shadows the builtin: only consult the
        // panic-family and standard-output resolvers when no such function is registered.
        if !self.functions.contains_key(func_name) {
            if let Some(ret) = self.resolve_panic_builtin(func_name, args, span) {
                return Some(ret);
            }
            if let Some(ret) = self.resolve_io_builtin(func_name, args, span) {
                return Some(ret);
            }
        }

        // A local binding of function type — a closure or a function-typed
        // parameter — is callable directly: `f(args)`. It shadows a same-named
        // top-level function, matching the usual locals-over-globals precedence.
        if let Some(Type::Function { params, ret }) =
            self.symbols.lookup(func_name).map(|info| info.ty.clone())
        {
            self.check_call_args(args, &params, span);
            return Some(*ret);
        }

        let func_ty = if let Some(ty) = self.functions.get(func_name) {
            ty.clone()
        } else {
            self.record_error(TypeError::UndefinedFunction {
                name: func_name.to_string(),
                span,
            });
            return Some(Type::Unknown);
        };

        let (param_types, return_type) = match func_ty {
            Type::Function { params, ret } => (params, *ret),
            _ => {
                self.record_error(TypeError::NotCallable { ty: func_ty, span });
                return Some(Type::Unknown);
            }
        };

        if args.len() != param_types.len() {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: param_types.len(),
                found: args.len(),
                span,
            });
        }

        for (arg, expected_ty) in args.iter().zip(param_types.iter()) {
            if let Some(arg_ty) = self.check_expr(arg, Some(expected_ty)) {
                if !self.assignable(&arg_ty, expected_ty) {
                    self.record_error(TypeError::Mismatch {
                        expected: expected_ty.clone(),
                        found: arg_ty,
                        span: arg.span(),
                    });
                }
            }
            // By-value argument passing moves a non-Copy binding into the callee.
            self.record_move(arg);
        }

        Some(return_type)
    }

    /// Resolve a method call on a bounded type parameter to a trait method signature
    /// Returning the visible (non-`self`) parameter types and the return type.
    ///
    /// Searches every trait named in the parameter's bounds; the first trait declaring a
    /// method of this name wins. Returns `None` when no bound trait declares it.
    pub(super) fn resolve_generic_trait_method(
        &self,
        param: &str,
        method: &str,
    ) -> Option<(Vec<Type>, Type)> {
        let bounds = self.generic_bounds.get(param)?;
        for trait_name in bounds {
            if let Some(sig) = self
                .traits
                .get(trait_name)
                .and_then(|info| info.methods.get(method))
            {
                return Some((sig.params.clone(), sig.ret.clone()));
            }
        }
        None
    }

    /// Validate a call's arguments against the callee's visible parameter types: arity,
    /// per-argument compatibility, and by-value move recording. Shared by the trait
    /// method-dispatch path.
    pub(super) fn check_call_args(
        &mut self,
        args: &[ast_types::Expr],
        visible_params: &[Type],
        span: Span,
    ) {
        if args.len() != visible_params.len() {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: visible_params.len(),
                found: args.len(),
                span,
            });
        }
        for (arg, expected_ty) in args.iter().zip(visible_params.iter()) {
            if let Some(arg_ty) = self.check_expr(arg, Some(expected_ty)) {
                if !self.assignable(&arg_ty, expected_ty) {
                    self.record_error(TypeError::Mismatch {
                        expected: expected_ty.clone(),
                        found: arg_ty,
                        span: arg.span(),
                    });
                }
            }
            self.record_move(arg);
        }
    }

    /// Verify each bounded type parameter's concrete argument implements every required
    /// trait. A concrete struct satisfies `T: Tr` when an `impl Tr for Struct`
    /// exists; a type parameter passed through from an enclosing generic satisfies it
    /// when that parameter carries the same bound. Any other type (e.g. a primitive) has
    /// no user-trait impl and therefore fails the bound.
    pub(super) fn check_trait_bounds(
        &mut self,
        bounds: &HashMap<String, Vec<String>>,
        subst: &HashMap<String, Type>,
        span: Span,
    ) {
        for (param, traits) in bounds {
            let Some(concrete) = subst.get(param) else {
                continue;
            };
            for trait_name in traits {
                // A bound naming an unknown trait is reported once at the impl/decl site;
                // skip it here so the same typo is not echoed at every call.
                if !self.traits.contains_key(trait_name) {
                    continue;
                }
                let satisfied = match concrete {
                    Type::Struct(name) => self
                        .trait_impls
                        .contains(&(trait_name.clone(), name.clone())),
                    Type::Generic(name) => self
                        .generic_bounds
                        .get(name)
                        .is_some_and(|b| b.contains(trait_name)),
                    _ => false,
                };
                if !satisfied {
                    self.record_error(TypeError::TraitBoundNotSatisfied {
                        param: param.clone(),
                        ty: concrete.clone(),
                        trait_name: trait_name.clone(),
                        span,
                    });
                }
            }
        }
    }

    /// Type-check a call to a generic function: infer each type parameter from
    /// the corresponding argument, validate arity and per-argument compatibility, and
    /// return the substituted return type.
    ///
    /// Type arguments are restricted to `Copy` types this phase: generic bodies are
    /// checked abstractly (a bare `T` has no move semantics), which is sound precisely
    /// when the concrete argument is `Copy`. Non-`Copy` generics await broader move
    /// support. Bounds are not enforced (the trait system does not exist yet).
    pub(super) fn check_generic_call(
        &mut self,
        func_name: &str,
        type_args: &[ast_types::GenericArg],
        args: &[ast_types::Expr],
        span: shared_types::Span,
    ) -> Type {
        let sig = match self.generic_funcs.get(func_name) {
            Some(s) => s.clone(),
            None => return Type::Unknown,
        };

        if args.len() != sig.params.len() {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: sig.params.len(),
                found: args.len(),
                span,
            });
        }

        // Seed the substitution with explicit turbofish arguments, then infer the
        // rest from the call arguments. A const parameter binds to `Type::ConstValue`.
        let mut subst: std::collections::HashMap<String, Type> = std::collections::HashMap::new();
        self.seed_turbofish(
            &sig.param_names,
            &sig.const_types,
            type_args,
            &mut subst,
            span,
        );

        for (arg, param) in args.iter().zip(sig.params.iter()) {
            let arg_ty = self.check_expr(arg, None).unwrap_or(Type::Unknown);
            if !matches!(arg_ty, Type::Unknown)
                && !declarations::unify_generic(param, &arg_ty, &mut subst)
            {
                self.record_error(TypeError::Mismatch {
                    expected: declarations::substitute_generic(param, &subst),
                    found: arg_ty,
                    span: arg.span(),
                });
            }
            // A by-value argument moves a non-Copy binding into the callee.
            self.record_move(arg);
        }

        // Every parameter must be bound (by inference or turbofish); a type argument must
        // be Copy (the abstract-body soundness condition). A const parameter binds to a
        // `ConstValue`, which is exempt from the Copy check.
        for pname in &sig.param_names {
            match subst.get(pname) {
                Some(Type::ConstValue(_)) => {}
                Some(ty) if !self.is_type_copy(ty) => {
                    self.record_error(TypeError::GenericArgumentNotCopy {
                        param: pname.clone(),
                        ty: ty.clone(),
                        span,
                    });
                }
                Some(_) => {}
                None => {
                    self.record_error(TypeError::GenericParamNotInferable {
                        name: pname.clone(),
                        span,
                    });
                }
            }
        }

        // Trait bounds (`T: Drawable`) are satisfied when the inferred concrete type
        // argument has a matching `impl Trait for T`.
        self.check_trait_bounds(&sig.bounds, &subst, span);

        // Value predicates (`where N > 0`) are checked against the concrete const values.
        self.check_where_predicates(&sig.where_predicates, &subst);

        declarations::substitute_generic(&sig.ret, &subst)
    }

    /// Bind explicit turbofish generic arguments into `subst`, positionally against
    /// the callee's declared parameters. A const parameter takes a const argument (bound
    /// to [`Type::ConstValue`]); a type parameter takes a type argument. Kind or count
    /// mismatches are reported.
    pub(super) fn seed_turbofish(
        &mut self,
        param_names: &[String],
        const_types: &std::collections::HashMap<String, Type>,
        type_args: &[ast_types::GenericArg],
        subst: &mut std::collections::HashMap<String, Type>,
        span: shared_types::Span,
    ) {
        if type_args.is_empty() {
            return;
        }
        if type_args.len() != param_names.len() {
            self.record_error(TypeError::TurbofishCountMismatch {
                name: param_names.first().cloned().unwrap_or_default(),
                expected: param_names.len(),
                found: type_args.len(),
                span,
            });
            return;
        }
        for (pname, arg) in param_names.iter().zip(type_args.iter()) {
            let is_const = const_types.contains_key(pname);
            match arg {
                ast_types::GenericArg::Const { value, .. } if is_const => {
                    subst.insert(pname.clone(), Type::ConstValue(*value as u64));
                }
                ast_types::GenericArg::Type(ty) if !is_const => {
                    if let Some(resolved) = self.resolve_type(ty) {
                        subst.insert(pname.clone(), resolved);
                    }
                }
                _ => {
                    let expected = if is_const { "const" } else { "type" };
                    self.record_error(TypeError::TurbofishKindMismatch {
                        param: pname.clone(),
                        expected: expected.to_string(),
                        span,
                    });
                }
            }
        }
    }

    /// Evaluate every value predicate from a `where` clause against the concrete
    /// const values in `subst`. A predicate that resolves to `false` is an error; one that
    /// cannot be fully evaluated (still symbolic) is skipped — it is re-checked at the
    /// concrete instantiation.
    pub(crate) fn check_where_predicates(
        &mut self,
        predicates: &[ast_types::Expr],
        subst: &std::collections::HashMap<String, Type>,
    ) {
        for pred in predicates {
            if let Some(false) = eval_const_predicate(pred, subst) {
                self.record_error(TypeError::ConstPredicateViolated { span: pred.span() });
            }
        }
    }

    pub(super) fn check_call_expr(
        &mut self,
        func: &Expr,
        type_args: &[GenericArg],
        args: &[Expr],
        span: &Span,
        expected: Option<&Type>,
    ) -> Option<Type> {
        match func {
            Expr::Identifier(ident) => self.check_plain_call(&ident.name, type_args, args, *span),

            // Method call: `instance.method(args)`
            // The object type determines which struct's methods to search.
            Expr::FieldAccess {
                object,
                field,
                span: fa_span,
            } => {
                let obj_ty = self.check_expr(object, None).unwrap_or(Type::Unknown);
                if matches!(obj_ty, Type::Unknown) {
                    return Some(Type::Unknown);
                }
                // Auto-deref through an immutable borrow: `r.method()` where
                // `r: &Struct` dispatches on `Struct`. The borrow is never moved.
                let struct_name = match obj_ty.referent() {
                    Type::Struct(n) => n.clone(),
                    // Trait-method dispatch on a bounded type parameter inside a
                    // generic body: `T: Drawable` lets `obj.draw()` resolve
                    // to the trait's declared signature. Monomorphization later
                    // rebinds it to the concrete type's impl method.
                    Type::Generic(param) => {
                        if let Some((visible_params, ret)) =
                            self.resolve_generic_trait_method(param, &field.name)
                        {
                            self.check_call_args(args, &visible_params, *span);
                            return Some(ret);
                        }
                        self.record_error(TypeError::MethodNotFound {
                            struct_name: param.clone(),
                            method_name: field.name.clone(),
                            span: *fa_span,
                        });
                        return Some(Type::Unknown);
                    }
                    // Dynamic dispatch through a trait object: the call
                    // resolves against the trait's declared signature, and the
                    // concrete implementation is selected at runtime via the
                    // vtable. A `&mut self` method needs a `&mut dyn Trait`.
                    Type::DynObject(trait_name) => {
                        let trait_name = trait_name.clone();
                        let Some(sig) = self
                            .traits
                            .get(&trait_name)
                            .and_then(|t| t.methods.get(&field.name))
                            .cloned()
                        else {
                            self.record_error(TypeError::MethodNotFound {
                                struct_name: format!("dyn {}", trait_name),
                                method_name: field.name.clone(),
                                span: *fa_span,
                            });
                            return Some(Type::Unknown);
                        };
                        if matches!(sig.self_param, Some(ast_types::SelfParam::RefMut))
                            && !matches!(obj_ty, Type::Reference { mutable: true, .. })
                        {
                            self.record_error(TypeError::CannotBorrowMutably {
                                name: format!("dyn {}", trait_name),
                                span: *fa_span,
                            });
                        }
                        self.check_call_args(args, &sig.params, *span);
                        return Some(sig.ret.clone());
                    }
                    _ => {
                        // Builtin (non-struct) receivers dispatch a fixed,
                        // compiler-known set of intrinsic methods. The original
                        // (possibly `&T`) type is passed so `resolve_builtin_method`
                        // can auto-deref `&string` but keep integer intrinsics
                        // value-only.
                        if let Some(ret) = self.resolve_collection_method(
                            &obj_ty,
                            object,
                            &field.name,
                            args,
                            *span,
                        ) {
                            return Some(ret);
                        }
                        if let Some(ret) =
                            self.resolve_builtin_method(&obj_ty, object, &field.name, args, *span)
                        {
                            return Some(ret);
                        }
                        self.record_error(TypeError::MethodNotFound {
                            struct_name: obj_ty.to_string(),
                            method_name: field.name.clone(),
                            span: *fa_span,
                        });
                        return Some(Type::Unknown);
                    }
                };

                let mangled = match self
                    .impl_methods
                    .get(&struct_name)
                    .and_then(|m| m.get(&field.name))
                {
                    Some(k) => k.clone(),
                    None => {
                        // `.clone()` on a struct that derives `Clone` (or `Copy`) is a
                        // compiler-known builtin — a deep copy yielding the same
                        // struct type. A user-defined `clone` method shadows it (handled
                        // above by the impl_methods lookup).
                        if field.name == CLONE_METHOD && self.struct_is_clone(&struct_name) {
                            if !args.is_empty() {
                                self.record_error(TypeError::ArgumentCountMismatch {
                                    expected: 0,
                                    found: args.len(),
                                    span: *span,
                                });
                            }
                            return Some(Type::Struct(struct_name));
                        }
                        self.record_error(TypeError::MethodNotFound {
                            struct_name,
                            method_name: field.name.clone(),
                            span: *fa_span,
                        });
                        return Some(Type::Unknown);
                    }
                };

                // Calling a `&mut self` method takes an exclusive borrow of the
                // receiver for the call: the receiver must be a mutable
                // place and must not already be borrowed.
                if self.mut_self_methods.contains(&mangled) {
                    self.check_mut_self_receiver(object, &obj_ty, *fa_span);
                }

                // The mangled function's first parameter is `self` (the struct).
                // Callers provide only the non-self arguments, so we skip param[0]
                // when checking arity and types.
                let func_ty = self.functions.get(&mangled).cloned();
                let (param_types, return_type) = match func_ty {
                    Some(Type::Function { params, ret }) => (params, *ret),
                    _ => return Some(Type::Unknown),
                };

                // param_types[0] is the implicit `self`; user-visible params start at [1]
                let visible_params = if param_types.is_empty() {
                    &param_types[..]
                } else {
                    &param_types[1..]
                };

                if args.len() != visible_params.len() {
                    self.record_error(TypeError::ArgumentCountMismatch {
                        expected: visible_params.len(),
                        found: args.len(),
                        span: *span,
                    });
                }

                for (arg, expected_ty) in args.iter().zip(visible_params.iter()) {
                    if let Some(arg_ty) = self.check_expr(arg, Some(expected_ty)) {
                        if !self.assignable(&arg_ty, expected_ty) {
                            self.record_error(TypeError::Mismatch {
                                expected: expected_ty.clone(),
                                found: arg_ty,
                                span: arg.span(),
                            });
                        }
                    }
                    self.record_move(arg);
                }

                Some(return_type)
            }

            // Associated function call: `TypeName::func(args)`, or a
            // tuple-variant enum construction `Enum::Variant(args)`.
            Expr::Path {
                type_name,
                member,
                span: path_span,
            } => {
                // `Vec::new()` and friends: a compiler-known constructor, unless
                // the program declares its own type of that name.
                if let Some(kind) = CollectionKind::from_name(&type_name.name) {
                    if member.name == COLLECTION_CTOR
                        && !self.struct_defs.contains_key(&type_name.name)
                        && !self.enum_defs.contains_key(&type_name.name)
                    {
                        return Some(self.check_collection_new(kind, args, expected, *path_span));
                    }
                }
                if self.enum_defs.contains_key(&type_name.name) {
                    return Some(self.check_enum_tuple_call(
                        &type_name.name,
                        &member.name,
                        args,
                        *path_span,
                        expected,
                    ));
                }
                if !self.struct_defs.contains_key(&type_name.name) {
                    self.record_error(TypeError::UnknownPathType {
                        type_name: type_name.name.clone(),
                        member: member.name.clone(),
                        span: *path_span,
                    });
                    return Some(Type::Unknown);
                }

                let mangled = format!("{}__{}", type_name.name, member.name);
                let func_ty = if let Some(ty) = self.functions.get(&mangled) {
                    ty.clone()
                } else {
                    self.record_error(TypeError::UnknownAssociatedFunction {
                        type_name: type_name.name.clone(),
                        member: member.name.clone(),
                        span: *path_span,
                    });
                    return Some(Type::Unknown);
                };

                let (param_types, return_type) = match func_ty {
                    Type::Function { params, ret } => (params, *ret),
                    _ => return Some(Type::Unknown),
                };

                if args.len() != param_types.len() {
                    self.record_error(TypeError::ArgumentCountMismatch {
                        expected: param_types.len(),
                        found: args.len(),
                        span: *span,
                    });
                }

                for (arg, expected_ty) in args.iter().zip(param_types.iter()) {
                    if let Some(arg_ty) = self.check_expr(arg, Some(expected_ty)) {
                        if !self.assignable(&arg_ty, expected_ty) {
                            self.record_error(TypeError::Mismatch {
                                expected: expected_ty.clone(),
                                found: arg_ty,
                                span: arg.span(),
                            });
                        }
                    }
                    self.record_move(arg);
                }

                Some(return_type)
            }

            _ => {
                let expr_ty = self.check_expr(func, None).unwrap_or(Type::Unknown);
                self.record_error(TypeError::NotCallable {
                    ty: expr_ty,
                    span: *span,
                });
                Some(Type::Unknown)
            }
        }
    }

    pub(super) fn check_path_expr(
        &mut self,
        type_name: &Identifier,
        member: &Identifier,
        span: &Span,
        expected: Option<&Type>,
    ) -> Option<Type> {
        // A standalone path is either a unit-variant enum value `E::V`
        // or an associated-function reference `Type::func`.
        if self.enum_defs.contains_key(&type_name.name) {
            return Some(self.check_enum_unit_path(&type_name.name, &member.name, *span, expected));
        }
        // Standalone path expression (not used as a call target).
        // Validate the struct and member exist; the type is a function type.
        if !self.struct_defs.contains_key(&type_name.name) {
            self.record_error(TypeError::UnknownPathType {
                type_name: type_name.name.clone(),
                member: member.name.clone(),
                span: *span,
            });
            return Some(Type::Unknown);
        }
        let mangled = format!("{}__{}", type_name.name, member.name);
        if let Some(ty) = self.functions.get(&mangled) {
            Some(ty.clone())
        } else {
            self.record_error(TypeError::UnknownAssociatedFunction {
                type_name: type_name.name.clone(),
                member: member.name.clone(),
                span: *span,
            });
            Some(Type::Unknown)
        }
    }
}
