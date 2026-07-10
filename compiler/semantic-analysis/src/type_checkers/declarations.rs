use super::{EnumVariantInfo, TypeChecker, VariantForm};
use crate::errors::TypeError;
use crate::types::{ArrayLen, Type};
use ast_types::{
    ConstDef, EnumDef, Expr, FunctionDef, ImplDef, NewtypeDef, SelfParam, Stmt, StructDef,
    VariantPayload,
};
use shared_types::Span;
use std::collections::{HashMap, HashSet};

/// Built-in type names a newtype may not shadow (§3.15).
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
/// The compiler-known `Drop` lang-item trait name (§2.1).
const DROP_TRAIT: &str = "Drop";
/// The destructor method name required inside an `impl Drop` block.
const DROP_METHOD: &str = "drop";

impl TypeChecker {
    /// Check a function definition.
    ///
    /// A generic function (§3.8, non-empty `func.generics`) is a template: its type
    /// parameters are put in scope so the signature and body type-check with
    /// [`Type::Generic`] placeholders, and its signature is recorded in `generic_funcs`
    /// rather than `functions`. Concrete instantiation happens per call site.
    pub(crate) fn check_function(&mut self, func: &FunctionDef) -> Option<()> {
        // Put the generic type + const parameters in scope for signature + body
        // resolution. A parameter may not shadow a built-in type name.
        self.enter_generic_scope(&func.generics);

        // Check for duplicate parameter names
        use std::collections::HashSet;
        let mut param_names = HashSet::new();
        for param in &func.params {
            if !param_names.insert(&param.name.name) {
                self.record_error(TypeError::VariableAlreadyDefined {
                    name: param.name.name.clone(),
                    span: param.name.span,
                });
            }
        }

        // Resolve parameter types
        let mut param_types = Vec::new();
        for param in &func.params {
            if let Some(param_ty) = self.resolve_type(&param.ty) {
                param_types.push(param_ty);
            } else {
                // Skip this parameter if type resolution failed
                param_types.push(Type::Unknown);
            }
        }

        // Resolve return type (default to Void if not specified)
        let return_type = if let Some(ret_ty) = &func.return_type {
            self.resolve_type(ret_ty).unwrap_or(Type::Void)
        } else {
            Type::Void
        };

        // Register function signature.
        if self.functions.contains_key(&func.name.name)
            || self.generic_funcs.contains_key(&func.name.name)
        {
            self.record_error(TypeError::FunctionAlreadyDefined {
                name: func.name.name.clone(),
                span: func.name.span,
            });
            self.exit_generic_scope();
            return None;
        }

        if func.generics.is_empty() {
            self.functions.insert(
                func.name.name.clone(),
                Type::Function {
                    params: param_types.clone(),
                    ret: Box::new(return_type.clone()),
                },
            );
        } else {
            // A generic template is registered separately; its signature carries the
            // `Type::Generic` placeholders and is instantiated at each call site. A
            // parameter that cannot be inferred from the arguments must be supplied by a
            // turbofish at the call — enforced per call, not here (§3.8).
            let const_types: HashMap<String, Type> = func
                .generics
                .iter()
                .filter_map(|g| match &g.kind {
                    ast_types::GenericParamKind::Const(_) => Some((
                        g.name.name.clone(),
                        self.const_scope
                            .get(&g.name.name)
                            .cloned()
                            .unwrap_or(Type::Unknown),
                    )),
                    ast_types::GenericParamKind::Type => None,
                })
                .collect();
            self.generic_funcs.insert(
                func.name.name.clone(),
                super::GenericFnSig {
                    param_names: func.generics.iter().map(|g| g.name.name.clone()).collect(),
                    const_types,
                    params: param_types.clone(),
                    ret: return_type.clone(),
                    where_predicates: func.where_predicates.clone(),
                },
            );
        }

        // Enter function scope
        self.symbols.push_scope();
        self.current_function_return_type = Some(return_type.clone());

        // Reference-typed parameters outlive the call, so a returned reference may
        // safely borrow one (single-input-reference elision, §2.6). Owned
        // parameters and body locals do not outlive the call.
        self.current_fn_outliving = func
            .params
            .iter()
            .zip(param_types.iter())
            .filter(|(_, ty)| matches!(ty, Type::Reference { .. }))
            .map(|(param, _)| param.name.name.clone())
            .collect();

        // Define parameters in function scope (parameters are immutable by default)
        for (param, param_ty) in func.params.iter().zip(param_types.iter()) {
            // Skip Unknown types to avoid cascading errors
            if matches!(param_ty, Type::Unknown) {
                continue;
            }

            if let Err(duplicate_name) = self.symbols.define(
                param.name.name.clone(),
                param_ty.clone(),
                false, // Function parameters are immutable
            ) {
                self.record_error(TypeError::VariableAlreadyDefined {
                    name: duplicate_name,
                    span: param.name.span,
                });
            }
        }

        // Check function body
        for stmt in &func.body {
            let _ = self.check_stmt(stmt);
        }

        // A trailing expression acts as an expression-based return, so it must
        // match the declared return type.
        if !matches!(return_type, Type::Void) && !func.body.is_empty() {
            if let Some(Stmt::Expr(expr)) = func.body.last() {
                if let Some(expr_type) = self.check_expr(expr, Some(&return_type)) {
                    if !expr_type.is_compatible_with(&return_type) {
                        self.record_error(TypeError::ReturnTypeMismatch {
                            expected: return_type.clone(),
                            found: expr_type,
                            span: expr.span(),
                        });
                    }
                }
                // A trailing reference expression is an implicit return; verify it
                // does not borrow a function-local place (§2.6).
                if matches!(return_type, Type::Reference { .. }) {
                    self.check_returned_reference(expr);
                }
                // Note: If check_expr failed, the error is already recorded
            }
            // Note: Other statement types at the end are allowed - LLVM will catch missing returns
        }

        // Exit function scope
        self.symbols.pop_scope();
        self.current_function_return_type = None;
        self.current_fn_outliving.clear();
        self.exit_generic_scope();

        Some(())
    }

    /// Put a definition's generic parameters in scope for signature and body resolution
    /// (§3.8): type parameters as [`Type::Generic`] placeholders, const parameters as
    /// in-scope values of their declared integer type. Replaces any previous scope.
    fn enter_generic_scope(&mut self, generics: &[ast_types::GenericParam]) {
        self.generic_scope.clear();
        self.const_scope.clear();
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
    }

    /// Register an enum definition: its variants, their construction form, and each
    /// payload field's resolved type (§3.5).
    ///
    /// Payload types are restricted to scalar `Copy` primitives in this phase
    /// (integers, floats, `bool`, `char`); a non-scalar payload (string, struct,
    /// array, tuple, reference) is rejected with `UnsupportedEnumPayload` so the
    /// tagged-union codegen stays a fixed-width slot layout. Broader payloads land
    /// with pattern matching and heap support.
    pub(crate) fn register_enum(&mut self, def: &EnumDef) {
        if self.enum_defs.contains_key(&def.name.name)
            || self.struct_defs.contains_key(&def.name.name)
        {
            self.record_error(TypeError::EnumAlreadyDefined {
                name: def.name.name.clone(),
                span: def.name.span,
            });
            return;
        }

        let mut variants: Vec<EnumVariantInfo> = Vec::new();
        for variant in &def.variants {
            let (form, fields) = match &variant.payload {
                VariantPayload::Unit => (VariantForm::Unit, Vec::new()),
                VariantPayload::Tuple(tys) => {
                    let mut fields = Vec::with_capacity(tys.len());
                    for ty in tys {
                        let resolved = self.resolve_enum_payload_type(ty);
                        fields.push((None, resolved));
                    }
                    (VariantForm::Tuple, fields)
                }
                VariantPayload::Struct(field_defs) => {
                    let mut fields = Vec::with_capacity(field_defs.len());
                    for field in field_defs {
                        let resolved = self.resolve_enum_payload_type(&field.ty);
                        fields.push((Some(field.name.name.clone()), resolved));
                    }
                    (VariantForm::Struct, fields)
                }
            };
            variants.push(EnumVariantInfo {
                name: variant.name.name.clone(),
                form,
                fields,
            });
        }

        self.enum_defs.insert(def.name.name.clone(), variants);
    }

    /// Resolve an enum-variant payload type, rejecting any non-scalar payload with
    /// `UnsupportedEnumPayload` and recovering as `Type::Unknown`.
    fn resolve_enum_payload_type(&mut self, ty: &ast_types::Type) -> Type {
        let Some(resolved) = self.resolve_type(ty) else {
            return Type::Unknown;
        };
        if Self::is_scalar_payload(&resolved) {
            resolved
        } else {
            self.record_error(TypeError::UnsupportedEnumPayload {
                ty: resolved,
                span: ty.span(),
            });
            Type::Unknown
        }
    }

    /// Whether `ty` is a scalar `Copy` primitive admissible as an enum payload in
    /// this phase: any integer, full- or half-precision float, `bool`, or `char`.
    fn is_scalar_payload(ty: &Type) -> bool {
        ty.is_integer()
            || ty.is_float()
            || ty.is_half_float()
            || matches!(ty, Type::Bool | Type::Char)
    }

    /// Pre-register a newtype's NAME (§3.15) with a placeholder inner type, so the
    /// name resolves everywhere before its inner type is resolved in
    /// [`Self::resolve_newtype_inners`]. Rejects a name that collides with a builtin,
    /// struct, enum, or another newtype.
    pub(crate) fn predeclare_newtype(&mut self, def: &NewtypeDef) {
        let name = &def.name.name;
        if is_builtin_type_name(name)
            || self.struct_defs.contains_key(name)
            || self.enum_defs.contains_key(name)
            || self.newtype_defs.contains_key(name)
        {
            self.record_error(TypeError::NewtypeAlreadyDefined {
                name: name.clone(),
                span: def.name.span,
            });
            return;
        }
        self.newtype_defs.insert(name.clone(), Type::Unknown);
    }

    /// Resolve each newtype's inner type now that every nominal name is registered,
    /// then reject cyclic newtypes and non-Copy inner types (§3.15).
    pub(crate) fn resolve_newtype_inners(&mut self, items: &[ast_types::Item]) {
        // Phase 1: resolve every accepted newtype's inner type. A duplicate/rejected
        // declaration (still absent or already resolved) is skipped so it cannot
        // overwrite the first, valid definition.
        let mut spans: HashMap<String, Span> = HashMap::new();
        for item in items {
            if let ast_types::Item::Newtype(def) = item {
                let name = def.name.name.clone();
                if spans.contains_key(&name) || self.newtype_defs.get(&name) != Some(&Type::Unknown)
                {
                    continue;
                }
                let inner = self.resolve_type(&def.inner).unwrap_or(Type::Unknown);
                self.newtype_defs.insert(name.clone(), inner);
                spans.insert(name, def.inner.span());
            }
        }

        // Phase 2: with all inners resolved, reject cycles (which would otherwise make
        // the Copy check below recurse forever) and non-Copy inner types.
        for (name, inner_span) in &spans {
            let mut seen = HashSet::new();
            if self.newtype_cycles(name, &mut seen) {
                self.record_error(TypeError::CyclicNewtype {
                    name: name.clone(),
                    span: *inner_span,
                });
                // Break the cycle so downstream Copy checks terminate.
                self.newtype_defs.insert(name.clone(), Type::Unknown);
                continue;
            }
            let inner = self
                .newtype_defs
                .get(name)
                .cloned()
                .unwrap_or(Type::Unknown);
            if !matches!(inner, Type::Unknown) && !self.is_type_copy(&inner) {
                self.record_error(TypeError::NewtypeInnerNotCopy {
                    name: name.clone(),
                    inner,
                    span: *inner_span,
                });
            }
        }
    }

    /// Whether newtype `name` reaches itself through its inner type chain (§3.15),
    /// following newtype wrappers and the Copy-recursing aggregates (arrays, tuples,
    /// references). `seen` is the current DFS path; a revisit is a back-edge.
    fn newtype_cycles(&self, name: &str, seen: &mut HashSet<String>) -> bool {
        if !seen.insert(name.to_string()) {
            return true;
        }
        let cyclic = match self.newtype_defs.get(name) {
            Some(inner) => self.type_reaches_cyclic_newtype(&inner.clone(), seen),
            None => false,
        };
        seen.remove(name);
        cyclic
    }

    /// Whether `ty` reaches a newtype currently on the DFS path in `seen`.
    fn type_reaches_cyclic_newtype(&self, ty: &Type, seen: &mut HashSet<String>) -> bool {
        match ty {
            Type::Newtype(n) => self.newtype_cycles(n, seen),
            Type::Array { element, .. } => self.type_reaches_cyclic_newtype(element, seen),
            Type::Tuple(elements) => elements
                .iter()
                .any(|e| self.type_reaches_cyclic_newtype(e, seen)),
            Type::Reference { inner, .. } => self.type_reaches_cyclic_newtype(inner, seen),
            _ => false,
        }
    }

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
        self.record_derive_intent(def);
        Some(())
    }

    /// Record the `@derive(Copy, Clone)` intent declared on a struct.
    ///
    /// Only `Copy` and `Clone` are acted upon; any other derive argument (e.g. `Debug`)
    /// is accepted and ignored so the surface stays forward compatible. `Copy` implies
    /// `Clone` (a Copy type is trivially cloneable), matching §2.3 and Rust.
    fn record_derive_intent(&mut self, def: &StructDef) {
        let mut derives_copy = false;
        let mut derives_clone = false;
        for attr in &def.attributes {
            if attr.name.name != DERIVE_ATTRIBUTE {
                continue;
            }
            for arg in &attr.args {
                match arg.name.as_str() {
                    COPY_TRAIT => derives_copy = true,
                    CLONE_TRAIT => derives_clone = true,
                    _ => {}
                }
            }
        }
        if derives_copy {
            self.copy_structs.insert(def.name.name.clone());
        }
        if derives_copy || derives_clone {
            self.clone_structs.insert(def.name.name.clone());
        }
    }

    /// Validate that a struct deriving `Copy` has only `Copy` fields (§2.3).
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

    /// Register a generic struct template (§3.8).
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

        self.enter_generic_scope(&def.generics);
        let mut fields: Vec<(String, Type)> = Vec::new();
        for field in &def.fields {
            if let Some(ty) = self.resolve_type(&field.ty) {
                fields.push((field.name.name.clone(), ty));
            }
        }
        self.exit_generic_scope();

        self.struct_defs.insert(def.name.name.clone(), fields);
        self.record_derive_intent(def);
        self.generic_structs
            .insert(def.name.name.clone(), def.clone());
    }

    /// Register a generic `impl` template (§3.8), e.g. `impl<T> Wrapper<T>`.
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
        self.enter_generic_scope(&def.generics);
        let _ = self.register_impl(def);
        self.exit_generic_scope();
        self.generic_impls
            .entry(base)
            .or_default()
            .push(def.clone());
    }

    /// Type-check a generic `impl` block's method bodies once, abstractly (§3.8):
    /// the impl's type parameters are in scope, so a field typed `T` resolves to a
    /// placeholder — exactly the soundness contract of a bounds-free type parameter.
    pub(crate) fn check_generic_impl(&mut self, def: &ImplDef) {
        self.enter_generic_scope(&def.generics);
        self.check_impl(def);
        self.exit_generic_scope();
    }

    /// Materialize a monomorphized instance of a generic struct with concrete type
    /// arguments (§3.8), registering its concrete fields and impl methods on demand,
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

            if self.copy_structs.contains(base) {
                self.copy_structs.insert(mangled.clone());
            }
            if self.clone_structs.contains(base) {
                self.clone_structs.insert(mangled.clone());
            }

            self.instantiate_impls_for(base, &mangled, args);
        }

        Type::Struct(mangled)
    }

    /// Register the methods of every generic `impl` of `base` for the concrete
    /// instance `mangled`, substituting the impl's type parameters (mapped positionally
    /// from the impl's type arguments to the struct's concrete arguments) into each
    /// method signature and rewriting the receiver's `Struct(base)` to `Struct(mangled)`.
    fn instantiate_impls_for(&mut self, base: &str, mangled: &str, args: &[Type]) {
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
        }
    }

    /// Register all method signatures from an `impl` block into the global
    /// function table under mangled names (`StructName__methodName`).
    ///
    /// Consuming `self` is rejected here so it never reaches codegen; `&mut self`
    /// is recorded in `mut_self_methods` so call sites can enforce its exclusive
    /// borrow of the receiver (§2.5).
    pub(crate) fn register_impl(&mut self, def: &ImplDef) -> Option<()> {
        if !self.struct_defs.contains_key(&def.type_name.name) {
            self.record_error(TypeError::UnknownStruct {
                name: def.type_name.name.clone(),
                span: def.type_name.span,
            });
            return None;
        }

        let struct_name = def.type_name.name.clone();

        // Recognize the compiler-known `Drop` lang-item (§2.1). It is matched by name
        // here exactly like `Copy`/`Clone` derives, without the general trait system.
        if def
            .trait_name
            .as_ref()
            .is_some_and(|t| t.name == DROP_TRAIT)
        {
            self.register_drop_impl(def, &struct_name);
        }

        // Accumulate (method_name, mangled_key) to insert into impl_methods after
        // all mutable borrows of `self` for type resolution are finished.
        let mut method_entries: Vec<(String, String)> = Vec::new();

        for method in &def.methods {
            // Consuming `self` still needs the by-value struct ABI, so reject it.
            // `&mut self` is supported (§2.5) and recorded below.
            if matches!(method.self_param, Some(SelfParam::Owned)) {
                self.errors.push(TypeError::UnsupportedSelfParam {
                    type_name: struct_name.clone(),
                    self_param: "self".to_string(),
                    span: method.span,
                });
                continue;
            }

            let mangled = format!("{}__{}", struct_name, method.name.name);

            if matches!(method.self_param, Some(SelfParam::RefMut)) {
                self.mut_self_methods.insert(mangled.clone());
            }

            // Build the full parameter type list: implicit `self` first for instance methods.
            let mut param_types: Vec<Type> = Vec::new();
            if method.self_param.is_some() {
                param_types.push(Type::Struct(struct_name.clone()));
            }
            for param in &method.params {
                if let Some(ty) = self.resolve_type(&param.ty) {
                    param_types.push(ty);
                } else {
                    param_types.push(Type::Unknown);
                }
            }

            let return_type = if let Some(ret_ty) = &method.return_type {
                self.resolve_type(ret_ty).unwrap_or(Type::Void)
            } else {
                Type::Void
            };

            let func_ty = Type::Function {
                params: param_types,
                ret: Box::new(return_type),
            };

            if self.functions.contains_key(&mangled) {
                self.record_error(TypeError::FunctionAlreadyDefined {
                    name: mangled.clone(),
                    span: method.name.span,
                });
                continue;
            }

            self.functions.insert(mangled.clone(), func_ty);
            method_entries.push((method.name.name.clone(), mangled));
        }

        // Insert collected entries now that all borrows of `self` are released.
        let method_map = self.impl_methods.entry(struct_name).or_default();
        for (name, mangled) in method_entries {
            method_map.insert(name, mangled);
        }

        Some(())
    }

    /// Validate and record an `impl Drop for T` block (§2.1).
    ///
    /// A Drop type must contain exactly the destructor `drop(&mut self)` — no
    /// parameters, no return — and must not also be `Copy` (a type with a
    /// destructor is moved, never duplicated, §2.3). The method itself is
    /// registered by the normal `impl` path under `T__drop`; this only enforces the
    /// lang-item shape and records `T` as a Drop type for scope-exit insertion.
    fn register_drop_impl(&mut self, def: &ImplDef, struct_name: &str) {
        if self.copy_structs.contains(struct_name) {
            self.record_error(TypeError::DropTypeCannotBeCopy {
                type_name: struct_name.to_string(),
                span: def.type_name.span,
            });
        }

        let mut reason: Option<String> = None;
        match def.methods.as_slice() {
            [method] if method.name.name == DROP_METHOD => {
                if !matches!(method.self_param, Some(SelfParam::RefMut)) {
                    reason = Some("`drop` must take `&mut self`".to_string());
                } else if !method.params.is_empty() {
                    reason =
                        Some("`drop` must take no parameters other than `&mut self`".to_string());
                } else if method.return_type.is_some() {
                    reason = Some("`drop` must not return a value".to_string());
                }
            }
            _ => {
                reason = Some(
                    "an `impl Drop` block must contain exactly one method: `drop(&mut self)`"
                        .to_string(),
                );
            }
        }

        if let Some(reason) = reason {
            self.record_error(TypeError::InvalidDropImpl {
                type_name: struct_name.to_string(),
                reason,
                span: def.span,
            });
        }
    }

    /// Register a module-level constant name and type in the constants map.
    ///
    /// Called in a pre-pass so forward references to other consts resolve correctly.
    pub(crate) fn register_const_item(&mut self, def: &ConstDef) -> Option<()> {
        if self.constants.contains_key(&def.name.name) {
            self.record_error(TypeError::ConstAlreadyDefined {
                name: def.name.name.clone(),
                span: def.name.span,
            });
            return None;
        }

        let ty = self.resolve_type(&def.ty)?;
        self.constants.insert(def.name.name.clone(), ty);
        Some(())
    }

    /// Validate a module-level constant declaration.
    pub(crate) fn check_const_item(&mut self, def: &ConstDef) -> Option<()> {
        let declared_ty = self.resolve_type(&def.ty)?;

        if !self.is_const_expr(&def.value) {
            self.record_error(TypeError::InvalidConstExpr {
                span: def.value.span(),
            });
            return None;
        }

        if let Some(expr_ty) = self.check_expr(&def.value, Some(&declared_ty)) {
            if !expr_ty.is_compatible_with(&declared_ty) {
                self.record_error(TypeError::Mismatch {
                    expected: declared_ty,
                    found: expr_ty,
                    span: def.value.span(),
                });
            }
        }

        Some(())
    }

    /// Returns true if `expr` is a valid constant expression.
    ///
    /// Valid constant expressions are: literals, arithmetic/unary on literal
    /// sub-expressions, parenthesized const expressions, and identifiers that
    /// refer to a previously declared `const`.
    pub(crate) fn is_const_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Literal(_, _) => true,
            Expr::Paren(inner, _) => self.is_const_expr(inner),
            Expr::Unary { operand, .. } => self.is_const_expr(operand),
            Expr::Binary { left, right, .. } => {
                self.is_const_expr(left) && self.is_const_expr(right)
            }
            Expr::Cast { expr: inner, .. } => self.is_const_expr(inner),
            Expr::Identifier(ident) => self.constants.contains_key(&ident.name),
            _ => false,
        }
    }

    /// Type-check the body of each method in an `impl` block.
    pub(crate) fn check_impl(&mut self, def: &ImplDef) {
        let struct_name = def.type_name.name.clone();

        for method in &def.methods {
            // Skip consuming `self` methods, which were rejected during registration.
            if matches!(method.self_param, Some(SelfParam::Owned)) {
                continue;
            }

            let mangled = format!("{}__{}", struct_name, method.name.name);

            let func_ty = match self.functions.get(&mangled).cloned() {
                Some(ty) => ty,
                None => continue,
            };

            let (param_types, return_type) = match func_ty {
                Type::Function { params, ret } => (params, *ret),
                _ => continue,
            };

            self.symbols.push_scope();
            self.current_function_return_type = Some(return_type.clone());

            // Bind `self` as a variable of the struct type. A `&mut self` receiver
            // is mutable so the body may assign to `self.field` (§2.5); `&self` is
            // immutable.
            if method.self_param.is_some() {
                let self_ty = Type::Struct(struct_name.clone());
                let self_mutable = matches!(method.self_param, Some(SelfParam::RefMut));
                let _ = self
                    .symbols
                    .define("self".to_string(), self_ty, self_mutable);
            }

            // Bind remaining parameters (skip param[0] which is the implicit self).
            let non_self_params = if method.self_param.is_some() && !param_types.is_empty() {
                &param_types[1..]
            } else {
                &param_types[..]
            };

            // `self` (`&self` or `&mut self`) and reference parameters outlive the
            // call, so a returned reference may borrow them (the receiver lifetime
            // is applied to method outputs, §2.6).
            self.current_fn_outliving = method
                .params
                .iter()
                .zip(non_self_params.iter())
                .filter(|(_, ty)| matches!(ty, Type::Reference { .. }))
                .map(|(param, _)| param.name.name.clone())
                .collect();
            if method.self_param.is_some() {
                self.current_fn_outliving.insert("self".to_string());
            }

            for (param, param_ty) in method.params.iter().zip(non_self_params.iter()) {
                if matches!(param_ty, Type::Unknown) {
                    continue;
                }
                if let Err(dup) =
                    self.symbols
                        .define(param.name.name.clone(), param_ty.clone(), false)
                {
                    self.record_error(TypeError::VariableAlreadyDefined {
                        name: dup,
                        span: param.name.span,
                    });
                }
            }

            for stmt in &method.body {
                let _ = self.check_stmt(stmt);
            }

            // Validate trailing expression return (same rule as free functions).
            if !matches!(return_type, Type::Void) && !method.body.is_empty() {
                if let Some(Stmt::Expr(expr)) = method.body.last() {
                    if let Some(expr_type) = self.check_expr(expr, Some(&return_type)) {
                        if !expr_type.is_compatible_with(&return_type) {
                            self.record_error(TypeError::ReturnTypeMismatch {
                                expected: return_type.clone(),
                                found: expr_type,
                                span: expr.span(),
                            });
                        }
                    }
                    if matches!(return_type, Type::Reference { .. }) {
                        self.check_returned_reference(expr);
                    }
                }
            }

            self.symbols.pop_scope();
            self.current_function_return_type = None;
            self.current_fn_outliving.clear();
        }
    }
}

/// Unify a (possibly generic) parameter type against a concrete argument type,
/// recording each type parameter's binding in `subst` (§3.8). Returns `false` when the
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
/// `subst` (§3.8). An unbound parameter is left as-is (the caller reports the failure).
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

/// The distinct nominal name of a monomorphized generic-struct instance (§3.8),
/// e.g. `Pair<i32, f64>`. This name is internal to the checker (it never reaches a
/// backend), so it is chosen for readable diagnostics rather than symbol safety.
fn mangle_struct_instance(base: &str, args: &[Type]) -> String {
    let parts: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    format!("{}<{}>", base, parts.join(", "))
}

/// Rewrite a monomorphized method's signature: substitute the impl's type parameters
/// and rename the receiver's `Struct(base)` to the concrete `Struct(mangled)` (§3.8).
fn remap_method_type(ty: &Type, subst: &HashMap<String, Type>, base: &str, mangled: &str) -> Type {
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
/// within a single type, recursing through references, arrays, and tuples (§3.8).
fn remap_type(ty: &Type, subst: &HashMap<String, Type>, base: &str, mangled: &str) -> Type {
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

/// Unify a template array length against an argument's (§3.8). A const-parameter length
/// binds that parameter to the argument's concrete value (recorded as a [`Type::ConstValue`]
/// in `subst`); two fixed lengths must be equal; a fixed template length against a symbolic
/// argument (only inside another template) matches structurally by name.
fn unify_array_len(param: &ArrayLen, arg: &ArrayLen, subst: &mut HashMap<String, Type>) -> bool {
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

/// Substitute a template array length using an inferred substitution (§3.8): a const
/// parameter bound to a [`Type::ConstValue`] becomes a concrete `Fixed` length; anything
/// else is left as-is.
fn substitute_array_len(size: &ArrayLen, subst: &HashMap<String, Type>) -> ArrayLen {
    match size {
        ArrayLen::Param(name) => match subst.get(name) {
            Some(Type::ConstValue(v)) => ArrayLen::Fixed(*v as usize),
            _ => size.clone(),
        },
        ArrayLen::Fixed(_) => size.clone(),
    }
}
