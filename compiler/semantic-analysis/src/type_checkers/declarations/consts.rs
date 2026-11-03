//! Module-level `const` items and the constant-expression rule they must satisfy.
//!
//! One of the declaration-kind modules under `declarations`; each adds methods
//! to the same `impl TypeChecker` block.

use crate::errors::TypeError;
use crate::type_checkers::TypeChecker;
use ast_types::{ConstDef, Expr};
use std::collections::HashSet;

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
        self.constant_values
            .insert(def.name.name.clone(), def.value.clone());
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
        if self.const_reads(&def.name.name, &def.value, &mut HashSet::new()) {
            self.record_error(TypeError::CyclicConst {
                name: def.name.name.clone(),
                span: def.name.span,
            });
            return None;
        }

        if let Some(expr_ty) = self.check_expr(&def.value, Some(&declared_ty))
            && !expr_ty.is_compatible_with(&declared_ty)
        {
            self.record_error(TypeError::Mismatch {
                expected: declared_ty,
                found: expr_ty,
                span: def.value.span(),
            });
            return None;
        }
        self.check_const_value(&def.value, &declared_ty);

        Some(())
    }

    /// Whether the constant expression `expr` reads the constant `target`, directly or
    /// through the initializers of the constants it names. `seen` keeps a cycle that does
    /// not pass through `target` from looping.
    fn const_reads(&self, target: &str, expr: &Expr, seen: &mut HashSet<String>) -> bool {
        match expr {
            Expr::Paren(inner, _) | Expr::Unary { operand: inner, .. } => {
                self.const_reads(target, inner, seen)
            }
            Expr::Cast { expr: inner, .. } => self.const_reads(target, inner, seen),
            Expr::Binary { left, right, .. } => {
                self.const_reads(target, left, seen) || self.const_reads(target, right, seen)
            }
            Expr::Identifier(ident) => {
                ident.name == target
                    || (seen.insert(ident.name.clone())
                        && self
                            .constant_values
                            .get(&ident.name)
                            .is_some_and(|value| self.const_reads(target, value, seen)))
            }
            _ => false,
        }
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
