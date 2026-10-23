// Calls: free functions, methods, associated functions, and generic instantiation.
//
// Reached from the `check_expr` dispatch in this module's `mod.rs`. Every file
// here adds methods to the same `impl TypeChecker` block.

use super::builtins::TO_CHECKED_METHOD;
use super::{COLLECTION_CTOR, TypeChecker, declarations, eval_const_predicate};
use crate::errors::TypeError;
use crate::type_checkers::closures::closure_context;
use crate::type_checkers::declarations::traits::collect_self_assoc;
use crate::type_checkers::derivable::{CLONE_TRAIT, COPY_TRAIT, EQ_METHOD, PARTIAL_EQ_TRAIT};
use crate::type_checkers::statements::{borrow_target_of, function_value_position};
use crate::type_checkers::tensors::TENSOR_TYPE_NAME;
use crate::type_checkers::{BoundInfo, GenericFnSig};
use crate::types::{CollectionKind, FunctionMode, Type};
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
            if let Some(ret) = self.resolve_einsum_builtin(func_name, args, span) {
                return Some(ret);
            }
        }

        // A local binding of function type (a closure or a function-typed
        // parameter) is callable directly: `f(args)`. It shadows a same-named
        // top-level function, matching the usual locals-over-globals precedence.
        if let Some(Type::Function { params, ret, mode }) =
            self.symbols.lookup(func_name).map(|info| info.ty.clone())
        {
            if mode == FunctionMode::Once {
                self.consume_once_callee(func_name, span);
            }
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
            Type::Function { params, ret, .. } => (params, *ret),
            _ => {
                self.record_error(TypeError::NotCallable { ty: func_ty, span });
                return Some(Type::Unknown);
            }
        };

        let args = self.borrow_kernel_inputs(func_name, args);
        self.check_call_args(&args, &param_types, span);

        Some(return_type)
    }

    /// Resolve a method call on a bounded type parameter to a trait method signature
    /// Returning the visible (non-`self`) parameter types and the return type.
    ///
    /// Searches every trait named in the parameter's bounds; the first trait declaring a
    /// method of this name wins. Returns `None` when no bound trait declares it.
    ///
    /// A signature naming `Self` or an associated type is re-resolved from the trait's
    /// declaration, with `Self` standing for the parameter and each associated type for
    /// the bound's `Trait<Assoc = T>` constraint, exactly as an impl's own bindings
    /// resolve it. A bare bound constrains no associated type, so such a call is
    /// reported instead.
    pub(super) fn resolve_generic_trait_method(
        &mut self,
        param: &str,
        method: &str,
        span: Span,
    ) -> Option<(Vec<Type>, Type)> {
        let mut bounds = self.generic_bounds.get(param)?.clone();
        // `Copy: Clone`, so a `Copy` bound reaches `.clone()` as a `Clone` bound would.
        if bounds.iter().any(|b| b.trait_name == COPY_TRAIT) {
            bounds.push(BoundInfo {
                trait_name: CLONE_TRAIT.to_string(),
                assoc: Vec::new(),
            });
        }
        for bound in bounds {
            let Some(sig) = self
                .traits
                .get(&bound.trait_name)
                .and_then(|info| info.methods.get(method))
                .cloned()
            else {
                continue;
            };
            if !sig.resolved_per_use {
                return Some((sig.params.clone(), sig.ret));
            }
            let mut named = Vec::new();
            for ty in sig
                .decl
                .params
                .iter()
                .map(|p| &p.ty)
                .chain(sig.decl.return_type.iter())
            {
                collect_self_assoc(ty, &mut named);
            }
            if let Some(open) = named
                .iter()
                .find(|a| !bound.assoc.iter().any(|(n, _)| n == &a.name))
            {
                self.record_error(TypeError::UnconstrainedAssociatedType {
                    trait_name: bound.trait_name.clone(),
                    method: method.to_string(),
                    assoc: open.name.clone(),
                    span,
                });
                return Some((sig.params.clone(), sig.ret.clone()));
            }
            let saved = std::mem::replace(
                &mut self.self_assoc,
                bound.assoc.iter().cloned().collect::<HashMap<_, _>>(),
            );
            let saved_self = self.self_type.replace(Type::Generic(param.to_string()));
            let params: Vec<Type> = sig
                .decl
                .params
                .iter()
                .map(|p| self.resolve_type(&p.ty).unwrap_or(Type::Unknown))
                .collect();
            let ret = sig
                .decl
                .return_type
                .as_ref()
                .map(|t| self.resolve_type(t).unwrap_or(Type::Unknown))
                .unwrap_or(Type::Void);
            self.self_assoc = saved;
            self.self_type = saved_self;
            return Some((params, ret));
        }
        None
    }

    /// Call a `once` binding: the call may consume what the closure captured, so it uses
    /// the binding up, and a call after that is a use of a moved value.
    fn consume_once_callee(&mut self, name: &str, span: Span) {
        let callee = ast_types::Expr::Identifier(Identifier {
            name: name.to_string(),
            span,
        });
        if let Some(moved_at) = self
            .symbols
            .lookup(name)
            .and_then(|info| info.moves.conflict_with_whole())
        {
            self.record_error(TypeError::UseOfMovedValue {
                name: name.to_string(),
                span,
                moved_at,
            });
        }
        self.record_move(&callee);
    }

    /// Validate a call's arguments against the callee's visible parameter types: arity,
    /// per-argument compatibility, and by-value move recording. Shared by the trait
    /// method-dispatch path.
    pub(crate) fn check_call_args(
        &mut self,
        args: &[ast_types::Expr],
        visible_params: &[Type],
        span: Span,
    ) {
        self.check_receiver_call_args(None, args, visible_params, span);
    }

    /// [`check_call_args`](TypeChecker::check_call_args) for a method, whose receiver
    /// is reborrowed for the call alongside the arguments; `receiver` pairs it with
    /// whether the method takes it `&mut`.
    pub(super) fn check_receiver_call_args(
        &mut self,
        receiver: Option<(&ast_types::Expr, bool)>,
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
            // A function-typed parameter only calls what it is given, and the callee
            // returns before the caller's scope ends, so a scope-bound closure may go in.
            if matches!(expected_ty, Type::Function { .. }) {
                self.function_value_slot = function_value_position(arg);
            }
            let checked = self.check_expr(arg, Some(expected_ty));
            self.function_value_slot = None;
            if let Some(arg_ty) = checked {
                let arg_ty = shared_reborrow(arg_ty, expected_ty);
                if !self.assignable(&arg_ty, expected_ty) {
                    self.record_type_mismatch(expected_ty, arg_ty, arg.span());
                }
            }
            self.record_move(arg);
        }
        self.check_reborrow_exclusivity(receiver, args, visible_params);
        self.hold_argument_stores(receiver, args, visible_params, span);
    }

    /// A callee may store any borrow it is handed into a `&mut` argument whose referent
    /// can hold one (`func set(h: &mut H, s: &string) { h.r = s }`), and nothing in an
    /// erased signature says which, so the binding behind each such argument holds every
    /// borrow the call's other inputs carry: a borrowed argument, what an argument value
    /// holds, and a borrow of the receiver with whatever it holds.
    fn hold_argument_stores(
        &mut self,
        receiver: Option<(&ast_types::Expr, bool)>,
        args: &[ast_types::Expr],
        params: &[Type],
        span: Span,
    ) {
        // Each holder with the argument it came in as; `None` is the receiver.
        let mut holders: Vec<(Option<usize>, Vec<String>)> = Vec::new();
        for (index, (arg, param)) in args.iter().zip(params).enumerate() {
            if let Type::Reference {
                inner,
                mutable: true,
            } = param
                && self.type_holds_reference(inner)
            {
                holders.push((Some(index), self.store_target_bindings(arg)));
            }
        }
        let receiver_root = receiver.and_then(|(expr, _)| Self::place_root_name(expr));
        if let Some((expr, true)) = receiver
            && self
                .projection_type(expr)
                .is_some_and(|ty| self.type_holds_reference(ty.referent()))
        {
            holders.push((None, self.store_target_bindings(expr)));
        }
        for (own, targets) in holders {
            for holder in targets {
                let before = self.symbols.borrow_count(&holder);
                for (index, (arg, param)) in args.iter().zip(params).enumerate() {
                    if Some(index) != own {
                        self.hold_carried_borrows(&holder, arg, param);
                    }
                }
                if own.is_some()
                    && let Some(root) = &receiver_root
                    && *root != holder
                {
                    self.symbols.attach_borrow(&holder, root, false);
                    self.symbols.inherit_borrows(&holder, root);
                }
                self.refuse_outliving_borrows(&holder, before, span);
            }
        }
    }

    /// The bindings a store through `arg`, handed to a `&mut` parameter, writes into:
    /// the place `&mut place` names, or what a `&mut` binding passed on borrows. A
    /// parameter or `self` reached that way is the caller's storage, so it is not one.
    fn store_target_bindings(&self, arg: &ast_types::Expr) -> Vec<String> {
        if let Some((place, _)) = borrow_target_of(arg) {
            return vec![place];
        }
        let Some(root) = Self::place_root_name(arg) else {
            return Vec::new();
        };
        match self.symbols.lookup(&root).map(|info| &info.ty) {
            Some(Type::Reference { .. }) => self.symbols.borrowed_places(&root),
            Some(_) => vec![root],
            None => Vec::new(),
        }
    }

    /// The `&mut` binding `arg` names bare. A call reborrows such a binding rather than
    /// moving it, so it is not a borrow expression the transient counts see.
    fn reborrowed_binding(&self, arg: &ast_types::Expr) -> Option<String> {
        let mut place = arg;
        while let ast_types::Expr::Paren(inner, _) = place {
            place = inner;
        }
        let ast_types::Expr::Identifier(ident) = place else {
            return None;
        };
        match self.symbols.lookup(&ident.name).map(|info| &info.ty) {
            Some(Type::Reference { mutable: true, .. }) => Some(ident.name.clone()),
            _ => None,
        }
    }

    /// Reject one `&mut` binding reborrowed twice by a single call where either
    /// reborrow is exclusive. Both live until the call returns, so the callee would hold
    /// two paths to one place and write through one while reading through the other:
    /// the one-`&mut`-at-a-time rule, which `g(&mut x, &mut x)` already meets at the
    /// borrow site. Separate calls do not overlap, so this is per call, not per statement.
    fn check_reborrow_exclusivity(
        &mut self,
        receiver: Option<(&ast_types::Expr, bool)>,
        args: &[ast_types::Expr],
        params: &[Type],
    ) {
        let mut seen: Vec<(String, bool)> = receiver
            .and_then(|(expr, exclusive)| {
                self.reborrowed_binding(expr).map(|name| (name, exclusive))
            })
            .into_iter()
            .collect();
        for (arg, param) in args.iter().zip(params) {
            let Some(name) = self.reborrowed_binding(arg) else {
                continue;
            };
            let exclusive = matches!(param, Type::Reference { mutable: true, .. });
            if seen
                .iter()
                .any(|(prior, prior_exclusive)| *prior == name && (*prior_exclusive || exclusive))
            {
                self.record_error(TypeError::CannotMutablyBorrowWhileBorrowed {
                    name: name.clone(),
                    span: arg.span(),
                });
            }
            seen.push((name, exclusive));
        }
    }

    /// Verify each bounded type parameter's concrete argument implements every required
    /// trait. A concrete struct or enum satisfies `T: Tr` when an `impl Tr for Type`
    /// exists; a type parameter passed through from an enclosing generic satisfies it
    /// when that parameter carries the same bound. Any other type (e.g. a primitive) has
    /// no user-trait impl and therefore fails the bound.
    pub(super) fn check_trait_bounds(
        &mut self,
        bounds: &HashMap<String, Vec<BoundInfo>>,
        subst: &HashMap<String, Type>,
        span: Span,
    ) {
        for (param, param_bounds) in bounds {
            let Some(concrete) = subst.get(param) else {
                continue;
            };
            for bound in param_bounds {
                // A bound naming an unknown trait is reported once at the impl/decl site;
                // skip it here so the same typo is not echoed at every call.
                if !self.is_checkable_bound(&bound.trait_name) {
                    continue;
                }
                if !self.satisfies_bound(concrete, &bound.trait_name) {
                    self.record_error(TypeError::TraitBoundNotSatisfied {
                        param: param.clone(),
                        ty: concrete.clone(),
                        trait_name: bound.trait_name.clone(),
                        span,
                    });
                    continue;
                }
                self.check_assoc_bindings(bound, concrete, span);
            }
        }
    }

    /// Whether a bound naming `trait_name` is checked against a call's type arguments: a
    /// declared trait is, and so is `PartialEq`, which the compiler knows by name instead.
    pub(crate) fn is_checkable_bound(&self, trait_name: &str) -> bool {
        trait_name == PARTIAL_EQ_TRAIT || self.traits.contains_key(trait_name)
    }

    /// Verify a `Trait<Assoc = T>` bound against what the concrete type argument's impl
    /// bound that associated type to. A type parameter passed through from an enclosing
    /// generic has no impl to read, so its own bound is what must answer.
    pub(crate) fn check_assoc_bindings(&mut self, bound: &BoundInfo, concrete: &Type, span: Span) {
        for (assoc, expected) in &bound.assoc {
            let found = match concrete {
                Type::Struct(name) => self
                    .impl_assoc
                    .get(&(bound.trait_name.clone(), name.clone()))
                    .and_then(|b| b.get(assoc))
                    .cloned(),
                Type::Generic(name) => self
                    .generic_bounds
                    .get(name)
                    .and_then(|bounds| bounds.iter().find(|o| o.trait_name == bound.trait_name))
                    .and_then(|outer| {
                        outer
                            .assoc
                            .iter()
                            .find(|(n, _)| n == assoc)
                            .map(|(_, ty)| ty.clone())
                    }),
                _ => None,
            };
            if found
                .as_ref()
                .is_some_and(|f| f.is_compatible_with(expected))
            {
                continue;
            }
            self.record_error(TypeError::AssociatedTypeBoundMismatch {
                trait_name: bound.trait_name.clone(),
                assoc: assoc.clone(),
                expected: expected.clone(),
                found: found.unwrap_or(Type::Unknown),
                ty: concrete.clone(),
                span,
            });
        }
    }

    /// Type-check a call to a generic function: infer each type parameter from
    /// the corresponding argument, validate arity and per-argument compatibility, and
    /// return the substituted return type.
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
        self.check_generic_call_with(func_name, &sig, type_args, args, None, span)
    }

    /// [`Self::check_generic_call`] against an explicit signature: an associated function
    /// of a generic type is generic over the type's parameters without being a template
    /// of its own, and a generic method over its own. `receiver` is a method's, paired
    /// with whether it is taken `&mut`, as [`Self::check_receiver_call_args`] takes it.
    fn check_generic_call_with(
        &mut self,
        func_name: &str,
        sig: &GenericFnSig,
        type_args: &[ast_types::GenericArg],
        args: &[ast_types::Expr],
        receiver: Option<(&ast_types::Expr, bool)>,
        span: shared_types::Span,
    ) -> Type {
        let args = &*self.borrow_kernel_inputs(func_name, args);

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
            func_name,
            &sig.param_names,
            &sig.const_types,
            type_args,
            &mut subst,
            span,
        );

        // A closure literal handed to a function-typed parameter waits for the other
        // arguments, which may bind the type parameters its own parameters are written
        // in (`apply(|x| x * x, 5)`). Lowering takes the arguments in the same order.
        let deferred = |arg: &ast_types::Expr, param: &Type| {
            matches!(param, Type::Function { .. }) && closure_literal_arg(arg)
        };
        let order = args
            .iter()
            .zip(sig.params.iter())
            .filter(|(arg, param)| !deferred(arg, param))
            .chain(
                args.iter()
                    .zip(sig.params.iter())
                    .filter(|(arg, param)| deferred(arg, param)),
            );
        for (arg, param) in order {
            // A scalar parameter, written as one or a type parameter the turbofish or an
            // earlier argument has bound, types its argument as a non-generic call's does,
            // so a literal there takes it. A function-typed one hands a closure literal
            // the signature inferred so far. Lowering applies the same rule.
            let expected = match param {
                Type::Generic(name) => subst.get(name).cloned(),
                Type::Function { .. } => {
                    let open: Vec<String> = sig
                        .param_names
                        .iter()
                        .filter(|name| !subst.contains_key(*name))
                        .cloned()
                        .collect();
                    Some(self.partially_substituted(param, &subst, &open, arg.span()))
                }
                other => Some(other.clone()),
            }
            .filter(|ty| ty.is_scalar() || matches!(ty, Type::Function { .. }));
            if matches!(param, Type::Function { .. }) {
                self.function_value_slot = function_value_position(arg);
            }
            let checked = self.check_expr(arg, expected.as_ref());
            self.function_value_slot = None;
            let arg_ty = shared_reborrow(checked.unwrap_or(Type::Unknown), param);
            if !matches!(arg_ty, Type::Unknown) && !self.unify_generic(param, &arg_ty, &mut subst) {
                // A shape parameter this argument contradicts is reported by name: the
                // expected type below was itself inferred from an earlier argument, so
                // printing it alone leaves the reader to work out which extent moved.
                match declarations::conflicting_shape_param(param, &arg_ty, &subst) {
                    Some((name, expected, found)) => {
                        self.record_error(TypeError::TensorShapeParamConflict {
                            name,
                            expected,
                            found,
                            span: arg.span(),
                        })
                    }
                    None => {
                        let expected = self.substitute_generic(param, &subst, arg.span());
                        self.record_type_mismatch(&expected, arg_ty, arg.span())
                    }
                }
            }
            // A by-value argument moves a non-Copy binding into the callee.
            self.record_move(arg);
        }
        self.check_reborrow_exclusivity(receiver, args, &sig.params);
        self.hold_argument_stores(receiver, args, &sig.params, span);

        // Every parameter must be bound, by inference or turbofish. A type argument is
        // unconstrained: the body was move-checked against an abstract `T` that is
        // conservatively non-`Copy`, so it stays valid whichever type arrives here.
        for pname in &sig.param_names {
            if !subst.contains_key(pname) {
                self.record_error(TypeError::GenericParamNotInferable {
                    name: pname.clone(),
                    span,
                });
            }
        }

        // Trait bounds (`T: Drawable`) are satisfied when the inferred concrete type
        // argument has a matching `impl Trait for T`.
        self.check_trait_bounds(&sig.bounds, &subst, span);

        // Value predicates (`where N > 0`) are checked against the concrete const values.
        self.check_where_predicates(&sig.where_predicates, &subst);

        self.substitute_generic(&sig.ret, &subst, span)
    }

    /// `ty` with every type parameter `subst` binds replaced, and every position that
    /// names one of `open`, the callee's parameters nothing has bound yet, left as
    /// `Unknown`, which a closure's context reads as "not fixed".
    fn partially_substituted(
        &mut self,
        ty: &Type,
        subst: &HashMap<String, Type>,
        open: &[String],
        span: Span,
    ) -> Type {
        match ty {
            Type::Function { params, ret, mode } => Type::Function {
                params: params
                    .iter()
                    .map(|p| self.partially_substituted(p, subst, open, span))
                    .collect(),
                ret: Box::new(self.partially_substituted(ret, subst, open, span)),
                mode: *mode,
            },
            other if self.names_any(other, open) => Type::Unknown,
            other => self.substitute_generic(other, subst, span),
        }
    }

    /// Whether `ty` names any of the type parameters `names`.
    fn names_any(&self, ty: &Type, names: &[String]) -> bool {
        match ty {
            Type::Generic(name) => names.contains(name),
            Type::Reference { inner, .. } => self.names_any(inner, names),
            Type::Array { element, .. } | Type::Tensor { element, .. } => {
                self.names_any(element, names)
            }
            Type::Tuple(elements) | Type::Collection { args: elements, .. } => {
                elements.iter().any(|e| self.names_any(e, names))
            }
            Type::Function { params, ret, .. } => {
                params.iter().any(|p| self.names_any(p, names)) || self.names_any(ret, names)
            }
            Type::Struct(name) | Type::Enum(name) => self
                .instance_parts(name)
                .is_some_and(|(_, args)| args.iter().any(|a| self.names_any(a, names))),
            _ => false,
        }
    }

    /// Check a method call on an instance of a generic type against the bounds the
    /// method's `impl` puts on the type's parameters: `c.get()` on a `Cell<i32>` needs
    /// `i32: Show` when the impl is `impl<T: Show> Cell<T>`.
    fn check_impl_bounds(&mut self, receiver: &str, method: &str, span: Span) {
        let Some((base, args)) = self.instance_parts(receiver) else {
            return;
        };
        let Some(bounds) = self.impl_bounds.get(&format!("{base}__{method}")).cloned() else {
            return;
        };
        let generics = match self.generic_enums.get(&base) {
            Some(def) => &def.generics,
            None => match self.generic_structs.get(&base) {
                Some(def) => &def.generics,
                None => return,
            },
        };
        let subst: HashMap<String, Type> = generics
            .iter()
            .map(|g| g.name.name.clone())
            .zip(args)
            .collect();
        self.check_trait_bounds(&bounds, &subst, span);
    }

    /// The key of `method` on `receiver` when that method is generic.
    fn generic_method_key(&self, receiver: &Type, method: &str) -> Option<&String> {
        let (Type::Struct(name) | Type::Enum(name) | Type::Newtype(name)) = receiver.referent()
        else {
            return None;
        };
        let key = self.impl_methods.get(name)?.get(method)?;
        self.generic_methods.contains_key(key).then_some(key)
    }

    /// Bind explicit turbofish generic arguments into `subst`, positionally against
    /// the callee's declared parameters. A const parameter takes a const argument (bound
    /// to [`Type::ConstValue`]); a type parameter takes a type argument. Kind or count
    /// mismatches are reported.
    pub(super) fn seed_turbofish(
        &mut self,
        callee: &str,
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
                name: callee.to_string(),
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
    /// const values in `subst`. A predicate that resolves to `false` is an error, and so is
    /// one whose arithmetic has no value (a zero divisor, an overflow); one that cannot be
    /// fully evaluated (still symbolic) is skipped; it is re-checked at the concrete
    /// instantiation.
    pub(crate) fn check_where_predicates(
        &mut self,
        predicates: &[ast_types::Expr],
        subst: &std::collections::HashMap<String, Type>,
    ) {
        for pred in predicates {
            match eval_const_predicate(pred, subst) {
                Ok(Some(false)) => {
                    self.record_error(TypeError::ConstPredicateViolated { span: pred.span() });
                }
                Err((reason, span)) => {
                    self.record_error(TypeError::ConstHasNoValue { reason, span });
                }
                Ok(_) => {}
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
                // The handle is not a value, so it is never checked as one here.
                if let Some(ty) = self.check_kernel_partition(object, field, args, *span) {
                    return Some(ty);
                }
                // The receiver is read through every borrow: `r.abs()` on a `&f64` is
                // `abs` on the `f64`, and `rr.get()` on a `&&P` dispatches on `&P`.
                let obj_ty = self
                    .check_expr(object, None)
                    .unwrap_or(Type::Unknown)
                    .auto_deref();
                if matches!(obj_ty, Type::Unknown) {
                    return Some(Type::Unknown);
                }
                // A turbofish names a generic method's own parameters, so on any other
                // method it names nothing, exactly as on a non-generic free function.
                // `.to_checked::<T>()` is the one builtin that reads its turbofish, as its
                // target type.
                let reads_turbofish = field.name == TO_CHECKED_METHOD
                    && (obj_ty.referent().is_float() || obj_ty.referent().is_half_float());
                if !type_args.is_empty()
                    && !reads_turbofish
                    && self.generic_method_key(&obj_ty, &field.name).is_none()
                {
                    self.record_error(TypeError::TurbofishCountMismatch {
                        name: field.name.clone(),
                        expected: 0,
                        found: type_args.len(),
                        span: *span,
                    });
                }
                // Auto-deref through an immutable borrow: `r.method()` where
                // `r: &Struct` dispatches on `Struct`. The borrow is never moved.
                let struct_name = match obj_ty.referent() {
                    Type::Struct(n) => n.clone(),
                    // An enum's own `impl` is consulted first; a method it does not declare
                    // falls through to the compiler-known surface below, which is where
                    // `Option` / `Result` keep `.unwrap()` and the rest.
                    Type::Enum(n) | Type::Newtype(n)
                        if self
                            .impl_methods
                            .get(n)
                            .is_some_and(|m| m.contains_key(&field.name)) =>
                    {
                        n.clone()
                    }
                    // Trait-method dispatch on a bounded type parameter inside a
                    // generic body: `T: Drawable` lets `obj.draw()` resolve
                    // to the trait's declared signature. Monomorphization later
                    // rebinds it to the concrete type's impl method.
                    Type::Generic(param) => {
                        if let Some((visible_params, ret)) =
                            self.resolve_generic_trait_method(param, &field.name, *span)
                        {
                            self.check_call_args(args, &visible_params, *span);
                            return Some(ret);
                        }
                        // `PartialEq` is the operator lang-item, not a declared trait, so
                        // its `eq` is typed here; `Comparable: PartialEq` reaches it too.
                        let this = Type::Generic(param.clone());
                        if field.name == EQ_METHOD
                            && (self.satisfies_bound(&this, PARTIAL_EQ_TRAIT)
                                || self.satisfies_bound(&this, "Comparable"))
                        {
                            let rhs = Type::Reference {
                                inner: Box::new(this),
                                mutable: false,
                            };
                            self.check_call_args(args, &[rhs], *span);
                            return Some(Type::Bool);
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
                        let exclusive_receiver =
                            matches!(sig.self_param, Some(ast_types::SelfParam::RefMut));
                        self.check_receiver_call_args(
                            Some((object, exclusive_receiver)),
                            args,
                            &sig.params,
                            *span,
                        );
                        return Some(sig.ret.clone());
                    }
                    _ => {
                        // Builtin (non-struct) receivers dispatch a fixed,
                        // compiler-known set of intrinsic methods. A scalar receiver
                        // arrives read through its borrows; a `&string`, `&Vec` or
                        // `&Tensor` keeps its one `&`, which some intrinsics refuse.
                        if let Some(ret) = self.resolve_collection_method(
                            &obj_ty,
                            object,
                            &field.name,
                            args,
                            *span,
                        ) {
                            return Some(ret);
                        }
                        if let Some(ret) = self.resolve_builtin_method(
                            &obj_ty,
                            object,
                            &field.name,
                            type_args,
                            args,
                            *span,
                        ) {
                            return Some(ret);
                        }
                        if let Some(ret) =
                            self.check_supplied_trait_method(&obj_ty, &field.name, args, *span)
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
                        // A derived trait has no method in `impl_methods`: `.clone()`,
                        // `.hash()` and `.fmt_debug(out)` on a struct carrying the derive
                        // are generated instead. A hand-written impl was found above.
                        if let Some(ret) =
                            self.check_supplied_trait_method(&obj_ty, &field.name, args, *span)
                        {
                            return Some(ret);
                        }
                        self.record_error(TypeError::MethodNotFound {
                            struct_name,
                            method_name: field.name.clone(),
                            span: *fa_span,
                        });
                        return Some(Type::Unknown);
                    }
                };

                self.check_impl_bounds(&struct_name, &field.name, *span);

                // Calling a `&mut self` method takes an exclusive borrow of the
                // receiver for the call: the receiver must be a mutable
                // place and must not already be borrowed.
                if self.mut_self_methods.contains(&mangled) {
                    self.check_mut_self_receiver(object, &obj_ty, *fa_span);
                }

                // Calling a consuming (`self`) method hands the receiver to the callee,
                // which destroys it: the caller loses the value. A borrowed receiver has
                // nothing to hand over — the value belongs to whoever it was borrowed
                // from — so reaching one is an error rather than a move.
                if self.consuming_self_methods.contains(&mangled) {
                    self.record_consumed_receiver(object, &obj_ty, *fa_span);
                }

                // The mangled function's first parameter is `self` (the struct).
                // Callers provide only the non-self arguments, so we skip param[0]
                // when checking arity and types.
                let func_ty = self.functions.get(&mangled).cloned();
                let (param_types, return_type) = match func_ty {
                    Some(Type::Function { params, ret, .. }) => (params, *ret),
                    _ => return Some(Type::Unknown),
                };

                // param_types[0] is the implicit `self`; user-visible params start at [1]
                let visible_params = if param_types.is_empty() {
                    &param_types[..]
                } else {
                    &param_types[1..]
                };

                let exclusive_receiver = self.mut_self_methods.contains(&mangled);
                if let Some(own) = self.generic_methods.get(&mangled) {
                    let sig = own.signature(visible_params.to_vec(), return_type);
                    return Some(self.check_generic_call_with(
                        &field.name,
                        &sig,
                        type_args,
                        args,
                        Some((object, exclusive_receiver)),
                        *span,
                    ));
                }
                self.check_receiver_call_args(
                    Some((object, exclusive_receiver)),
                    args,
                    visible_params,
                    *span,
                );

                Some(return_type)
            }

            // Associated function call: `TypeName::func(args)`, or a
            // tuple-variant enum construction `Enum::Variant(args)`.
            Expr::Path {
                type_name,
                member,
                span: path_span,
            } => {
                // `Tensor::<f32, [3, 3]>::zeros()` and the five other construction helpers,
                // unless the program declares its own `Tensor`.
                if type_name.name == TENSOR_TYPE_NAME && self.tensor_name_is_free() {
                    return Some(
                        self.check_tensor_construction(
                            member, type_args, args, expected, *path_span,
                        ),
                    );
                }
                // `Vec::new()` and friends: a compiler-known constructor, unless
                // the program declares its own type of that name.
                if let Some(kind) = CollectionKind::from_name(&type_name.name)
                    && member.name == COLLECTION_CTOR
                    && !self.struct_defs.contains_key(&type_name.name)
                    && !self.enum_defs.contains_key(&type_name.name)
                {
                    return Some(self.check_collection_new(kind, args, expected, *path_span));
                }
                // A member name is never both a variant and an associated function (the
                // impl registration rejects the pair), so a registered function wins here.
                let mangled = format!("{}__{}", type_name.name, member.name);
                let is_enum = self.enum_defs.contains_key(&type_name.name);
                if is_enum && !self.functions.contains_key(&mangled) {
                    return Some(self.check_enum_tuple_call(
                        &type_name.name,
                        &member.name,
                        args,
                        *path_span,
                        expected,
                    ));
                }
                if !is_enum
                    && !self.struct_defs.contains_key(&type_name.name)
                    && !self.newtype_defs.contains_key(&type_name.name)
                {
                    self.record_error(TypeError::UnknownPathType {
                        type_name: type_name.name.clone(),
                        member: member.name.clone(),
                        span: *path_span,
                    });
                    return Some(Type::Unknown);
                }

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
                    Type::Function { params, ret, .. } => (params, *ret),
                    _ => return Some(Type::Unknown),
                };

                // `Cell::new(42)`: an associated function of a generic type is generic
                // over the type's parameters, which the call infers like a generic
                // function's, since lowering builds it on one instance of the type.
                // Its own parameters, if it declares any, follow the type's in the one
                // turbofish the call can write.
                let callee = format!("{}::{}", type_name.name, member.name);
                let method_generics = self.generic_methods.get(&mangled).cloned();
                if let Some((_, type_params)) = self.instance_parts(&type_name.name) {
                    let mut sig = GenericFnSig {
                        param_names: type_params.iter().map(|g| g.to_string()).collect(),
                        const_types: HashMap::new(),
                        params: param_types,
                        ret: return_type,
                        where_predicates: Vec::new(),
                        bounds: HashMap::new(),
                    };
                    if let Some(own) = method_generics {
                        sig.param_names.extend(own.param_names);
                        sig.const_types = own.const_types;
                        sig.where_predicates = own.where_predicates;
                        sig.bounds = own.bounds;
                    }
                    // The impl's own bounds name the type's parameters, which this call
                    // infers alongside the function's.
                    if let Some(impl_bounds) = self.impl_bounds.get(&mangled) {
                        for (param, bounds) in impl_bounds {
                            sig.bounds
                                .entry(param.clone())
                                .or_default()
                                .extend(bounds.iter().cloned());
                        }
                    }
                    return Some(
                        self.check_generic_call_with(&callee, &sig, type_args, args, None, *span),
                    );
                }
                if let Some(own) = method_generics {
                    let sig = own.signature(param_types, return_type);
                    return Some(
                        self.check_generic_call_with(&callee, &sig, type_args, args, None, *span),
                    );
                }

                self.check_call_args(args, &param_types, *span);

                Some(return_type)
            }

            _ if unannotated_closure(func) => Some(self.check_closure_call(func, args, *span)),

            _ => {
                self.function_value_slot = function_value_position(func);
                let expr_ty = self.check_expr(func, None).unwrap_or(Type::Unknown);
                self.function_value_slot = None;
                // Any expression of function type is callable, a closure literal called in
                // place included: the value is the same one a binding would hold.
                if let Type::Function { params, ret, .. } = expr_ty {
                    self.check_call_args(args, &params, *span);
                    return Some(*ret);
                }
                self.record_error(TypeError::NotCallable {
                    ty: expr_ty,
                    span: *span,
                });
                Some(Type::Unknown)
            }
        }
    }

    /// A closure literal called where it is written, `(|x| x + 1)(4)`: its arguments are
    /// its context, so a parameter without an annotation takes its argument's type. The
    /// arguments are checked first for that reason; lowering takes the same order.
    fn check_closure_call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> Type {
        let mut literal = callee;
        while let Expr::Paren(inner, _) = literal {
            literal = inner;
        }
        let Expr::Closure { params, .. } = literal else {
            return Type::Unknown;
        };
        if args.len() != params.len() {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: params.len(),
                found: args.len(),
                span,
            });
        }
        let mut arg_types = Vec::with_capacity(args.len());
        for (index, arg) in args.iter().enumerate() {
            let written = params
                .get(index)
                .and_then(|p| p.ty.as_ref())
                .and_then(|ty| self.resolve_type(ty));
            if matches!(written, Some(Type::Function { .. })) {
                self.function_value_slot = function_value_position(arg);
            }
            let found = self
                .check_expr(arg, written.as_ref())
                .unwrap_or(Type::Unknown);
            self.function_value_slot = None;
            if let Some(written) = &written {
                let found = shared_reborrow(found.clone(), written);
                if !matches!(found, Type::Unknown) && !self.assignable(&found, written) {
                    self.record_type_mismatch(written, found, arg.span());
                }
            }
            self.record_move(arg);
            arg_types.push(found);
        }
        self.function_value_slot = function_value_position(callee);
        let context = closure_context(callee, arg_types, Type::Unknown);
        let ty = self.check_expr(callee, Some(&context));
        self.function_value_slot = None;
        match ty {
            Some(Type::Function { ret, .. }) => *ret,
            _ => Type::Unknown,
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
        if !self.struct_defs.contains_key(&type_name.name)
            && !self.newtype_defs.contains_key(&type_name.name)
        {
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

/// A `&mut T` argument at a `&T` parameter, typed as the shared reborrow the call takes
/// of it. Anything else is returned unchanged.
fn shared_reborrow(found: Type, expected: &Type) -> Type {
    match (found, expected) {
        (
            Type::Reference {
                inner,
                mutable: true,
            },
            Type::Reference { mutable: false, .. },
        ) => Type::Reference {
            inner,
            mutable: false,
        },
        (found, _) => found,
    }
}

/// Whether `callee` is a closure literal, through any parentheses, with a parameter
/// that carries no annotation.
fn unannotated_closure(callee: &Expr) -> bool {
    let mut callee = callee;
    while let Expr::Paren(inner, _) = callee {
        callee = inner;
    }
    matches!(callee, Expr::Closure { params, .. } if params.iter().any(|p| p.ty.is_none()))
}

/// Whether `arg` is a closure literal, through any parentheses.
fn closure_literal_arg(arg: &Expr) -> bool {
    let mut arg = arg;
    while let Expr::Paren(inner, _) = arg {
        arg = inner;
    }
    matches!(arg, Expr::Closure { .. })
}
