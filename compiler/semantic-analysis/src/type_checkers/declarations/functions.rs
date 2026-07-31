//! Function declarations: signature registration, body checking, and the
//! return-position `impl Trait` resolution its tail expression drives.
//!
//! One of the declaration-kind modules under `declarations`; each adds methods
//! to the same `impl TypeChecker` block.

use crate::errors::TypeError;
use crate::type_checkers::{GenericFnSig, TypeChecker};
use crate::types::Type;
use ast_types::{Expr, FunctionDef, Stmt};
use shared_types::Span;
use std::collections::HashMap;

impl TypeChecker {
    /// Register a function's signature without checking its body.
    ///
    /// Run over every function before any body is checked, so a call resolves
    /// regardless of source order — the same order-independence structs, enums,
    /// traits, and constants already get, and what mutual recursion requires.
    ///
    /// A generic function (non-empty `func.generics`) is a template: its type
    /// parameters are put in scope so the signature resolves with [`Type::Generic`]
    /// placeholders, and it is recorded in `generic_funcs` rather than `functions`.
    /// Concrete instantiation happens per call site.
    pub(crate) fn register_function_signature(&mut self, func: &FunctionDef) -> Option<()> {
        // Put the generic type + const parameters in scope for signature
        // resolution. A parameter may not shadow a built-in type name.
        self.enter_generic_scope(&func.generics, &func.lifetimes);

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

        // Resolve return type (default to Void if not specified). Return-position
        // `impl Trait` is static dispatch: it resolves transparently to the one
        // concrete type the body constructs, so callers see that type directly.
        let return_type = match &func.return_type {
            Some(ast_types::Type::ImplTrait { trait_name, span }) => {
                self.resolve_impl_return(&trait_name.name, &func.body, *span)
            }
            Some(ret_ty) => self.resolve_type(ret_ty).unwrap_or(Type::Void),
            None => Type::Void,
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
            // turbofish at the call — enforced per call, not here.
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
            let bounds: HashMap<String, Vec<String>> = func
                .generics
                .iter()
                .filter(|g| !g.bounds.is_empty())
                .map(|g| {
                    (
                        g.name.name.clone(),
                        g.bounds.iter().map(|b| b.name.clone()).collect(),
                    )
                })
                .collect();
            self.generic_funcs.insert(
                func.name.name.clone(),
                GenericFnSig {
                    param_names: func.generics.iter().map(|g| g.name.name.clone()).collect(),
                    const_types,
                    params: param_types.clone(),
                    ret: return_type.clone(),
                    where_predicates: func.where_predicates.clone(),
                    bounds,
                },
            );
        }

        self.exit_generic_scope();
        Some(())
    }

    /// Check a function body against the signature [`Self::register_function_signature`]
    /// already recorded for it.
    pub(crate) fn check_function(&mut self, func: &FunctionDef) -> Option<()> {
        // Re-enter the generic scope the signature was resolved in; the body needs the
        // same `Type::Generic` placeholders and const-parameter values.
        self.enter_generic_scope(&func.generics, &func.lifetimes);

        let (param_types, return_type) = match self.lookup_registered_signature(func) {
            Some(sig) => sig,
            // Signature registration failed (a duplicate definition, already reported);
            // there is nothing sound to check the body against.
            None => {
                self.exit_generic_scope();
                return None;
            }
        };

        // Enter function scope
        self.symbols.push_scope();
        self.current_function_return_type = Some(return_type.clone());

        // Reference-typed parameters outlive the call, so a returned reference may
        // safely borrow one (single-input-reference elision). Owned
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

        // Check function body. A trailing bare expression is the implicit return and
        // is checked once, below, against the declared return type — checking it here
        // as well would run its effects twice: a by-value argument would be recorded
        // as moved a second time (and then reported as a use of the value it moved
        // itself), and any diagnostic it produced would be recorded twice.
        let tail_returns =
            !matches!(return_type, Type::Void) && matches!(func.body.last(), Some(Stmt::Expr(_)));
        let leading = if tail_returns {
            &func.body[..func.body.len() - 1]
        } else {
            &func.body[..]
        };
        for stmt in leading {
            let _ = self.check_stmt(stmt);
        }

        // A trailing expression acts as an expression-based return, so it must
        // match the declared return type.
        if tail_returns {
            if let Some(Stmt::Expr(expr)) = func.body.last() {
                if let Some(expr_type) = self.check_expr(expr, Some(&return_type)) {
                    if !self.assignable(&expr_type, &return_type) {
                        self.record_error(TypeError::ReturnTypeMismatch {
                            expected: return_type.clone(),
                            found: expr_type,
                            span: expr.span(),
                        });
                    }
                }
                self.symbols.clear_transient_borrows();
                // A trailing reference expression is an implicit return; verify it
                // does not borrow a function-local place.
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

    /// The `(parameter types, return type)` recorded for `func` by the signature pass,
    /// read back from whichever table its genericity put it in.
    fn lookup_registered_signature(&self, func: &FunctionDef) -> Option<(Vec<Type>, Type)> {
        if func.generics.is_empty() {
            let Some(Type::Function { params, ret }) = self.functions.get(&func.name.name) else {
                return None;
            };
            return Some((params.clone(), (**ret).clone()));
        }
        let sig = self.generic_funcs.get(&func.name.name)?;
        Some((sig.params.clone(), sig.ret.clone()))
    }

    /// Resolve a return-position `impl Trait` to the single concrete type the
    /// body produces, and verify that type implements the named trait.
    ///
    /// `impl Trait` in return position is static dispatch: exactly one concrete type
    /// flows out of the function, so it is resolved transparently rather than kept
    /// opaque — the caller receives that concrete type at zero runtime cost. The
    /// concrete type is read structurally from the body's result expression, which this
    /// phase restricts to a direct constructor (struct literal or enum value); richer
    /// forms await closures and iterators.
    pub(super) fn resolve_impl_return(
        &mut self,
        trait_name: &str,
        body: &[Stmt],
        span: Span,
    ) -> Type {
        if !self.traits.contains_key(trait_name) {
            self.record_error(TypeError::UnknownTrait {
                trait_name: trait_name.to_string(),
                span,
            });
            return Type::Unknown;
        }
        let Some(concrete) = Self::body_result_expr(body).and_then(|e| self.shallow_result_type(e))
        else {
            self.record_error(TypeError::ImplReturnNotInferable {
                trait_name: trait_name.to_string(),
                span,
            });
            return Type::Unknown;
        };
        if !self.type_implements_trait(&concrete, trait_name) {
            self.record_error(TypeError::ImplReturnDoesNotImplement {
                trait_name: trait_name.to_string(),
                ty: concrete.clone(),
                span,
            });
        }
        concrete
    }

    /// The expression a function body evaluates to: its trailing expression, or the
    /// operand of a trailing `return`. Used only for return-position `impl Trait`
    /// resolution.
    pub(super) fn body_result_expr(body: &[Stmt]) -> Option<&Expr> {
        match body.last()? {
            Stmt::Expr(expr) => Some(expr),
            Stmt::Return { value, .. } => value.as_ref(),
            _ => None,
        }
    }

    /// The concrete type of a directly-constructed expression, read structurally without
    /// full type checking. Only the forms whose nominal type is evident from the
    /// syntax are recognized — a struct literal, an enum value, or a newtype
    /// construction — plus the tail of a block or `if`. Any other shape yields `None`,
    /// which surfaces as `ImplReturnNotInferable`.
    pub(super) fn shallow_result_type(&self, expr: &Expr) -> Option<Type> {
        match expr {
            Expr::Paren(inner, _) => self.shallow_result_type(inner),
            Expr::StructLiteral { name, .. } => self
                .struct_defs
                .contains_key(&name.name)
                .then(|| Type::Struct(name.name.clone())),
            Expr::EnumStructLiteral { enum_name, .. } => self.shallow_enum_type(&enum_name.name),
            // A unit (`E::V`) or tuple (`E::V(..)`) enum value; a same-shaped path that
            // does not name an enum is an associated-function call and is not inferable.
            Expr::Path { type_name, .. } => self.shallow_enum_type(&type_name.name),
            Expr::Call { func, .. } => match func.as_ref() {
                Expr::Path { type_name, .. } => self.shallow_enum_type(&type_name.name),
                Expr::Identifier(ident) => self
                    .newtype_defs
                    .contains_key(&ident.name)
                    .then(|| Type::Newtype(ident.name.clone())),
                _ => None,
            },
            Expr::Block { stmts, .. } => {
                Self::body_result_expr(stmts).and_then(|e| self.shallow_result_type(e))
            }
            Expr::If { then_block, .. } => {
                Self::body_result_expr(then_block).and_then(|e| self.shallow_result_type(e))
            }
            _ => None,
        }
    }

    /// The nominal enum type a construction names, for return-position `impl Trait`
    /// inference. A generic enum yields `None`: its base name is not a type, and the
    /// concrete instance depends on a payload this shallow read does not inspect.
    pub(super) fn shallow_enum_type(&self, name: &str) -> Option<Type> {
        (self.enum_defs.contains_key(name) && !self.is_generic_enum(name))
            .then(|| Type::Enum(name.to_string()))
    }
}
