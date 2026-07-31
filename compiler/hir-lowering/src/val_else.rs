//! Lowering for `val PATTERN = value else |binding| { ... }`.
//!
//! The success test and the pattern's bindings reuse the `match` machinery verbatim;
//! what this adds is the type-directed resolution of the `else |binding|` name and the
//! fact that the pattern's bindings are defined in the enclosing scope rather than an
//! arm's.

use ast_types::{Expr, Pattern, Stmt};
use neuro_hir::{HirBindingSource, HirMatchBinding, HirStmt, HirType};
use shared_types::{Identifier, Span};

use crate::{Lowerer, LoweringError};

/// The fallible enum whose `else |name|` binds a payload rather than the scrutinee.
const RESULT_ENUM: &str = "Result";
/// The `Result` variant an `else` branch is reached through.
const RESULT_FAILURE_VARIANT: &str = "Err";
/// `Option::None` carries no payload, so its `else` binding resolves to nothing; the
/// checker has already rejected a named one.
const OPTION_ENUM: &str = "Option";

impl Lowerer {
    /// Lower a `val-else` statement.
    pub(crate) fn lower_val_else(
        &mut self,
        pattern: &Pattern,
        value: &Expr,
        else_binding: Option<&Identifier>,
        else_block: &[Stmt],
        span: Span,
    ) -> Result<HirStmt, LoweringError> {
        let scrutinee = self.lower_expr(value, None)?;
        let scrut_ty = scrutinee.ty.clone();

        let test = self.pattern_test(pattern, &scrut_ty)?;
        let bindings = self.pattern_bindings(pattern, &scrut_ty)?;

        let else_binding = match else_binding.filter(|ident| ident.name != "_") {
            Some(ident) => self.resolve_else_binding(&ident.name, &scrut_ty)?,
            None => None,
        };

        self.push_scope();
        if let Some(binding) = &else_binding {
            self.define(binding.name.clone(), binding.ty.clone());
        }
        let else_block = self.lower_stmt_list(else_block);
        self.pop_scope();
        let else_block = else_block?;

        // The bindings outlive the statement: every later statement in this block sees
        // them, which is what separates `val-else` from a one-armed `match`.
        for binding in &bindings {
            self.define(binding.name.clone(), binding.ty.clone());
        }

        Ok(HirStmt::ValElse {
            scrutinee,
            test,
            bindings,
            else_binding,
            else_block,
            span,
        })
    }

    /// Resolve what `else |name|` binds, per the documented table: a `Result`'s `Err`
    /// payload, nothing for an `Option`, and the whole scrutinee otherwise.
    fn resolve_else_binding(
        &self,
        name: &str,
        scrut_ty: &HirType,
    ) -> Result<Option<HirMatchBinding>, LoweringError> {
        let whole = || {
            Some(HirMatchBinding {
                name: name.to_string(),
                ty: scrut_ty.clone(),
                source: HirBindingSource::Scrutinee,
            })
        };

        let HirType::Enum(instance) = scrut_ty.referent() else {
            return Ok(whole());
        };
        // A monomorphized `Result<i32, string>` answers with the template it came from;
        // a program shadowing the prelude with its own enum is its own base.
        let base = self
            .enum_instance_base
            .get(instance)
            .map(String::as_str)
            .unwrap_or(instance.as_str());

        match base {
            OPTION_ENUM => Ok(None),
            RESULT_ENUM => {
                let (_, fields) = self.enum_variant(instance, RESULT_FAILURE_VARIANT)?;
                let Some((_, ty)) = fields.first() else {
                    return Ok(None);
                };
                Ok(Some(HirMatchBinding {
                    name: name.to_string(),
                    ty: ty.clone(),
                    source: HirBindingSource::EnumPayload { slot: 0 },
                }))
            }
            _ => Ok(whole()),
        }
    }
}
