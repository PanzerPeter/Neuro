//! Closure lowering: lift each closure literal to a top-level
//! [`HirItem::Closure`] and produce a [`HirExprKind::Closure`] value that names it.
//!
//! Captures are the free variables of the body that resolve to an enclosing *local*
//! binding (module constants and functions are referenced directly, so they are not
//! captured). Each is settled the way the checker settles it: a `Copy` value
//! the body only reads is copied, a `move` closure copies or moves every capture, and
//! any other capture is borrowed. A borrowed capture's environment field holds
//! `&mut name`, and every use of `name` in the body reads or writes through it, so the
//! backend sees a closure over a reference and nothing new.
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
    ///
    /// `hint` is what the literal's context fixes of its type, read the way the checker
    /// reads it: an annotation wins, a parameter without one takes the context's type,
    /// and a missing return type is the context's or else the body's.
    pub(crate) fn lower_closure(
        &mut self,
        params: &[ClosureParam],
        ret: Option<&ast_types::Type>,
        body: &Expr,
        is_move: bool,
        hint: &ClosureHint,
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let mut hir_params = Vec::with_capacity(params.len());
        for (index, p) in params.iter().enumerate() {
            let ty = match (&p.ty, hint.params.get(index).cloned().flatten()) {
                (Some(t), _) => self.resolve_type(t)?,
                (None, Some(fixed)) => fixed,
                // The checker refuses a parameter nothing types, so a missing one here is
                // a frontend inconsistency rather than a user error.
                (None, None) => {
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

        let settled = self.collect_captures(params, body, is_move);
        let ret_hint = match ret {
            Some(t) => Some(self.resolve_type(t)?),
            None => hint.ret.clone(),
        };

        // Lower the body with the captures in a scope of their own and the parameters in
        // one inside it, where the body's top-level bindings land too: a body binding
        // that shadows a capture then hides it rather than overwriting it. A block body
        // with a return type is lowered like a function body; without one, and for a
        // single expression, the body's own type is the return type.
        self.push_scope();
        let capture_scope = self.scopes.len() - 1;
        for (capture, visible) in &settled {
            self.define(capture.name.clone(), visible.clone());
            if visible != &capture.ty {
                self.borrowed_captures
                    .insert((capture_scope, capture.name.clone()));
            }
        }
        self.push_scope();
        for p in &hir_params {
            self.define(p.name.clone(), p.ty.clone());
        }
        let lowered = self.lower_closure_body(body, ret_hint);
        self.pop_scope();
        self.pop_scope();
        self.borrowed_captures
            .retain(|(scope, _)| *scope != capture_scope);
        let (body_stmts, return_type) = lowered?;
        let captures: Vec<HirCapture> = settled.into_iter().map(|(capture, _)| capture).collect();

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

    /// Lower a closure body whose captures and parameters are already bound, handing back
    /// its statements and return type.
    fn lower_closure_body(
        &mut self,
        body: &Expr,
        ret_hint: Option<HirType>,
    ) -> Result<(Vec<HirStmt>, HirType), LoweringError> {
        Ok(match body {
            Expr::Block { stmts, .. } => match ret_hint {
                Some(return_type) => {
                    let lowered = self.lower_body(stmts, &return_type)?;
                    (lowered, return_type)
                }
                None => self.lower_inferred_body(stmts)?,
            },
            single => {
                let body_hir = self.lower_expr(single, ret_hint.as_ref())?;
                let return_type = ret_hint.unwrap_or_else(|| body_hir.ty.clone());
                (vec![HirStmt::Expr(body_hir)], return_type)
            }
        })
    }

    /// Lower a block body that nothing gives a return type, and hand back its statements
    /// with the type of what it ends with: its last expression, or a trailing `if` with an
    /// `else`. A body ending in any other statement returns nothing. The checker infers
    /// the same, and refuses a `return` in such a body.
    fn lower_inferred_body(
        &mut self,
        stmts: &[Stmt],
    ) -> Result<(Vec<HirStmt>, HirType), LoweringError> {
        let saved_return = std::mem::replace(&mut self.current_return, HirType::Void);
        let lowered = self.lower_inferred_stmts(stmts);
        self.current_return = saved_return;
        lowered
    }

    fn lower_inferred_stmts(
        &mut self,
        stmts: &[Stmt],
    ) -> Result<(Vec<HirStmt>, HirType), LoweringError> {
        let mut out = Vec::with_capacity(stmts.len());
        let Some((last, init)) = stmts.split_last() else {
            return Ok((out, HirType::Void));
        };
        for stmt in init {
            self.lower_stmt_into(stmt, &mut out)?;
        }
        let tail = match last {
            Stmt::Expr(expr) => self.lower_expr(expr, None)?,
            Stmt::If {
                condition,
                then_block,
                else_if_blocks,
                else_block: else_block @ Some(_),
                span,
            } => self.lower_if_expr(
                condition,
                then_block,
                else_if_blocks,
                else_block,
                None,
                *span,
            )?,
            other => {
                self.lower_stmt_into(other, &mut out)?;
                return Ok((out, HirType::Void));
            }
        };
        let return_type = tail.ty.clone();
        out.push(HirStmt::Expr(tail));
        Ok((out, return_type))
    }

    /// Lower `arg`, which a function-typed position takes: a closure literal with what
    /// `hint` fixes of its type, anything else as it is.
    pub(crate) fn lower_function_arg(
        &mut self,
        arg: &Expr,
        hint: &ClosureHint,
    ) -> Result<HirExpr, LoweringError> {
        match peel_parens(arg) {
            Expr::Closure {
                params,
                ret,
                body,
                is_move,
                span,
            } => self.lower_closure(params, ret.as_ref(), body, *is_move, hint, *span),
            _ => self.lower_expr(arg, None),
        }
    }

    /// The value of the binding `name` of type `ty`: the binding itself, or, for a
    /// closure body's borrowed capture, a read through the reference standing for it.
    pub(crate) fn binding_value(&self, name: &str, ty: HirType, span: Span) -> HirExpr {
        let variable = |ty| HirExpr::new(HirExprKind::Variable(name.to_string()), ty, span);
        match self.capture_reference(name) {
            Some(reference) => HirExpr::new(
                HirExprKind::Deref {
                    operand: Box::new(variable(reference)),
                },
                ty,
                span,
            ),
            None => variable(ty),
        }
    }

    /// `&name` / `&mut name` of a borrowed capture is the reference the environment
    /// already holds. `None` for anything else, which lowers as an ordinary borrow.
    ///
    /// The shared borrow of a `string` is the exception that still reads through it: its
    /// representation is the fat pointer by value, not the address the capture holds.
    pub(crate) fn borrow_of_capture(
        &self,
        operand: &Expr,
        mutable: bool,
        span: Span,
    ) -> Option<HirExpr> {
        let Expr::Identifier(ident) = peel_parens(operand) else {
            return None;
        };
        let HirType::Reference { inner, .. } = self.capture_reference(&ident.name)? else {
            return None;
        };
        let ty = HirType::Reference {
            inner: inner.clone(),
            mutable,
        };
        if !mutable && *inner == HirType::String {
            let read = self.binding_value(&ident.name, *inner, span);
            return Some(HirExpr::new(
                HirExprKind::Reference {
                    operand: Box::new(read),
                    mutable,
                },
                ty,
                span,
            ));
        }
        Some(HirExpr::new(
            HirExprKind::Variable(ident.name.clone()),
            ty,
            span,
        ))
    }

    /// The reference a closure body reads `name` through, when the binding `name`
    /// resolves to is one of the body's borrowed captures.
    fn capture_reference(&self, name: &str) -> Option<HirType> {
        let (scope, ty) = self
            .scopes
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, scope)| scope.get(name).map(|ty| (index, ty)))?;
        self.borrowed_captures
            .contains(&(scope, name.to_string()))
            .then(|| HirType::Reference {
                inner: Box::new(ty.clone()),
                mutable: true,
            })
    }

    /// Whether a value of `ty` is `Copy`, mirroring the checker: duplicated rather than
    /// moved, and so copied into a closure that only reads it.
    fn is_copy(&self, ty: &HirType) -> bool {
        match ty {
            HirType::String
            | HirType::Void
            | HirType::Function { .. }
            | HirType::Collection { .. }
            | HirType::Tensor { .. }
            | HirType::DynObject(_)
            | HirType::Slice(_) => false,
            HirType::Struct(name) => self.copy_structs.contains(name),
            HirType::Enum(name) => self.enums.get(name).is_some_and(|variants| {
                variants
                    .iter()
                    .flat_map(|variant| &variant.fields)
                    .all(|(_, field)| self.is_copy(field))
            }),
            HirType::Newtype { inner, .. } => self.is_copy(inner),
            HirType::Array { element, .. } => self.is_copy(element),
            HirType::Tuple(elements) => elements.iter().all(|e| self.is_copy(e)),
            _ => true,
        }
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
    /// to an enclosing local binding. Each comes with the type the body sees it at,
    /// which differs from the capture's own exactly when the body reads it through a
    /// reference.
    ///
    /// A capture the enclosing closure itself borrowed is already a reference, so this
    /// closure copies that reference rather than borrowing it again.
    fn collect_captures(
        &self,
        params: &[ClosureParam],
        body: &Expr,
        is_move: bool,
    ) -> Vec<(HirCapture, HirType)> {
        let mut walk = FreeVars::default();
        walk.scoped(|fv| {
            for p in params {
                fv.bind(&p.name.name);
            }
            collect_expr(body, fv);
        });
        let mut captures = Vec::new();
        let mut seen = HashSet::new();
        for name in &walk.reads {
            if !seen.insert(name) {
                continue;
            }
            let Some(ty) = self.lookup_local(name) else {
                continue;
            };
            let name = name.clone();
            if let Some(reference) = self.capture_reference(&name) {
                let capture = HirCapture {
                    name,
                    ty: reference,
                    borrowed: false,
                };
                captures.push((capture, ty));
                continue;
            }
            let writes = walk.writes(&name, &self.mut_self_method_names);
            if is_move || (self.is_copy(&ty) && !writes) {
                let capture = HirCapture {
                    name,
                    ty: ty.clone(),
                    borrowed: false,
                };
                captures.push((capture, ty));
                continue;
            }
            let capture = HirCapture {
                name,
                ty: HirType::Reference {
                    inner: Box::new(ty.clone()),
                    mutable: true,
                },
                borrowed: true,
            };
            captures.push((capture, ty));
        }
        captures
    }
}

/// What a closure literal's context fixes of its type, position by position: `None` for
/// a parameter or result the context leaves open.
#[derive(Clone, Default)]
pub(crate) struct ClosureHint {
    pub(crate) params: Vec<Option<HirType>>,
    pub(crate) ret: Option<HirType>,
}

impl ClosureHint {
    /// The hint a context's expected type gives: all of it, when it is a function type.
    pub(crate) fn of(expected: Option<&HirType>) -> Self {
        match expected {
            Some(HirType::Function { params, ret }) => Self {
                params: params.iter().cloned().map(Some).collect(),
                ret: Some((**ret).clone()),
            },
            _ => Self::default(),
        }
    }
}

/// Whether `callee` is a closure literal whose parameter `index` carries no annotation.
pub(crate) fn closure_param_unannotated(callee: &Expr, index: usize) -> bool {
    matches!(peel_parens(callee), Expr::Closure { params, .. }
        if params.get(index).is_some_and(|p| p.ty.is_none()))
}

pub(crate) fn peel_parens(expr: &Expr) -> &Expr {
    match expr {
        Expr::Paren(inner, _) => peel_parens(inner),
        other => other,
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
    /// Free names the body writes: an assignment's root, the operand of `&mut`.
    writes: Vec<String>,
    /// Free names a method is called on, with the method's name.
    receivers: Vec<(String, String)>,
}

impl FreeVars {
    fn is_free(&self, name: &str) -> bool {
        !self.scopes.iter().any(|scope| scope.contains(name))
    }

    fn read(&mut self, name: &str) {
        if self.is_free(name) {
            self.reads.push(name.to_string());
        }
    }

    fn write(&mut self, name: &str) {
        if self.is_free(name) {
            self.writes.push(name.to_string());
        }
    }

    fn receive(&mut self, name: &str, method: &str) {
        if self.is_free(name) {
            self.receivers.push((name.to_string(), method.to_string()));
        }
    }

    /// Whether the body writes the capture `name`, the checker's test: an assignment
    /// rooted at it, a `&mut` of it, or a call of a `&mut self` method on it. The method
    /// is matched by name alone, which can only borrow a capture the checker copies, never
    /// the reverse.
    fn writes(&self, name: &str, mut_self_methods: &HashSet<String>) -> bool {
        self.writes.iter().any(|written| written == name)
            || self
                .receivers
                .iter()
                .any(|(root, method)| root == name && mut_self_methods.contains(method))
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
            if let Some(root) = place.root() {
                fv.write(&root.name);
            }
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

/// The binding a method receiver is a place of, as the checker roots one: through
/// parentheses, fields, elements and dereferences.
fn place_root(expr: &Expr) -> Option<&Identifier> {
    match expr {
        Expr::Identifier(ident) => Some(ident),
        Expr::Paren(inner, _) => place_root(inner),
        Expr::FieldAccess { object, .. }
        | Expr::Index { object, .. }
        | Expr::TupleIndex { object, .. } => place_root(object),
        Expr::Deref { operand, .. } => place_root(operand),
        _ => None,
    }
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
            if let Expr::FieldAccess { object, field, .. } = func.as_ref()
                && let Some(root) = place_root(object)
            {
                fv.receive(&root.name, &field.name);
            }
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
        Expr::Reference {
            operand, mutable, ..
        } => {
            if *mutable && let Expr::Identifier(ident) = peel_parens(operand) {
                fv.write(&ident.name);
            }
            collect_expr(operand, fv)
        }
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
