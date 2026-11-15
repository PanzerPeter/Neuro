//! Function declarations: signature registration and body checking.
//!
//! One of the declaration-kind modules under `declarations`; each adds methods
//! to the same `impl TypeChecker` block.

use crate::errors::TypeError;
use crate::type_checkers::val_else::{stmt_diverges, stmts_diverge};
use crate::type_checkers::{BoundInfo, GenericFnSig, TypeChecker};
use crate::types::{FunctionMode, Type};
use ast_types::{FunctionDef, Stmt};
use std::collections::HashMap;

/// The program's entry point, called by the C runtime.
const MAIN_FUNCTION: &str = "main";

/// The module the program was compiled from: module resolution numbers the root file
/// first, and a program that never reaches it is one module, numbered the same.
const ROOT_MODULE: ast_types::ModuleId = 0;

impl TypeChecker {
    /// The program's entry point is called by the C runtime, which passes `argc` as the
    /// first argument and reads the return register as the exit status. Only the root
    /// module's `main` is that entry point; another module's `main` reaches here under its
    /// module path and is an ordinary function.
    fn check_main_signature(&mut self, func: &FunctionDef, params: &[Type], ret: &Type) {
        if func.name.name != MAIN_FUNCTION || func.module != ROOT_MODULE {
            return;
        }
        let problem = if !func.generics.is_empty() {
            "it may not be generic".to_string()
        } else if !params.is_empty() {
            format!("it takes no parameters, found {}", params.len())
        } else if *ret != Type::I32 {
            format!("it returns the exit status as `i32`, found `{ret}`")
        } else {
            return;
        };
        self.record_error(TypeError::InvalidMainSignature {
            problem,
            span: func.name.span,
        });
    }

    /// Register a function's signature without checking its body.
    ///
    /// Run over every function before any body is checked, so a call resolves
    /// regardless of source order: the same order-independence structs, enums,
    /// traits, and constants already get, and what mutual recursion requires.
    ///
    /// A generic function (non-empty `func.generics`) is a template: its type
    /// parameters are put in scope so the signature resolves with [`Type::Generic`]
    /// placeholders, and it is recorded in `generic_funcs` rather than `functions`.
    /// Concrete instantiation happens per call site.
    ///
    /// A return-position `impl Trait` is registered as `Unknown`: its concrete type is
    /// what the body produces, which [`Self::resolve_impl_returns`] reads once every
    /// signature is known.
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

        // Resolve parameter types. A kernel's tensors are borrowed at the call, so its
        // `Tensor` and `KernelOut` parameters resolve to the references they are.
        let kernel = crate::type_checkers::kernel::kernel_attribute(&func.attributes).is_some();
        let mut param_types = Vec::new();
        for param in &func.params {
            let resolved = if kernel {
                self.resolve_kernel_param(&param.ty)
            } else {
                self.resolve_type(&param.ty)
            };
            if let Some(param_ty) = resolved {
                param_types.push(param_ty);
            } else {
                // Skip this parameter if type resolution failed
                param_types.push(Type::Unknown);
            }
        }

        // Resolve return type (default to Void if not specified).
        let return_type = match &func.return_type {
            Some(ast_types::Type::ImplTrait { .. }) => Type::Unknown,
            // A return type that failed to resolve was reported; `Unknown` keeps each call
            // site from reporting it again as a `void` it never wrote.
            Some(ret_ty) => self.resolve_type(ret_ty).unwrap_or(Type::Unknown),
            None => Type::Void,
        };

        self.check_main_signature(func, &param_types, &return_type);

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

        if kernel {
            let params = func.params.iter().map(|param| param.ty.clone()).collect();
            self.kernel_params.insert(func.name.name.clone(), params);
        }
        if func.generics.is_empty() {
            self.functions.insert(
                func.name.name.clone(),
                Type::Function {
                    params: param_types.clone(),
                    ret: Box::new(return_type.clone()),
                    mode: FunctionMode::Shared,
                },
            );
        } else {
            // A generic template is registered separately; its signature carries the
            // `Type::Generic` placeholders and is instantiated at each call site. A
            // parameter that cannot be inferred from the arguments must be supplied by a
            // turbofish at the call; enforced per call, not here.
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
            let bounds: HashMap<String, Vec<BoundInfo>> = func
                .generics
                .iter()
                .filter(|g| !g.bounds.is_empty())
                .map(|g| {
                    (
                        g.name.name.clone(),
                        self.generic_bounds
                            .get(&g.name.name)
                            .cloned()
                            .unwrap_or_default(),
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
                    fixed: HashMap::new(),
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

        // Signature registration failed (a duplicate definition, already reported);
        // there is nothing sound to check the body against.
        let Some((param_types, return_type)) = self.lookup_registered_signature(func) else {
            self.exit_generic_scope();
            return None;
        };

        let warnings_before = self.warnings.len();
        self.enter_function_body(func, &param_types, &return_type);

        // Check function body. The implicit return is checked once, below, against the
        // declared return type: checking it here as well would run its effects twice:
        // a by-value argument would be recorded as moved a second time (and then
        // reported as a use of the value it moved itself), and any diagnostic it
        // produced would be recorded twice.
        let tail_returns = Self::tail_is_implicit_return(&func.body, &return_type);
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
            self.check_implicit_return(&func.body, &return_type);
        } else if !matches!(return_type, Type::Void | Type::Unknown) && !stmts_diverge(&func.body) {
            // Nothing produces the declared value and control can reach the end of the
            // body. The backend leaves that block without a return, LLVM terminates it
            // with `unreachable` (a legal terminator, so the verifier stays silent),
            // and the program runs off the end of the function at runtime.
            self.record_error(TypeError::MissingReturn {
                expected: return_type.clone(),
                span: func.name.span,
            });
        }

        self.drop_allowed_warnings(warnings_before, &func.attributes);

        self.leave_function_body();
        self.exit_generic_scope();

        Some(())
    }

    /// Open `func`'s body scope with its parameters bound, as every body check starts.
    pub(super) fn enter_function_body(
        &mut self,
        func: &FunctionDef,
        param_types: &[Type],
        return_type: &Type,
    ) {
        self.symbols.push_scope();
        self.current_function_return_type = Some(return_type.clone());
        self.in_kernel = crate::type_checkers::kernel::kernel_attribute(&func.attributes).is_some();

        // Reference-typed parameters outlive the call, so a returned reference may
        // safely borrow one (single-input-reference elision). Owned
        // parameters and body locals do not outlive the call.
        self.backward_losses = crate::type_checkers::backward::backward_losses(&func.body);
        self.current_fn_outliving = func
            .params
            .iter()
            .zip(param_types.iter())
            .filter(|(_, ty)| matches!(ty, Type::Reference { .. }))
            .map(|(param, _)| param.name.name.clone())
            .collect();

        // Define parameters in function scope (parameters are immutable by default).
        // A parameter whose type failed to resolve is still bound, at `Unknown`: the
        // error is already reported, and leaving the name undefined turns every use of
        // it in the body into a second, misleading "undefined variable" report.
        for (param, param_ty) in func.params.iter().zip(param_types.iter()) {
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
            self.bind_function_param(&param.name.name, param_ty);
        }
        if self.in_kernel {
            self.enter_kernel_outs(func);
        }
    }

    /// Close the scope [`Self::enter_function_body`] opened.
    pub(super) fn leave_function_body(&mut self) {
        self.pop_scope();
        self.current_function_return_type = None;
        self.in_kernel = false;
        self.kernel_outs.clear();
        self.current_fn_outliving.clear();
    }

    /// Whether the last statement of a body is its implicit return.
    ///
    /// A trailing bare expression is one. So is a trailing `if`/`else`: the parser
    /// always shapes an `if` in statement position as `Stmt::If`, and HIR lowering
    /// already lowers a trailing one as the body's value, so leaving it out here meant
    /// the declared return type was never checked against it at all. An `if` whose
    /// every arm leaves the function carries no value of its own: it is a statement,
    /// and the caller's return-path check is what covers it.
    pub(super) fn tail_is_implicit_return(body: &[Stmt], return_type: &Type) -> bool {
        if matches!(return_type, Type::Void) {
            return false;
        }
        match body.last() {
            Some(Stmt::Expr(_)) => true,
            Some(
                stmt @ Stmt::If {
                    else_block: Some(_),
                    ..
                },
            ) => !stmt_diverges(stmt),
            _ => false,
        }
    }

    /// Check a body's implicit return against the declared return type. Call only
    /// when [`Self::tail_is_implicit_return`] holds, and only once per body.
    pub(super) fn check_implicit_return(&mut self, body: &[Stmt], return_type: &Type) {
        match body.last() {
            Some(Stmt::Expr(expr)) => {
                if let Some(expr_type) = self.check_expr(expr, Some(return_type))
                    && !self.assignable(&expr_type, return_type)
                {
                    self.record_error(TypeError::ReturnTypeMismatch {
                        expected: return_type.clone(),
                        found: expr_type,
                        span: expr.span(),
                    });
                }
                self.symbols.clear_transient_borrows();
                // Returning a value moves it out of the function, the same as `return`.
                self.record_move(expr);
                // A trailing reference expression is an implicit return; verify it
                // does not borrow a function-local place.
                if self.may_hold_borrow(return_type) {
                    self.check_returned_reference(expr);
                }
            }
            Some(Stmt::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                span,
            }) => {
                let ty = self
                    .check_if_expr(
                        condition,
                        then_block,
                        else_if_blocks,
                        else_block,
                        span,
                        Some(return_type),
                    )
                    .unwrap_or(Type::Unknown);
                if !self.assignable(&ty, return_type) {
                    self.record_error(TypeError::ReturnTypeMismatch {
                        expected: return_type.clone(),
                        found: ty,
                        span: *span,
                    });
                }
                self.symbols.clear_transient_borrows();
                if let Some(tail) = body.last() {
                    self.record_tail_move(std::slice::from_ref(tail), &[]);
                }
                // Each arm's tail is a returned value, checked as a trailing expression is.
                if self.may_hold_borrow(return_type) {
                    let arms = std::iter::once(then_block)
                        .chain(else_if_blocks.iter().map(|(_, block)| block))
                        .chain(else_block.iter());
                    for block in arms {
                        if let Some(tail) = crate::type_checkers::statements::tail_expr(block) {
                            self.check_returned_reference(tail);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// The `(parameter types, return type)` recorded for `func` by the signature pass,
    /// read back from whichever table its genericity put it in.
    pub(crate) fn lookup_registered_signature(
        &self,
        func: &FunctionDef,
    ) -> Option<(Vec<Type>, Type)> {
        if func.generics.is_empty() {
            let Some(Type::Function { params, ret, .. }) = self.functions.get(&func.name.name)
            else {
                return None;
            };
            return Some((params.clone(), (**ret).clone()));
        }
        let sig = self.generic_funcs.get(&func.name.name)?;
        Some((sig.params.clone(), sig.ret.clone()))
    }
}
