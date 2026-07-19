//! Statement lowering.

use ast_types::Stmt;
use neuro_hir::{HirStmt, HirType};

use crate::{LoopCtx, Lowerer, LoweringError};

impl Lowerer {
    /// Lower a single statement, threading scope and loop-stack side effects.
    pub(crate) fn lower_stmt(&mut self, stmt: &Stmt) -> Result<HirStmt, LoweringError> {
        match stmt {
            Stmt::VarDecl {
                name,
                ty,
                init,
                mutable,
                span,
            } => {
                let declared = match ty {
                    Some(t) => Some(self.resolve_type(t)?),
                    None => None,
                };
                let init = match init {
                    Some(expr) => Some(self.lower_expr(expr, declared.as_ref())?),
                    None => None,
                };
                // Final type: the declared annotation when present, otherwise the
                // initializer's type (Phase 1 inference). The checker guarantees one
                // of the two exists for a well-typed binding.
                let final_ty = match (&declared, &init) {
                    (Some(d), _) => d.clone(),
                    (None, Some(e)) => e.ty.clone(),
                    (None, None) => {
                        return Err(LoweringError::Malformed {
                            detail: format!(
                                "binding '{}' has neither type nor initializer",
                                name.name
                            ),
                        })
                    }
                };
                self.define(name.name.clone(), final_ty.clone());
                Ok(HirStmt::VarDecl {
                    name: name.name.clone(),
                    ty: final_ty,
                    init,
                    mutable: *mutable,
                    span: *span,
                })
            }

            Stmt::Assignment {
                target,
                value,
                span,
            } => {
                let expected = self.lookup(&target.name);
                let value = self.lower_expr(value, expected.as_ref())?;
                Ok(HirStmt::Assignment {
                    target: target.name.clone(),
                    value,
                    span: *span,
                })
            }

            Stmt::Return { value, span } => {
                let value = match value {
                    Some(expr) => {
                        let expected = self.current_return.clone();
                        Some(self.lower_expr(expr, Some(&expected))?)
                    }
                    None => None,
                };
                Ok(HirStmt::Return { value, span: *span })
            }

            Stmt::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                span,
            } => {
                let condition = self.lower_expr(condition, Some(&HirType::Bool))?;
                let then_block = self.lower_stmt_block(then_block)?;
                let mut elifs = Vec::with_capacity(else_if_blocks.len());
                for (cond, block) in else_if_blocks {
                    let cond = self.lower_expr(cond, Some(&HirType::Bool))?;
                    elifs.push((cond, self.lower_stmt_block(block)?));
                }
                let else_block = match else_block {
                    Some(block) => Some(self.lower_stmt_block(block)?),
                    None => None,
                };
                Ok(HirStmt::If {
                    condition,
                    then_block,
                    else_if_blocks: elifs,
                    else_block,
                    span: *span,
                })
            }

            Stmt::While {
                label,
                condition,
                body,
                span,
            } => {
                let condition = self.lower_expr(condition, Some(&HirType::Bool))?;
                let body = self.lower_loop_body(label, false, body)?;
                Ok(HirStmt::While {
                    label: label.as_ref().map(|l| l.name.clone()),
                    condition,
                    body,
                    span: *span,
                })
            }

            Stmt::Loop { label, body, span } => {
                let body = self.lower_loop_body(label, true, body)?;
                Ok(HirStmt::Loop {
                    label: label.as_ref().map(|l| l.name.clone()),
                    body,
                    span: *span,
                })
            }

            Stmt::ForRange {
                label,
                iterator,
                start,
                end,
                inclusive,
                body,
                span,
            } => {
                let start = self.lower_expr(start, None)?;
                let end = self.lower_expr(end, Some(&start.ty))?;
                let iter_ty = start.ty.clone();
                let body = self.lower_loop_body_with(label, false, |lo| {
                    lo.define(iterator.name.clone(), iter_ty.clone());
                    lo.lower_stmt_list(body)
                })?;
                Ok(HirStmt::ForRange {
                    label: label.as_ref().map(|l| l.name.clone()),
                    iterator: iterator.name.clone(),
                    start,
                    end,
                    inclusive: *inclusive,
                    body,
                    span: *span,
                })
            }

            Stmt::ForEach {
                label,
                iterator,
                iterable,
                body,
                span,
            } => {
                let iterable = self.lower_expr(iterable, None)?;
                let element_ty = match iterable.ty.referent() {
                    HirType::Array { element, .. } => (**element).clone(),
                    other => {
                        return Err(LoweringError::Malformed {
                            detail: format!("for-each over non-array type '{}'", other),
                        })
                    }
                };
                let body = self.lower_loop_body_with(label, false, |lo| {
                    lo.define(iterator.name.clone(), element_ty.clone());
                    lo.lower_stmt_list(body)
                })?;
                Ok(HirStmt::ForEach {
                    label: label.as_ref().map(|l| l.name.clone()),
                    iterator: iterator.name.clone(),
                    iterable,
                    body,
                    span: *span,
                })
            }

            Stmt::Break { label, value, span } => {
                let value = match value {
                    Some(expr) => Some(self.lower_expr(expr, None)?),
                    None => None,
                };
                if let Some(v) = &value {
                    self.record_break_value(label.as_ref().map(|l| l.name.as_str()), v.ty.clone());
                }
                Ok(HirStmt::Break {
                    label: label.as_ref().map(|l| l.name.clone()),
                    value,
                    span: *span,
                })
            }

            Stmt::Continue { label, span } => Ok(HirStmt::Continue {
                label: label.as_ref().map(|l| l.name.clone()),
                span: *span,
            }),

            Stmt::FieldAssignment {
                object,
                field,
                value,
                span,
            } => {
                let field_ty = self.struct_field_type_of_binding(&object.name, &field.name);
                let value = self.lower_expr(value, field_ty.as_ref())?;
                Ok(HirStmt::FieldAssignment {
                    object: object.name.clone(),
                    field: field.name.clone(),
                    value,
                    span: *span,
                })
            }

            Stmt::DerefAssignment {
                pointer,
                value,
                span,
            } => {
                let pointer = self.lower_expr(pointer, None)?;
                let inner = match &pointer.ty {
                    HirType::Reference { inner, .. } => Some((**inner).clone()),
                    _ => None,
                };
                let value = self.lower_expr(value, inner.as_ref())?;
                Ok(HirStmt::DerefAssignment {
                    pointer,
                    value,
                    span: *span,
                })
            }

            Stmt::IndexAssignment {
                target,
                index,
                value,
                span,
            } => {
                let element_ty = match self.lookup(&target.name) {
                    Some(HirType::Array { element, .. }) => Some(*element),
                    _ => None,
                };
                let index = self.lower_expr(index, None)?;
                let value = self.lower_expr(value, element_ty.as_ref())?;
                Ok(HirStmt::IndexAssignment {
                    target: target.name.clone(),
                    index,
                    value,
                    span: *span,
                })
            }

            Stmt::Const {
                name,
                ty,
                value,
                span,
            } => {
                let ty = self.resolve_type(ty)?;
                let value = self.lower_expr(value, Some(&ty))?;
                // A function-scope const is visible to later statements.
                self.constants.insert(name.name.clone(), ty.clone());
                Ok(HirStmt::Const {
                    name: name.name.clone(),
                    ty,
                    value,
                    span: *span,
                })
            }

            Stmt::Expr(expr) => Ok(HirStmt::Expr(self.lower_expr(expr, None)?)),
        }
    }

    /// Lower a statement block in a fresh lexical scope (an `if`/`while`/`for` arm).
    pub(crate) fn lower_stmt_block(
        &mut self,
        stmts: &[Stmt],
    ) -> Result<Vec<HirStmt>, LoweringError> {
        self.push_scope();
        let lowered = self.lower_stmt_list(stmts);
        self.pop_scope();
        lowered
    }

    /// Lower a flat statement list with no scope management of its own.
    pub(crate) fn lower_stmt_list(
        &mut self,
        stmts: &[Stmt],
    ) -> Result<Vec<HirStmt>, LoweringError> {
        let mut out = Vec::with_capacity(stmts.len());
        for stmt in stmts {
            out.push(self.lower_stmt(stmt)?);
        }
        Ok(out)
    }

    /// Lower a loop body under a fresh loop context and scope.
    fn lower_loop_body(
        &mut self,
        label: &Option<shared_types::Identifier>,
        is_value: bool,
        body: &[Stmt],
    ) -> Result<Vec<HirStmt>, LoweringError> {
        self.lower_loop_body_with(label, is_value, |lo| lo.lower_stmt_list(body))
    }

    /// Lower a loop body, running `inner` (which binds any loop variable and lowers
    /// the statements) inside the loop's context and scope. The loop context lets a
    /// `break v` resolve to this loop and accumulate its value type.
    fn lower_loop_body_with(
        &mut self,
        label: &Option<shared_types::Identifier>,
        is_value: bool,
        inner: impl FnOnce(&mut Self) -> Result<Vec<HirStmt>, LoweringError>,
    ) -> Result<Vec<HirStmt>, LoweringError> {
        self.loop_stack.push(LoopCtx {
            label: label.as_ref().map(|l| l.name.clone()),
            is_value,
            value_ty: None,
        });
        self.push_scope();
        let body = inner(self);
        self.pop_scope();
        self.loop_stack.pop();
        body
    }

    /// Record a value-carrying `break`'s type against its target loop: the loop named
    /// by `label`, or the innermost loop. Only a value-capable `loop` accepts one.
    fn record_break_value(&mut self, label: Option<&str>, ty: HirType) {
        let target = match label {
            Some(l) => self
                .loop_stack
                .iter_mut()
                .rev()
                .find(|ctx| ctx.label.as_deref() == Some(l)),
            None => self.loop_stack.last_mut(),
        };
        if let Some(ctx) = target {
            if ctx.is_value && ctx.value_ty.is_none() {
                ctx.value_ty = Some(ty);
            }
        }
    }

    /// The declared type of `field` on the struct bound to `binding`, if resolvable.
    fn struct_field_type_of_binding(&self, binding: &str, field: &str) -> Option<HirType> {
        let HirType::Struct(name) = self.lookup(binding)? else {
            return None;
        };
        self.structs
            .get(&name)?
            .iter()
            .find(|(n, _)| n == field)
            .map(|(_, t)| t.clone())
    }
}
