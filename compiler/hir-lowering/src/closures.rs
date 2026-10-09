//! Closure lowering: lift each closure literal to a top-level
//! [`HirItem::Closure`] and produce a [`HirExprKind::Closure`] value that names it.
//!
//! Captures are the free variables of the body that resolve to an enclosing *local*
//! binding (module constants and functions are referenced directly, so they are not
//! captured). Every capture is Copy this phase, so the environment is a plain
//! by-value snapshot.

use std::collections::HashSet;

use ast_types::{ClosureParam, Expr, InterpPart, Place, Stmt, TensorIndexArg};
use neuro_hir::{
    HirCapture, HirClosure, HirExpr, HirExprKind, HirItem, HirParam, HirStmt, HirType,
};
use shared_types::{Identifier, Span};

use crate::{Lowerer, LoweringError};

/// The name of the parameter the composed closure binds its argument to. Not
/// spellable in source, so it can never collide with a name the body reads.
const COMPOSE_PARAM: &str = "__compose_arg";
/// Prefix of the parameters of the closure a function named as a value lowers to; the
/// `__` keeps them apart from every name a program can declare.
const FUNCTION_VALUE_PARAM: &str = "__fn_value_arg";

impl Lowerer {
    /// Lower a closure literal to its fat-pointer value, lifting the body to a
    /// top-level closure item collected in [`Lowerer::closure_items`].
    pub(crate) fn lower_closure(
        &mut self,
        params: &[ClosureParam],
        ret: Option<&ast_types::Type>,
        body: &Expr,
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let mut hir_params = Vec::with_capacity(params.len());
        for p in params {
            let ty = match &p.ty {
                Some(t) => self.resolve_type(t)?,
                // The checker requires an annotation, so a missing one here is a
                // frontend inconsistency rather than a user error.
                None => {
                    return Err(LoweringError::UnresolvedType {
                        name: format!("closure parameter '{}'", p.name.name),
                    });
                }
            };
            hir_params.push(HirParam {
                name: p.name.name.clone(),
                ty,
                span: p.span,
            });
        }

        let captures = self.collect_captures(params, body);
        let ret_hint = match ret {
            Some(t) => Some(self.resolve_type(t)?),
            None => None,
        };

        // Lower the body with the captures and parameters bound in a fresh scope. A
        // block body is lowered like a function body (checker-guaranteed annotation
        // supplies its return type); a single-expression body infers its type.
        self.push_scope();
        for c in &captures {
            self.define(c.name.clone(), c.ty.clone());
        }
        for p in &hir_params {
            self.define(p.name.clone(), p.ty.clone());
        }
        let (body_stmts, return_type) = match body {
            Expr::Block { stmts, .. } => {
                let return_type = ret_hint.clone().unwrap_or(HirType::Void);
                let lowered = self.lower_body(stmts, &return_type)?;
                (lowered, return_type)
            }
            single => {
                let body_hir = self.lower_expr(single, ret_hint.as_ref())?;
                let return_type = ret_hint.clone().unwrap_or_else(|| body_hir.ty.clone());
                (vec![HirStmt::Expr(body_hir)], return_type)
            }
        };
        self.pop_scope();

        let name = format!("__closure_{}", self.closure_counter);
        self.closure_counter += 1;

        self.closure_items.push(HirItem::Closure(HirClosure {
            name: name.clone(),
            captures: captures.clone(),
            params: hir_params.clone(),
            return_type: return_type.clone(),
            body: body_stmts,
            span,
        }));

        let fn_ty = HirType::Function {
            params: hir_params.iter().map(|p| p.ty.clone()).collect(),
            ret: Box::new(return_type),
        };
        Ok(HirExpr::new(
            HirExprKind::Closure { name, captures },
            fn_ty,
            span,
        ))
    }

    /// Lower `f >> g >> h` to the capture-free closure that applies each stage in turn.
    ///
    /// The chain is a function *value*, so it becomes a lifted closure item exactly as
    /// a closure literal does. Its body is the nested call the chain stands for, built
    /// as AST and lowered through the ordinary call path so composition needs no
    /// lowering rules of its own. It captures nothing: every stage is a named function,
    /// which the backend references directly.
    pub(crate) fn lower_compose(
        &mut self,
        functions: &[Identifier],
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let first = functions.first().ok_or_else(|| LoweringError::Malformed {
            detail: "a composition reached lowering with no functions".to_string(),
        })?;
        let param_ty = self
            .functions
            .get(&first.name)
            .and_then(|(params, _)| params.first())
            .cloned()
            .ok_or_else(|| LoweringError::UnresolvedType {
                name: format!("the parameter of '{}'", first.name),
            })?;

        let argument = Expr::Identifier(Identifier {
            name: COMPOSE_PARAM.to_string(),
            span,
        });
        let body = functions
            .iter()
            .fold(argument, |value, function| Expr::Call {
                func: Box::new(Expr::Identifier(function.clone())),
                type_args: Vec::new(),
                args: vec![value],
                arg_labels: Vec::new(),
                span,
            });
        self.lift_capture_free(vec![(COMPOSE_PARAM.to_string(), param_ty)], &body, span)
    }

    /// Lower a function named as a value (`val f = square`) to the capture-free closure
    /// that forwards its arguments to it, the same shape a one-stage composition has.
    pub(crate) fn lower_function_value(
        &mut self,
        function: &Identifier,
    ) -> Result<HirExpr, LoweringError> {
        let span = function.span;
        let (param_types, _) = self.functions.get(&function.name).cloned().ok_or_else(|| {
            LoweringError::UnresolvedBinding {
                name: function.name.clone(),
            }
        })?;
        let params: Vec<(String, HirType)> = param_types
            .into_iter()
            .enumerate()
            .map(|(i, ty)| (format!("{FUNCTION_VALUE_PARAM}{i}"), ty))
            .collect();
        let body = Expr::Call {
            func: Box::new(Expr::Identifier(function.clone())),
            type_args: Vec::new(),
            args: params
                .iter()
                .map(|(name, _)| {
                    Expr::Identifier(Identifier {
                        name: name.clone(),
                        span,
                    })
                })
                .collect(),
            arg_labels: Vec::new(),
            span,
        };
        self.lift_capture_free(params, &body, span)
    }

    /// Lift `body`, over `params`, into a closure item that captures nothing, and hand
    /// back the value referencing it.
    fn lift_capture_free(
        &mut self,
        params: Vec<(String, HirType)>,
        body: &Expr,
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        self.push_scope();
        for (name, ty) in &params {
            self.define(name.clone(), ty.clone());
        }
        let body = self.lower_expr(body, None);
        self.pop_scope();
        let body = body?;

        let return_type = body.ty.clone();
        let name = format!("__closure_{}", self.closure_counter);
        self.closure_counter += 1;
        let param_types: Vec<HirType> = params.iter().map(|(_, ty)| ty.clone()).collect();
        self.closure_items.push(HirItem::Closure(HirClosure {
            name: name.clone(),
            captures: Vec::new(),
            params: params
                .into_iter()
                .map(|(name, ty)| HirParam { name, ty, span })
                .collect(),
            return_type: return_type.clone(),
            body: vec![HirStmt::Expr(body)],
            span,
        }));

        Ok(HirExpr::new(
            HirExprKind::Closure {
                name,
                captures: Vec::new(),
            },
            HirType::Function {
                params: param_types,
                ret: Box::new(return_type),
            },
            span,
        ))
    }

    /// Compute the ordered, de-duplicated capture list: free variables of the body
    /// (names no scope of the closure binds at the point they are read) that resolve
    /// to an enclosing local binding, paired with that binding's type.
    fn collect_captures(&self, params: &[ClosureParam], body: &Expr) -> Vec<HirCapture> {
        let mut walk = FreeVars::default();
        walk.scoped(|fv| {
            for p in params {
                fv.bind(&p.name.name);
            }
            collect_expr(body, fv);
        });
        let mut captures = Vec::new();
        let mut seen = HashSet::new();
        for name in walk.reads {
            if seen.contains(&name) {
                continue;
            }
            if let Some(ty) = self.lookup_local(&name) {
                captures.push(HirCapture {
                    name: name.clone(),
                    ty,
                });
            }
            seen.insert(name);
        }
        captures
    }
}

/// A closure body's free-variable footprint: the identifiers it reads that no scope
/// of the body binds at the point of the read, in first-seen order. The scopes follow
/// the lowering's own, so a name the body binds hides an enclosing one only where the
/// lowered body would resolve it to the inner binding: a read before the binding, in
/// a sibling arm, or outside an inner closure binding it as a parameter is a capture.
#[derive(Default)]
struct FreeVars {
    /// Names bound inside the body, one set per open scope, innermost last.
    scopes: Vec<HashSet<String>>,
    reads: Vec<String>,
}

impl FreeVars {
    fn read(&mut self, name: &str) {
        if !self.scopes.iter().any(|scope| scope.contains(name)) {
            self.reads.push(name.to_string());
        }
    }

    fn bind(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string());
        }
    }

    /// Run `walk` inside a fresh scope, whose bindings end with it.
    fn scoped(&mut self, walk: impl FnOnce(&mut Self)) {
        self.scopes.push(HashSet::new());
        walk(self);
        self.scopes.pop();
    }
}

fn collect_stmt(stmt: &Stmt, fv: &mut FreeVars) {
    match stmt {
        Stmt::VarDecl { name, init, .. } => {
            if let Some(init) = init {
                collect_expr(init, fv);
            }
            fv.bind(&name.name);
        }
        Stmt::Assign { place, value, .. } => {
            collect_place(place, fv);
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
            index,
            iterator,
            start,
            end,
            step,
            adapters,
            body,
            ..
        } => {
            collect_expr(start, fv);
            collect_expr(end, fv);
            if let Some(step) = step {
                collect_expr(step, fv);
            }
            for adapter in adapters {
                collect_expr(&adapter.callee, fv);
            }
            collect_loop_body(index.as_ref(), iterator, body, fv);
        }
        Stmt::ForEach {
            index,
            iterator,
            iterable,
            adapters,
            body,
            ..
        } => {
            collect_expr(iterable, fv);
            for adapter in adapters {
                collect_expr(&adapter.callee, fv);
            }
            collect_loop_body(index.as_ref(), iterator, body, fv);
        }
        Stmt::Break { value, .. } => {
            if let Some(value) = value {
                collect_expr(value, fv);
            }
        }
        Stmt::Continue { .. } => {}
        Stmt::ValElse {
            pattern,
            value,
            else_binding,
            else_block,
            ..
        } => {
            collect_expr(value, fv);
            fv.scoped(|fv| {
                if let Some(binding) = else_binding {
                    fv.bind(&binding.name);
                }
                collect_block(else_block, fv);
            });
            // The pattern's bindings belong to the enclosing scope, after the `else`.
            for name in pattern.binding_names() {
                fv.bind(&name);
            }
        }
        Stmt::Const { name, value, .. } => {
            collect_expr(value, fv);
            fv.bind(&name.name);
        }
        Stmt::Expr(expr) => collect_expr(expr, fv),
    }
}

/// A block's statements, in a scope of their own.
fn collect_block(stmts: &[Stmt], fv: &mut FreeVars) {
    fv.scoped(|fv| {
        for stmt in stmts {
            collect_stmt(stmt, fv);
        }
    });
}

/// A `for` body, with the loop's position and element bindings in scope.
fn collect_loop_body(
    index: Option<&Identifier>,
    iterator: &Identifier,
    body: &[Stmt],
    fv: &mut FreeVars,
) {
    fv.scoped(|fv| {
        if let Some(index) = index {
            fv.bind(&index.name);
        }
        fv.bind(&iterator.name);
        collect_block(body, fv);
    });
}

/// Every name an assignment target reads: its root and any index on the way to it.
fn collect_place(place: &Place, fv: &mut FreeVars) {
    match place {
        Place::Var(ident) => fv.read(&ident.name),
        Place::Field { object, .. } => collect_expr(object, fv),
        Place::Index { object, index, .. } => {
            collect_expr(object, fv);
            collect_expr(index, fv);
        }
        Place::TensorIndex {
            object, indices, ..
        } => {
            collect_expr(object, fv);
            collect_tensor_indices(indices, fv);
        }
        Place::Deref { pointer, .. } => collect_expr(pointer, fv),
    }
}

fn collect_tensor_indices(indices: &[TensorIndexArg], fv: &mut FreeVars) {
    for index in indices {
        match index {
            TensorIndexArg::Position(expr) => collect_expr(expr, fv),
            TensorIndexArg::Range {
                start, end, step, ..
            } => {
                collect_expr(start, fv);
                collect_expr(end, fv);
                if let Some(step) = step {
                    collect_expr(step, fv);
                }
            }
            TensorIndexArg::FullAxis(_) => {}
        }
    }
}

fn collect_expr(expr: &Expr, fv: &mut FreeVars) {
    match expr {
        // A composition names functions, and a function is referenced directly rather
        // than captured.
        Expr::Literal(_, _) | Expr::Path { .. } | Expr::Compose { .. } => {}
        Expr::Identifier(ident) => fv.read(&ident.name),
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
        Expr::Try { operand, .. } => collect_expr(operand, fv),
        Expr::InterpString { parts, .. } => {
            for part in parts {
                if let InterpPart::Formatted { expr, .. } = part {
                    collect_expr(expr, fv);
                }
            }
        }
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
        Expr::Block { stmts, .. }
        | Expr::Unsafe { stmts, .. }
        | Expr::Pool { stmts, .. }
        | Expr::Loop { body: stmts, .. } => collect_block(stmts, fv),
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
        Expr::TensorIndex {
            object, indices, ..
        } => {
            collect_expr(object, fv);
            collect_tensor_indices(indices, fv);
        }
        Expr::TupleIndex { object, .. } => collect_expr(object, fv),
        Expr::ArrayRest { array, .. } => collect_expr(array, fv),
        Expr::Match {
            scrutinee, arms, ..
        } => {
            collect_expr(scrutinee, fv);
            for arm in arms {
                fv.scoped(|fv| {
                    for pattern in &arm.patterns {
                        for name in pattern.binding_names() {
                            fv.bind(&name);
                        }
                    }
                    if let Some(guard) = &arm.guard {
                        collect_expr(guard, fv);
                    }
                    collect_expr(&arm.body, fv);
                });
            }
        }
        Expr::Closure { params, body, .. } => fv.scoped(|fv| {
            for p in params {
                fv.bind(&p.name.name);
            }
            collect_expr(body, fv);
        }),
    }
}
