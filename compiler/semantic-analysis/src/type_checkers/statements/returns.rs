//! Returning a reference: the rule that a returned borrow must reach something that
//! outlives the function, not one of its own locals.

use ast_types::{Expr, Stmt};

use crate::errors::TypeError;
use crate::type_checkers::TypeChecker;

/// The trailing value expression of a block: the last statement when it is a
/// bare expression. Used to follow a returned reference into the tail of an
/// `if`/`else` arm or a bare block.
pub(super) fn tail_expr(stmts: &[Stmt]) -> Option<&Expr> {
    match stmts.last() {
        Some(Stmt::Expr(expr)) => Some(expr),
        _ => None,
    }
}

/// The base place identifier a borrow points into, peeling parentheses, field
/// access, and dereference (`&self.field` roots at `self`, `&(x)` at `x`). A
/// non-place operand (literal, call) has no root and yields `None`.
pub(super) fn root_place_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(ident) => Some(ident.name.clone()),
        Expr::Paren(inner, _) => root_place_name(inner),
        Expr::FieldAccess { object, .. } => root_place_name(object),
        Expr::Deref { operand, .. } => root_place_name(operand),
        _ => None,
    }
}

impl TypeChecker {
    /// Whether `name` is a binding local to the current function: present in the
    /// symbol table but not in the set of places that outlive the call.
    /// Function locals and by-value parameters are local; reference parameters and
    /// `self` outlive. A name absent from the table (a constant, an out-of-scope
    /// place) is conservatively treated as non-local so a valid program is never
    /// rejected.
    pub(super) fn is_local_to_function(&self, name: &str) -> bool {
        self.symbols.lookup(name).is_some() && !self.current_fn_outliving.contains(name)
    }

    /// Verify a returned reference does not borrow a function-local place.
    ///
    /// Called only when the current function's declared return type is a reference.
    /// A `&place` return dangles when `place` is local; returning an existing
    /// reference binding dangles when that binding borrows a local place. The walk
    /// follows `if`/`else` arms and bare blocks so each tail that produces the
    /// returned reference is checked. Returning a reference parameter (or one
    /// derived from `self`) is sound and passes.
    pub(crate) fn check_returned_reference(&mut self, expr: &Expr) {
        match expr {
            Expr::Paren(inner, _) => self.check_returned_reference(inner),
            Expr::Reference { operand, span, .. } => {
                if let Some(name) = root_place_name(operand)
                    && self.is_local_to_function(&name)
                {
                    self.record_error(TypeError::ReturnsReferenceToLocal { name, span: *span });
                }
            }
            Expr::Identifier(ident) => {
                if let Some(place) = self.symbols.borrow_provenance(&ident.name)
                    && self.is_local_to_function(&place)
                {
                    self.record_error(TypeError::ReturnsReferenceToLocal {
                        name: place,
                        span: ident.span,
                    });
                }
            }
            Expr::If {
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => {
                if let Some(tail) = tail_expr(then_block) {
                    self.check_returned_reference(tail);
                }
                for (_, block) in else_if_blocks {
                    if let Some(tail) = tail_expr(block) {
                        self.check_returned_reference(tail);
                    }
                }
                if let Some(block) = else_block
                    && let Some(tail) = tail_expr(block)
                {
                    self.check_returned_reference(tail);
                }
            }
            Expr::Block { stmts, .. } | Expr::Unsafe { stmts, .. } => {
                if let Some(tail) = tail_expr(stmts) {
                    self.check_returned_reference(tail);
                }
            }
            // Each arm body can produce the returned reference.
            Expr::Match { arms, .. } => {
                for arm in arms {
                    self.check_returned_reference(&arm.body);
                }
            }
            _ => {}
        }
    }
}
