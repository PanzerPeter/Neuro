// Block-shaped expressions: `if`, bare blocks, `unsafe`, and `loop`.
//
// Reached from the `check_expr` dispatch in this module's `mod.rs`. Every file
// here adds methods to the same `impl TypeChecker` block.

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::Expr;
use shared_types::{Identifier, Span};

impl TypeChecker {
    pub(super) fn check_if_expr(
        &mut self,
        condition: &Expr,
        then_block: &[ast_types::Stmt],
        else_if_blocks: &[(Expr, Vec<ast_types::Stmt>)],
        else_block: &Option<Vec<ast_types::Stmt>>,
        span: &Span,
    ) -> Option<Type> {
        let cond_ty = self
            .check_expr(condition, Some(&Type::Bool))
            .unwrap_or(Type::Unknown);
        if !matches!(cond_ty, Type::Unknown) && !cond_ty.is_bool() {
            self.record_error(TypeError::Mismatch {
                expected: Type::Bool,
                found: cond_ty,
                span: condition.span(),
            });
        }

        // Each arm runs on its own path, so a move inside one arm must not
        // leak onto the others or past the `if`. Snapshot the move state
        // after the (unconditional) condition and restore it between arms.
        let move_snapshot = self.symbols.snapshot_moves();

        // Collect arm types: then + each else-if + optional else
        let then_ty = self.check_block_expr_type(then_block);

        let mut arm_types: Vec<Type> = vec![then_ty.clone()];

        for (elif_cond, elif_block) in else_if_blocks {
            self.symbols.restore_moves(&move_snapshot);
            let elif_cond_ty = self
                .check_expr(elif_cond, Some(&Type::Bool))
                .unwrap_or(Type::Unknown);
            if !matches!(elif_cond_ty, Type::Unknown) && !elif_cond_ty.is_bool() {
                self.record_error(TypeError::Mismatch {
                    expected: Type::Bool,
                    found: elif_cond_ty,
                    span: elif_cond.span(),
                });
            }
            arm_types.push(self.check_block_expr_type(elif_block));
        }

        self.symbols.restore_moves(&move_snapshot);
        if let Some(else_stmts) = else_block {
            arm_types.push(self.check_block_expr_type(else_stmts));
            self.symbols.restore_moves(&move_snapshot);
        } else {
            return Some(Type::Void);
        }

        // All arms must agree on type
        let result_ty = arm_types[0].clone();
        for arm_ty in &arm_types[1..] {
            if !arm_ty.is_compatible_with(&result_ty) {
                self.record_error(TypeError::Mismatch {
                    expected: result_ty.clone(),
                    found: arm_ty.clone(),
                    span: *span,
                });
                return Some(Type::Unknown);
            }
        }
        Some(result_ty)
    }

    pub(super) fn check_bare_block_expr(&mut self, stmts: &[ast_types::Stmt]) -> Option<Type> {
        self.symbols.push_scope();
        let ty = self.check_block_expr_type(stmts);
        self.symbols.pop_scope();
        Some(ty)
    }

    /// A `loop` evaluates to the value carried by its value-producing
    /// `break`s (which must all agree on type); with only plain `break`s it
    /// yields unit. `while`/`for` have no expression form.
    ///
    /// A `loop` that no `break` targets has no exit edge at all: it either
    /// runs forever or leaves via `return`. It therefore produces no value
    /// and must satisfy whatever type its context demands — the same
    /// divergent contract the panic-family builtins carry.
    pub(super) fn check_loop_expr(
        &mut self,
        label: &Option<Identifier>,
        body: &[ast_types::Stmt],
        expected: Option<&Type>,
    ) -> Option<Type> {
        let exit = self.check_loop_body(label.as_ref(), true, body);
        match exit.value_ty {
            Some(ty) => Some(ty),
            None if exit.has_break => Some(Type::Void),
            None => Some(expected.cloned().unwrap_or(Type::Void)),
        }
    }

    /// `unsafe` is inert in Phase 1.7: it introduces a scope and yields
    /// its trailing expression's type, exactly like a bare block.
    pub(super) fn check_unsafe_block_expr(&mut self, stmts: &[ast_types::Stmt]) -> Option<Type> {
        self.symbols.push_scope();
        let ty = self.check_block_expr_type(stmts);
        self.symbols.pop_scope();
        Some(ty)
    }

    /// Check all stmts in a block and return the type of the trailing expression, or Void.
    pub(super) fn check_block_expr_type(&mut self, stmts: &[ast_types::Stmt]) -> Type {
        self.symbols.push_scope();
        let mut result = Type::Void;
        for (i, stmt) in stmts.iter().enumerate() {
            if i == stmts.len() - 1 {
                if let ast_types::Stmt::Expr(expr) = stmt {
                    result = self.check_expr(expr, None).unwrap_or(Type::Unknown);
                    self.symbols.pop_scope();
                    return result;
                }
            }
            let _ = self.check_stmt(stmt);
        }
        self.symbols.pop_scope();
        result
    }
}
