//! Closure literal type-checking and capture analysis.
//!
//! A closure is type-checked as an anonymous callable of type
//! [`Type::Function`]. Free variables referenced in the body that resolve to an
//! enclosing local binding are *captures*, each settled per variable by how the body
//! uses it: a `Copy` value it only reads is copied, a value it writes is
//! borrowed `&mut`, any other value is borrowed `&`, and a `move` closure takes every
//! capture by value. The strongest use decides the closure's call mode.
use std::collections::{HashMap, HashSet};

use ast_types::{ClosureParam, Expr, InterpPart, Place, Stmt, TensorIndexArg};
use shared_types::{Identifier, Span};

use super::TypeChecker;
use crate::errors::TypeError;
use crate::symbol_table::MoveState;
use crate::types::{FunctionMode, Type};

impl TypeChecker {
    /// Type-check a closure literal, returning its [`Type::Function`] type.
    ///
    /// Parameters require an explicit type annotation this phase (parameter-type
    /// inference is deferred), unless `expected` fixes the signature: the closure a
    /// compiler-known method takes, whose parameters and result are the method's to say.
    /// An annotation must then match the expected type. The body is checked with the
    /// parameters bound on top of the still-visible enclosing scope, so captured
    /// variables resolve normally.
    pub(crate) fn check_closure(
        &mut self,
        params: &[ClosureParam],
        ret: Option<&ast_types::Type>,
        body: &Expr,
        is_move: bool,
        expected: Option<(&[Type], &Type)>,
        span: Span,
    ) -> Type {
        let in_slot = self.function_value_slot == Some(span);
        let moves_before = self.symbols.snapshot_moves();
        let transients_before = self.symbols.transient_borrows();
        let receivers_before = self.mut_receivers.len();
        let mut param_types = Vec::with_capacity(params.len());
        for (index, p) in params.iter().enumerate() {
            let fixed = expected.and_then(|(types, _)| types.get(index));
            match (&p.ty, fixed) {
                (Some(ty), Some(fixed)) => {
                    let written = self.resolve_type(ty).unwrap_or(Type::Unknown);
                    if !matches!(written, Type::Unknown) && written != *fixed {
                        self.record_error(TypeError::Mismatch {
                            expected: fixed.clone(),
                            found: written,
                            span: p.span,
                        });
                    }
                    param_types.push(fixed.clone());
                }
                (Some(ty), None) => {
                    param_types.push(self.resolve_type(ty).unwrap_or(Type::Unknown))
                }
                (None, Some(fixed)) => param_types.push(fixed.clone()),
                (None, None) => {
                    self.record_error(TypeError::ClosureParamNeedsType {
                        name: p.name.name.clone(),
                        span: p.span,
                    });
                    param_types.push(Type::Unknown);
                }
            }
        }

        // Resolve an explicit return annotation up front so the body is checked
        // against it; without one, the body's type is the closure's return type.
        let written_ret = ret.and_then(|t| self.resolve_type(t));
        let ret_ty = match expected {
            Some((_, fixed)) => {
                if let Some(written) = written_ret
                    && written != *fixed
                {
                    self.record_error(TypeError::Mismatch {
                        expected: fixed.clone(),
                        found: written,
                        span,
                    });
                }
                Some(fixed.clone())
            }
            None => written_ret,
        };

        // An early `return` inside the closure body returns from the *closure*, not the
        // enclosing function, so redirect the return-type context for the body check.
        let saved_return = self.current_function_return_type.take();
        self.current_function_return_type = ret_ty.clone();

        // For the same reason, an enclosing loop is not in scope inside the body: a
        // `break` cannot leave the closure to reach it, and letting the enclosing
        // loops stay visible passed the program to codegen, which has no such target.
        let saved_loops = std::mem::take(&mut self.loop_stack);

        self.symbols.push_scope();
        // The parameter scope is fresh, so a failed define is a repeated parameter name.
        for (p, ty) in params.iter().zip(param_types.iter()) {
            if let Err(duplicate) = self.symbols.define(p.name.name.clone(), ty.clone(), false) {
                self.record_error(TypeError::VariableAlreadyDefined {
                    name: duplicate,
                    span: p.name.span,
                });
            }
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
        self.loop_stack = saved_loops;
        self.refuse_function_valued_return(&result_ret, span);

        // The body's own statements ended its borrows as they went, and with them any
        // the enclosing statement had in flight; what the closure borrows is added below.
        self.symbols.set_transient_borrows(&transients_before);
        let mode = self.settle_captures(
            params,
            body,
            is_move,
            CaptureEvidence {
                moves_before: &moves_before,
                receivers_before,
            },
            span,
        );
        if self.scope_bound_closures.contains_key(&span) && !in_slot {
            self.record_error(TypeError::ScopeBoundClosureStored { span });
        }

        Type::Function {
            params: param_types,
            ret: Box::new(result_ret),
            mode,
        }
    }

    /// Check a block-bodied closure like a function body: every statement is checked,
    /// and a trailing expression must match the declared return type. A trailing
    /// `return`/`if` statement needs no value check; its `return`s are validated
    /// against the redirected return-type context.
    fn check_closure_block(&mut self, stmts: &[Stmt], declared: &Type) {
        self.symbols.push_scope();
        if let Some((last, init)) = stmts.split_last() {
            for stmt in init {
                let _ = self.check_stmt(stmt);
            }
            match last {
                Stmt::Expr(e) if !matches!(declared, Type::Void) => {
                    if let Some(t) = self.check_expr(e, Some(declared))
                        && !self.assignable(&t, declared)
                    {
                        self.record_error(TypeError::Mismatch {
                            expected: declared.clone(),
                            found: t,
                            span: e.span(),
                        });
                    }
                }
                other => {
                    let _ = self.check_stmt(other);
                }
            }
        }
        self.symbols.pop_scope();
    }

    /// Decide how the closure holds each capture and return its call mode.
    ///
    /// Runs after the body is checked, because two uses only the body check sees decide a
    /// mode: a move out of a capture (its move state changed) and a `&mut self` call on
    /// one. A borrowed capture takes its borrow here, transient until the binding the
    /// literal initializes takes it over; a `move` closure moves each non-`Copy`
    /// capture out of its source. `lowering`'s capture walk settles the same way.
    fn settle_captures(
        &mut self,
        params: &[ClosureParam],
        body: &Expr,
        is_move: bool,
        evidence: CaptureEvidence<'_>,
        span: Span,
    ) -> FunctionMode {
        let fv = free_vars(params, body);
        let consumed: HashMap<String, Span> = self
            .symbols
            .moves_since(evidence.moves_before)
            .into_iter()
            .collect();
        let receivers: HashSet<String> = self.mut_receivers[evidence.receivers_before..]
            .iter()
            .cloned()
            .collect();
        let mutated = |name: &String| {
            fv.assigns
                .iter()
                .chain(&fv.mut_borrows)
                .any(|(n, _)| n == name)
                || receivers.contains(name)
        };

        let mut mode = FunctionMode::Shared;
        let mut borrows = Vec::new();
        let mut scope_bound = false;
        let mut seen: HashSet<&String> = HashSet::new();
        for (name, use_span) in fv.reads.iter().chain(&fv.assigns) {
            if !seen.insert(name) {
                continue;
            }
            // A reference is `Copy`, so a kernel's output handle would pass the check below.
            if self.in_kernel && self.names_kernel_out(name) {
                self.refuse_kernel_out_use(name, *use_span);
                continue;
            }
            let Some(ty) = self.symbols.lookup(name).map(|info| info.ty.clone()) else {
                continue;
            };
            let copy = self.is_type_copy(&ty);
            let writes = mutated(name);
            if self.in_kernel {
                self.refuse_kernel_capture(name, ty, copy, writes, *use_span);
                continue;
            }
            if writes {
                mode = mode.max(FunctionMode::Mut);
            }
            if let Some(moved_at) = consumed.get(name) {
                mode = FunctionMode::Once;
                if !is_move && !copy {
                    self.record_error(TypeError::ClosureMovesCapture {
                        name: name.clone(),
                        span: *moved_at,
                    });
                    continue;
                }
            }
            if is_move {
                if !copy {
                    scope_bound = true;
                    self.record_move(&Expr::Identifier(Identifier {
                        name: name.clone(),
                        span: *use_span,
                    }));
                }
                continue;
            }
            // A copy of a `Copy` value the body only reads is indistinguishable from a
            // borrow of it, and holds nothing the caller could trip over.
            if copy && !writes {
                continue;
            }
            scope_bound = true;
            self.borrow_capture(name, writes, *use_span);
            borrows.push((name.clone(), writes));
        }
        if scope_bound {
            self.scope_bound_closures.insert(span, borrows);
        }
        mode
    }

    /// Take the borrow a capture by reference holds, refusing one that conflicts with a
    /// borrow the enclosing statement has in flight (`f(&mut v, || v.len())`). A
    /// conflict with a borrow a binding holds was already reported where the body used
    /// the capture.
    fn borrow_capture(&mut self, name: &str, exclusive: bool, span: Span) {
        let (Some((shared, held_exclusive)), Some((held_shared, persistent_exclusive))) = (
            self.symbols.borrow_counts(name),
            self.symbols.persistent_borrow_counts(name),
        ) else {
            return;
        };
        let in_flight_shared = shared - held_shared;
        let in_flight_exclusive = held_exclusive - persistent_exclusive;
        if exclusive && in_flight_shared + in_flight_exclusive > 0 {
            self.record_error(TypeError::CannotMutablyBorrowWhileBorrowed {
                name: name.to_string(),
                span,
            });
        } else if !exclusive && in_flight_exclusive > 0 {
            self.record_error(TypeError::CannotBorrowWhileMutablyBorrowed {
                name: name.to_string(),
                span,
            });
        }
        self.symbols.add_transient_borrow(name, exclusive);
    }

    /// A kernel body lowers through MLIR, which takes a closure's captures by copy only.
    fn refuse_kernel_capture(
        &mut self,
        name: &str,
        ty: Type,
        copy: bool,
        writes: bool,
        span: Span,
    ) {
        if !copy {
            self.record_error(TypeError::ClosureCapturesNonCopy {
                name: name.to_string(),
                ty,
                span,
            });
        } else if writes {
            self.record_error(TypeError::ClosureAssignsCapture {
                name: name.to_string(),
                span,
            });
        }
    }
}

/// What the body check left behind that capture settlement reads: the move states before
/// it, and where the receiver log stood.
struct CaptureEvidence<'a> {
    moves_before: &'a [MoveState],
    receivers_before: usize,
}

/// The names a closure reads from its enclosing scope, each once: its captures, plus any
/// module-level name the body reads, which the caller tells apart by looking it up.
pub(crate) fn closure_reads(params: &[ClosureParam], body: &Expr) -> HashSet<String> {
    free_vars(params, body)
        .reads
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

/// The free-variable footprint of a closure with `params` and `body`.
fn free_vars(params: &[ClosureParam], body: &Expr) -> FreeVars {
    let mut fv = FreeVars::default();
    fv.scoped(|fv| {
        for p in params {
            fv.bind(&p.name.name);
        }
        collect_expr(body, fv);
    });
    fv
}

/// The free-variable footprint of a closure body: the identifier reads and assignment
/// roots that no scope of the body binds at that point. The scopes are the ones
/// `hir-lowering` walks to build the capture list, so the two agree on what is captured:
/// a name the body binds hides an enclosing one only after the binding and inside its
/// scope. A read before it, in a sibling arm, or outside an inner closure that binds it
/// as a parameter reaches the enclosing binding.
#[derive(Default)]
struct FreeVars {
    /// Names bound inside the body, one set per open scope, innermost last.
    scopes: Vec<HashSet<String>>,
    reads: Vec<(String, Span)>,
    assigns: Vec<(String, Span)>,
    /// Bindings the body borrows `&mut` by name.
    mut_borrows: Vec<(String, Span)>,
}

impl FreeVars {
    fn is_bound(&self, name: &str) -> bool {
        self.scopes.iter().any(|scope| scope.contains(name))
    }

    fn read(&mut self, name: &str, span: Span) {
        if !self.is_bound(name) {
            self.reads.push((name.to_string(), span));
        }
    }

    fn assign(&mut self, name: &str, span: Span) {
        if !self.is_bound(name) {
            self.assigns.push((name.to_string(), span));
        }
    }

    fn mut_borrow(&mut self, name: &str, span: Span) {
        if !self.is_bound(name) {
            self.mut_borrows.push((name.to_string(), span));
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
        Stmt::Assign {
            place, value, span, ..
        } => {
            if let Some(root) = place.root() {
                fv.assign(&root.name, *span);
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

fn collect_expr(expr: &Expr, fv: &mut FreeVars) {
    match expr {
        // A composition names functions, and a function is not a capturable binding.
        Expr::Literal(_, _) | Expr::Path { .. } | Expr::Compose { .. } => {}
        Expr::Identifier(ident) => fv.read(&ident.name, ident.span),
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
            if *mutable && let Some(ident) = bare_identifier(operand) {
                fv.mut_borrow(&ident.name, ident.span);
            }
            collect_expr(operand, fv)
        }
        Expr::Deref { operand, .. } => collect_expr(operand, fv),
        Expr::Try { operand, .. } => collect_expr(operand, fv),
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
        // A nested closure's parameters are bound in its own scope; its body may
        // reference this closure's captures, so its reads flow into the same footprint.
        Expr::Closure { params, body, .. } => fv.scoped(|fv| {
            for p in params {
                fv.bind(&p.name.name);
            }
            collect_expr(body, fv);
        }),
    }
}

/// The binding `expr` names, through any parentheses.
fn bare_identifier(expr: &Expr) -> Option<&Identifier> {
    match expr {
        Expr::Identifier(ident) => Some(ident),
        Expr::Paren(inner, _) => bare_identifier(inner),
        _ => None,
    }
}

/// Record the free variables the expressions inside an assignment place reach. The
/// root binding is recorded as an assignment by the caller, not as a read.
fn collect_place(place: &Place, fv: &mut FreeVars) {
    match place {
        Place::Var(_) => {}
        Place::Field { object, .. }
        | Place::Deref {
            pointer: object, ..
        } => collect_expr(object, fv),
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

#[cfg(test)]
mod tests {
    use super::super::tests::semantic_errors;
    use crate::errors::TypeError;

    /// Whether the closure borrows the outer `s`, which shows as the move after it being
    /// refused while the closure lives.
    fn captures_non_copy(body: &str) -> bool {
        let errors = semantic_errors(&format!(
            "enum E {{ A(i32), B }}
func main() -> i32 {{
    val s = \"ab\" + \"cd\"
    val o = E::B
    val g = |a: i32| -> u64 {{
        {body}
    }}
    val t = s
    0
}}"
        ));
        errors
            .iter()
            .any(|e| matches!(e, TypeError::CannotMoveWhileBorrowed { name, .. } if name == "s"))
    }

    /// A name the body binds hides an enclosing one only after the binding and inside its
    /// scope, which is where lowering resolves it to the inner binding too. A read
    /// anywhere else captures the enclosing `string`.
    #[test]
    fn a_read_outside_the_inner_binding_is_a_capture() {
        for body in [
            // Read before the shadowing declaration.
            "val y = s.len()\n val s = 1\n y",
            // A sibling arm binds the name.
            "match o {\n E::A(s) => 0,\n E::B => s.len()\n }",
            // An inner closure's parameter binds it.
            "val h = |s: i32| -> i32 { s }\n s.len()",
            // Read only in an assignment target's index.
            "mut buf = [0, 0]\n buf[s.len()] = a\n val s = 1\n 0",
            // A block's binding ends with the block.
            "{\n val s = 1\n }\n s.len()",
        ] {
            assert!(captures_non_copy(body), "{body}: expected a capture of 's'");
        }
    }

    #[test]
    fn a_read_of_the_inner_binding_is_not_a_capture() {
        for body in [
            "val s = 1\n s as u64",
            "match o {\n E::A(s) => s as u64,\n E::B => 0\n }",
            "val h = |s: i32| -> i32 { s }\n h(1) as u64",
        ] {
            assert!(!captures_non_copy(body), "{body}: 's' is the inner binding");
        }
    }

    /// A closure that writes a capture is `mut`, so a slot asking for a plain function
    /// refuses it, and one asking for `mut` or `once` takes it.
    #[test]
    fn a_written_capture_makes_the_closure_mut() {
        let refused = semantic_errors(
            "func want(f: () -> void) { f() }
func main() -> i32 {
    mut c = 0
    want(|| -> void { c += 1 })
    0
}",
        );
        assert!(
            refused
                .iter()
                .any(|e| matches!(e, TypeError::Mismatch { .. })),
            "{refused:?}"
        );
        let accepted = semantic_errors(
            "func want(f: mut () -> void) { f() }
func last(f: once () -> void) { f() }
func main() -> i32 {
    mut c = 0
    want(|| -> void { c += 1 })
    last(|| -> void { c += 1 })
    c
}",
        );
        assert!(accepted.is_empty(), "{accepted:?}");
    }

    /// A `Copy` value the body only reads is copied, so the binding stays free to change.
    #[test]
    fn a_read_only_copy_capture_holds_no_borrow() {
        let errors = semantic_errors(
            "func main() -> i32 {
    mut x = 1
    val f = |y: i32| -> i32 { x + y }
    x = 2
    f(x)
}",
        );
        assert!(errors.is_empty(), "{errors:?}");
    }

    /// A `&mut self` call on a capture writes it: the closure borrows it exclusively.
    #[test]
    fn a_mutating_method_on_a_capture_borrows_it_exclusively() {
        let errors = semantic_errors(
            "func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    val push = |x: i32| -> void { v.push(x) }
    push(1)
    v.len() as i32
}",
        );
        assert!(
            errors.iter().any(
                |e| matches!(e, TypeError::CannotUseWhileMutablyBorrowed { name, .. } if name == "v")
            ),
            "{errors:?}"
        );
    }

    /// A `move` closure takes its non-`Copy` captures, and one that gives a capture away
    /// is `once`: its binding is used up by the first call.
    #[test]
    fn a_move_closure_owns_its_captures_and_a_consuming_one_is_called_once() {
        let errors = semantic_errors(
            "func consume(s: string) -> u64 { s.len() }
func main() -> i32 {
    val s = \"a\" + \"b\"
    val f = move || -> u64 { consume(s) }
    val n = s.len()
    val a = f()
    (a + f()) as i32
}",
        );
        let moved: Vec<&str> = errors
            .iter()
            .filter_map(|e| match e {
                TypeError::UseOfMovedValue { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(moved, ["s", "f"], "{errors:?}");
    }

    #[test]
    fn a_scope_bound_closure_cannot_be_stored() {
        for stored in [
            "mut g = |x: i32| -> u64 { 0 }\n    g = |x: i32| -> u64 { s.len() }",
            "val f = |x: i32| -> u64 { s.len() }\n    val pair = (f, 1)",
            "val g = {\n        |x: i32| -> u64 { s.len() }\n    }",
        ] {
            let errors = semantic_errors(&format!(
                "func main() -> i32 {{
    val s = \"a\" + \"b\"
    {stored}
    0
}}"
            ));
            assert!(
                errors
                    .iter()
                    .any(|e| matches!(e, TypeError::ScopeBoundClosureStored { .. })),
                "{stored}: {errors:?}"
            );
        }
    }
}
