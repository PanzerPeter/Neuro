//! Closure literal type-checking and capture analysis.
//!
//! A closure is type-checked as an anonymous callable of type
//! [`Type::Function`]. Free variables referenced in the body that resolve to an
//! enclosing local binding are *captures*. This phase captures Copy values by
//! value; a non-Copy capture or an assignment to a capture is rejected.

use std::collections::HashSet;

use ast_types::{ClosureParam, Expr, InterpPart, Place, Stmt, TensorIndexArg};
use shared_types::{Identifier, Span};

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;

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
        expected: Option<(&[Type], &Type)>,
        span: Span,
    ) -> Type {
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

        self.validate_captures(params, body);

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

        Type::Function {
            params: param_types,
            ret: Box::new(result_ret),
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

    /// Validate that every captured variable is Copy and is not assigned through.
    /// A capture is a free variable of the body that resolves to an enclosing local
    /// binding (module constants and top-level functions are referenced directly, not
    /// captured, so they are excluded here).
    fn validate_captures(&mut self, params: &[ClosureParam], body: &Expr) {
        let fv = free_vars(params, body);

        let mut seen: HashSet<String> = HashSet::new();
        for (name, span) in &fv.reads {
            if !seen.insert(name.clone()) {
                continue;
            }
            // A reference is `Copy`, so a kernel's output handle would pass the check below.
            if self.in_kernel && self.names_kernel_out(name) {
                self.refuse_kernel_out_use(name, *span);
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
            if self.symbols.lookup(name).is_some() {
                self.record_error(TypeError::ClosureAssignsCapture {
                    name: name.clone(),
                    span: *span,
                });
            }
        }
    }
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
        Expr::Reference { operand, .. } => collect_expr(operand, fv),
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

    fn captures_non_copy(body: &str) -> bool {
        let errors = semantic_errors(&format!(
            "enum E {{ A(i32), B }}
func main() -> i32 {{
    val s = \"ab\" + \"cd\"
    val o = E::B
    val g = |a: i32| -> u64 {{
        {body}
    }}
    0
}}"
        ));
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ClosureCapturesNonCopy { name, .. } if name == "s"))
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
}
