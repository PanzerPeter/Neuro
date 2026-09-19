use super::{LoopContext, TypeChecker};
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::{BinaryOp, Expr, Place, Stmt, TensorIndexArg};
use shared_types::{Identifier, Span};

/// How a checked loop body is left: the agreed type of its value-carrying
/// `break`s (`None` when it has none), and whether any `break` targeted it at all.
pub(crate) struct LoopExit {
    pub(crate) value_ty: Option<Type>,
    pub(crate) has_break: bool,
}

/// If `expr` is a direct borrow of a named place (`&x` / `&mut x`, or a
/// `x.slice(range)` view into `x`), return that place's name and whether the borrow is
/// exclusive. A borrow wrapped in any other expression (a block, an `if`, a call result)
/// is not tracked as persistent: only a direct initializer creates a held borrow,
/// which keeps the analysis free of false positives at the cost of missing some
/// borrows that escape through compound expressions.
fn borrow_target_of(expr: &Expr) -> Option<(String, bool)> {
    let mut outer = expr;
    while let Expr::Paren(inner, _) = outer {
        outer = inner;
    }
    if let Some(place) = slice_receiver_of(outer) {
        // A `.slice` view is always a shared borrow: there is no `&mut` slicing form.
        return Some((place, false));
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

/// The binding a `.slice(range)` / `.char_slice(range)` call borrows from, when the
/// receiver roots at one. The intrinsics hand back a view into the receiver's storage,
/// so the checker treats the call exactly like an `&receiver` for provenance and
/// aliasing; `register_slice_borrow` records the matching transient borrow, which this
/// promotes to a persistent one when the call initializes a binding.
fn slice_receiver_of(expr: &Expr) -> Option<String> {
    if !matches!(expr, Expr::Call { .. }) {
        return None;
    }
    TypeChecker::slice_borrow_root(expr)
}

/// The trailing value expression of a block: the last statement when it is a
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
    /// Whether `name` is a binding local to the current function: present in the
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

    /// Check a loop body under a fresh [`LoopContext`], returning how the loop is
    /// left. Loop bodies run any number of times, so a move inside is not a
    /// straight-line move; the move state is snapshotted and restored on exit.
    /// `is_value_loop` is true only for `loop`: the sole construct that can yield
    /// a value.
    pub(crate) fn check_loop_body(
        &mut self,
        label: Option<&Identifier>,
        is_value_loop: bool,
        expected: Option<&Type>,
        body: &[Stmt],
    ) -> LoopExit {
        let move_snapshot = self.symbols.snapshot_moves();
        self.loop_stack.push(LoopContext {
            label: label.map(|l| l.name.clone()),
            is_value_loop,
            break_value_ty: None,
            expected_ty: expected.cloned(),
            has_break: false,
        });
        self.symbols.push_scope();
        for stmt in body {
            let _ = self.check_stmt(stmt);
        }
        self.symbols.pop_scope();
        let ctx = self.loop_stack.pop();
        self.report_loop_body_moves(&move_snapshot, body);
        self.symbols.restore_moves(&move_snapshot);
        match ctx {
            Some(ctx) => LoopExit {
                value_ty: ctx.break_value_ty,
                has_break: ctx.has_break,
            },
            None => LoopExit {
                value_ty: None,
                has_break: false,
            },
        }
    }

    /// Record that a `break` targets the loop named by `label`, or the innermost
    /// loop when unlabeled.
    fn record_break_target(&mut self, label: Option<&Identifier>) {
        let target = match label {
            Some(label) => self
                .loop_stack
                .iter_mut()
                .rev()
                .find(|ctx| ctx.label.as_deref() == Some(label.name.as_str())),
            None => self.loop_stack.last_mut(),
        };
        if let Some(ctx) = target {
            ctx.has_break = true;
        }
    }

    /// The expected type of the loop a `break` targets, so `break v` checks `v`
    /// against the same annotation the loop expression is checked against. Without
    /// it a literal in a value loop is typed on its own and then fails to match an
    /// annotation an `if` arm or a block tail in the same position would satisfy.
    fn break_target_expected(&self, label: Option<&Identifier>) -> Option<Type> {
        let target = match label {
            Some(label) => self
                .loop_stack
                .iter()
                .rev()
                .find(|ctx| ctx.label.as_deref() == Some(label.name.as_str())),
            None => self.loop_stack.last(),
        };
        target.and_then(|ctx| ctx.expected_ty.clone())
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
    /// the statement that created it. Clearing transient borrows here (after the
    /// statement and its nested sub-statements are fully checked)
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
                // `Type::Unknown` comes back for two unrelated reasons — an error was
                // reported here, or the initializer diverges (`panic`, `unreachable`) —
                // and the two want opposite treatment below, so record which one it was.
                let errors_before = self.errors.len();
                let init_ty = if let Some(init_expr) = init {
                    self.check_expr(init_expr, declared_ty.as_ref())
                } else {
                    None
                };
                let init_errored = self.errors.len() > errors_before;

                let final_ty = match (declared_ty, init_ty) {
                    (Some(decl), Some(init)) => {
                        // Both declared and initialized: types must match
                        if !self.assignable(&init, &decl) {
                            self.record_type_mismatch(&decl, init, *span);
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

                // A binding whose initializer was already reported is still bound, at
                // `Unknown`, exactly as a parameter whose type failed to resolve is
                // (`declarations/functions.rs`). `Unknown` is compatible with
                // everything, so binding it is what actually stops the cascade;
                // leaving the name undefined turned every later use of it into a
                // second, misleading "undefined variable" report chasing an error
                // already given.
                if init_errored && matches!(final_ty, Type::Unknown) {
                    if let Err(duplicate_name) =
                        self.symbols.define(name.name.clone(), final_ty, *mutable)
                    {
                        self.record_error(TypeError::VariableAlreadyDefined {
                            name: duplicate_name,
                            span: name.span,
                        });
                    }
                    return Some(());
                }

                // A binding needs a value, and `void` is not one. Reaching codegen
                // with it is only answerable there as an internal error, because the
                // value path has no representation for the absence of a value. A
                // diverging initializer (`panic`, `unreachable`) is the same case
                // wearing `Unknown`: it never produces a value to bind either.
                if matches!(final_ty, Type::Void | Type::Unknown) {
                    self.record_error(TypeError::VoidBinding {
                        name: name.name.clone(),
                        span: *span,
                    });
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

            Stmt::Assign {
                place,
                op,
                value,
                span,
            } => self.check_assign(place, *op, value, *span),

            Stmt::Return { value, span } => {
                self.check_pool_return(*span);
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
                let _ = self.check_loop_body(label.as_ref(), false, None, body);

                Some(())
            }

            Stmt::ForRange {
                label,
                index,
                iterator,
                start,
                end,
                inclusive: _,
                reversed: _,
                adapters,
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

                // The adapter chain is checked in the enclosing scope: its functions are
                // evaluated once, before the loop, and cannot see the loop binding.
                let element_ty = match start_ty {
                    Type::Unknown => None,
                    ref known => Some(known.clone()),
                };
                let element_ty = self.check_loop_adapters(element_ty, adapters);

                // As with `while`, body moves are not guaranteed straight-line;
                // restore the move state after the loop. A `for` yields unit,
                // so it is not a value loop.
                let move_snapshot = self.symbols.snapshot_moves();
                self.loop_stack.push(LoopContext {
                    label: label.as_ref().map(|l| l.name.clone()),
                    is_value_loop: false,
                    break_value_ty: None,
                    expected_ty: None,
                    has_break: false,
                });
                self.symbols.push_scope();

                self.define_loop_index(index);
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
                self.report_loop_body_moves(&move_snapshot, body);
                self.symbols.restore_moves(&move_snapshot);

                Some(())
            }

            // `for x in arr` over an array, `Vec`, or borrowed slice. `x` binds each
            // element by value. Lowered as a counted loop.
            Stmt::ForEach {
                label,
                index,
                iterator,
                iterable,
                adapters,
                body,
                span: _,
            } => {
                // `text.char_indices()` is a head form rather than a method: it types as
                // the `Chars` iterator it drives, and its position binding is the byte
                // offset the lowering reads off that iterator between steps.
                let iterable_ty = match super::iteration::char_indices_receiver(iterable) {
                    Some(receiver) => self.check_char_indices_head(receiver, iterable.span()),
                    None => self.check_expr(iterable, None).unwrap_or(Type::Unknown),
                };
                let element_ty = match self.collection_element(&iterable_ty) {
                    Some(element) => Some(element),
                    None => match iterable_ty.referent() {
                        Type::Array { element, .. } | Type::Slice(element) => {
                            // A by-value head hands the loop the array's elements: each
                            // iteration's binding owns the one it holds and destroys it,
                            // which is what codegen already disowns the source for. The
                            // head is therefore a consuming position and the binding
                            // owns nothing after the loop. `for x in &arr` borrows and
                            // moves nothing.
                            if !matches!(iterable_ty, Type::Reference { .. })
                                && self.is_type_move_tracked(element)
                            {
                                self.record_move(iterable);
                            }
                            Some((**element).clone())
                        }
                        Type::Unknown => None,
                        other => match self.iteration_item(other) {
                            // A nominal head is iterable exactly when the protocol says
                            // so; the head is consumed into the loop's iterator, so it
                            // moves like any other by-value placement.
                            Some(item) => {
                                // The protocol is declared on the OWNED type, and the
                                // referent peel above is what makes a borrow of it look
                                // resolvable. Lowering has no path for that head, so
                                // reject it here where the span is still available.
                                if matches!(iterable_ty, Type::Reference { .. }) {
                                    self.record_error(TypeError::BorrowedIterableHead {
                                        found: iterable_ty.clone(),
                                        span: iterable.span(),
                                    });
                                    None
                                } else {
                                    self.record_move(iterable);
                                    Some(item)
                                }
                            }
                            None => {
                                self.record_error(TypeError::NotIterable {
                                    found: other.clone(),
                                    span: iterable.span(),
                                });
                                None
                            }
                        },
                    },
                };

                let element_ty = self.check_loop_adapters(element_ty, adapters);

                // Body moves are not guaranteed straight-line; restore move state after
                // the loop. A `for` yields unit, so it is not a value loop.
                let move_snapshot = self.symbols.snapshot_moves();
                self.loop_stack.push(LoopContext {
                    label: label.as_ref().map(|l| l.name.clone()),
                    is_value_loop: false,
                    break_value_ty: None,
                    expected_ty: None,
                    has_break: false,
                });
                self.symbols.push_scope();

                self.define_loop_index(index);
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
                self.report_loop_body_moves(&move_snapshot, body);
                self.symbols.restore_moves(&move_snapshot);

                Some(())
            }

            Stmt::Break { label, value, span } => {
                self.check_loop_control_label(label.as_ref(), *span, true);
                self.check_pool_loop_jump("break", label.as_ref().map(|l| l.name.as_str()), *span);
                self.record_break_target(label.as_ref());
                if let Some(value_expr) = value {
                    let expected = self.break_target_expected(label.as_ref());
                    let value_ty = self
                        .check_expr(value_expr, expected.as_ref())
                        .unwrap_or(Type::Unknown);
                    if !matches!(value_ty, Type::Unknown) {
                        self.record_break_value(label.as_ref(), value_ty, *span);
                    }
                }
                Some(())
            }

            Stmt::Continue { label, span } => {
                self.check_loop_control_label(label.as_ref(), *span, false);
                self.check_pool_loop_jump(
                    "continue",
                    label.as_ref().map(|l| l.name.as_str()),
                    *span,
                );
                Some(())
            }

            Stmt::ValElse {
                pattern,
                value,
                else_binding,
                else_block,
                span,
            } => self.check_val_else(pattern, value, else_binding.as_ref(), else_block, *span),

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

    /// Check `place = value` and `place OP= value`.
    ///
    /// The place is resolved first: its type decides between the in-place update a
    /// tensor takes and the `place = place OP value` desugaring everything else
    /// takes, and its root binding is what mutability, borrow and pool residency are
    /// all keyed by.
    fn check_assign(
        &mut self,
        place: &Place,
        op: Option<BinaryOp>,
        value: &Expr,
        span: Span,
    ) -> Option<()> {
        let place_ty = self.resolve_place(place, span)?;
        match op {
            // The operator-trait dispatch rule: a type implementing the matching
            // `*Assign` trait updates in place; everything else desugars and allocates
            // a fresh value. Tensors are the one type on the first path today, and the
            // one where the difference is observable: the desugaring would move the
            // tensor out of its own place and reallocate its buffer.
            Some(op) if matches!(place_ty, Type::Tensor { .. }) => {
                self.check_tensor_compound_assign(place, &place_ty, op, value, span)
            }
            Some(op) => {
                let desugared = Expr::Binary {
                    left: Box::new(place.to_expr()),
                    op,
                    right: Box::new(value.clone()),
                    span,
                };
                self.check_place_store(place, &place_ty, &desugared, span)
            }
            None => self.check_place_store(place, &place_ty, value, span),
        }
    }

    /// The type of the storage an assignment writes to, reporting the place's own
    /// errors: an undefined root, an immutable one, a field that does not exist, a
    /// non-indexable base.
    fn resolve_place(&mut self, place: &Place, span: Span) -> Option<Type> {
        match place {
            Place::Var(target) => {
                let Some(symbol) = self.symbols.lookup(&target.name) else {
                    self.record_error(TypeError::UndefinedVariable {
                        name: target.name.clone(),
                        span: target.span,
                    });
                    return None;
                };
                let ty = symbol.ty.clone();
                let mutable = symbol.mutable;
                if !mutable {
                    self.record_error(TypeError::AssignToImmutable {
                        name: target.name.clone(),
                        span: target.span,
                    });
                    return None;
                }
                Some(ty)
            }

            Place::Field {
                object,
                field,
                span: field_span,
            } => {
                let obj_ty = self.check_expr(object, None)?;
                let Type::Struct(struct_name) = obj_ty.referent().clone() else {
                    self.record_error(TypeError::UnknownField {
                        struct_name: obj_ty.to_string(),
                        field_name: field.name.clone(),
                        span: field.span,
                    });
                    return None;
                };
                if !self.place_is_writable(&obj_ty, place) {
                    let root = place.root().map(|r| r.name.clone()).unwrap_or_default();
                    self.record_error(TypeError::AssignToImmutableField {
                        var_name: root,
                        field_name: field.name.clone(),
                        span: *field_span,
                    });
                    return None;
                }
                let field_ty = self
                    .struct_defs
                    .get(&struct_name)
                    .and_then(|def| def.iter().find(|(n, _)| n == &field.name))
                    .map(|(_, t)| t.clone());
                let Some(field_ty) = field_ty else {
                    self.record_error(TypeError::UnknownField {
                        struct_name,
                        field_name: field.name.clone(),
                        span: field.span,
                    });
                    return None;
                };
                self.reject_private_field(&struct_name, &field.name, field.span);
                Some(field_ty)
            }

            Place::Index {
                object,
                index,
                span: index_span,
            } => {
                let obj_ty = self.check_expr(object, None)?;
                if !self.place_is_writable(&obj_ty, place) {
                    self.report_immutable_place(place);
                    return None;
                }
                // A rank-1 tensor is indexed with one argument, which parses as the
                // ordinary index form; the axis rules are the tensor's either way.
                if let Type::Tensor { element, shape } = obj_ty.referent().clone() {
                    let axes = [TensorIndexArg::Position((**index).clone())];
                    return self.resolve_tensor_element(&element, &shape, &axes, *index_span);
                }
                let idx_ty = self.check_expr(index, None).unwrap_or(Type::Unknown);
                if !matches!(idx_ty, Type::Unknown) && !idx_ty.is_integer() {
                    self.record_error(TypeError::IndexNotInteger {
                        found: idx_ty,
                        span: index.span(),
                    });
                }
                if let Some(element) = self.collection_element(&obj_ty) {
                    return Some(element);
                }
                match obj_ty.referent() {
                    Type::Array { element, .. } | Type::Slice(element) => Some((**element).clone()),
                    other => {
                        self.record_error(TypeError::NotIndexable {
                            found: other.clone(),
                            span,
                        });
                        None
                    }
                }
            }

            Place::TensorIndex {
                object,
                indices,
                span: index_span,
            } => {
                let obj_ty = self.check_expr(object, None)?;
                if !self.place_is_writable(&obj_ty, place) {
                    self.report_immutable_place(place);
                    return None;
                }
                let Type::Tensor { element, shape } = obj_ty.referent().clone() else {
                    self.record_error(TypeError::TensorIndexOnNonTensor {
                        found: obj_ty,
                        span: *index_span,
                    });
                    return None;
                };
                self.resolve_tensor_element(&element, &shape, indices, *index_span)
            }

            Place::Deref {
                pointer,
                span: deref_span,
            } => {
                let pointer_ty = self.check_expr(pointer, None).unwrap_or(Type::Unknown);
                match &pointer_ty {
                    Type::Unknown => None,
                    Type::Reference {
                        inner,
                        mutable: true,
                    } => Some((**inner).clone()),
                    Type::Reference {
                        inner,
                        mutable: false,
                    } => {
                        self.record_error(TypeError::CannotAssignThroughRef {
                            inner: (**inner).clone(),
                            span: *deref_span,
                        });
                        None
                    }
                    other => {
                        self.record_error(TypeError::CannotDereference {
                            found: other.clone(),
                            span: pointer.span(),
                        });
                        None
                    }
                }
            }
        }
    }

    /// The element a tensor index names, rejecting an index that leaves an axis
    /// standing: a slice is a fresh tensor, so there is no storage to write into.
    fn resolve_tensor_element(
        &mut self,
        element: &Type,
        shape: &[crate::types::TensorAxis],
        indices: &[TensorIndexArg],
        span: Span,
    ) -> Option<Type> {
        let indexed = self.check_tensor_index(element, shape, indices, span);
        if matches!(indexed, Type::Tensor { .. }) {
            self.record_error(TypeError::AssignToTensorSlice { span });
            return None;
        }
        Some(indexed)
    }

    /// Whether a sub-place may be written through.
    ///
    /// Write permission through a borrow comes from the borrow, not the binding:
    /// `xs: &mut [T]` is an immutable binding holding a mutable view, and a `&[T]`
    /// binding declared `mut` still may not write. Everything else inherits the
    /// mutability of the binding the place is rooted at.
    fn place_is_writable(&self, object_ty: &Type, place: &Place) -> bool {
        if let Type::Reference { mutable, .. } = object_ty {
            return *mutable;
        }
        match place.root() {
            Some(root) => self
                .symbols
                .lookup(&root.name)
                .map(|info| info.mutable)
                .unwrap_or(false),
            None => false,
        }
    }

    fn report_immutable_place(&mut self, place: &Place) {
        let (name, span) = match place.root() {
            Some(root) => (root.name.clone(), root.span),
            None => (String::new(), place.span()),
        };
        self.record_error(TypeError::AssignToImmutable { name, span });
    }

    /// Store `value` into an already-resolved place.
    fn check_place_store(
        &mut self,
        place: &Place,
        place_ty: &Type,
        value: &Expr,
        span: Span,
    ) -> Option<()> {
        if let Place::Var(target) = place {
            return self.check_binding_store(target, place_ty, value, span);
        }

        match place {
            Place::Deref { .. } => self.check_pool_ref_store(place_ty, value, span),
            _ => {
                if let Some(root) = place.root() {
                    let root = root.name.clone();
                    self.check_pool_store(&root, place_ty, value, span);
                }
            }
        }

        let value_ty = self
            .check_expr(value, Some(place_ty))
            .unwrap_or(Type::Unknown);
        // Storing the value into the place moves it out of its source.
        self.record_move(value);
        if !matches!(value_ty, Type::Unknown) && !value_ty.is_compatible_with(place_ty) {
            self.record_error(TypeError::Mismatch {
                expected: place_ty.clone(),
                found: value_ty,
                span,
            });
        }
        Some(())
    }

    /// Store into a whole binding: the one place form that also replaces what the
    /// binding held, so borrow, move and mutability state on the name itself change.
    fn check_binding_store(
        &mut self,
        target: &Identifier,
        expected_ty: &Type,
        value: &Expr,
        span: Span,
    ) -> Option<()> {
        let expected_ty = Some(expected_ty.clone());

        // Replacing the value destroys what every live borrow of the target points at,
        // so the borrowee rules apply to the write as much as to a read or a move.
        // Tested before the RHS is checked: a `&target` appearing in the RHS is a borrow
        // this assignment does not conflict with.
        if let Some((shared, exclusive)) = self.symbols.borrow_counts(&target.name) {
            if shared > 0 || exclusive > 0 {
                self.record_error(TypeError::CannotAssignWhileBorrowed {
                    name: target.name.clone(),
                    span: target.span,
                });
            }
        }

        // If the target was a reference binding, its previous borrow ends
        // here: release it before the new value is checked so that
        // re-borrowing the same place (`r = &mut x`) is not a false
        // conflict against the borrow being overwritten.
        self.symbols.release_borrow_of(&target.name);

        let value_ty = self
            .check_expr(value, expected_ty.as_ref())
            .unwrap_or(Type::Unknown);

        // The RHS is moved into the target, and the target now owns a
        // fresh value, clearing any prior moved-out state on it.
        self.record_move(value);
        self.symbols.clear_moved(&target.name);

        self.check_pool_store(&target.name, &value_ty, value, span);

        // A direct `&place` / `&mut place` RHS makes the target hold a new
        // persistent borrow of that place.
        if let Some((place, exclusive)) = borrow_target_of(value) {
            self.symbols.attach_borrow(&target.name, &place, exclusive);
        }

        let symbol_info = self.symbols.lookup(&target.name)?;
        if !matches!(value_ty, Type::Unknown) && !value_ty.is_compatible_with(&symbol_info.ty) {
            self.record_error(TypeError::Mismatch {
                expected: symbol_info.ty.clone(),
                found: value_ty,
                span,
            });
        }
        Some(())
    }

    /// Define an enumerated loop's position binding in the already-pushed loop
    /// scope. It is `u64` whatever the sequence holds, matching `.len()` and the
    /// index expression, so `xs[i]` works on the binding directly.
    fn define_loop_index(&mut self, index: &Option<Identifier>) {
        let Some(index) = index else {
            return;
        };
        if let Err(duplicate_name) = self.symbols.define(index.name.clone(), Type::U64, false) {
            self.record_error(TypeError::VariableAlreadyDefined {
                name: duplicate_name,
                span: index.span,
            });
        }
    }
}
