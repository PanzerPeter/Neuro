use super::{LoopContext, TypeChecker};
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::{Expr, Stmt};
use shared_types::Identifier;

/// If `expr` is a direct borrow of a named place (`&x` / `&mut x`, possibly
/// parenthesised), return that place's name and whether the borrow is exclusive.
/// A borrow wrapped in any other expression (a block, an `if`, a call result) is
/// not tracked as persistent — only a direct initializer creates a held borrow,
/// which keeps the analysis free of false positives at the cost of missing some
/// borrows that escape through compound expressions.
fn borrow_target_of(expr: &Expr) -> Option<(String, bool)> {
    let mut outer = expr;
    while let Expr::Paren(inner, _) = outer {
        outer = inner;
    }
    let Expr::Reference {
        operand, mutable, ..
    } = outer
    else {
        return None;
    };
    let mut place = operand.as_ref();
    while let Expr::Paren(inner, _) = place {
        place = inner;
    }
    match place {
        Expr::Identifier(ident) => Some((ident.name.clone(), *mutable)),
        _ => None,
    }
}

/// The trailing value expression of a block — the last statement when it is a
/// bare expression. Used to follow a returned reference into the tail of an
/// `if`/`else` arm or a bare block.
fn tail_expr(stmts: &[Stmt]) -> Option<&Expr> {
    match stmts.last() {
        Some(Stmt::Expr(expr)) => Some(expr),
        _ => None,
    }
}

/// The base place identifier a borrow points into, peeling parentheses, field
/// access, and dereference (`&self.field` roots at `self`, `&(x)` at `x`). A
/// non-place operand (literal, call) has no root and yields `None`.
fn root_place_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(ident) => Some(ident.name.clone()),
        Expr::Paren(inner, _) => root_place_name(inner),
        Expr::FieldAccess { object, .. } => root_place_name(object),
        Expr::Deref { operand, .. } => root_place_name(operand),
        _ => None,
    }
}

impl TypeChecker {
    /// Whether `name` is a binding local to the current function — present in the
    /// symbol table but not in the set of places that outlive the call.
    /// Function locals and by-value parameters are local; reference parameters and
    /// `self` outlive. A name absent from the table (a constant, an out-of-scope
    /// place) is conservatively treated as non-local so a valid program is never
    /// rejected.
    fn is_local_to_function(&self, name: &str) -> bool {
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
                if let Some(name) = root_place_name(operand) {
                    if self.is_local_to_function(&name) {
                        self.record_error(TypeError::ReturnsReferenceToLocal { name, span: *span });
                    }
                }
            }
            Expr::Identifier(ident) => {
                if let Some(place) = self.symbols.borrow_provenance(&ident.name) {
                    if self.is_local_to_function(&place) {
                        self.record_error(TypeError::ReturnsReferenceToLocal {
                            name: place,
                            span: ident.span,
                        });
                    }
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
                if let Some(block) = else_block {
                    if let Some(tail) = tail_expr(block) {
                        self.check_returned_reference(tail);
                    }
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

    /// Validate a `break` / `continue` against the active loop stack.
    ///
    /// An unlabeled control statement requires any enclosing loop; a labeled one
    /// requires an enclosing loop carrying that exact label. `is_break`
    /// distinguishes the two error codes for an out-of-loop unlabeled statement.
    fn check_loop_control_label(
        &mut self,
        label: Option<&Identifier>,
        span: shared_types::Span,
        is_break: bool,
    ) {
        match label {
            Some(label) => {
                let in_scope = self
                    .loop_stack
                    .iter()
                    .any(|ctx| ctx.label.as_deref() == Some(label.name.as_str()));
                if !in_scope {
                    self.record_error(TypeError::UndefinedLabel {
                        name: label.name.clone(),
                        span: label.span,
                    });
                }
            }
            None if self.loop_stack.is_empty() => {
                if is_break {
                    self.record_error(TypeError::BreakOutsideLoop { span });
                } else {
                    self.record_error(TypeError::ContinueOutsideLoop { span });
                }
            }
            None => {}
        }
    }

    /// Check a loop body under a fresh [`LoopContext`], returning the agreed
    /// value-break type accumulated inside (`None` when no value-carrying `break`
    /// targeted this loop). Loop bodies run any number of times, so a move inside
    /// is not a straight-line move; the move state is snapshotted and restored on
    /// exit. `is_value_loop` is true only for `loop` — the sole construct
    /// that can yield a value.
    pub(crate) fn check_loop_body(
        &mut self,
        label: Option<&Identifier>,
        is_value_loop: bool,
        body: &[Stmt],
    ) -> Option<Type> {
        let move_snapshot = self.symbols.snapshot_moves();
        self.loop_stack.push(LoopContext {
            label: label.map(|l| l.name.clone()),
            is_value_loop,
            break_value_ty: None,
        });
        self.symbols.push_scope();
        for stmt in body {
            let _ = self.check_stmt(stmt);
        }
        self.symbols.pop_scope();
        let ctx = self.loop_stack.pop();
        self.symbols.restore_moves(&move_snapshot);
        ctx.and_then(|c| c.break_value_ty)
    }

    /// Record a value-carrying `break v` against its target loop: the
    /// innermost loop, or the loop named by `label`. Reports an error if the
    /// target is a `while`/`for` (unit-only) or if `value_ty` disagrees with an
    /// earlier value-break for the same loop.
    fn record_break_value(
        &mut self,
        label: Option<&Identifier>,
        value_ty: Type,
        span: shared_types::Span,
    ) {
        let target = match label {
            Some(label) => self
                .loop_stack
                .iter_mut()
                .rev()
                .find(|ctx| ctx.label.as_deref() == Some(label.name.as_str())),
            None => self.loop_stack.last_mut(),
        };
        // A missing target was already reported by `check_loop_control_label`.
        let Some(ctx) = target else {
            return;
        };
        if !ctx.is_value_loop {
            self.record_error(TypeError::BreakValueInUnitLoop { span });
            return;
        }
        match &ctx.break_value_ty {
            None => ctx.break_value_ty = Some(value_ty),
            Some(existing) => {
                if !value_ty.is_compatible_with(existing) {
                    let expected = existing.clone();
                    self.record_error(TypeError::Mismatch {
                        expected,
                        found: value_ty,
                        span,
                    });
                }
            }
        }
    }

    /// Check a statement, then drop any transient borrows it took.
    ///
    /// A borrow passed to a call, used in a condition, or returned lives only for
    /// the statement that created it. Clearing transient borrows here
    /// — after the statement and its nested sub-statements are fully checked —
    /// frees the place for a later borrow without leaking the borrow forward.
    /// Persistent borrows held by reference bindings are untouched; they are
    /// released when their binding leaves scope.
    pub(crate) fn check_stmt(&mut self, stmt: &Stmt) -> Option<()> {
        let result = self.check_stmt_inner(stmt);
        self.symbols.clear_transient_borrows();
        result
    }

    /// Check a statement.
    /// Returns None if there was a fatal error, Some(()) otherwise.
    /// Non-fatal errors are recorded and checking continues.
    fn check_stmt_inner(&mut self, stmt: &Stmt) -> Option<()> {
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

                // Pass any declared type as the expected hint for inference.
                let init_ty = if let Some(init_expr) = init {
                    self.check_expr(init_expr, declared_ty.as_ref())
                } else {
                    None
                };

                let final_ty = match (declared_ty, init_ty) {
                    (Some(decl), Some(init)) => {
                        // Both declared and initialized: types must match
                        if !self.assignable(&init, &decl) {
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

                if let Err(duplicate_name) =
                    self.symbols.define(name.name.clone(), final_ty, *mutable)
                {
                    self.record_error(TypeError::VariableAlreadyDefined {
                        name: duplicate_name,
                        span: name.span,
                    });
                    return None;
                }

                // Binding the initializer moves it out of its source.
                if let Some(init_expr) = init {
                    self.record_move(init_expr);

                    // A direct `&place` / `&mut place` initializer makes this
                    // binding hold a persistent borrow of that place, live until
                    // the binding leaves scope.
                    if let Some((place, exclusive)) = borrow_target_of(init_expr) {
                        self.symbols.attach_borrow(&name.name, &place, exclusive);
                    }
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

                // If the target was a reference binding, its previous borrow ends
                // here — release it before the new value is checked so that
                // re-borrowing the same place (`r = &mut x`) is not a false
                // conflict against the borrow being overwritten.
                self.symbols.release_borrow_of(&target.name);

                let value_ty = self
                    .check_expr(value, expected_ty.as_ref())
                    .unwrap_or(Type::Unknown);

                // The RHS is moved into the target, and the target now owns a
                // fresh value — clearing any prior moved-out state on it.
                self.record_move(value);
                self.symbols.clear_moved(&target.name);

                // A direct `&place` / `&mut place` RHS makes the target hold a new
                // persistent borrow of that place.
                if let Some((place, exclusive)) = borrow_target_of(value) {
                    self.symbols.attach_borrow(&target.name, &place, exclusive);
                }

                // Lookup the target variable again for validation
                if let Some(symbol_info) = self.symbols.lookup(&target.name) {
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
                    self.record_error(TypeError::UndefinedVariable {
                        name: target.name.clone(),
                        span: target.span,
                    });
                    None
                }
            }

            Stmt::Return { value, span } => {
                // Cloned to release the borrow on `self` before `check_expr`.
                let expected_return = self.current_function_return_type.clone();
                let return_ty = if let Some(expr) = value {
                    self.check_expr(expr, expected_return.as_ref())
                        .unwrap_or(Type::Unknown)
                } else {
                    Type::Void
                };

                // Check against expected return type (skip if return type is unknown)
                if let Some(expected) = self.current_function_return_type.clone() {
                    if !matches!(return_ty, Type::Unknown)
                        && !self.assignable(&return_ty, &expected)
                    {
                        self.record_error(TypeError::ReturnTypeMismatch {
                            expected: expected.clone(),
                            found: return_ty,
                            span: *span,
                        });
                    }
                }

                // Returning a value moves it out of the function.
                if let Some(expr) = value {
                    self.record_move(expr);

                    // A returned reference must outlive the call: reject a borrow of
                    // a function-local place.
                    if matches!(expected_return, Some(Type::Reference { .. })) {
                        self.check_returned_reference(expr);
                    }
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
                // so only unconditional (straight-line) moves persist.
                let move_snapshot = self.symbols.snapshot_moves();

                self.symbols.push_scope();
                for stmt in then_block {
                    let _ = self.check_stmt(stmt);
                }
                self.symbols.pop_scope();
                self.symbols.restore_moves(&move_snapshot);

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
                label,
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

                // A `while` always yields unit, so it is not a value loop.
                let _ = self.check_loop_body(label.as_ref(), false, body);

                Some(())
            }

            Stmt::Loop {
                label,
                body,
                span: _,
            } => {
                // A `loop` is value-capable; in statement position the value
                // is simply discarded, but value-breaks are still type-checked.
                let _ = self.check_loop_body(label.as_ref(), true, body);

                Some(())
            }

            Stmt::ForRange {
                label,
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
                // restore the move state after the loop. A `for` yields unit,
                // so it is not a value loop.
                let move_snapshot = self.symbols.snapshot_moves();
                self.loop_stack.push(LoopContext {
                    label: label.as_ref().map(|l| l.name.clone()),
                    is_value_loop: false,
                    break_value_ty: None,
                });
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
                self.loop_stack.pop();
                self.symbols.restore_moves(&move_snapshot);

                Some(())
            }

            // `for x in arr` over an array value. The iterable must be an array
            // (or a borrow of one); `x` binds each element. Lowered as a counted loop.
            Stmt::ForEach {
                label,
                iterator,
                iterable,
                body,
                span: _,
            } => {
                let iterable_ty = self.check_expr(iterable, None).unwrap_or(Type::Unknown);
                let element_ty = match iterable_ty.referent() {
                    Type::Array { element, .. } => Some((**element).clone()),
                    Type::Unknown => None,
                    other => {
                        self.record_error(TypeError::NotIndexable {
                            found: other.clone(),
                            span: iterable.span(),
                        });
                        None
                    }
                };

                // Body moves are not guaranteed straight-line; restore move state after
                // the loop. A `for` yields unit, so it is not a value loop.
                let move_snapshot = self.symbols.snapshot_moves();
                self.loop_stack.push(LoopContext {
                    label: label.as_ref().map(|l| l.name.clone()),
                    is_value_loop: false,
                    break_value_ty: None,
                });
                self.symbols.push_scope();

                if let Some(element_ty) = element_ty {
                    if let Err(duplicate_name) =
                        self.symbols
                            .define(iterator.name.clone(), element_ty, false)
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
                self.loop_stack.pop();
                self.symbols.restore_moves(&move_snapshot);

                Some(())
            }

            // Array element assignment `arr[i] = v`. The target must be a mutable
            // array binding; the index an integer; the value the element type.
            Stmt::IndexAssignment {
                target,
                index,
                value,
                span,
            } => {
                let symbol = if let Some(s) = self.symbols.lookup(&target.name) {
                    s.clone()
                } else {
                    self.record_error(TypeError::UndefinedVariable {
                        name: target.name.clone(),
                        span: target.span,
                    });
                    return None;
                };

                if !symbol.mutable {
                    self.record_error(TypeError::AssignToImmutable {
                        name: target.name.clone(),
                        span: target.span,
                    });
                    return None;
                }

                let element_ty = match &symbol.ty {
                    Type::Array { element, .. } => (**element).clone(),
                    other => {
                        self.record_error(TypeError::NotIndexable {
                            found: other.clone(),
                            span: *span,
                        });
                        return None;
                    }
                };

                let idx_ty = self.check_expr(index, None).unwrap_or(Type::Unknown);
                if !matches!(idx_ty, Type::Unknown) && !idx_ty.is_integer() {
                    self.record_error(TypeError::IndexNotInteger {
                        found: idx_ty,
                        span: index.span(),
                    });
                }

                let value_ty = self
                    .check_expr(value, Some(&element_ty))
                    .unwrap_or(Type::Unknown);
                if !matches!(value_ty, Type::Unknown) && !value_ty.is_compatible_with(&element_ty) {
                    self.record_error(TypeError::Mismatch {
                        expected: element_ty,
                        found: value_ty,
                        span: *span,
                    });
                }
                self.record_move(value);

                Some(())
            }

            Stmt::Break { label, value, span } => {
                self.check_loop_control_label(label.as_ref(), *span, true);
                if let Some(value_expr) = value {
                    let value_ty = self.check_expr(value_expr, None).unwrap_or(Type::Unknown);
                    if !matches!(value_ty, Type::Unknown) {
                        self.record_break_value(label.as_ref(), value_ty, *span);
                    }
                }
                Some(())
            }

            Stmt::Continue { label, span } => {
                self.check_loop_control_label(label.as_ref(), *span, false);
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

                // Storing the value into a field moves it out of its source.
                self.record_move(value);

                Some(())
            }

            // Assignment through a mutable reference `*pointer = value`. The
            // pointer must have a `&mut T` type; the value is checked against `T`.
            Stmt::DerefAssignment {
                pointer,
                value,
                span,
            } => {
                let pointer_ty = self.check_expr(pointer, None).unwrap_or(Type::Unknown);
                let inner_ty = match &pointer_ty {
                    Type::Unknown => return Some(()),
                    Type::Reference {
                        inner,
                        mutable: true,
                    } => (**inner).clone(),
                    Type::Reference {
                        inner,
                        mutable: false,
                    } => {
                        self.record_error(TypeError::CannotAssignThroughRef {
                            inner: (**inner).clone(),
                            span: *span,
                        });
                        return None;
                    }
                    other => {
                        self.record_error(TypeError::CannotDereference {
                            found: other.clone(),
                            span: pointer.span(),
                        });
                        return None;
                    }
                };

                let value_ty = self
                    .check_expr(value, Some(&inner_ty))
                    .unwrap_or(Type::Unknown);
                // The stored value is moved into the location the reference points at.
                self.record_move(value);

                if !matches!(value_ty, Type::Unknown) && !value_ty.is_compatible_with(&inner_ty) {
                    self.record_error(TypeError::Mismatch {
                        expected: inner_ty,
                        found: value_ty,
                        span: *span,
                    });
                }

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
