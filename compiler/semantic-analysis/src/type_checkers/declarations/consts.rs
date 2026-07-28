//! Module-level `const` items and the constant-expression rule they must satisfy.
//!
//! One of the declaration-kind modules under `declarations`; each adds methods
//! to the same `impl TypeChecker` block.

use crate::errors::TypeError;
use crate::type_checkers::TypeChecker;
use ast_types::{ConstDef, Expr};

impl TypeChecker {
    /// Register a module-level constant name and type in the constants map.
    ///
    /// Called in a pre-pass so forward references to other consts resolve correctly.
    pub(crate) fn register_const_item(&mut self, def: &ConstDef) -> Option<()> {
        if self.constants.contains_key(&def.name.name) {
            self.record_error(TypeError::ConstAlreadyDefined {
                name: def.name.name.clone(),
                span: def.name.span,
            });
            return None;
        }

        let ty = self.resolve_type(&def.ty)?;
        self.constants.insert(def.name.name.clone(), ty);
        Some(())
    }

    /// Validate a module-level constant declaration.
    pub(crate) fn check_const_item(&mut self, def: &ConstDef) -> Option<()> {
        let declared_ty = self.resolve_type(&def.ty)?;

        if !self.is_const_expr(&def.value) {
            self.record_error(TypeError::InvalidConstExpr {
                span: def.value.span(),
            });
            return None;
        }

        if let Some(expr_ty) = self.check_expr(&def.value, Some(&declared_ty)) {
            if !expr_ty.is_compatible_with(&declared_ty) {
                self.record_error(TypeError::Mismatch {
                    expected: declared_ty,
                    found: expr_ty,
                    span: def.value.span(),
                });
            }
        }

        Some(())
    }

    /// Returns true if `expr` is a valid constant expression.
    ///
    /// Valid constant expressions are: literals, arithmetic/unary on literal
    /// sub-expressions, parenthesized const expressions, and identifiers that
    /// refer to a previously declared `const`.
    pub(crate) fn is_const_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Literal(_, _) => true,
            Expr::Paren(inner, _) => self.is_const_expr(inner),
            Expr::Unary { operand, .. } => self.is_const_expr(operand),
            Expr::Binary { left, right, .. } => {
                self.is_const_expr(left) && self.is_const_expr(right)
            }
            Expr::Cast { expr: inner, .. } => self.is_const_expr(inner),
            Expr::Identifier(ident) => self.constants.contains_key(&ident.name),
            _ => false,
        }
    }
}
