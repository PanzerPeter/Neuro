//! `.backward()`: running a `@grad` call's derivative and parking its gradients.
//!
//! `.backward()` never reaches a backend. A block holding
//!
//! ```text
//! val loss = f(x, &mut w)
//! ...
//! loss.backward()
//! ```
//!
//! lowers to
//!
//! ```text
//! val __backward_N = __f__rev(x, &mut w)
//! val loss = __backward_N.0
//! ...
//! (&mut w).__set_grad(__backward_N.1.w)     // one per differentiated argument
//! ```
//!
//! and a method whose `wrt:` names `self.layer.w` adds `m.layer.w.__set_grad(...)` for the
//! receiver `m` the call was made on.
//!
//! A method call `m.f(x, &mut w)` becomes `m.__f__rev(x, &mut w)` the same way: the
//! derivative of a `@grad` method is a method of the same type.
//!
//! The derivative runs where the call ran, so it sees exactly the arguments the primal
//! would have, and the loss is computed once: `__f__rev` returns the same loss `f` does.
//! No derivative runs for a call with no `.backward()`.
//!
//! The `&mut` argument, or the receiver, is evaluated a second time at the `.backward()`.
//! That is sound because the checker accepted this pairing only for an argument that is
//! `&mut name` or a `&mut` binding and a receiver rooted at a binding, and held its borrow
//! from the call to here, so nothing can have moved or reassigned it in between.
//!
//! The gradients come out of `__f__rev`, which allocates them in its own body, never inside
//! a `pool` block's arena, so the slot a gradient lands in always holds heap memory, even
//! when the `.backward()` runs inside a `pool`.
//!
//! A call passing a function value to a function-typed parameter runs a derivative
//! specialized to the targets the call names, `__f__with<N>__rev`, which takes a closure's
//! captures as trailing arguments read where the call is: the closure literal itself would
//! snapshot the same values there.

use ast_types::{Expr, Stmt};
use neuro_hir::{HirExpr, HirExprKind, HirStmt, HirType};
use shared_types::Span;

use crate::{Lowerer, LoweringError};

use super::{bundle_name, field_key, field_place, reverse_name, Specialization, Target};

const BACKWARD_METHOD: &str = "backward";

/// The private slot write each differentiated argument's gradient moves through. The
/// checker reserves `__` in every declared name, so no program can spell it.
const SET_GRAD_METHOD: &str = "__set_grad";

/// Joins a `@grad` function's name to a specialization's number in the derivative's key.
const SPECIALIZATION_INFIX: &str = "__with";

/// What a function value passed to a `@grad` call is refused as when the call does not
/// name its target.
const UNNAMED_TARGET: &str = "a function value passed to a `@grad` call without naming its target; pass the function or the closure at the call, or a local bound to one that captures nothing";

/// The binding a `.backward()` statement is called on, and the statement's span.
fn backward_receiver(stmt: &Stmt) -> Option<(&str, Span)> {
    let Stmt::Expr(Expr::Call { func, span, .. }) = stmt else {
        return None;
    };
    let Expr::FieldAccess { object, field, .. } = func.as_ref() else {
        return None;
    };
    match object.as_ref() {
        Expr::Identifier(ident) if field.name == BACKWARD_METHOD => Some((&ident.name, *span)),
        _ => None,
    }
}

/// The callee of the derivative call replacing a call through `callee`: `__<key>__rev` for
/// a function, taking `extra` after its own parameters, or the receiver's `__m__rev`
/// method, which takes the receiver as `m` does.
fn reverse_callee(
    callee: &HirExpr,
    key: &str,
    extra: &[HirType],
    pair_ty: &HirType,
) -> Result<HirExpr, LoweringError> {
    let kind = match &callee.kind {
        HirExprKind::Variable(function) => {
            let HirType::Function { params, .. } = &callee.ty else {
                return Err(LoweringError::Malformed {
                    detail: format!("'{function}' is not typed as a function"),
                });
            };
            let ty = HirType::Function {
                params: params.iter().chain(extra).cloned().collect(),
                ret: Box::new(pair_ty.clone()),
            };
            return Ok(HirExpr::new(
                HirExprKind::Variable(reverse_name(key)),
                ty,
                callee.span,
            ));
        }
        // The method-name callee carries the call's result type, as every method call's does.
        HirExprKind::FieldAccess { object, field } => HirExprKind::FieldAccess {
            object: object.clone(),
            field: reverse_name(field),
        },
        _ => {
            return Err(LoweringError::Malformed {
                detail: "a `@grad` call through neither a name nor a method".to_string(),
            })
        }
    };
    Ok(HirExpr::new(kind, pair_ty.clone(), callee.span))
}

/// `target.__set_grad(bundle.field)`: move one gradient out of the bundle into the slot of
/// the tensor `target` names, owned or borrowed.
fn set_grad(target: HirExpr, bundle: &HirExpr, field: &str, span: Span) -> HirStmt {
    let gradient = HirExpr::new(
        HirExprKind::FieldAccess {
            object: Box::new(bundle.clone()),
            field: field.to_string(),
        },
        target.ty.referent().clone(),
        span,
    );
    let slot = HirExpr::new(
        HirExprKind::FieldAccess {
            object: Box::new(target),
            field: SET_GRAD_METHOD.to_string(),
        },
        HirType::Void,
        span,
    );
    HirStmt::Expr(HirExpr::new(
        HirExprKind::Call {
            callee: Box::new(slot),
            args: vec![gradient],
        },
        HirType::Void,
        span,
    ))
}

impl Lowerer {
    /// The loss a `.backward()` statement is called on, and the statement's span. A struct
    /// with a method of its own called `backward` makes an ordinary call instead.
    pub(crate) fn backward_statement<'s>(&self, stmt: &'s Stmt) -> Option<(&'s str, Span)> {
        backward_receiver(stmt).filter(|(name, _)| {
            self.lookup_local(name)
                .is_some_and(|ty| matches!(ty, HirType::Tensor { .. }))
        })
    }

    /// Lower `stmt` onto the end of its block, `out`, pairing a `.backward()` with the
    /// `@grad` call already lowered into the same block.
    pub(crate) fn lower_stmt_into(
        &mut self,
        stmt: &Stmt,
        out: &mut Vec<HirStmt>,
    ) -> Result<(), LoweringError> {
        let Some((name, span)) = self.backward_statement(stmt) else {
            out.push(self.lower_stmt(stmt)?);
            return Ok(());
        };
        self.lower_backward(name, span, out)
    }

    /// Rewrite the declaration of `loss` in `out` to call the derivative, and append one
    /// slot write per differentiated argument.
    fn lower_backward(
        &mut self,
        loss: &str,
        span: Span,
        out: &mut Vec<HirStmt>,
    ) -> Result<(), LoweringError> {
        let malformed = |detail: String| LoweringError::Malformed { detail };
        let position = out
            .iter()
            .rposition(|stmt| matches!(stmt, HirStmt::VarDecl { name, .. } if name == loss))
            .ok_or_else(|| {
                malformed(format!(
                    "`.backward()` on '{loss}', which its block did not declare"
                ))
            })?;
        let HirStmt::VarDecl {
            ty: loss_ty,
            init: Some(init),
            mutable,
            span: decl_span,
            ..
        } = &out[position]
        else {
            return Err(malformed(format!(
                "`.backward()` on '{loss}', which has no value"
            )));
        };
        let HirExprKind::Call { callee, args } = &init.kind else {
            return Err(malformed(format!(
                "`.backward()` on '{loss}', which was not bound to a call"
            )));
        };
        let key = self.grad_key(callee).ok_or_else(|| {
            malformed(format!(
                "`.backward()` on '{loss}', which was not bound to a call of a function or method by name"
            ))
        })?;
        let grad = self.grad_params.get(&key).cloned().ok_or_else(|| {
            malformed(format!(
                "`.backward()` on '{loss}', the result of '{key}', which has no derivative"
            ))
        })?;

        let (loss_ty, mutable, decl_span) = (loss_ty.clone(), *mutable, *decl_span);
        let args = args.clone();
        // A method's receiver, where the slots of the fields its `wrt:` names are.
        let receiver = match &callee.kind {
            HirExprKind::FieldAccess { object, .. } => Some((**object).clone()),
            _ => None,
        };
        let (key, captured) = if args
            .iter()
            .any(|arg| matches!(arg.ty, HirType::Function { .. }))
        {
            self.specialize(&key, callee, &args, &grad.names, out)?
        } else {
            (key, Vec::new())
        };
        let extra: Vec<HirType> = captured.iter().map(|read| read.ty.clone()).collect();
        let pair = self.next_backward_binding();
        let bundle_ty = HirType::Struct(bundle_name(&key));
        let pair_ty = HirType::Tuple(vec![loss_ty.clone(), bundle_ty.clone()]);
        let reverse = reverse_callee(callee, &key, &extra, &pair_ty)?;
        let call = HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(reverse),
                args: args.iter().cloned().chain(captured).collect(),
            },
            pair_ty.clone(),
            init.span,
        );
        let pair_var = HirExpr::new(HirExprKind::Variable(pair.clone()), pair_ty, decl_span);
        let loss_value = HirExpr::new(
            HirExprKind::TupleIndex {
                object: Box::new(pair_var.clone()),
                index: 0,
            },
            loss_ty.clone(),
            decl_span,
        );
        out.splice(
            position..=position,
            [
                HirStmt::VarDecl {
                    name: pair,
                    ty: call.ty.clone(),
                    init: Some(call),
                    mutable: false,
                    span: decl_span,
                },
                HirStmt::VarDecl {
                    name: loss.to_string(),
                    ty: loss_ty,
                    init: Some(loss_value),
                    mutable,
                    span: decl_span,
                },
            ],
        );

        let bundle = HirExpr::new(
            HirExprKind::TupleIndex {
                object: Box::new(pair_var),
                index: 1,
            },
            bundle_ty,
            span,
        );
        for (arg, param) in args.into_iter().zip(&grad.names) {
            if grad.wrt.selects(param, &arg.ty) {
                out.push(set_grad(arg, &bundle, param, span));
            }
        }
        if let Some(receiver) = receiver {
            for path in grad.wrt.fields() {
                let place = field_place(receiver.clone(), path, &self.structs, span)?;
                out.push(set_grad(place, &bundle, &field_key(path), span));
            }
        }
        Ok(())
    }

    /// The derivative a call of `function` passing function values runs, keyed by the
    /// targets it passes, and the reads of the captured bindings it takes after `args`.
    /// Recorded once per distinct set of targets; `lower_program` derives each.
    fn specialize(
        &mut self,
        function: &str,
        callee: &HirExpr,
        args: &[HirExpr],
        params: &[String],
        block: &[HirStmt],
    ) -> Result<(String, Vec<HirExpr>), LoweringError> {
        let refuse = |construct: &str, span: Span| LoweringError::NotDifferentiable {
            function: function.to_string(),
            construct: construct.to_string(),
            span,
        };
        if !matches!(callee.kind, HirExprKind::Variable(_)) {
            return Err(refuse(
                "a function value passed to a `@grad` method",
                callee.span,
            ));
        }
        let mut targets = Vec::new();
        let mut captured = Vec::new();
        for (arg, param) in args.iter().zip(params) {
            if !matches!(arg.ty, HirType::Function { .. }) {
                continue;
            }
            let target = self
                .call_site_target(arg, block)
                .ok_or_else(|| refuse(UNNAMED_TARGET, arg.span))?;
            for capture in &target.captures {
                let read = HirExprKind::Variable(capture.name.clone());
                captured.push(HirExpr::new(read, capture.ty.clone(), arg.span));
            }
            targets.push((param.clone(), target));
        }
        let known = self
            .grad_specializations
            .iter()
            .find(|known| known.function == function && known.targets == targets);
        if let Some(known) = known {
            return Ok((known.key.clone(), captured));
        }
        let key = format!(
            "{function}{SPECIALIZATION_INFIX}{}",
            self.grad_specializations.len()
        );
        self.grad_specializations.push(Specialization {
            function: function.to_string(),
            key: key.clone(),
            targets,
        });
        Ok((key, captured))
    }

    /// The target of the function value `arg` as the call site writes it: a closure
    /// literal, a function by name, or a local of `block` bound to one of those. Through a
    /// local only a target capturing nothing counts, since reading its captures at the call
    /// would read their bindings' current values, not the ones the closure took.
    fn call_site_target(&self, arg: &HirExpr, block: &[HirStmt]) -> Option<Target> {
        match &arg.kind {
            HirExprKind::Closure { name, captures } => Some(Target {
                name: name.clone(),
                captures: captures.clone(),
            }),
            HirExprKind::Variable(name) if self.lookup_local(name).is_none() => {
                self.functions.contains_key(name).then(|| Target {
                    name: name.clone(),
                    captures: Vec::new(),
                })
            }
            HirExprKind::Variable(name) => block
                .iter()
                .rev()
                .find_map(|stmt| match stmt {
                    HirStmt::VarDecl {
                        name: declared,
                        init: Some(init),
                        mutable: false,
                        ..
                    } if declared == name => Some(init),
                    _ => None,
                })
                .and_then(|init| self.call_site_target(init, block))
                .filter(|target| target.captures.is_empty()),
            _ => None,
        }
    }

    /// The key `callee` names a derivative under: a function's own name, or the
    /// `Type__method` key of a method called on a receiver of a known struct type.
    fn grad_key(&self, callee: &HirExpr) -> Option<String> {
        match &callee.kind {
            HirExprKind::Variable(function) => Some(function.clone()),
            HirExprKind::FieldAccess { object, field } => match object.ty.referent() {
                HirType::Struct(type_name) => self.impl_methods.get(type_name)?.get(field).cloned(),
                _ => None,
            },
            _ => None,
        }
    }

    /// A fresh name for the `(loss, gradients)` pair one `.backward()` unpacks.
    fn next_backward_binding(&mut self) -> String {
        self.backward_counter += 1;
        format!("__backward_{}", self.backward_counter)
    }
}
