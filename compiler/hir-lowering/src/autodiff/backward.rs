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
//! The derivative runs where the call ran, so it sees exactly the arguments the primal
//! would have, and the loss is computed once: `__f__rev` returns the same loss `f` does.
//! No derivative runs for a call with no `.backward()`.
//!
//! The `&mut` argument is evaluated a second time at the `.backward()`. That is sound because
//! the checker accepted this pairing only for an argument that is `&mut name` or a `&mut`
//! binding, and held its borrow from the call to here, so nothing can have moved or
//! reassigned it in between.
//!
//! The gradients come out of `__f__rev`, which allocates them in its own body, never inside
//! a `pool` block's arena, so the slot a gradient lands in always holds heap memory, even
//! when the `.backward()` runs inside a `pool`.

use ast_types::{Expr, Stmt};
use neuro_hir::{HirExpr, HirExprKind, HirStmt, HirType};
use shared_types::Span;

use crate::{Lowerer, LoweringError};

use super::{bundle_name, is_differentiated, reverse_name};

const BACKWARD_METHOD: &str = "backward";

/// The private slot write each differentiated argument's gradient moves through. The
/// checker reserves `__` in every declared name, so no program can spell it.
const SET_GRAD_METHOD: &str = "__set_grad";

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
        let HirExprKind::Variable(function) = &callee.kind else {
            return Err(malformed(format!(
                "`.backward()` on '{loss}', which was not bound to a call by name"
            )));
        };
        let params = self.grad_params.get(function).cloned().ok_or_else(|| {
            malformed(format!(
                "`.backward()` on '{loss}', the result of '{function}', which has no derivative"
            ))
        })?;
        let HirType::Function {
            params: param_tys, ..
        } = &callee.ty
        else {
            return Err(malformed(format!(
                "'{function}' is not typed as a function"
            )));
        };

        let (loss_ty, mutable, decl_span) = (loss_ty.clone(), *mutable, *decl_span);
        let args = args.clone();
        let pair = self.next_backward_binding();
        let bundle_ty = HirType::Struct(bundle_name(function));
        let pair_ty = HirType::Tuple(vec![loss_ty.clone(), bundle_ty.clone()]);
        let reverse = HirExpr::new(
            HirExprKind::Variable(reverse_name(function)),
            HirType::Function {
                params: param_tys.clone(),
                ret: Box::new(pair_ty.clone()),
            },
            callee.span,
        );
        let call = HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(reverse),
                args: args.clone(),
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
        for (arg, param) in args.into_iter().zip(&params) {
            if !is_differentiated(&arg.ty) {
                continue;
            }
            let gradient = HirExpr::new(
                HirExprKind::FieldAccess {
                    object: Box::new(bundle.clone()),
                    field: param.clone(),
                },
                arg.ty.referent().clone(),
                span,
            );
            let slot = HirExpr::new(
                HirExprKind::FieldAccess {
                    object: Box::new(arg),
                    field: SET_GRAD_METHOD.to_string(),
                },
                HirType::Void,
                span,
            );
            out.push(HirStmt::Expr(HirExpr::new(
                HirExprKind::Call {
                    callee: Box::new(slot),
                    args: vec![gradient],
                },
                HirType::Void,
                span,
            )));
        }
        Ok(())
    }

    /// A fresh name for the `(loss, gradients)` pair one `.backward()` unpacks.
    fn next_backward_binding(&mut self) -> String {
        self.backward_counter += 1;
        format!("__backward_{}", self.backward_counter)
    }
}
