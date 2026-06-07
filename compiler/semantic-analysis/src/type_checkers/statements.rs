use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::Stmt;

impl TypeChecker {
    /// Check a statement.
    /// Returns None if there was a fatal error, Some(()) otherwise.
    /// Non-fatal errors are recorded and checking continues.
    pub(crate) fn check_stmt(&mut self, stmt: &Stmt) -> Option<()> {
        match stmt {
            Stmt::VarDecl {
                name,
                ty,
                init,
                mutable,
                span,
            } => {
                // Resolve declared type if present
                let declared_ty = if let Some(ty) = ty {
                    self.resolve_type(ty)
                } else {
                    None
                };

                // Check initializer type with expected type hint
                // If declared type exists, pass it as expected for type inference
                let init_ty = if let Some(init_expr) = init {
                    self.check_expr(init_expr, declared_ty.as_ref())
                } else {
                    None
                };

                // Determine final type
                let final_ty = match (declared_ty, init_ty) {
                    (Some(decl), Some(init)) => {
                        // Both declared and initialized: types must match
                        if !init.is_compatible_with(&decl) {
                            self.record_error(TypeError::Mismatch {
                                expected: decl.clone(),
                                found: init,
                                span: *span,
                            });
                            // Use declared type to avoid cascading errors
                        }
                        decl
                    }
                    (Some(decl), None) => {
                        // Only declared: use declared type
                        decl
                    }
                    (None, Some(init)) => {
                        // Only initialized: infer from initializer (Phase 1: simple inference)
                        init
                    }
                    (None, None) => {
                        // Neither declared nor initialized: error
                        self.record_error(TypeError::UninitializedVariable {
                            name: name.name.clone(),
                            span: *span,
                        });
                        return None;
                    }
                };

                // Skip Unknown types to avoid cascading errors
                if matches!(final_ty, Type::Unknown) {
                    return Some(());
                }

                // Define variable in current scope
                if let Err(duplicate_name) =
                    self.symbols.define(name.name.clone(), final_ty, *mutable)
                {
                    self.record_error(TypeError::VariableAlreadyDefined {
                        name: duplicate_name,
                        span: name.span,
                    });
                    return None;
                }

                // Binding the initializer moves it out of its source (§2.2).
                if let Some(init_expr) = init {
                    self.record_move(init_expr);
                }

                Some(())
            }

            Stmt::Assignment {
                target,
                value,
                span,
            } => {
                // Lookup the target variable first to get expected type
                let expected_ty = self.symbols.lookup(&target.name).map(|s| s.ty.clone());

                // Check the value expression with expected type hint
                let value_ty = self
                    .check_expr(value, expected_ty.as_ref())
                    .unwrap_or(Type::Unknown);

                // The RHS is moved into the target, and the target now owns a
                // fresh value — clearing any prior moved-out state on it (§2.2).
                self.record_move(value);
                self.symbols.clear_moved(&target.name);

                // Lookup the target variable again for validation
                if let Some(symbol_info) = self.symbols.lookup(&target.name) {
                    // Check if variable is mutable
                    if !symbol_info.mutable {
                        self.record_error(TypeError::AssignToImmutable {
                            name: target.name.clone(),
                            span: target.span,
                        });
                        return None;
                    }

                    // Check type compatibility (skip if value type is unknown)
                    if !matches!(value_ty, Type::Unknown)
                        && !value_ty.is_compatible_with(&symbol_info.ty)
                    {
                        self.record_error(TypeError::Mismatch {
                            expected: symbol_info.ty.clone(),
                            found: value_ty,
                            span: *span,
                        });
                    }

                    Some(())
                } else {
                    // Variable not defined
                    self.record_error(TypeError::UndefinedVariable {
                        name: target.name.clone(),
                        span: target.span,
                    });
                    None
                }
            }

            Stmt::Return { value, span } => {
                // Check return value with expected return type hint
                // Clone the expected type to avoid borrow checker issues
                let expected_return = self.current_function_return_type.clone();
                let return_ty = if let Some(expr) = value {
                    self.check_expr(expr, expected_return.as_ref())
                        .unwrap_or(Type::Unknown)
                } else {
                    Type::Void
                };

                // Check against expected return type (skip if return type is unknown)
                if let Some(expected) = &self.current_function_return_type {
                    if !matches!(return_ty, Type::Unknown)
                        && !return_ty.is_compatible_with(expected)
                    {
                        self.record_error(TypeError::ReturnTypeMismatch {
                            expected: expected.clone(),
                            found: return_ty,
                            span: *span,
                        });
                    }
                }

                // Returning a value moves it out of the function (§2.2).
                if let Some(expr) = value {
                    self.record_move(expr);
                }

                Some(())
            }

            Stmt::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                span: _,
            } => {
                // Check condition is boolean - no type inference needed (must be bool)
                if let Some(cond_ty) = self.check_expr(condition, Some(&Type::Bool)) {
                    if !cond_ty.is_bool() {
                        self.record_error(TypeError::Mismatch {
                            expected: Type::Bool,
                            found: cond_ty,
                            span: condition.span(),
                        });
                    }
                }

                // A move inside one arm must not invalidate the binding on paths
                // that never ran that arm. Restore the move state after each arm
                // so only unconditional (straight-line) moves persist (§2.2).
                let move_snapshot = self.symbols.snapshot_moves();

                // Check then block
                self.symbols.push_scope();
                for stmt in then_block {
                    let _ = self.check_stmt(stmt);
                }
                self.symbols.pop_scope();
                self.symbols.restore_moves(&move_snapshot);

                // Check else-if blocks
                for (else_if_cond, else_if_stmts) in else_if_blocks {
                    if let Some(cond_ty) = self.check_expr(else_if_cond, Some(&Type::Bool)) {
                        if !cond_ty.is_bool() {
                            self.record_error(TypeError::Mismatch {
                                expected: Type::Bool,
                                found: cond_ty,
                                span: else_if_cond.span(),
                            });
                        }
                    }

                    self.symbols.push_scope();
                    for stmt in else_if_stmts {
                        let _ = self.check_stmt(stmt);
                    }
                    self.symbols.pop_scope();
                    self.symbols.restore_moves(&move_snapshot);
                }

                // Check else block
                if let Some(else_stmts) = else_block {
                    self.symbols.push_scope();
                    for stmt in else_stmts {
                        let _ = self.check_stmt(stmt);
                    }
                    self.symbols.pop_scope();
                    self.symbols.restore_moves(&move_snapshot);
                }

                Some(())
            }

            Stmt::While {
                condition,
                body,
                span: _,
            } => {
                if let Some(cond_ty) = self.check_expr(condition, Some(&Type::Bool)) {
                    if !cond_ty.is_bool() {
                        self.record_error(TypeError::Mismatch {
                            expected: Type::Bool,
                            found: cond_ty,
                            span: condition.span(),
                        });
                    }
                }

                // The body may run zero or many times, so a move inside it is not
                // a guaranteed straight-line move; restore afterwards (§2.2).
                let move_snapshot = self.symbols.snapshot_moves();
                self.loop_depth += 1;
                self.symbols.push_scope();
                for stmt in body {
                    let _ = self.check_stmt(stmt);
                }
                self.symbols.pop_scope();
                self.loop_depth -= 1;
                self.symbols.restore_moves(&move_snapshot);

                Some(())
            }

            Stmt::ForRange {
                iterator,
                start,
                end,
                inclusive: _,
                body,
                span: _,
            } => {
                let start_ty = self.check_expr(start, None).unwrap_or(Type::Unknown);
                if !matches!(start_ty, Type::Unknown) && !start_ty.is_integer() {
                    self.record_error(TypeError::InvalidForRangeType {
                        found: start_ty.clone(),
                        span: start.span(),
                    });
                }

                let end_ty = self
                    .check_expr(end, Some(&start_ty))
                    .unwrap_or(Type::Unknown);
                if !matches!(end_ty, Type::Unknown) && !end_ty.is_integer() {
                    self.record_error(TypeError::InvalidForRangeType {
                        found: end_ty.clone(),
                        span: end.span(),
                    });
                }

                if !matches!(start_ty, Type::Unknown)
                    && !matches!(end_ty, Type::Unknown)
                    && !end_ty.is_compatible_with(&start_ty)
                {
                    self.record_error(TypeError::Mismatch {
                        expected: start_ty.clone(),
                        found: end_ty,
                        span: end.span(),
                    });
                }

                // As with `while`, body moves are not guaranteed straight-line;
                // restore the move state after the loop (§2.2).
                let move_snapshot = self.symbols.snapshot_moves();
                self.loop_depth += 1;
                self.symbols.push_scope();

                if !matches!(start_ty, Type::Unknown) {
                    if let Err(duplicate_name) =
                        self.symbols.define(iterator.name.clone(), start_ty, false)
                    {
                        self.record_error(TypeError::VariableAlreadyDefined {
                            name: duplicate_name,
                            span: iterator.span,
                        });
                    }
                }

                for stmt in body {
                    let _ = self.check_stmt(stmt);
                }

                self.symbols.pop_scope();
                self.loop_depth -= 1;
                self.symbols.restore_moves(&move_snapshot);

                Some(())
            }

            Stmt::Break { span } => {
                if self.loop_depth == 0 {
                    self.record_error(TypeError::BreakOutsideLoop { span: *span });
                }

                Some(())
            }

            Stmt::Continue { span } => {
                if self.loop_depth == 0 {
                    self.record_error(TypeError::ContinueOutsideLoop { span: *span });
                }

                Some(())
            }

            Stmt::FieldAssignment {
                object,
                field,
                value,
                span,
            } => {
                let symbol = if let Some(s) = self.symbols.lookup(&object.name) {
                    s.clone()
                } else {
                    self.record_error(TypeError::UndefinedVariable {
                        name: object.name.clone(),
                        span: object.span,
                    });
                    return None;
                };

                if !symbol.mutable {
                    self.record_error(TypeError::AssignToImmutableField {
                        var_name: object.name.clone(),
                        field_name: field.name.clone(),
                        span: *span,
                    });
                    return None;
                }

                let struct_name = match &symbol.ty {
                    Type::Struct(n) => n.clone(),
                    other => {
                        self.record_error(TypeError::UnknownField {
                            struct_name: other.to_string(),
                            field_name: field.name.clone(),
                            span: field.span,
                        });
                        return None;
                    }
                };

                let field_ty = {
                    let def = self.struct_defs.get(&struct_name).cloned();
                    if let Some(def) = def {
                        def.iter()
                            .find(|(n, _)| n == &field.name)
                            .map(|(_, t)| t.clone())
                    } else {
                        None
                    }
                };

                if let Some(expected_ty) = field_ty {
                    if let Some(actual_ty) = self.check_expr(value, Some(&expected_ty)) {
                        if !actual_ty.is_compatible_with(&expected_ty) {
                            self.record_error(TypeError::Mismatch {
                                expected: expected_ty,
                                found: actual_ty,
                                span: *span,
                            });
                        }
                    }
                } else {
                    self.record_error(TypeError::UnknownField {
                        struct_name,
                        field_name: field.name.clone(),
                        span: field.span,
                    });
                    return None;
                }

                // Storing the value into a field moves it out of its source (§2.2).
                self.record_move(value);

                Some(())
            }

            Stmt::Const {
                name,
                ty,
                value,
                span,
            } => {
                if self.constants.contains_key(&name.name) {
                    self.record_error(TypeError::ConstAlreadyDefined {
                        name: name.name.clone(),
                        span: name.span,
                    });
                    return None;
                }

                let declared_ty = self.resolve_type(ty)?;

                if !self.is_const_expr(value) {
                    self.record_error(TypeError::InvalidConstExpr { span: value.span() });
                    return None;
                }

                if let Some(expr_ty) = self.check_expr(value, Some(&declared_ty)) {
                    if !expr_ty.is_compatible_with(&declared_ty) {
                        self.record_error(TypeError::Mismatch {
                            expected: declared_ty.clone(),
                            found: expr_ty,
                            span: *span,
                        });
                        return None;
                    }
                }

                self.constants.insert(name.name.clone(), declared_ty);
                Some(())
            }

            Stmt::Expr(expr) => {
                // Expression statements have no expected type context
                let _ = self.check_expr(expr, None);
                Some(())
            }
        }
    }
}
