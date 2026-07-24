//! Closure literal type-checking and capture analysis (§3.12).
//!
//! A closure is type-checked as an anonymous callable of type
//! [`Type::Function`]. Free variables referenced in the body that resolve to an
//! enclosing local binding are *captures*. This phase captures Copy values by
//! value; a non-Copy capture or an assignment to a capture is rejected.

use std::collections::HashSet;

use ast_types::{ClosureParam, EnumPatternPayload, Expr, Pattern, Stmt};
use shared_types::Span;

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;

impl TypeChecker {
    /// Type-check a closure literal, returning its [`Type::Function`] type.
    ///
    /// Parameters require an explicit type annotation this phase (parameter-type
    /// inference is deferred). The body is checked with the parameters bound on top
    /// of the still-visible enclosing scope, so captured variables resolve normally.
    pub(crate) fn check_closure(
        &mut self,
        params: &[ClosureParam],
        ret: Option<&ast_types::Type>,
        body: &Expr,
        _is_move: bool,
        span: Span,
    ) -> Type {
        let mut param_types = Vec::with_capacity(params.len());
        for p in params {
            match &p.ty {
                Some(ty) => param_types.push(self.resolve_type(ty).unwrap_or(Type::Unknown)),
                None => {
                    self.record_error(TypeError::ClosureParamNeedsType {
                        name: p.name.name.clone(),
                        span: p.span,
                    });
                    param_types.push(Type::Unknown);
                }
            }
        }

        self.validate_captures(params, body);

        // Resolve an explicit return annotation up front so the body is checked
        // against it; without one, the body's type is the closure's return type.
        let ret_ty = ret.and_then(|t| self.resolve_type(t));

        // An early `return` inside the closure body returns from the *closure*, not the
        // enclosing function, so redirect the return-type context for the body check.
        let saved_return = self.current_function_return_type.take();
        self.current_function_return_type = ret_ty.clone();

        self.symbols.push_scope();
        for (p, ty) in params.iter().zip(param_types.iter()) {
            let _ = self.symbols.define(p.name.name.clone(), ty.clone(), false);
        }

        // A block body is checked like a function body (a trailing expression is the
        // implicit return; a trailing `return`/`if` is fine) and requires an explicit
        // return type. A single-expression body infers its return type.
        let result_ret = match body {
            Expr::Block { stmts, .. } => {
                let declared = ret_ty.clone().unwrap_or_else(|| {
                    self.record_error(TypeError::ClosureBlockNeedsReturnType { span });
                    Type::Unknown
                });
                self.check_closure_block(stmts, &declared);
                declared
            }
            single => {
                let body_ty = self
                    .check_expr(single, ret_ty.as_ref())
                    .unwrap_or(Type::Unknown);
                match ret_ty {
                    Some(declared) => {
                        if !self.assignable(&body_ty, &declared) {
                            self.record_error(TypeError::Mismatch {
                                expected: declared.clone(),
                                found: body_ty,
                                span: single.span(),
                            });
                        }
                        declared
                    }
                    None => body_ty,
                }
            }
        };

        self.symbols.pop_scope();
        self.current_function_return_type = saved_return;

        Type::Function {
            params: param_types,
            ret: Box::new(result_ret),
        }
    }

    /// Check a block-bodied closure like a function body: every statement is checked,
    /// and a trailing expression must match the declared return type. A trailing
    /// `return`/`if` statement needs no value check — its `return`s are validated
    /// against the redirected return-type context.
    fn check_closure_block(&mut self, stmts: &[Stmt], declared: &Type) {
        self.symbols.push_scope();
        if let Some((last, init)) = stmts.split_last() {
            for stmt in init {
                let _ = self.check_stmt(stmt);
            }
            match last {
                Stmt::Expr(e) if !matches!(declared, Type::Void) => {
                    if let Some(t) = self.check_expr(e, Some(declared)) {
                        if !self.assignable(&t, declared) {
                            self.record_error(TypeError::Mismatch {
                                expected: declared.clone(),
                                found: t,
                                span: e.span(),
                            });
                        }
                    }
                }
                other => {
                    let _ = self.check_stmt(other);
                }
            }
        }
        self.symbols.pop_scope();
    }

    /// Validate that every captured variable is Copy and is not assigned through.
    /// A capture is a free variable of the body that resolves to an enclosing local
    /// binding (module constants and top-level functions are referenced directly, not
    /// captured, so they are excluded here).
    fn validate_captures(&mut self, params: &[ClosureParam], body: &Expr) {
        let mut fv = FreeVars::default();
        collect_expr(body, &mut fv);
        for p in params {
            fv.bound.insert(p.name.name.clone());
        }

        let mut seen: HashSet<String> = HashSet::new();
        for (name, span) in &fv.reads {
            if fv.bound.contains(name) || !seen.insert(name.clone()) {
                continue;
            }
            if let Some(info) = self.symbols.lookup(name) {
                let ty = info.ty.clone();
                if !self.is_type_copy(&ty) {
                    self.record_error(TypeError::ClosureCapturesNonCopy {
                        name: name.clone(),
                        ty,
                        span: *span,
                    });
                }
            }
        }

        for (name, span) in &fv.assigns {
            if fv.bound.contains(name) {
                continue;
            }
            if self.symbols.lookup(name).is_some() {
                self.record_error(TypeError::ClosureAssignsCapture {
                    name: name.clone(),
                    span: *span,
                });
            }
        }
    }
}

/// The free-variable footprint of a closure body: names bound inside the body,
/// identifier reads, and assignment targets. `bound` is a flat over-approximation
/// (every locally-introduced name across all nested blocks); a read of a name in
/// `bound` is never a capture.
#[derive(Default)]
struct FreeVars {
    bound: HashSet<String>,
    reads: Vec<(String, Span)>,
    assigns: Vec<(String, Span)>,
}

fn collect_stmt(stmt: &Stmt, fv: &mut FreeVars) {
    match stmt {
        Stmt::VarDecl { name, init, .. } => {
            if let Some(init) = init {
                collect_expr(init, fv);
            }
            fv.bound.insert(name.name.clone());
        }
        Stmt::Assignment {
            target,
            value,
            span,
        } => {
            fv.assigns.push((target.name.clone(), *span));
            collect_expr(value, fv);
        }
        Stmt::Return { value, .. } => {
            if let Some(value) = value {
                collect_expr(value, fv);
            }
        }
        Stmt::If {
            condition,
            then_block,
            else_if_blocks,
            else_block,
            ..
        } => {
            collect_expr(condition, fv);
            collect_block(then_block, fv);
            for (cond, block) in else_if_blocks {
                collect_expr(cond, fv);
                collect_block(block, fv);
            }
            if let Some(block) = else_block {
                collect_block(block, fv);
            }
        }
        Stmt::While {
            condition, body, ..
        } => {
            collect_expr(condition, fv);
            collect_block(body, fv);
        }
        Stmt::ForRange {
            iterator,
            start,
            end,
            body,
            ..
        } => {
            collect_expr(start, fv);
            collect_expr(end, fv);
            fv.bound.insert(iterator.name.clone());
            collect_block(body, fv);
        }
        Stmt::ForEach {
            iterator,
            iterable,
            body,
            ..
        } => {
            collect_expr(iterable, fv);
            fv.bound.insert(iterator.name.clone());
            collect_block(body, fv);
        }
        Stmt::Loop { body, .. } => collect_block(body, fv),
        Stmt::Break { value, .. } => {
            if let Some(value) = value {
                collect_expr(value, fv);
            }
        }
        Stmt::Continue { .. } => {}
        Stmt::FieldAssignment {
            object,
            value,
            span,
            ..
        } => {
            fv.assigns.push((object.name.clone(), *span));
            collect_expr(value, fv);
        }
        Stmt::DerefAssignment { pointer, value, .. } => {
            collect_expr(pointer, fv);
            collect_expr(value, fv);
        }
        Stmt::IndexAssignment {
            target,
            index,
            value,
            span,
        } => {
            fv.assigns.push((target.name.clone(), *span));
            collect_expr(index, fv);
            collect_expr(value, fv);
        }
        Stmt::Const { name, value, .. } => {
            collect_expr(value, fv);
            fv.bound.insert(name.name.clone());
        }
        Stmt::Expr(expr) => collect_expr(expr, fv),
    }
}

fn collect_block(stmts: &[Stmt], fv: &mut FreeVars) {
    for stmt in stmts {
        collect_stmt(stmt, fv);
    }
}

fn collect_expr(expr: &Expr, fv: &mut FreeVars) {
    match expr {
        Expr::Literal(_, _) | Expr::Path { .. } => {}
        Expr::Identifier(ident) => fv.reads.push((ident.name.clone(), ident.span)),
        Expr::Binary { left, right, .. } => {
            collect_expr(left, fv);
            collect_expr(right, fv);
        }
        Expr::Call { func, args, .. } => {
            collect_expr(func, fv);
            for arg in args {
                collect_expr(arg, fv);
            }
        }
        Expr::Unary { operand, .. } => collect_expr(operand, fv),
        Expr::Paren(inner, _) => collect_expr(inner, fv),
        Expr::StructLiteral { fields, base, .. } => {
            for field in fields {
                collect_expr(&field.value, fv);
            }
            if let Some(base) = base {
                collect_expr(base, fv);
            }
        }
        Expr::FieldAccess { object, .. } => collect_expr(object, fv),
        Expr::EnumStructLiteral { fields, .. } => {
            for field in fields {
                collect_expr(&field.value, fv);
            }
        }
        Expr::Cast { expr, .. } => collect_expr(expr, fv),
        Expr::If {
            condition,
            then_block,
            else_if_blocks,
            else_block,
            ..
        } => {
            collect_expr(condition, fv);
            collect_block(then_block, fv);
            for (cond, block) in else_if_blocks {
                collect_expr(cond, fv);
                collect_block(block, fv);
            }
            if let Some(block) = else_block {
                collect_block(block, fv);
            }
        }
        Expr::Block { stmts, .. } | Expr::Unsafe { stmts, .. } => collect_block(stmts, fv),
        Expr::Loop { body, .. } => collect_block(body, fv),
        Expr::Reference { operand, .. } => collect_expr(operand, fv),
        Expr::Deref { operand, .. } => collect_expr(operand, fv),
        Expr::Range { start, end, .. } => {
            collect_expr(start, fv);
            collect_expr(end, fv);
        }
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            for el in elements {
                collect_expr(el, fv);
            }
        }
        Expr::Index { object, index, .. } => {
            collect_expr(object, fv);
            collect_expr(index, fv);
        }
        Expr::TupleIndex { object, .. } => collect_expr(object, fv),
        Expr::ArrayRest { array, .. } => collect_expr(array, fv),
        Expr::Match {
            scrutinee, arms, ..
        } => {
            collect_expr(scrutinee, fv);
            for arm in arms {
                for pattern in &arm.patterns {
                    collect_pattern_bindings(pattern, fv);
                }
                if let Some(guard) = &arm.guard {
                    collect_expr(guard, fv);
                }
                collect_expr(&arm.body, fv);
            }
        }
        // A nested closure's parameters are bound; its body may reference this
        // closure's captures, so its reads flow into the same footprint.
        Expr::Closure { params, body, .. } => {
            for p in params {
                fv.bound.insert(p.name.name.clone());
            }
            collect_expr(body, fv);
        }
    }
}

/// Record the names a match pattern binds — they are locals of the arm body,
/// never captures.
fn collect_pattern_bindings(pattern: &Pattern, fv: &mut FreeVars) {
    match pattern {
        Pattern::Wildcard(_) | Pattern::Literal(_, _) | Pattern::Range { .. } => {}
        Pattern::Binding(ident) => {
            fv.bound.insert(ident.name.clone());
        }
        Pattern::Enum { payload, .. } => match payload {
            EnumPatternPayload::Unit => {}
            EnumPatternPayload::Tuple(patterns) => {
                for p in patterns {
                    collect_pattern_bindings(p, fv);
                }
            }
            EnumPatternPayload::Struct(fields) => {
                for f in fields {
                    collect_pattern_bindings(&f.pattern, fv);
                }
            }
        },
    }
}
