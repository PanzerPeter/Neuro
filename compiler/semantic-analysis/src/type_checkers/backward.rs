// The materialization layer's rules: `.backward()`, `.grad()` and `.zero_grad()`.
//
// `.backward()` runs the derivative of the `@grad` call its receiver came from and moves
// each gradient into the slot of the tensor that call borrowed `&mut`. That write happens
// after the call returned, so the call's `&mut` borrows do not end with it: a `val` bound
// directly to a `@grad` call holds them, exactly as a reference binding holds its borrow,
// and its `.backward()` releases them. With no `.backward()` they last as long as the
// binding, which is what the checker's lexical model gives every other held borrow. The
// loss itself stays a plain tensor; it is the arguments that stay borrowed.
//
// The pairing is also what the lowering reads to know which derivative to run, so a
// `.backward()` it cannot make is refused here rather than guessed at there: the receiver
// must be that binding, in the same block, and it runs once.

use std::collections::HashSet;

use ast_types::{Expr, Stmt};
use shared_types::Span;

use super::statements::borrow_target_of;
use super::TypeChecker;
use crate::errors::TypeError;
use crate::symbol_table::GradLoss;
use crate::types::Type;

pub(crate) const BACKWARD_METHOD: &str = "backward";
pub(crate) const GRAD_METHOD: &str = "grad";
pub(crate) const ZERO_GRAD_METHOD: &str = "zero_grad";

/// Whether a parameter of type `ty` is differentiated: a mutably borrowed tensor, which
/// the signature rules already require of every tensor parameter.
fn is_differentiated(ty: &Type) -> bool {
    matches!(ty, Type::Reference { inner, mutable: true } if matches!(**inner, Type::Tensor { .. }))
}

fn peel_parens(mut expr: &Expr) -> &Expr {
    while let Expr::Paren(inner, _) = expr {
        expr = inner;
    }
    expr
}

/// The names a `.backward()` statement is called on anywhere in `body`, found before the
/// body is checked. A `@grad` call's result holds its borrows only when a `.backward()` may
/// follow: with none there is no deferred write to protect, and evaluating a loss must not
/// freeze its parameters. The scan reaches every block a statement can open (arms, loop
/// bodies, `pool`, `unsafe`, bare blocks, `match` arms, closures); a `.backward()` hidden in
/// a block nested inside some other expression is not found, and is then refused at the
/// call rather than left unchecked.
pub(crate) fn backward_losses(body: &[Stmt]) -> HashSet<String> {
    let mut names = HashSet::new();
    scan_block(body, &mut names);
    names
}

fn scan_block(stmts: &[Stmt], names: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Expr(expr) => {
                if let Some(name) = backward_receiver(expr) {
                    names.insert(name.to_string());
                }
                scan_expr(expr, names);
            }
            Stmt::VarDecl {
                init: Some(value), ..
            }
            | Stmt::Assign { value, .. }
            | Stmt::Return {
                value: Some(value), ..
            } => scan_expr(value, names),
            Stmt::If {
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => {
                scan_block(then_block, names);
                for (_, block) in else_if_blocks {
                    scan_block(block, names);
                }
                if let Some(block) = else_block {
                    scan_block(block, names);
                }
            }
            Stmt::While { body, .. } | Stmt::ForRange { body, .. } | Stmt::ForEach { body, .. } => {
                scan_block(body, names)
            }
            Stmt::ValElse { else_block, .. } => scan_block(else_block, names),
            _ => {}
        }
    }
}

fn scan_expr(expr: &Expr, names: &mut HashSet<String>) {
    match expr {
        Expr::Paren(inner, _) => scan_expr(inner, names),
        Expr::Block { stmts, .. } | Expr::Unsafe { stmts, .. } | Expr::Pool { stmts, .. } => {
            scan_block(stmts, names)
        }
        Expr::Loop { body, .. } => scan_block(body, names),
        Expr::If {
            then_block,
            else_if_blocks,
            else_block,
            ..
        } => {
            scan_block(then_block, names);
            for (_, block) in else_if_blocks {
                scan_block(block, names);
            }
            if let Some(block) = else_block {
                scan_block(block, names);
            }
        }
        Expr::Match { arms, .. } => {
            for arm in arms {
                scan_expr(&arm.body, names);
            }
        }
        Expr::Closure { body, .. } => scan_expr(body, names),
        _ => {}
    }
}

/// The binding `expr` calls `.backward()` on, when it is that call.
fn backward_receiver(expr: &Expr) -> Option<&str> {
    let Expr::Call { func, .. } = expr else {
        return None;
    };
    let Expr::FieldAccess { object, field, .. } = func.as_ref() else {
        return None;
    };
    match peel_parens(object) {
        Expr::Identifier(ident) if field.name == BACKWARD_METHOD => Some(&ident.name),
        _ => None,
    }
}

/// The binding a `.grad()` value borrows from, when `ty` shows `expr` really is the
/// gradient view, a shared borrow of a tensor, and not a user method that happens to be
/// called `grad`. A binding initialized with it holds that borrow, so the view blocks the
/// `.zero_grad()` or `.backward()` that would release what it points at.
pub(crate) fn gradient_view_root(expr: &Expr, ty: &Type) -> Option<String> {
    let Type::Reference {
        inner,
        mutable: false,
    } = ty
    else {
        return None;
    };
    if !matches!(**inner, Type::Tensor { .. }) {
        return None;
    }
    let Expr::Call { func, .. } = peel_parens(expr) else {
        return None;
    };
    match func.as_ref() {
        Expr::FieldAccess { object, field, .. } if field.name == GRAD_METHOD => {
            TypeChecker::place_root_name(object)
        }
        _ => None,
    }
}

impl TypeChecker {
    /// Make `holder`, a `val` just bound to `init`, hold the `&mut` borrows of the `@grad`
    /// call `init` is, when it is one and a `.backward()` on `holder` follows. A method's
    /// receiver is a constant when nothing selects it with `wrt:`, so it is borrowed for
    /// the call alone, like any other argument that is not differentiated. Each
    /// differentiated argument has to be a place the borrow can be held on: `&mut name`,
    /// or a `&mut` binding passed on. Anything else leaves the loss unable to run a
    /// `.backward()`, which reports it there.
    pub(crate) fn hold_grad_call_borrows(&mut self, holder: &str, init: &Expr) {
        if !self.backward_losses.contains(holder) {
            return;
        }
        let Expr::Call { func, args, .. } = peel_parens(init) else {
            return;
        };
        let Some(params) = self.grad_call_params(func) else {
            return;
        };

        let mut state = GradLoss::Pending;
        for (position, (arg, param)) in args.iter().zip(&params).enumerate() {
            if !is_differentiated(param) {
                continue;
            }
            match borrow_target_of(arg) {
                Some((place, true)) => self.symbols.attach_borrow(holder, &place, true),
                _ => match peel_parens(arg) {
                    Expr::Identifier(binding)
                        if matches!(
                            self.symbols.lookup(&binding.name).map(|info| &info.ty),
                            Some(Type::Reference { mutable: true, .. })
                        ) =>
                    {
                        self.symbols.hold_reborrow(holder, &binding.name)
                    }
                    _ if state == GradLoss::Pending => state = GradLoss::Untracked(position),
                    _ => {}
                },
            }
        }
        self.symbols.set_grad_loss(holder, state);
    }

    /// The declared parameters of the `@grad` function or method `func` calls, the
    /// receiver excluded, or `None` when `func` names no derivative. A method is known
    /// only through its receiver's static type, so a call through a trait object is none.
    fn grad_call_params(&self, func: &Expr) -> Option<Vec<Type>> {
        match func {
            Expr::Identifier(callee) => {
                // A local of function type named like the function shadows it, and a call
                // through a function value runs no derivative.
                if !self.grad_functions.contains(&callee.name)
                    || self.symbols.lookup(&callee.name).is_some()
                {
                    return None;
                }
                match self.functions.get(&callee.name) {
                    Some(Type::Function { params, .. }) => Some(params.clone()),
                    _ => Some(self.generic_funcs.get(&callee.name)?.params.clone()),
                }
            }
            Expr::FieldAccess { .. } => {
                let key = self.callee_key(func)?;
                if !self.grad_functions.contains(&key) {
                    return None;
                }
                // A method's registered signature carries its receiver first.
                match self.functions.get(&key)? {
                    Type::Function { params, .. } => params.get(1..).map(<[Type]>::to_vec),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Check `object.backward()`, whose receiver type-checked as a tensor, and end the
    /// borrows it pairs with.
    pub(crate) fn check_backward(&mut self, object: &Expr, args: &[Expr], span: Span) -> Type {
        if !args.is_empty() {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: 0,
                found: args.len(),
                span,
            });
        }
        let Expr::Identifier(ident) = peel_parens(object) else {
            self.record_error(TypeError::BackwardUnavailable {
                problem: "needs a binding as its receiver: bind the `@grad` call's result with `val` and call `.backward()` on that binding".to_string(),
                span,
            });
            return Type::Void;
        };
        let name = &ident.name;
        // An undefined name was reported when the receiver was checked.
        let Some(info) = self.symbols.lookup(name) else {
            return Type::Void;
        };
        let same_block = self.symbols.defining_depth(name) == self.symbols.depth().checked_sub(1);
        let problem = match &info.grad_loss {
            None => format!("needs '{name}' to be a `val` bound directly to the result of a `@grad` function call"),
            Some(GradLoss::Untracked(position)) => format!(
                "cannot run for '{name}': argument {} of the `@grad` call that produced it is neither `&mut name` nor a `&mut` binding, so its borrow cannot be held until here",
                position + 1
            ),
            Some(GradLoss::Done) => format!(
                "already ran for '{name}'; the result of a `@grad` call is backpropagated once"
            ),
            Some(GradLoss::Pending) if !same_block => format!(
                "must be in the same block as the `@grad` call that produced '{name}', where the borrows it ends were taken"
            ),
            Some(GradLoss::Pending) => {
                self.symbols.finish_backward(name);
                return Type::Void;
            }
        };
        self.record_error(TypeError::BackwardUnavailable { problem, span });
        Type::Void
    }

    /// Check `object.grad()`: a shared borrow of the receiver's gradient, typed like the
    /// receiver's tensor. The borrow is of the receiver, so a live one blocks the
    /// `.zero_grad()` or `.backward()` that would release what it points at.
    pub(crate) fn check_grad_read(
        &mut self,
        recv: &Type,
        object: &Expr,
        args: &[Expr],
        span: Span,
    ) -> Type {
        if !args.is_empty() {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: 0,
                found: args.len(),
                span,
            });
        }
        self.register_slice_borrow(object, span);
        Type::Reference {
            inner: Box::new(recv.referent().clone()),
            mutable: false,
        }
    }

    /// Check `object.zero_grad()`: it releases the receiver's gradient, so it needs the
    /// receiver exclusively, the way a `&mut self` method does.
    pub(crate) fn check_zero_grad(
        &mut self,
        recv: &Type,
        object: &Expr,
        args: &[Expr],
        span: Span,
    ) -> Type {
        if !args.is_empty() {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: 0,
                found: args.len(),
                span,
            });
        }
        self.check_mut_self_receiver(object, recv, span);
        Type::Void
    }
}
