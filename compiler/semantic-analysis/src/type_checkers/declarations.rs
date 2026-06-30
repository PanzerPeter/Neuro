use super::{EnumVariantInfo, TypeChecker, VariantForm};
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::{
    ConstDef, EnumDef, Expr, FunctionDef, ImplDef, SelfParam, Stmt, StructDef, VariantPayload,
};
use shared_types::Span;

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
    /// Check a function definition
    pub(crate) fn check_function(&mut self, func: &FunctionDef) -> Option<()> {
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

        // Register function signature
        let func_ty = Type::Function {
            params: param_types.clone(),
            ret: Box::new(return_type.clone()),
        };

        if self.functions.contains_key(&func.name.name) {
            self.record_error(TypeError::FunctionAlreadyDefined {
                name: func.name.name.clone(),
                span: func.name.span,
            });
            return None;
        }

        self.functions.insert(func.name.name.clone(), func_ty);

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

        Some(())
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
