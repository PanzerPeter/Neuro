//! Closure lowering (§3.12): lift each closure literal to a top-level
//! [`HirItem::Closure`] and produce a [`HirExprKind::Closure`] value that names it.
//!
//! Captures are the free variables of the body that resolve to an enclosing *local*
//! binding (module constants and functions are referenced directly, so they are not
//! captured). Every capture is Copy this phase, so the environment is a plain
//! by-value snapshot.

use std::collections::HashSet;

use ast_types::{ClosureParam, EnumPatternPayload, Expr, Pattern, Stmt};
use neuro_hir::{
    HirCapture, HirClosure, HirExpr, HirExprKind, HirItem, HirParam, HirStmt, HirType,
};
use shared_types::Span;

use crate::{Lowerer, LoweringError};

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
                    })
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

    /// Compute the ordered, de-duplicated capture list: free variables of the body
    /// (excluding names bound inside it or by the parameters) that resolve to an
    /// enclosing local binding, paired with that binding's type.
    fn collect_captures(&self, params: &[ClosureParam], body: &Expr) -> Vec<HirCapture> {
        let mut walk = FreeVars::default();
        collect_expr(body, &mut walk);
        for p in params {
            walk.bound.insert(p.name.name.clone());
        }
        let mut captures = Vec::new();
        let mut seen = HashSet::new();
        for name in &walk.reads {
            if walk.bound.contains(name) || !seen.insert(name.clone()) {
                continue;
            }
            if let Some(ty) = self.lookup_local(name) {
                captures.push(HirCapture {
                    name: name.clone(),
                    ty,
                });
            }
        }
        captures
    }
}

/// A closure body's free-variable footprint: names bound inside the body and the
/// identifiers it reads, in first-seen order. A flat `bound` over-approximation is
/// sound for exclusion — a read of a locally-bound name is never a capture.
#[derive(Default)]
struct FreeVars {
    bound: HashSet<String>,
    reads: Vec<String>,
}

fn collect_stmt(stmt: &Stmt, fv: &mut FreeVars) {
    match stmt {
        Stmt::VarDecl { name, init, .. } => {
            if let Some(init) = init {
                collect_expr(init, fv);
            }
            fv.bound.insert(name.name.clone());
        }
        Stmt::Assignment { target, value, .. } => {
            fv.reads.push(target.name.clone());
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
        Stmt::FieldAssignment { object, value, .. } => {
            fv.reads.push(object.name.clone());
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
            ..
        } => {
            fv.reads.push(target.name.clone());
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
        Expr::Identifier(ident) => fv.reads.push(ident.name.clone()),
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
        Expr::Closure { params, body, .. } => {
            for p in params {
                fv.bound.insert(p.name.name.clone());
            }
            collect_expr(body, fv);
        }
    }
}

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
