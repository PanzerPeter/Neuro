use super::{TypeChecker, VariantForm};
use crate::errors::TypeError;
use crate::types::{ArrayLen, Type};
use ast_types::FieldInit;
use ast_types::{BinaryOp, Expr, UnaryOp};
use shared_types::{Identifier, Literal, Span};
use std::collections::HashMap;

/// The builtin deep-copy method name shared by `string` and Clone-deriving structs.
const CLONE_METHOD: &str = "clone";

impl TypeChecker {
    /// Resolve a compiler-known intrinsic method on a builtin (non-struct) receiver.
    ///
    /// Returns `Some(return_type)` when `method` names an intrinsic for `recv` — recording
    /// an arity diagnostic when the argument count is wrong — and `None` when no such
    /// intrinsic exists, so the caller falls through to the standard `MethodNotFound` error.
    fn resolve_builtin_method(
        &mut self,
        recv: &Type,
        method: &str,
        args: &[Expr],
        call_span: Span,
    ) -> Option<Type> {
        // String methods auto-deref through an immutable borrow `&string`, so the
        // referent drives the string match below.
        match (recv.referent(), method) {
            // O(1) byte length read from the string fat pointer's stored `len`.
            (Type::String, "len") => {
                if !args.is_empty() {
                    self.record_error(TypeError::ArgumentCountMismatch {
                        expected: 0,
                        found: args.len(),
                        span: call_span,
                    });
                }
                Some(Type::U64)
            }
            // Explicit deep copy of an owned string. Takes no arguments and yields a
            // fresh `string`. The canonical opt-out of move-by-default for non-`Copy` types.
            (Type::String, "clone") => {
                if !args.is_empty() {
                    self.record_error(TypeError::ArgumentCountMismatch {
                        expected: 0,
                        found: args.len(),
                        span: call_span,
                    });
                }
                Some(Type::String)
            }
            // Borrowed sub-slice. Takes a single range argument `a..b` / `a..=b`
            // and yields a `&string` view into the receiver's UTF-8 data (zero copy).
            (Type::String, "slice") => Some(self.check_string_slice(args, call_span)),
            // Array length, the compile-time `N` of `[T; N]`. Auto-derefs through
            // a borrow of an array (`&[T; N]`). Takes no arguments and yields `u64`.
            (Type::Array { .. }, "len") => {
                if !args.is_empty() {
                    self.record_error(TypeError::ArgumentCountMismatch {
                        expected: 0,
                        found: args.len(),
                        span: call_span,
                    });
                }
                Some(Type::U64)
            }
            // Wrapping/saturating arithmetic and the right-shift method.
            // Each takes one same-typed argument and returns the receiver's integer type.
            // Matched on `recv` (not the referent): integer intrinsics require a value
            // receiver, since reading a scalar through `&T` needs the deref operator.
            (
                _,
                "wrapping_add" | "wrapping_sub" | "wrapping_mul" | "saturating_add"
                | "saturating_sub" | "saturating_mul" | "shr",
            ) if recv.is_integer() => {
                self.check_unary_int_intrinsic_arg(recv, args, call_span);
                Some(recv.clone())
            }
            _ => None,
        }
    }

    /// Validate the single argument of an integer intrinsic (`wrapping_*`, `saturating_*`,
    /// `.shr`): exactly one argument whose type matches the receiver's integer type. Records
    /// an arity or mismatch diagnostic on violation; the call's result type is unaffected.
    fn check_unary_int_intrinsic_arg(&mut self, recv: &Type, args: &[Expr], call_span: Span) {
        if args.len() != 1 {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: 1,
                found: args.len(),
                span: call_span,
            });
            return;
        }

        if let Some(arg_ty) = self.check_expr(&args[0], Some(recv)) {
            if !arg_ty.is_compatible_with(recv) {
                self.record_error(TypeError::Mismatch {
                    expected: recv.clone(),
                    found: arg_ty,
                    span: args[0].span(),
                });
            }
        }
    }

    /// Type-check `string.slice(range)`: exactly one `a..b` / `a..=b` argument
    /// whose bounds are integers. Returns the `&string` slice type; on any violation a
    /// diagnostic is recorded and the `&string` type is still returned so checking
    /// continues with the documented result type.
    fn check_string_slice(&mut self, args: &[Expr], call_span: Span) -> Type {
        let slice_ty = Type::Reference {
            inner: Box::new(Type::String),
            mutable: false,
        };

        if args.len() != 1 {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: 1,
                found: args.len(),
                span: call_span,
            });
            return slice_ty;
        }

        let Expr::Range { start, end, .. } = &args[0] else {
            self.record_error(TypeError::SliceExpectsRange {
                span: args[0].span(),
            });
            return slice_ty;
        };

        for bound in [start.as_ref(), end.as_ref()] {
            if let Some(bound_ty) = self.check_expr(bound, Some(&Type::U64)) {
                if !matches!(bound_ty, Type::Unknown) && !bound_ty.is_integer() {
                    self.record_error(TypeError::Mismatch {
                        expected: Type::U64,
                        found: bound_ty,
                        span: bound.span(),
                    });
                }
            }
        }

        slice_ty
    }

    /// Type-check a call to a compiler-known panic-family builtin:
    /// `panic(msg: string)`, `assert(cond: bool)`, or `unreachable()`.
    ///
    /// Returns `Some(ty)` when `func_name` names a builtin — recording an arity or
    /// argument-type diagnostic on violation — and `None` otherwise, so the caller falls
    /// through to ordinary function resolution. The result type is `Type::Unknown`: these
    /// builtins **diverge** (they abort and never return), so the call must satisfy any
    /// context — a unit statement, a non-`void` tail return (`func f() -> i32 { panic(..) }`),
    /// or a value binding. `Type::Unknown` is the type system's "compatible with everything"
    /// type, which is exactly the divergent (`never`) contract until a dedicated `!` type lands.
    fn resolve_panic_builtin(
        &mut self,
        func_name: &str,
        args: &[Expr],
        span: Span,
    ) -> Option<Type> {
        // Each builtin's single fixed parameter type, or `None` for the nullary `unreachable`.
        let expected_param = match func_name {
            "panic" => Some(Type::String),
            "assert" => Some(Type::Bool),
            "unreachable" => None,
            _ => return None,
        };

        let expected_arity = if expected_param.is_some() { 1 } else { 0 };
        if args.len() != expected_arity {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: expected_arity,
                found: args.len(),
                span,
            });
            return Some(Type::Unknown);
        }

        if let (Some(expected), Some(arg)) = (expected_param, args.first()) {
            if let Some(arg_ty) = self.check_expr(arg, Some(&expected)) {
                if !arg_ty.is_compatible_with(&expected) {
                    self.record_error(TypeError::Mismatch {
                        expected,
                        found: arg_ty,
                        span: arg.span(),
                    });
                }
            }
        }

        Some(Type::Unknown)
    }

    /// Enforce the rules for the receiver of a `&mut self` method call, which
    /// borrows the receiver mutably for the call's duration.
    ///
    /// A receiver reached through a `&mut T` borrow is already write-capable and
    /// passes; a `&T` receiver cannot yield write access, so it is rejected. An
    /// owned receiver must root in a `mut` binding — mutating `o.inner` needs `o`
    /// itself mutable. A receiver with no place root (a call or literal temporary)
    /// is not assignable, so it is rejected like any `&mut` of a value. Exclusivity
    /// is tracked at binding granularity (matching `&place` borrows), so only a
    /// receiver that *is* the binding registers the call's transient exclusive
    /// borrow and checks for a coexisting borrow; both clear at statement end.
    fn check_mut_self_receiver(&mut self, object: &Expr, obj_ty: &Type, span: shared_types::Span) {
        if let Type::Reference { mutable, .. } = obj_ty {
            if !mutable {
                let name = Self::place_root_name(object).unwrap_or_else(|| "value".to_string());
                self.record_error(TypeError::CannotBorrowMutably { name, span });
            }
            return;
        }

        let Some(name) = Self::place_root_name(object) else {
            self.record_error(TypeError::CannotBorrowValue { span });
            return;
        };
        let Some(info) = self.symbols.lookup(&name) else {
            return;
        };
        if !info.mutable {
            self.record_error(TypeError::CannotBorrowMutably {
                name: name.clone(),
                span,
            });
            return;
        }
        if Self::is_bare_binding(object) {
            if let Some((shared, exclusive)) = self.symbols.borrow_counts(&name) {
                if shared > 0 || exclusive > 0 {
                    self.record_error(TypeError::CannotMutablyBorrowWhileBorrowed {
                        name: name.clone(),
                        span,
                    });
                }
            }
            self.symbols.add_transient_borrow(&name, true);
        }
    }

    /// The root binding name of a place expression, peeling parentheses, field
    /// access, and dereference (`(o).inner` and `*o` both root at `o`). A receiver
    /// with no place root — a call or literal temporary — yields `None`.
    fn place_root_name(expr: &Expr) -> Option<String> {
        match expr {
            Expr::Identifier(ident) => Some(ident.name.clone()),
            Expr::Paren(inner, _) => Self::place_root_name(inner),
            Expr::FieldAccess { object, .. } => Self::place_root_name(object),
            Expr::Deref { operand, .. } => Self::place_root_name(operand),
            _ => None,
        }
    }

    /// Whether `expr` is exactly a binding (an identifier, possibly parenthesised),
    /// as opposed to a sub-place like a field access. Borrow tracking is keyed by
    /// binding, so only a bare binding registers a tracked borrow.
    fn is_bare_binding(expr: &Expr) -> bool {
        match expr {
            Expr::Identifier(_) => true,
            Expr::Paren(inner, _) => Self::is_bare_binding(inner),
            _ => false,
        }
    }

    /// Type-check a bare path enum construction `E::V`: valid only for a
    /// unit variant. A tuple/struct variant used here is a form error. Returns the
    /// enum type for error recovery in every case.
    fn check_enum_unit_path(&mut self, enum_name: &str, variant: &str, span: Span) -> Type {
        let recovery = Type::Enum(enum_name.to_string());
        let Some(info) = self.lookup_enum_variant(enum_name, variant) else {
            self.record_error(TypeError::UnknownEnumVariant {
                enum_name: enum_name.to_string(),
                variant: variant.to_string(),
                span,
            });
            return recovery;
        };
        match info.form {
            VariantForm::Unit => {}
            VariantForm::Tuple => self.record_error(TypeError::EnumVariantFormMismatch {
                enum_name: enum_name.to_string(),
                variant: variant.to_string(),
                expected: "tuple".to_string(),
                hint: "construct it with arguments, e.g. `E::V(...)`".to_string(),
                span,
            }),
            VariantForm::Struct => self.record_error(TypeError::EnumVariantFormMismatch {
                enum_name: enum_name.to_string(),
                variant: variant.to_string(),
                expected: "struct".to_string(),
                hint: "construct it with braces, e.g. `E::V { field: ... }`".to_string(),
                span,
            }),
        }
        recovery
    }

    /// Type-check a tuple-variant enum construction `E::V(args)`: the variant
    /// must be a tuple variant, and the arguments must match its field types by
    /// position. Returns the enum type for error recovery.
    fn check_enum_tuple_call(
        &mut self,
        enum_name: &str,
        variant: &str,
        args: &[Expr],
        span: Span,
    ) -> Type {
        let recovery = Type::Enum(enum_name.to_string());
        let info = match self.lookup_enum_variant(enum_name, variant) {
            Some(info) => info,
            None => {
                self.record_error(TypeError::UnknownEnumVariant {
                    enum_name: enum_name.to_string(),
                    variant: variant.to_string(),
                    span,
                });
                for arg in args {
                    let _ = self.check_expr(arg, None);
                }
                return recovery;
            }
        };

        match info.form {
            VariantForm::Tuple => {}
            VariantForm::Unit => {
                self.record_error(TypeError::EnumVariantFormMismatch {
                    enum_name: enum_name.to_string(),
                    variant: variant.to_string(),
                    expected: "unit".to_string(),
                    hint: "construct it without arguments, e.g. `E::V`".to_string(),
                    span,
                });
                for arg in args {
                    let _ = self.check_expr(arg, None);
                }
                return recovery;
            }
            VariantForm::Struct => {
                self.record_error(TypeError::EnumVariantFormMismatch {
                    enum_name: enum_name.to_string(),
                    variant: variant.to_string(),
                    expected: "struct".to_string(),
                    hint: "construct it with braces, e.g. `E::V { field: ... }`".to_string(),
                    span,
                });
                for arg in args {
                    let _ = self.check_expr(arg, None);
                }
                return recovery;
            }
        }

        // Clone the field types so the immutable enum-table borrow ends before the
        // mutable `check_expr` calls below.
        let field_tys: Vec<Type> = info.fields.iter().map(|(_, t)| t.clone()).collect();

        if args.len() != field_tys.len() {
            self.record_error(TypeError::EnumVariantArityMismatch {
                enum_name: enum_name.to_string(),
                variant: variant.to_string(),
                expected: field_tys.len(),
                found: args.len(),
                span,
            });
        }

        for (arg, expected_ty) in args.iter().zip(field_tys.iter()) {
            if let Some(arg_ty) = self.check_expr(arg, Some(expected_ty)) {
                if !arg_ty.is_compatible_with(expected_ty) {
                    self.record_error(TypeError::Mismatch {
                        expected: expected_ty.clone(),
                        found: arg_ty,
                        span: arg.span(),
                    });
                }
            }
        }
        recovery
    }

    /// Type-check a struct-variant enum construction `E::V { field: expr, ... }`
    /// Every declared field must be provided exactly once with a matching
    /// type, and no unknown fields. Returns the enum type for error recovery.
    fn check_enum_struct_literal(
        &mut self,
        enum_name: &Identifier,
        variant: &Identifier,
        fields: &[FieldInit],
        span: Span,
    ) -> Type {
        let recovery = Type::Enum(enum_name.name.clone());

        if !self.enum_defs.contains_key(&enum_name.name) {
            self.record_error(TypeError::UnknownPathType {
                type_name: enum_name.name.clone(),
                member: variant.name.clone(),
                span,
            });
            for field in fields {
                let _ = self.check_expr(&field.value, None);
            }
            return recovery;
        }

        let info_fields: Vec<(Option<String>, Type)> =
            match self.lookup_enum_variant(&enum_name.name, &variant.name) {
                Some(info) if info.form == VariantForm::Struct => info.fields.clone(),
                Some(_) => {
                    self.record_error(TypeError::EnumVariantFormMismatch {
                        enum_name: enum_name.name.clone(),
                        variant: variant.name.clone(),
                        expected: "non-struct".to_string(),
                        hint: "this variant is not constructed with braces".to_string(),
                        span,
                    });
                    for field in fields {
                        let _ = self.check_expr(&field.value, None);
                    }
                    return recovery;
                }
                None => {
                    self.record_error(TypeError::UnknownEnumVariant {
                        enum_name: enum_name.name.clone(),
                        variant: variant.name.clone(),
                        span,
                    });
                    for field in fields {
                        let _ = self.check_expr(&field.value, None);
                    }
                    return recovery;
                }
            };

        let mut seen: HashMap<String, Span> = HashMap::new();
        for FieldInit {
            name: fname,
            value,
            span: fspan,
        } in fields
        {
            if seen.insert(fname.name.clone(), *fspan).is_some() {
                self.record_error(TypeError::DuplicateEnumField {
                    enum_name: enum_name.name.clone(),
                    variant: variant.name.clone(),
                    field: fname.name.clone(),
                    span: *fspan,
                });
                continue;
            }

            match info_fields
                .iter()
                .find(|(n, _)| n.as_deref() == Some(&fname.name))
            {
                Some((_, expected_ty)) => {
                    if let Some(actual_ty) = self.check_expr(value, Some(expected_ty)) {
                        if !actual_ty.is_compatible_with(expected_ty) {
                            self.record_error(TypeError::Mismatch {
                                expected: expected_ty.clone(),
                                found: actual_ty,
                                span: value.span(),
                            });
                        }
                    }
                }
                None => {
                    self.record_error(TypeError::UnknownEnumField {
                        enum_name: enum_name.name.clone(),
                        variant: variant.name.clone(),
                        field: fname.name.clone(),
                        span: *fspan,
                    });
                    let _ = self.check_expr(value, None);
                }
            }
        }

        for (field_name, _) in &info_fields {
            if let Some(field_name) = field_name {
                if !seen.contains_key(field_name) {
                    self.record_error(TypeError::MissingEnumField {
                        enum_name: enum_name.name.clone(),
                        variant: variant.name.clone(),
                        field: field_name.clone(),
                        span,
                    });
                }
            }
        }

        recovery
    }

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
        // panic-family resolver when no such function is registered.
        if !self.functions.contains_key(func_name) {
            if let Some(ret) = self.resolve_panic_builtin(func_name, args, span) {
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
    fn resolve_generic_trait_method(&self, param: &str, method: &str) -> Option<(Vec<Type>, Type)> {
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
    fn check_call_args(&mut self, args: &[ast_types::Expr], visible_params: &[Type], span: Span) {
        if args.len() != visible_params.len() {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: visible_params.len(),
                found: args.len(),
                span,
            });
        }
        for (arg, expected_ty) in args.iter().zip(visible_params.iter()) {
            if let Some(arg_ty) = self.check_expr(arg, Some(expected_ty)) {
                if !arg_ty.is_compatible_with(expected_ty) {
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
    fn check_trait_bounds(
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
    fn check_generic_call(
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
                && !super::declarations::unify_generic(param, &arg_ty, &mut subst)
            {
                self.record_error(TypeError::Mismatch {
                    expected: super::declarations::substitute_generic(param, &subst),
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

        super::declarations::substitute_generic(&sig.ret, &subst)
    }

    /// Bind explicit turbofish generic arguments into `subst`, positionally against
    /// the callee's declared parameters. A const parameter takes a const argument (bound
    /// to [`Type::ConstValue`]); a type parameter takes a type argument. Kind or count
    /// mismatches are reported.
    fn seed_turbofish(
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

    /// Type-check a newtype construction `Name(value)`: exactly one argument,
    /// whose type must match the newtype's inner type. Yields the newtype.
    fn check_newtype_construction(
        &mut self,
        name: &str,
        inner: &Type,
        args: &[ast_types::Expr],
        span: shared_types::Span,
    ) -> Type {
        if args.len() != 1 {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: 1,
                found: args.len(),
                span,
            });
            // Still type-check any arguments so their own errors surface.
            for arg in args {
                let _ = self.check_expr(arg, Some(inner));
            }
            return Type::Newtype(name.to_string());
        }

        if let Some(arg_ty) = self.check_expr(&args[0], Some(inner)) {
            if !arg_ty.is_compatible_with(inner) {
                self.record_error(TypeError::Mismatch {
                    expected: inner.clone(),
                    found: arg_ty,
                    span: args[0].span(),
                });
            }
        }
        Type::Newtype(name.to_string())
    }

    /// Type-check a generic struct literal: infer each type parameter by
    /// unifying the template's field types against the provided field values, then
    /// monomorphize into a concrete instance. Type arguments are Copy-restricted
    /// (enforced by [`Self::instantiate_generic_struct`]).
    fn check_generic_struct_literal(
        &mut self,
        name: &shared_types::Identifier,
        fields: &[ast_types::FieldInit],
        base: &Option<Box<ast_types::Expr>>,
        span: shared_types::Span,
    ) -> Type {
        let template_fields = self
            .struct_defs
            .get(&name.name)
            .cloned()
            .unwrap_or_default();
        let generics: Vec<String> = self
            .generic_structs
            .get(&name.name)
            .map(|d| d.generics.iter().map(|g| g.name.name.clone()).collect())
            .unwrap_or_default();

        let mut subst: HashMap<String, Type> = HashMap::new();
        let mut seen: HashMap<String, Span> = HashMap::new();
        for ast_types::FieldInit {
            name: fname,
            value,
            span: fspan,
        } in fields
        {
            if seen.insert(fname.name.clone(), *fspan).is_some() {
                self.record_error(TypeError::DuplicateStructField {
                    field_name: fname.name.clone(),
                    span: *fspan,
                });
                continue;
            }
            match template_fields.iter().find(|(n, _)| n == &fname.name) {
                Some((_, expected)) => {
                    let expected = expected.clone();
                    // A field whose type is fully concrete (mentions no type/const
                    // parameter) gives the value its contextual type so a bare literal
                    // infers correctly; a parameterized field is checked with no
                    // expectation so it drives inference instead.
                    let expected_ctx = if mentions_type_parameter(&expected) {
                        None
                    } else {
                        Some(&expected)
                    };
                    let actual = self
                        .check_expr(value, expected_ctx)
                        .unwrap_or(Type::Unknown);
                    if !matches!(actual, Type::Unknown)
                        && !super::declarations::unify_generic(&expected, &actual, &mut subst)
                    {
                        self.record_error(TypeError::Mismatch {
                            expected: super::declarations::substitute_generic(&expected, &subst),
                            found: actual,
                            span: value.span(),
                        });
                    }
                    self.record_move(value);
                }
                None => {
                    self.record_error(TypeError::UnknownField {
                        struct_name: name.name.clone(),
                        field_name: fname.name.clone(),
                        span: *fspan,
                    });
                    let _ = self.check_expr(value, None);
                }
            }
        }

        // Without a `..base` source every field must be provided.
        if base.is_none() {
            for (field_name, _) in &template_fields {
                if !seen.contains_key(field_name) {
                    self.record_error(TypeError::MissingStructField {
                        struct_name: name.name.clone(),
                        field_name: field_name.clone(),
                        span,
                    });
                }
            }
        }

        // Every type parameter must have been inferred from a field value.
        let mut args = Vec::with_capacity(generics.len());
        for g in &generics {
            match subst.get(g) {
                Some(t) => args.push(t.clone()),
                None => return Type::Unknown,
            }
        }

        let inst = self.instantiate_generic_struct(&name.name, &args, span);

        // A `..base` source, when present, must be the same monomorphized instance.
        if let Some(base_expr) = base {
            if let Some(base_ty) = self.check_expr(base_expr, Some(&inst)) {
                if !base_ty.is_compatible_with(&inst) {
                    self.record_error(TypeError::Mismatch {
                        expected: inst.clone(),
                        found: base_ty,
                        span: base_expr.span(),
                    });
                }
            }
        }

        inst
    }

    /// Check an expression and return its type.
    /// Returns None if there was an error (which has been recorded).
    /// Use this for better error recovery - checking can continue with Unknown type.
    ///
    /// # Parameters
    /// - `expr`: The expression to type check
    /// - `expected`: Optional expected type for contextual type inference
    pub(crate) fn check_expr(&mut self, expr: &Expr, expected: Option<&Type>) -> Option<Type> {
        match expr {
            Expr::Literal(lit, span) => match lit {
                Literal::Integer(value, suffix_opt) => {
                    if let Some(suffix) = suffix_opt {
                        Some(self.infer_suffixed_integer_type(*value, suffix, *span))
                    } else {
                        Some(self.infer_integer_type(*value, expected, *span))
                    }
                }
                Literal::Float(_, suffix_opt) => {
                    if let Some(suffix) = suffix_opt {
                        Some(self.infer_suffixed_float_type(suffix))
                    } else {
                        Some(self.infer_float_type(expected))
                    }
                }
                Literal::Boolean(_) => Some(Type::Bool),
                Literal::Char(_) => Some(Type::Char), // Char literals have char type
                Literal::String(_) => Some(Type::String), // String literals have string type
            },

            Expr::Identifier(ident) => {
                // Variables take priority; constants are a fallback so locals can shadow consts.
                if let Some(symbol_info) = self.symbols.lookup(&ident.name) {
                    let ty = symbol_info.ty.clone();
                    if let Some(moved_at) = symbol_info.moved_at {
                        self.record_error(TypeError::UseOfMovedValue {
                            name: ident.name.clone(),
                            span: ident.span,
                            moved_at,
                        });
                    }
                    Some(ty)
                } else if let Some(const_ty) = self.constants.get(&ident.name).cloned() {
                    Some(const_ty)
                } else if let Some(const_param_ty) = self.const_scope.get(&ident.name).cloned() {
                    // A const generic parameter used as a value in a generic body
                    // has its declared integer type.
                    Some(const_param_ty)
                } else {
                    self.record_error(TypeError::UndefinedVariable {
                        name: ident.name.clone(),
                        span: ident.span,
                    });
                    None
                }
            }

            Expr::Binary {
                left,
                op,
                right,
                span,
            } => {
                if op.is_comparison() {
                    if let Expr::Binary { op: inner_op, .. } = left.as_ref() {
                        if inner_op.is_comparison() {
                            self.record_error(TypeError::ComparisonChain { span: *span });
                            return Some(Type::Unknown);
                        }
                    }
                }

                // Check both operands even if one fails, for better error reporting.
                // Left is checked bare to get its natural type, then right uses it
                // as the expected type for symmetric inference.
                let left_ty = self.check_expr(left, None).unwrap_or(Type::Unknown);
                let right_ty = self
                    .check_expr(right, Some(&left_ty))
                    .unwrap_or(Type::Unknown);

                // If either operand is Unknown (error), propagate Unknown
                if matches!(left_ty, Type::Unknown) || matches!(right_ty, Type::Unknown) {
                    return Some(Type::Unknown);
                }

                // Operator-trait dispatch on a user type: when the left operand is
                // a struct that implements the operator's trait, the operator lowers to
                // that impl's method and takes its result type. Checked before the
                // built-in numeric/bitwise/comparison paths, which reject struct operands.
                if let Type::Struct(name) = left_ty.referent() {
                    if let Some(dispatch) = self
                        .operator_binary_impls
                        .get(&(name.clone(), *op))
                        .cloned()
                    {
                        if !right_ty.referent().is_compatible_with(&dispatch.rhs) {
                            self.record_error(TypeError::InvalidBinaryOperator {
                                op: op.to_string(),
                                left: left_ty.clone(),
                                right: right_ty,
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }
                        return Some(dispatch.result);
                    }
                }

                match op {
                    // Arithmetic operators: require numeric types, return same type
                    BinaryOp::Add
                    | BinaryOp::Subtract
                    | BinaryOp::Multiply
                    | BinaryOp::Divide
                    | BinaryOp::Modulo => {
                        // String concatenation: `+` joins two strings into a new
                        // owned, immutable `string`. A `&string` slice participates too, so
                        // a single string reference is peeled exactly as equality does. The
                        // other arithmetic operators have no string meaning. Checked before
                        // the numeric path, which would reject a non-numeric operand.
                        let left_cat = left_ty.peel_string_ref();
                        let right_cat = right_ty.peel_string_ref();
                        if matches!(left_cat, Type::String) || matches!(right_cat, Type::String) {
                            if matches!(op, BinaryOp::Add)
                                && matches!(left_cat, Type::String)
                                && matches!(right_cat, Type::String)
                            {
                                return Some(Type::String);
                            }
                            self.record_error(TypeError::InvalidBinaryOperator {
                                op: op.to_string(),
                                left: left_ty.clone(),
                                right: right_ty.clone(),
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }

                        // Half-precision scalars have no arithmetic: point the
                        // programmer at the `f32` workaround rather than a generic error.
                        if let Some(half) = [&left_ty, &right_ty]
                            .into_iter()
                            .find(|t| t.is_half_float())
                        {
                            self.record_error(TypeError::HalfFloatArithmetic {
                                op: op.to_string(),
                                ty: half.clone(),
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }

                        if !left_ty.is_numeric() {
                            self.record_error(TypeError::InvalidBinaryOperator {
                                op: op.to_string(),
                                left: left_ty.clone(),
                                right: right_ty.clone(),
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }

                        if !left_ty.is_compatible_with(&right_ty) {
                            self.record_error(TypeError::Mismatch {
                                expected: left_ty.clone(),
                                found: right_ty,
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }

                        Some(left_ty)
                    }

                    // Comparison operators: require compatible types, return bool.
                    // `&string` is a borrowed string slice, so an owned `string`
                    // and a `&string` slice compare equal byte-wise in any combination.
                    BinaryOp::Equal | BinaryOp::NotEqual => {
                        let left_cmp = left_ty.peel_string_ref();
                        let right_cmp = right_ty.peel_string_ref();
                        if !left_cmp.is_compatible_with(&right_cmp) {
                            self.record_error(TypeError::Mismatch {
                                expected: left_ty,
                                found: right_ty,
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }
                        Some(Type::Bool)
                    }

                    // Ordering operators: require numeric or `char` operands (this gives
                    // `char` a built-in total order), return bool.
                    BinaryOp::Less
                    | BinaryOp::Greater
                    | BinaryOp::LessEqual
                    | BinaryOp::GreaterEqual => {
                        if !left_ty.is_compatible_with(&right_ty) {
                            self.record_error(TypeError::Mismatch {
                                expected: left_ty,
                                found: right_ty,
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }

                        if !left_ty.is_numeric() && !left_ty.is_char() {
                            self.record_error(TypeError::InvalidBinaryOperator {
                                op: op.to_string(),
                                left: left_ty.clone(),
                                right: right_ty.clone(),
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }

                        Some(Type::Bool)
                    }

                    // Bitwise operators: require integer types, return same type
                    BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor | BinaryOp::Shl => {
                        if !left_ty.is_integer() {
                            self.record_error(TypeError::InvalidBinaryOperator {
                                op: op.to_string(),
                                left: left_ty.clone(),
                                right: right_ty.clone(),
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }

                        if !left_ty.is_compatible_with(&right_ty) {
                            self.record_error(TypeError::Mismatch {
                                expected: left_ty.clone(),
                                found: right_ty,
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }

                        Some(left_ty)
                    }

                    // `??` is parsed (R-to-L per Appendix B) but unwrapping Option/Result
                    // arrives in Phase 2; reject here so codegen never sees it.
                    BinaryOp::NullCoalesce => {
                        self.record_error(TypeError::OperatorNotYetSupported {
                            op: op.to_string(),
                            hint: "requires Option<T> / Result<T, E> — available in Phase 2"
                                .to_string(),
                            span: *span,
                        });
                        Some(Type::Unknown)
                    }

                    // Logical operators: require bool types, return bool
                    BinaryOp::And | BinaryOp::Or => {
                        let mut has_error = false;

                        if !left_ty.is_bool() {
                            self.record_error(TypeError::InvalidBinaryOperator {
                                op: op.to_string(),
                                left: left_ty,
                                right: right_ty.clone(),
                                span: *span,
                            });
                            has_error = true;
                        }

                        if !right_ty.is_bool() {
                            self.record_error(TypeError::InvalidBinaryOperator {
                                op: op.to_string(),
                                left: Type::Bool,
                                right: right_ty,
                                span: *span,
                            });
                            has_error = true;
                        }

                        if has_error {
                            Some(Type::Unknown)
                        } else {
                            Some(Type::Bool)
                        }
                    }
                }
            }

            Expr::Unary { op, operand, span } => {
                // For unary operations, propagate expected type to operand if appropriate
                let expected_operand = match op {
                    UnaryOp::Negate => expected.filter(|t| t.is_numeric()),
                    UnaryOp::Not => None,
                    UnaryOp::BitNot => expected.filter(|t| t.is_integer()),
                };

                let operand_ty = self
                    .check_expr(operand, expected_operand)
                    .unwrap_or(Type::Unknown);

                if matches!(operand_ty, Type::Unknown) {
                    return Some(Type::Unknown);
                }

                // Operator-trait dispatch on a user type: `-a` via `Neg`, `~a` via
                // `Not`. The boolean `!a` (`UnaryOp::Not`) is never overloadable.
                if let Type::Struct(name) = operand_ty.referent() {
                    if let Some(result) =
                        self.operator_unary_impls.get(&(name.clone(), *op)).cloned()
                    {
                        return Some(result);
                    }
                }

                match op {
                    UnaryOp::Negate => {
                        if !operand_ty.is_numeric() {
                            self.record_error(TypeError::InvalidOperator {
                                op: op.to_string(),
                                ty: operand_ty,
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }
                        Some(operand_ty)
                    }
                    UnaryOp::Not => {
                        if !operand_ty.is_bool() {
                            self.record_error(TypeError::InvalidOperator {
                                op: op.to_string(),
                                ty: operand_ty,
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }
                        Some(Type::Bool)
                    }
                    UnaryOp::BitNot => {
                        if !operand_ty.is_integer() {
                            self.record_error(TypeError::InvalidOperator {
                                op: op.to_string(),
                                ty: operand_ty,
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }
                        Some(operand_ty)
                    }
                }
            }

            Expr::Cast {
                expr,
                target_type,
                span,
            } => {
                let from_type = self.check_expr(expr, None)?;
                if matches!(from_type, Type::Unknown) {
                    return Some(Type::Unknown);
                }

                let to_type = self.resolve_type(target_type)?;
                if matches!(to_type, Type::Unknown) {
                    return Some(Type::Unknown);
                }

                if to_type.is_valid_cast(&from_type) {
                    Some(to_type)
                } else {
                    self.record_error(TypeError::Mismatch {
                        expected: to_type.clone(),
                        found: from_type,
                        span: *span,
                    });
                    Some(Type::Unknown)
                }
            }

            Expr::Call {
                func,
                type_args,
                args,
                span,
            } => {
                match &**func {
                    Expr::Identifier(ident) => {
                        self.check_plain_call(&ident.name, type_args, args, *span)
                    }

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
                                if let Some(ret) =
                                    self.resolve_builtin_method(&obj_ty, &field.name, args, *span)
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
                                if field.name == CLONE_METHOD && self.struct_is_clone(&struct_name)
                                {
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
                                if !arg_ty.is_compatible_with(expected_ty) {
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
                        if self.enum_defs.contains_key(&type_name.name) {
                            return Some(self.check_enum_tuple_call(
                                &type_name.name,
                                &member.name,
                                args,
                                *path_span,
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
                                if !arg_ty.is_compatible_with(expected_ty) {
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

            Expr::Path {
                type_name,
                member,
                span,
            } => {
                // A standalone path is either a unit-variant enum value `E::V`
                // or an associated-function reference `Type::func`.
                if self.enum_defs.contains_key(&type_name.name) {
                    return Some(self.check_enum_unit_path(&type_name.name, &member.name, *span));
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

            Expr::Paren(inner, _) => {
                // Propagate expected type through parentheses
                self.check_expr(inner, expected)
            }

            Expr::StructLiteral {
                name,
                fields,
                base,
                span,
            } => {
                // A generic struct literal `Pair { first: 1, second: 2.0 }` infers its
                // type arguments from the field values and monomorphizes.
                if self.is_generic_struct(&name.name) {
                    return Some(self.check_generic_struct_literal(name, fields, base, *span));
                }

                let def = if let Some(d) = self.struct_defs.get(&name.name).cloned() {
                    d
                } else {
                    self.record_error(TypeError::UnknownStruct {
                        name: name.name.clone(),
                        span: name.span,
                    });
                    return None;
                };

                // Track which fields have been provided to detect duplicates and missing fields
                let mut seen: HashMap<String, Span> = HashMap::new();
                for FieldInit {
                    name: fname,
                    value,
                    span: fspan,
                } in fields
                {
                    if let Some(prev_span) = seen.insert(fname.name.clone(), *fspan) {
                        let _ = prev_span;
                        self.record_error(TypeError::DuplicateStructField {
                            field_name: fname.name.clone(),
                            span: *fspan,
                        });
                        continue;
                    }

                    let expected_field_ty = def
                        .iter()
                        .find(|(n, _)| n == &fname.name)
                        .map(|(_, t)| t.clone());

                    if let Some(ref expected_ty) = expected_field_ty {
                        if let Some(actual_ty) = self.check_expr(value, Some(expected_ty)) {
                            if !actual_ty.is_compatible_with(expected_ty) {
                                self.record_error(TypeError::Mismatch {
                                    expected: expected_ty.clone(),
                                    found: actual_ty,
                                    span: value.span(),
                                });
                            }
                        }
                    } else {
                        self.record_error(TypeError::UnknownField {
                            struct_name: name.name.clone(),
                            field_name: fname.name.clone(),
                            span: *fspan,
                        });
                        // Still check the value expression for cascaded errors
                        let _ = self.check_expr(value, None);
                    }
                }

                // A `..base` source supplies every unlisted field, so missing
                // fields are only an error for a plain literal. The base itself
                // must be the same struct type.
                if let Some(base_expr) = base {
                    let expected = Type::Struct(name.name.clone());
                    if let Some(base_ty) = self.check_expr(base_expr, Some(&expected)) {
                        if !base_ty.is_compatible_with(&expected) {
                            self.record_error(TypeError::Mismatch {
                                expected,
                                found: base_ty,
                                span: base_expr.span(),
                            });
                        }
                    }
                } else {
                    for (field_name, _) in &def {
                        if !seen.contains_key(field_name) {
                            self.record_error(TypeError::MissingStructField {
                                struct_name: name.name.clone(),
                                field_name: field_name.clone(),
                                span: *span,
                            });
                        }
                    }
                }

                Some(Type::Struct(name.name.clone()))
            }

            // Struct-variant enum construction `E::V { field: expr, ... }`.
            Expr::EnumStructLiteral {
                enum_name,
                variant,
                fields,
                span,
            } => Some(self.check_enum_struct_literal(enum_name, variant, fields, *span)),

            Expr::FieldAccess {
                object,
                field,
                span,
            } => {
                let obj_ty = self.check_expr(object, None).unwrap_or(Type::Unknown);
                if matches!(obj_ty, Type::Unknown) {
                    return Some(Type::Unknown);
                }

                // Auto-deref through an immutable borrow: `r.field` reads a field of the
                // referent when `r: &Struct`.
                let struct_name = match obj_ty.referent() {
                    Type::Struct(n) => n.clone(),
                    other => {
                        self.record_error(TypeError::UnknownField {
                            struct_name: other.to_string(),
                            field_name: field.name.clone(),
                            span: *span,
                        });
                        return Some(Type::Unknown);
                    }
                };

                let def = self.struct_defs.get(&struct_name).cloned();
                if let Some(def) = def {
                    if let Some((_, field_ty)) = def.iter().find(|(n, _)| n == &field.name) {
                        Some(field_ty.clone())
                    } else {
                        self.record_error(TypeError::UnknownField {
                            struct_name,
                            field_name: field.name.clone(),
                            span: field.span,
                        });
                        Some(Type::Unknown)
                    }
                } else {
                    self.record_error(TypeError::UnknownStruct {
                        name: struct_name,
                        span: *span,
                    });
                    Some(Type::Unknown)
                }
            }

            Expr::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                span,
            } => {
                let cond_ty = self
                    .check_expr(condition, Some(&Type::Bool))
                    .unwrap_or(Type::Unknown);
                if !matches!(cond_ty, Type::Unknown) && !cond_ty.is_bool() {
                    self.record_error(TypeError::Mismatch {
                        expected: Type::Bool,
                        found: cond_ty,
                        span: condition.span(),
                    });
                }

                // Each arm runs on its own path, so a move inside one arm must not
                // leak onto the others or past the `if`. Snapshot the move state
                // after the (unconditional) condition and restore it between arms.
                let move_snapshot = self.symbols.snapshot_moves();

                // Collect arm types: then + each else-if + optional else
                let then_ty = self.check_block_expr_type(then_block);

                let mut arm_types: Vec<Type> = vec![then_ty.clone()];

                for (elif_cond, elif_block) in else_if_blocks {
                    self.symbols.restore_moves(&move_snapshot);
                    let elif_cond_ty = self
                        .check_expr(elif_cond, Some(&Type::Bool))
                        .unwrap_or(Type::Unknown);
                    if !matches!(elif_cond_ty, Type::Unknown) && !elif_cond_ty.is_bool() {
                        self.record_error(TypeError::Mismatch {
                            expected: Type::Bool,
                            found: elif_cond_ty,
                            span: elif_cond.span(),
                        });
                    }
                    arm_types.push(self.check_block_expr_type(elif_block));
                }

                self.symbols.restore_moves(&move_snapshot);
                if let Some(else_stmts) = else_block {
                    arm_types.push(self.check_block_expr_type(else_stmts));
                    self.symbols.restore_moves(&move_snapshot);
                } else {
                    return Some(Type::Void);
                }

                // All arms must agree on type
                let result_ty = arm_types[0].clone();
                for arm_ty in &arm_types[1..] {
                    if !arm_ty.is_compatible_with(&result_ty) {
                        self.record_error(TypeError::Mismatch {
                            expected: result_ty.clone(),
                            found: arm_ty.clone(),
                            span: *span,
                        });
                        return Some(Type::Unknown);
                    }
                }
                Some(result_ty)
            }

            Expr::Block { stmts, .. } => {
                self.symbols.push_scope();
                let ty = self.check_block_expr_type(stmts);
                self.symbols.pop_scope();
                Some(ty)
            }

            // A `loop` in value position evaluates to the value carried by
            // its value-producing `break`s (which must all agree on type). With no
            // value-break it yields unit. `while`/`for` have no expression form.
            Expr::Loop { label, body, .. } => {
                let value_ty = self.check_loop_body(label.as_ref(), true, body);
                Some(value_ty.unwrap_or(Type::Void))
            }

            // `unsafe` is inert in Phase 1.7: it introduces a scope and yields
            // its trailing expression's type, exactly like a bare block.
            Expr::Unsafe { stmts, .. } => {
                self.symbols.push_scope();
                let ty = self.check_block_expr_type(stmts);
                self.symbols.pop_scope();
                Some(ty)
            }

            // Borrow `&place` / `&mut place`. The result type is `&T`
            // (or `&mut T`). Checking the operand reads its type without consuming it:
            // a borrow never moves the borrowed value, which is the whole point of a
            // reference.
            Expr::Reference {
                operand,
                mutable,
                span,
            } => {
                // Only a live binding (`val`/`mut`/parameter) is a borrowable place. A
                // `const` is an inlined value with no address, and temporaries
                // (literals, calls, operator results) are not places.
                let mut place = operand.as_ref();
                while let Expr::Paren(inner, _) = place {
                    place = inner;
                }
                let binding = match place {
                    Expr::Identifier(ident) => self
                        .symbols
                        .lookup(&ident.name)
                        .map(|info| (ident.name.clone(), info.mutable)),
                    _ => None,
                };
                let Some((name, is_mut_binding)) = binding else {
                    self.record_error(TypeError::CannotBorrowValue { span: *span });
                    let _ = self.check_expr(operand, None);
                    return Some(Type::Unknown);
                };
                // `&mut` demands a `mut` binding — you cannot acquire write access
                // through a reference to a value you may not write directly.
                if *mutable && !is_mut_binding {
                    self.record_error(TypeError::CannotBorrowMutably { name, span: *span });
                    let _ = self.check_expr(operand, None);
                    return Some(Type::Unknown);
                }
                let inner = self.check_expr(operand, None)?;
                if matches!(inner, Type::Unknown) {
                    return Some(Type::Unknown);
                }

                // Aliasing exclusivity. A `&mut` borrow is exclusive:
                // no other borrow of the place may be live at the same time. A
                // shared `&` borrow tolerates other shared borrows but excludes an
                // active `&mut`. The counts sum persistent borrows (held by live
                // reference bindings) and transient borrows (taken earlier in this
                // same statement, e.g. another argument of the same call).
                if let Some((shared, exclusive)) = self.symbols.borrow_counts(&name) {
                    if *mutable {
                        if shared > 0 || exclusive > 0 {
                            self.record_error(TypeError::CannotMutablyBorrowWhileBorrowed {
                                name: name.clone(),
                                span: *span,
                            });
                        }
                    } else if exclusive > 0 {
                        self.record_error(TypeError::CannotBorrowWhileMutablyBorrowed {
                            name: name.clone(),
                            span: *span,
                        });
                    }
                }
                // Every fresh borrow starts transient; a `val r = &place` initializer
                // is promoted to a persistent borrow by the `VarDecl` handler.
                self.symbols.add_transient_borrow(&name, *mutable);

                Some(Type::Reference {
                    inner: Box::new(inner),
                    mutable: *mutable,
                })
            }

            // Dereference `*operand`: the result is the referent type `T`. The
            // operand must have a reference type; dereferencing anything else is an
            // error. Reading through either `&T` or `&mut T` is permitted.
            Expr::Deref { operand, span } => {
                let operand_ty = self.check_expr(operand, None)?;
                if matches!(operand_ty, Type::Unknown) {
                    return Some(Type::Unknown);
                }
                match operand_ty {
                    Type::Reference { inner, .. } => Some(*inner),
                    other => {
                        self.record_error(TypeError::CannotDereference {
                            found: other,
                            span: *span,
                        });
                        Some(Type::Unknown)
                    }
                }
            }

            // A range `a..b` is not a first-class value: it is consumed directly
            // by `string.slice` via `check_string_slice`, so reaching it through the
            // general expression path means it was used somewhere a range is not allowed.
            // Still check the bounds for cascaded diagnostics.
            Expr::Range {
                start, end, span, ..
            } => {
                let _ = self.check_expr(start, None);
                let _ = self.check_expr(end, None);
                self.record_error(TypeError::RangeNotAllowed { span: *span });
                Some(Type::Unknown)
            }

            // Array literal `[e0, ...]`: all elements share one type, fixed by
            // the first and required of the rest. An empty literal needs a `[T; N]`
            // annotation to know its element type.
            Expr::ArrayLiteral { elements, span } => {
                let expected_element = match expected {
                    Some(Type::Array { element, .. }) => Some((**element).clone()),
                    _ => None,
                };

                if elements.is_empty() {
                    return match expected {
                        Some(Type::Array { element, size }) => {
                            if let ArrayLen::Fixed(n) = size {
                                if *n != 0 {
                                    self.record_error(TypeError::ArrayLengthMismatch {
                                        expected: *n,
                                        found: 0,
                                        span: *span,
                                    });
                                }
                            }
                            Some(Type::Array {
                                element: element.clone(),
                                size: ArrayLen::Fixed(0),
                            })
                        }
                        _ => {
                            self.record_error(TypeError::CannotInferEmptyArray { span: *span });
                            Some(Type::Unknown)
                        }
                    };
                }

                let element_ty = self
                    .check_expr(&elements[0], expected_element.as_ref())
                    .unwrap_or(Type::Unknown);
                for el in &elements[1..] {
                    let el_ty = self
                        .check_expr(el, Some(&element_ty))
                        .unwrap_or(Type::Unknown);
                    if !matches!(element_ty, Type::Unknown)
                        && !matches!(el_ty, Type::Unknown)
                        && !el_ty.is_compatible_with(&element_ty)
                    {
                        self.record_error(TypeError::Mismatch {
                            expected: element_ty.clone(),
                            found: el_ty,
                            span: el.span(),
                        });
                    }
                }

                if matches!(element_ty, Type::Unknown) {
                    return Some(Type::Unknown);
                }

                if !self.is_type_copy(&element_ty) {
                    self.record_error(TypeError::NonCopyArrayElement {
                        ty: element_ty,
                        span: *span,
                    });
                    return Some(Type::Unknown);
                }

                let size = elements.len();
                if let Some(Type::Array {
                    size: ArrayLen::Fixed(expected_size),
                    ..
                }) = expected
                {
                    if *expected_size != size {
                        self.record_error(TypeError::ArrayLengthMismatch {
                            expected: *expected_size,
                            found: size,
                            span: *span,
                        });
                    }
                }

                Some(Type::Array {
                    element: Box::new(element_ty),
                    size: ArrayLen::Fixed(size),
                })
            }

            // Array indexing `object[index]`: the object is an array (or a
            // borrow of one, auto-derefed per); the index is an integer; the
            // result is the element type.
            Expr::Index {
                object,
                index,
                span,
            } => {
                let obj_ty = self.check_expr(object, None).unwrap_or(Type::Unknown);
                let idx_ty = self.check_expr(index, None).unwrap_or(Type::Unknown);

                if !matches!(idx_ty, Type::Unknown) && !idx_ty.is_integer() {
                    self.record_error(TypeError::IndexNotInteger {
                        found: idx_ty,
                        span: index.span(),
                    });
                }

                if matches!(obj_ty, Type::Unknown) {
                    return Some(Type::Unknown);
                }

                match obj_ty.referent() {
                    Type::Array { element, .. } => Some((**element).clone()),
                    other => {
                        self.record_error(TypeError::NotIndexable {
                            found: other.clone(),
                            span: *span,
                        });
                        Some(Type::Unknown)
                    }
                }
            }

            // Array rest pattern remainder `..rest`: the compiler-internal node
            // a `val [a, b, ..rest] = arr` desugar produces. The source must be an
            // array; the result is the `[T; N - start]` tail. `exact` (no rest binding
            // in the pattern) requires the lengths to match precisely.
            Expr::ArrayRest {
                array,
                start,
                exact,
                span,
            } => {
                let arr_ty = self.check_expr(array, None).unwrap_or(Type::Unknown);
                if matches!(arr_ty, Type::Unknown) {
                    return Some(Type::Unknown);
                }
                match arr_ty.referent() {
                    Type::Array {
                        element,
                        size: ArrayLen::Fixed(n),
                    } => {
                        let n = *n;
                        let mismatch = if *exact { n != *start } else { *start > n };
                        if mismatch {
                            self.record_error(TypeError::ArrayPatternLengthMismatch {
                                expected: *start,
                                found: n,
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }
                        Some(Type::Array {
                            element: element.clone(),
                            size: ArrayLen::Fixed(n - *start),
                        })
                    }
                    // A rest pattern over a const-generic-sized array `[T; N]` cannot be
                    // split at compile time inside the template; it is resolved once
                    // monomorphized. Not supported as a template-body pattern this phase.
                    Type::Array { .. } => Some(Type::Unknown),
                    other => {
                        self.record_error(TypeError::NotIndexable {
                            found: other.clone(),
                            span: *span,
                        });
                        Some(Type::Unknown)
                    }
                }
            }

            // Tuple literal `(e0, e1, ...)`: each element is checked against the
            // corresponding element type of an expected tuple annotation, when present.
            Expr::TupleLiteral { elements, .. } => {
                let expected_elems = match expected {
                    Some(Type::Tuple(es)) if es.len() == elements.len() => Some(es.clone()),
                    _ => None,
                };
                let mut tys = Vec::with_capacity(elements.len());
                for (i, el) in elements.iter().enumerate() {
                    let hint = expected_elems.as_ref().map(|es| &es[i]);
                    let el_ty = self.check_expr(el, hint).unwrap_or(Type::Unknown);
                    if !self.is_type_copy(&el_ty) && !matches!(el_ty, Type::Unknown) {
                        self.record_error(TypeError::NonCopyTupleElement {
                            ty: el_ty.clone(),
                            span: el.span(),
                        });
                    }
                    tys.push(el_ty);
                }
                Some(Type::Tuple(tys))
            }

            // Tuple index `object.N`: the object must be a tuple (or a borrow of
            // one); `N` must be within bounds; the result is the N-th element type.
            Expr::TupleIndex {
                object,
                index,
                span,
            } => {
                let obj_ty = self.check_expr(object, None).unwrap_or(Type::Unknown);
                if matches!(obj_ty, Type::Unknown) {
                    return Some(Type::Unknown);
                }
                match obj_ty.referent() {
                    Type::Tuple(elements) => {
                        if let Some(el) = elements.get(*index) {
                            Some(el.clone())
                        } else {
                            self.record_error(TypeError::TupleIndexOutOfBounds {
                                index: *index,
                                arity: elements.len(),
                                span: *span,
                            });
                            Some(Type::Unknown)
                        }
                    }
                    // `.0` on a newtype reads its single inner value. A newtype
                    // has exactly one field, so any index other than 0 is out of range.
                    Type::Newtype(nt_name) => {
                        if *index == 0 {
                            Some(
                                self.lookup_newtype_inner(nt_name)
                                    .cloned()
                                    .unwrap_or(Type::Unknown),
                            )
                        } else {
                            self.record_error(TypeError::TupleIndexOutOfBounds {
                                index: *index,
                                arity: 1,
                                span: *span,
                            });
                            Some(Type::Unknown)
                        }
                    }
                    other => {
                        self.record_error(TypeError::NotATuple {
                            found: other.clone(),
                            span: *span,
                        });
                        Some(Type::Unknown)
                    }
                }
            }

            // Pattern matching `match scrutinee { ... }`.
            Expr::Match {
                scrutinee,
                arms,
                span,
            } => Some(self.check_match(scrutinee, arms, *span, expected)),

            Expr::Closure {
                params,
                ret,
                body,
                is_move,
                span,
            } => Some(self.check_closure(params, ret.as_ref(), body, *is_move, *span)),
        }
    }

    /// Check all stmts in a block and return the type of the trailing expression, or Void.
    fn check_block_expr_type(&mut self, stmts: &[ast_types::Stmt]) -> Type {
        self.symbols.push_scope();
        let mut result = Type::Void;
        for (i, stmt) in stmts.iter().enumerate() {
            if i == stmts.len() - 1 {
                if let ast_types::Stmt::Expr(expr) = stmt {
                    result = self.check_expr(expr, None).unwrap_or(Type::Unknown);
                    self.symbols.pop_scope();
                    return result;
                }
            }
            let _ = self.check_stmt(stmt);
        }
        self.symbols.pop_scope();
        result
    }
}

/// Evaluate a `where`-clause value predicate to a boolean, given the const
/// parameter values in `subst`. Returns `None` when the predicate is not a fully
/// resolved boolean over const values (it is then deferred to the concrete instance).
pub(crate) fn eval_const_predicate(expr: &Expr, subst: &HashMap<String, Type>) -> Option<bool> {
    match expr {
        Expr::Literal(Literal::Boolean(b), _) => Some(*b),
        Expr::Paren(inner, _) => eval_const_predicate(inner, subst),
        Expr::Binary {
            left, op, right, ..
        } => match op {
            BinaryOp::And => {
                Some(eval_const_predicate(left, subst)? && eval_const_predicate(right, subst)?)
            }
            BinaryOp::Or => {
                Some(eval_const_predicate(left, subst)? || eval_const_predicate(right, subst)?)
            }
            BinaryOp::Less
            | BinaryOp::Greater
            | BinaryOp::LessEqual
            | BinaryOp::GreaterEqual
            | BinaryOp::Equal
            | BinaryOp::NotEqual => {
                let l = eval_const_int(left, subst)?;
                let r = eval_const_int(right, subst)?;
                Some(match op {
                    BinaryOp::Less => l < r,
                    BinaryOp::Greater => l > r,
                    BinaryOp::LessEqual => l <= r,
                    BinaryOp::GreaterEqual => l >= r,
                    BinaryOp::Equal => l == r,
                    BinaryOp::NotEqual => l != r,
                    _ => unreachable!(),
                })
            }
            _ => None,
        },
        _ => None,
    }
}

/// Evaluate a const-integer expression: an integer literal, a const parameter
/// looked up in `subst`, or an arithmetic combination of these. `None` when it is not a
/// fully resolved const integer.
fn eval_const_int(expr: &Expr, subst: &HashMap<String, Type>) -> Option<i128> {
    match expr {
        Expr::Literal(Literal::Integer(v, _), _) => Some(*v as i128),
        Expr::Paren(inner, _) => eval_const_int(inner, subst),
        Expr::Identifier(id) => match subst.get(&id.name) {
            Some(Type::ConstValue(v)) => Some(*v as i128),
            _ => None,
        },
        Expr::Binary {
            left, op, right, ..
        } => {
            let l = eval_const_int(left, subst)?;
            let r = eval_const_int(right, subst)?;
            match op {
                BinaryOp::Add => Some(l + r),
                BinaryOp::Subtract => Some(l - r),
                BinaryOp::Multiply => Some(l * r),
                BinaryOp::Divide if r != 0 => Some(l / r),
                BinaryOp::Modulo if r != 0 => Some(l % r),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Whether a resolved type still mentions a generic type parameter, a const-parameter
/// array length, or an unresolved const value — i.e. it is not fully concrete.
fn mentions_type_parameter(ty: &Type) -> bool {
    match ty {
        Type::Generic(_) | Type::ConstValue(_) => true,
        Type::Reference { inner, .. } => mentions_type_parameter(inner),
        Type::Array { element, size } => {
            matches!(size, ArrayLen::Param(_)) || mentions_type_parameter(element)
        }
        Type::Tuple(elements) => elements.iter().any(mentions_type_parameter),
        Type::Function { params, ret } => {
            params.iter().any(mentions_type_parameter) || mentions_type_parameter(ret)
        }
        _ => false,
    }
}
