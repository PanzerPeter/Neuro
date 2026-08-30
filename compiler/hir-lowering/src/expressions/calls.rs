//! Calls: free functions, methods, associated functions, operator traits, builtins.
//!
//! Reached from the `lower_expr_uncoerced` dispatch in this module's `mod.rs`.
//! Every file here adds methods to the same `impl Lowerer` block.

use ast_types::Expr;
use neuro_hir::{HirExpr, HirExprKind, HirType};

use super::{CLONE_METHOD, IO_BUILTINS, PANIC_BUILTINS};
use crate::{is_full_float, is_integer, Lowerer, LoweringError};

impl Lowerer {
    /// Lower a call, dispatching on the callee shape: free/builtin function,
    /// instance method, or associated function.
    pub(super) fn lower_call(
        &mut self,
        func: &Expr,
        type_args: &[ast_types::GenericArg],
        args: &[Expr],
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        match func {
            // Newtype construction `Name(value)` takes precedence over a
            // same-named free function in call position, matching the checker.
            Expr::Identifier(ident) if self.newtypes.contains_key(&ident.name) => {
                self.lower_newtype_construct(&ident.name, args, span)
            }
            Expr::Identifier(ident) => {
                self.lower_ident_call(&ident.name, type_args, args, expected, span)
            }
            Expr::FieldAccess { object, field, .. } => {
                self.lower_method_call(object, &field.name, args, span)
            }
            // `Vec::new()` and friends build an empty standard collection, unless the
            // program declares its own type of that name.
            Expr::Path {
                type_name, member, ..
            } if member.name == crate::collections::COLLECTION_CTOR
                && crate::collections::collection_kind(&type_name.name).is_some()
                && !self.structs.contains_key(&type_name.name)
                && !self.enums.contains_key(&type_name.name) =>
            {
                let kind =
                    crate::collections::collection_kind(&type_name.name).ok_or_else(|| {
                        LoweringError::UnresolvedType {
                            name: type_name.name.clone(),
                        }
                    })?;
                self.lower_collection_new(kind, expected, span)
            }
            // `Enum::Variant(args)` is a tuple-variant construction when the
            // type names an enum; otherwise an associated-function call.
            Expr::Path {
                type_name, member, ..
            } if self.enums.contains_key(&type_name.name)
                || self.is_generic_enum(&type_name.name) =>
            {
                self.lower_enum_tuple_call(&type_name.name, &member.name, args, expected, span)
            }
            Expr::Path {
                type_name, member, ..
            } => self.lower_assoc_call(&type_name.name, &member.name, args, span),
            other => Err(LoweringError::Malformed {
                detail: format!(
                    "call of non-callable expression {:?}",
                    std::mem::discriminant(other)
                ),
            }),
        }
    }

    /// Lower a newtype construction `Name(value)` to a transparent
    /// [`HirExprKind::NewtypeConstruct`] wrapper. The checker guarantees exactly one
    /// argument that matches the inner type.
    pub(super) fn lower_newtype_construct(
        &mut self,
        name: &str,
        args: &[Expr],
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let inner_ast = self.newtypes[name].clone();
        let inner_ty = self.resolve_type(&inner_ast)?;
        let [arg] = args else {
            return Err(LoweringError::Malformed {
                detail: format!(
                    "newtype '{}' construction expects one argument, found {}",
                    name,
                    args.len()
                ),
            });
        };
        let value = self.lower_expr(arg, Some(&inner_ty))?;
        let nt_ty = HirType::Newtype {
            name: name.to_string(),
            inner: Box::new(inner_ty),
        };
        Ok(HirExpr::new(
            HirExprKind::NewtypeConstruct {
                name: name.to_string(),
                value: Box::new(value),
            },
            nt_ty,
            span,
        ))
    }

    /// Lower a plain identifier call: a registered free function, or one of the
    /// divergent panic-family builtins (which take their context's type).
    pub(super) fn lower_ident_call(
        &mut self,
        name: &str,
        type_args: &[ast_types::GenericArg],
        args: &[Expr],
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        // A local binding of function type — a closure or a function-typed
        // parameter — is called indirectly through its fat pointer. It shadows a
        // same-named top-level function, matching the frontend's precedence.
        if let Some(HirType::Function { params, ret }) = self.lookup_local(name) {
            let args = self.lower_args(args, &params)?;
            let callee = HirExpr::new(
                HirExprKind::Variable(name.to_string()),
                HirType::Function {
                    params,
                    ret: ret.clone(),
                },
                span,
            );
            return Ok(HirExpr::new(
                HirExprKind::Call {
                    callee: Box::new(callee),
                    args,
                },
                *ret,
                span,
            ));
        }

        // A call to a generic function: infer its type arguments, queue the
        // matching monomorphized instance, and emit a call to that instance's name.
        if self.generic_templates.contains_key(name) {
            return self.lower_generic_call(name, type_args, args, span);
        }

        if let Some((params, ret)) = self.functions.get(name).cloned() {
            let args = self.lower_args(args, &params)?;
            let callee = HirExpr::new(
                HirExprKind::Variable(name.to_string()),
                HirType::Function {
                    params,
                    ret: Box::new(ret.clone()),
                },
                span,
            );
            return Ok(HirExpr::new(
                HirExprKind::Call {
                    callee: Box::new(callee),
                    args,
                },
                ret,
                span,
            ));
        }

        if PANIC_BUILTINS.contains(&name) {
            let param = match name {
                "panic" => vec![HirType::String],
                "assert" => vec![HirType::Bool],
                _ => vec![],
            };
            let args = self.lower_args(args, &param)?;
            // Divergent: adopt the expected context type, or unit in statement position.
            let ret = expected.cloned().unwrap_or(HirType::Void);
            let callee = HirExpr::new(
                HirExprKind::Variable(name.to_string()),
                HirType::Function {
                    params: param,
                    ret: Box::new(HirType::Void),
                },
                span,
            );
            return Ok(HirExpr::new(
                HirExprKind::Call {
                    callee: Box::new(callee),
                    args,
                },
                ret,
                span,
            ));
        }

        if IO_BUILTINS.contains(&name) {
            let params = vec![HirType::String];
            let args = self.lower_args(args, &params)?;
            let callee = HirExpr::new(
                HirExprKind::Variable(name.to_string()),
                HirType::Function {
                    params,
                    ret: Box::new(HirType::Void),
                },
                span,
            );
            return Ok(HirExpr::new(
                HirExprKind::Call {
                    callee: Box::new(callee),
                    args,
                },
                HirType::Void,
                span,
            ));
        }

        Err(LoweringError::UnresolvedCall {
            target: name.to_string(),
        })
    }

    /// Lower a call to a generic function to a call to its monomorphized instance
    /// The concrete type arguments are inferred by unifying the template's
    /// parameter annotations against the lowered arguments' resolved types; the
    /// instance is queued for emission and the call refers to its mangled name.
    pub(super) fn lower_generic_call(
        &mut self,
        name: &str,
        type_args: &[ast_types::GenericArg],
        args: &[Expr],
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let template = self.generic_templates[name].clone();
        let gnames: std::collections::HashSet<String> = template
            .generics
            .iter()
            .filter(|g| matches!(g.kind, ast_types::GenericParamKind::Type))
            .map(|g| g.name.name.clone())
            .collect();
        let cnames: std::collections::HashSet<String> = template
            .generics
            .iter()
            .filter(|g| matches!(g.kind, ast_types::GenericParamKind::Const(_)))
            .map(|g| g.name.name.clone())
            .collect();

        let mut subst: std::collections::HashMap<String, HirType> =
            std::collections::HashMap::new();
        let mut const_subst: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        // Seed explicit turbofish arguments before inference, positionally.
        for (gp, arg) in template.generics.iter().zip(type_args.iter()) {
            match arg {
                ast_types::GenericArg::Const { value, .. } => {
                    const_subst.insert(gp.name.name.clone(), *value as u64);
                }
                ast_types::GenericArg::Type(ty) => {
                    subst.insert(gp.name.name.clone(), self.resolve_type(ty)?);
                }
            }
        }

        // Arguments drive inference, so lower them with no expected type first.
        let mut lowered_args = Vec::with_capacity(args.len());
        for arg in args {
            lowered_args.push(self.lower_expr(arg, None)?);
        }
        for (param, larg) in template.params.iter().zip(lowered_args.iter()) {
            crate::unify_ast_hir(
                &param.ty,
                &larg.ty,
                &gnames,
                &cnames,
                &mut subst,
                &mut const_subst,
            );
        }

        // Resolve the concrete parameter and return types under the inferred bindings.
        let saved_ty = std::mem::replace(&mut self.type_subst, subst.clone());
        let saved_c = std::mem::replace(&mut self.const_subst, const_subst.clone());
        let mut param_tys = Vec::with_capacity(template.params.len());
        for param in &template.params {
            param_tys.push(self.resolve_type(&param.ty)?);
        }
        let ret = match &template.return_type {
            Some(t) => self.resolve_type(t)?,
            None => HirType::Void,
        };
        self.type_subst = saved_ty;
        self.const_subst = saved_c;

        let mangled = crate::mangle_instance(name, &template.generics, &subst, &const_subst);
        if !self.mono_seen.contains(&mangled) {
            self.mono_seen.insert(mangled.clone());
            self.mono_pending.push(crate::MonoInstance {
                mangled: mangled.clone(),
                fn_name: name.to_string(),
                subst,
                const_subst,
            });
        }

        let callee = HirExpr::new(
            HirExprKind::Variable(mangled),
            HirType::Function {
                params: param_tys,
                ret: Box::new(ret.clone()),
            },
            span,
        );
        Ok(HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(callee),
                args: lowered_args,
            },
            ret,
            span,
        ))
    }

    /// Build the method call an overloaded binary operator desugars to:
    /// `a OP b` → `a.op(b)`. When the method's right parameter is a reference
    /// (`rhs: &Rhs`, the comparison traits), the argument is borrowed.
    pub(super) fn build_operator_call(
        &mut self,
        object: HirExpr,
        rhs: HirExpr,
        dispatch: crate::OpDispatch,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let arg = if let HirType::Reference { mutable, .. } = &dispatch.rhs_param {
            let mutable = *mutable;
            let ty = HirType::Reference {
                inner: Box::new(rhs.ty.clone()),
                mutable,
            };
            HirExpr::new(
                HirExprKind::Reference {
                    operand: Box::new(rhs),
                    mutable,
                },
                ty,
                span,
            )
        } else {
            rhs
        };
        let callee = HirExpr::new(
            HirExprKind::FieldAccess {
                object: Box::new(object),
                field: dispatch.method,
            },
            dispatch.result.clone(),
            span,
        );
        Ok(HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(callee),
                args: vec![arg],
            },
            dispatch.result,
            span,
        ))
    }

    /// Build the method call an overloaded unary operator desugars to:
    /// `-a` → `a.neg()`, `~a` → `a.not()`.
    pub(super) fn build_unary_operator_call(
        &mut self,
        operand: HirExpr,
        method: String,
        result: HirType,
        span: shared_types::Span,
    ) -> HirExpr {
        let callee = HirExpr::new(
            HirExprKind::FieldAccess {
                object: Box::new(operand),
                field: method,
            },
            result.clone(),
            span,
        );
        HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(callee),
                args: Vec::new(),
            },
            result,
            span,
        )
    }

    /// Lower `instance.method(args)`: a struct method (or the `.clone()` builtin on a
    /// `Clone` struct), or a compiler-known intrinsic on a builtin receiver.
    pub(super) fn lower_method_call(
        &mut self,
        object: &Expr,
        method: &str,
        args: &[Expr],
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let object = self.lower_expr(object, None)?;
        let recv = object.ty.clone();

        let (lowered_args, result_ty) = if let HirType::Struct(struct_name) = recv.referent() {
            let struct_name = struct_name.clone();
            if let Some(mangled) = self
                .impl_methods
                .get(&struct_name)
                .and_then(|m| m.get(method))
                .cloned()
            {
                let (params, ret) = self.functions.get(&mangled).cloned().ok_or_else(|| {
                    LoweringError::UnresolvedCall {
                        target: mangled.clone(),
                    }
                })?;
                // params[0] is the implicit `self`; callers pass only the rest.
                let visible = if params.is_empty() {
                    &params[..]
                } else {
                    &params[1..]
                };
                (self.lower_args(args, visible)?, ret)
            } else if method == CLONE_METHOD && self.clone_structs.contains(&struct_name) {
                (self.lower_args(args, &[])?, HirType::Struct(struct_name))
            } else {
                return Err(LoweringError::UnresolvedCall {
                    target: format!("{}.{}", struct_name, method),
                });
            }
        } else if let HirType::DynObject(trait_name) = recv.referent() {
            // Dynamic dispatch: the call is typed from the trait's declaration —
            // no implementor is named here, since the concrete method is selected at
            // runtime through the vtable. The backend keys off the receiver's type.
            let trait_name = trait_name.clone();
            let sig = self
                .traits
                .get(&trait_name)
                .and_then(|ms| ms.iter().find(|m| m.name == method))
                .ok_or_else(|| LoweringError::UnresolvedCall {
                    target: format!("dyn {}.{}", trait_name, method),
                })?;
            let (params, ret) = (sig.params.clone(), sig.ret.clone());
            (self.lower_args(args, &params)?, ret)
        } else if matches!(recv.referent(), HirType::Collection { .. }) {
            self.lower_collection_method(&recv, method, args)?
        } else {
            self.lower_builtin_method(&recv, method, args)?
        };

        // The method-name callee is a synthetic node (the language has no first-class
        // method value); it carries the call's result type as a convenience for
        // backends that key dispatch off the field name and receiver type.
        let callee = HirExpr::new(
            HirExprKind::FieldAccess {
                object: Box::new(object),
                field: method.to_string(),
            },
            result_ty.clone(),
            span,
        );
        Ok(HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(callee),
                args: lowered_args,
            },
            result_ty,
            span,
        ))
    }

    /// Resolve a compiler-known intrinsic on a builtin (non-struct) receiver,
    /// returning the lowered arguments and the result type.
    pub(super) fn lower_builtin_method(
        &mut self,
        recv: &HirType,
        method: &str,
        args: &[Expr],
    ) -> Result<(Vec<HirExpr>, HirType), LoweringError> {
        // String intrinsics auto-deref through `&string`, so match on the referent.
        match (recv.referent(), method) {
            (HirType::String, "len") => Ok((self.lower_args(args, &[])?, HirType::U64)),
            (HirType::String, "clone") => Ok((self.lower_args(args, &[])?, HirType::String)),
            (HirType::String, "slice") => {
                let arg = args.first().ok_or_else(|| LoweringError::Malformed {
                    detail: "string.slice expects a range argument".to_string(),
                })?;
                let range = self.lower_expr(arg, None)?;
                let slice_ty = HirType::Reference {
                    inner: Box::new(HirType::String),
                    mutable: false,
                };
                Ok((vec![range], slice_ty))
            }
            (HirType::Array { .. }, "len") => Ok((self.lower_args(args, &[])?, HirType::U64)),
            // `float.is_nan()` — nullary, `bool`. A value receiver only, matching the
            // integer intrinsics below, and full-precision only: `f16`/`bf16` have no
            // scalar arithmetic contract to produce a NaN with.
            (_, "is_nan") if is_full_float(recv) => {
                Ok((self.lower_args(args, &[])?, HirType::Bool))
            }
            (
                _,
                "wrapping_add" | "wrapping_sub" | "wrapping_mul" | "saturating_add"
                | "saturating_sub" | "saturating_mul" | "shr",
            ) if is_integer(recv) => {
                let args = self.lower_args(args, std::slice::from_ref(recv))?;
                Ok((args, recv.clone()))
            }
            (_, "checked_add" | "checked_sub" | "checked_mul") if is_integer(recv) => {
                let args = self.lower_args(args, std::slice::from_ref(recv))?;
                let result = self.option_of(recv.clone())?;
                Ok((args, result))
            }
            _ => Err(LoweringError::UnresolvedCall {
                target: format!("{}.{}", recv, method),
            }),
        }
    }

    /// Lower an associated-function call `TypeName::func(args)`.
    pub(super) fn lower_assoc_call(
        &mut self,
        type_name: &str,
        member: &str,
        args: &[Expr],
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let (params, ret) = self.assoc_signature(type_name, member)?;
        let args = self.lower_args(args, &params)?;
        let callee = HirExpr::new(
            HirExprKind::Path {
                type_name: type_name.to_string(),
                member: member.to_string(),
            },
            HirType::Function {
                params,
                ret: Box::new(ret.clone()),
            },
            span,
        );
        Ok(HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(callee),
                args,
            },
            ret,
            span,
        ))
    }

    /// Lower each argument against its corresponding parameter type (the contextual
    /// hint a callee imposes on its arguments). Extra arguments lower with no hint.
    pub(crate) fn lower_args(
        &mut self,
        args: &[Expr],
        params: &[HirType],
    ) -> Result<Vec<HirExpr>, LoweringError> {
        let mut out = Vec::with_capacity(args.len());
        for (i, arg) in args.iter().enumerate() {
            out.push(self.lower_expr(arg, params.get(i))?);
        }
        Ok(out)
    }
}
