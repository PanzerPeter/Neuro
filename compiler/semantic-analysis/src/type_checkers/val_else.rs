// Type checking for `val PATTERN = value else |binding| { ... }`.
//
// Two rules make this more than a one-armed `match`: the pattern's bindings land in
// the ENCLOSING scope (so the rest of the block sees them), and the `else` branch must
// leave that scope — a branch that can fall through would reach code whose bindings
// were never initialized.

use ast_types::{Expr, Pattern, Stmt};
use shared_types::{Identifier, Span};

use super::collections::OPTION_ENUM;
use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;

/// The fallible enum whose `else |name|` binds a payload rather than the scrutinee.
const RESULT_ENUM: &str = "Result";
/// The `Result` variant an `else` branch is reached through.
const RESULT_FAILURE_VARIANT: &str = "Err";
/// The names a builtin call must have to end the branch on its own.
const DIVERGING_BUILTINS: &[&str] = &["panic", "unreachable"];

impl TypeChecker {
    /// Check a `val-else` statement.
    ///
    /// The scrutinee is checked first so the pattern and the `else` binding both
    /// resolve against a concrete type; the `else` branch is then checked in its own
    /// scope, and only afterwards are the pattern's bindings introduced — the branch
    /// must not see the bindings its failure means were never produced.
    pub(crate) fn check_val_else(
        &mut self,
        pattern: &Pattern,
        value: &Expr,
        else_binding: Option<&Identifier>,
        else_block: &[Stmt],
        span: Span,
    ) -> Option<()> {
        let scrut_ty = self.check_expr(value, None).unwrap_or(Type::Unknown);

        let mut bindings: Vec<(String, Type, Span)> = Vec::new();
        self.check_pattern(pattern, &scrut_ty, &mut bindings);

        let else_binding = else_binding.filter(|ident| ident.name != "_");
        let else_binding_ty = match else_binding {
            Some(ident) => self.else_binding_type(ident, &scrut_ty),
            None => None,
        };

        self.symbols.push_scope();
        if let (Some(ident), Some(ty)) = (else_binding, else_binding_ty) {
            let _ = self.symbols.define(ident.name.clone(), ty, false);
        }
        for stmt in else_block {
            let _ = self.check_stmt(stmt);
        }
        self.symbols.pop_scope();

        if !stmts_diverge(else_block) {
            self.record_error(TypeError::ValElseMustDiverge { span });
        }

        // Binding the scrutinee moves it, exactly as an ordinary `val` does.
        self.record_move(value);
        for (name, ty, binding_span) in bindings {
            if matches!(ty, Type::Unknown) {
                continue;
            }
            if let Err(duplicate) = self.symbols.define(name, ty, false) {
                self.record_error(TypeError::VariableAlreadyDefined {
                    name: duplicate,
                    span: binding_span,
                });
            }
        }
        Some(())
    }

    /// The type `else |name|` names, per the documented table: a `Result`'s `Err` payload,
    /// nothing for an `Option` (whose failure variant carries none — reported), and
    /// the untouched scrutinee for every other type.
    fn else_binding_type(&mut self, ident: &Identifier, scrut_ty: &Type) -> Option<Type> {
        if matches!(scrut_ty, Type::Unknown) {
            return None;
        }
        let Type::Enum(instance) = scrut_ty.referent() else {
            return Some(scrut_ty.clone());
        };
        // A monomorphized `Result<i32, string>` answers with the template it came from;
        // a program shadowing the prelude with its own enum is its own base.
        let base = self
            .enum_instance_base(instance)
            .unwrap_or(instance.as_str());
        match base {
            OPTION_ENUM => {
                self.record_error(TypeError::ValElseBindingOnOption {
                    name: ident.name.clone(),
                    span: ident.span,
                });
                None
            }
            RESULT_ENUM => self
                .lookup_enum_variant(instance, RESULT_FAILURE_VARIANT)
                .and_then(|info| info.fields.first().map(|(_, ty)| ty.clone())),
            _ => Some(scrut_ty.clone()),
        }
    }
}

/// Whether a statement list is guaranteed to leave the enclosing scope.
///
/// Any diverging statement suffices — everything after it is unreachable, so the list
/// as a whole cannot fall through.
pub(crate) fn stmts_diverge(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_diverges)
}

pub(crate) fn stmt_diverges(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Return { .. } | Stmt::Break { .. } | Stmt::Continue { .. } => true,
        Stmt::If {
            then_block,
            else_if_blocks,
            else_block: Some(else_block),
            ..
        } => {
            stmts_diverge(then_block)
                && else_if_blocks.iter().all(|(_, block)| stmts_diverge(block))
                && stmts_diverge(else_block)
        }
        // A nested `val-else` diverges only through its own else branch, which cannot
        // stand in for this one: the outer branch still falls through on a match.
        Stmt::Expr(expr) => expr_diverges(expr),
        _ => false,
    }
}

pub(crate) fn expr_diverges(expr: &Expr) -> bool {
    match expr {
        Expr::Paren(inner, _) => expr_diverges(inner),
        Expr::Call { func, .. } => match func.as_ref() {
            Expr::Identifier(ident) => DIVERGING_BUILTINS.contains(&ident.name.as_str()),
            _ => false,
        },
        Expr::Block { stmts, .. } | Expr::Unsafe { stmts, .. } => stmts_diverge(stmts),
        Expr::If {
            then_block,
            else_if_blocks,
            else_block: Some(else_block),
            ..
        } => {
            stmts_diverge(then_block)
                && else_if_blocks.iter().all(|(_, block)| stmts_diverge(block))
                && stmts_diverge(else_block)
        }
        Expr::Match { arms, .. } => {
            !arms.is_empty() && arms.iter().all(|arm| expr_diverges(&arm.body))
        }
        _ => false,
    }
}
