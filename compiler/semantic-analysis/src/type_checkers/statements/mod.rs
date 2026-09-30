use ast_types::{Expr, Stmt};

use crate::errors::TypeError;
use crate::types::Type;

use super::backward::gradient_view_root;
use super::val_else::stmts_diverge;
use super::{LoopContext, TypeChecker};

mod loops;
mod places;
mod returns;

/// If `expr` is a direct borrow of a named place (`&x` / `&mut x`, or a
/// `x.slice(range)` view into `x`), return that place's name and whether the borrow is
/// exclusive. A borrow wrapped in any other expression (a block, an `if`, a call result)
/// is not tracked as persistent: only a direct initializer creates a held borrow,
/// which keeps the analysis free of false positives at the cost of missing some
/// borrows that escape through compound expressions.
pub(crate) fn borrow_target_of(expr: &Expr) -> Option<(String, bool)> {
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

impl TypeChecker {
    /// Make `holder`, a binding of reference type `ty` that `init` just initialized,
    /// hold the borrows a call's returned reference may come from.
    ///
    /// The returned reference borrows one of the call's borrowed inputs (lifetime elision),
    /// and a body may return any of its reference parameters, so each `&place` /
    /// `&mut place` argument is a candidate, and so is a borrowed receiver. Without this
    /// the borrow reached the binding attached to nothing, and the borrowee rules let the
    /// source be moved or freed while the reference still read it.
    pub(crate) fn hold_returned_borrows(&mut self, holder: &str, init: &Expr, ty: &Type) {
        if !matches!(ty, Type::Reference { .. }) {
            return;
        }
        let mut call = init;
        while let Expr::Paren(inner, _) = call {
            call = inner;
        }
        let Expr::Call { func, args, .. } = call else {
            return;
        };
        for arg in args {
            if let Some((place, exclusive)) = borrow_target_of(arg) {
                self.symbols.attach_borrow(holder, &place, exclusive);
            }
        }
        let Expr::FieldAccess { object, .. } = func.as_ref() else {
            return;
        };
        // A consuming receiver is gone once the call returns, so nothing borrows it.
        let (Some(key), Some(root)) = (self.callee_key(func), Self::place_root_name(object)) else {
            return;
        };
        if self.consuming_self_methods.contains(&key) {
            return;
        }
        let mutable = self.mut_self_methods.contains(&key);
        let root_is_reference = matches!(
            self.symbols.lookup(&root).map(|symbol| &symbol.ty),
            Some(Type::Reference { .. })
        );
        match (mutable, root_is_reference) {
            (true, true) => self.symbols.hold_reborrow(holder, &root),
            (_, false) => self.symbols.attach_borrow(holder, &root, mutable),
            // A shared borrow of a reference binding borrows nothing the frame owns.
            (false, true) => {}
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
                    // An initializer whose error was already reported binds at `Unknown`
                    // below, the same as one that came back `Unknown`.
                    (None, None) if init_errored => Type::Unknown,
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

                let view_root = init
                    .as_ref()
                    .and_then(|init_expr| gradient_view_root(init_expr, &final_ty));
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
                    if let Some(place) = view_root {
                        self.symbols.attach_borrow(&name.name, &place, false);
                    }
                    if let Some(ty) = self.symbols.lookup(&name.name).map(|s| s.ty.clone()) {
                        self.hold_returned_borrows(&name.name, init_expr, &ty);
                    }
                    // A `mut` loss could be reassigned, and the `.backward()` would then
                    // run the derivative of a call its value no longer came from.
                    if !*mutable {
                        self.hold_grad_call_borrows(&name.name, init_expr);
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
                if let Some(expected) = self.current_function_return_type.clone()
                    && !matches!(return_ty, Type::Unknown)
                    && !self.assignable(&return_ty, &expected)
                {
                    self.record_error(TypeError::ReturnTypeMismatch {
                        expected,
                        found: return_ty,
                        span: *span,
                    });
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
                if let Some(cond_ty) = self.check_expr(condition, Some(&Type::Bool))
                    && !matches!(cond_ty, Type::Unknown)
                    && !cond_ty.is_bool()
                {
                    self.record_error(TypeError::Mismatch {
                        expected: Type::Bool,
                        found: cond_ty,
                        span: condition.span(),
                    });
                }

                // A move inside one arm must not invalidate the binding in a sibling
                // arm that never ran it, so the state is restored between arms. Past
                // the `if`, a move any arm that falls through made may have happened,
                // so those arms' moves are joined back in at the end.
                let move_snapshot = self.symbols.snapshot_moves();
                let mut fell_through = Vec::new();

                self.symbols.push_scope();
                for stmt in then_block {
                    let _ = self.check_stmt(stmt);
                }
                self.symbols.pop_scope();
                if !stmts_diverge(then_block) {
                    fell_through.push(self.symbols.snapshot_moves());
                }
                self.symbols.restore_moves(&move_snapshot);

                for (else_if_cond, else_if_stmts) in else_if_blocks {
                    if let Some(cond_ty) = self.check_expr(else_if_cond, Some(&Type::Bool))
                        && !matches!(cond_ty, Type::Unknown)
                        && !cond_ty.is_bool()
                    {
                        self.record_error(TypeError::Mismatch {
                            expected: Type::Bool,
                            found: cond_ty,
                            span: else_if_cond.span(),
                        });
                    }

                    self.symbols.push_scope();
                    for stmt in else_if_stmts {
                        let _ = self.check_stmt(stmt);
                    }
                    self.symbols.pop_scope();
                    if !stmts_diverge(else_if_stmts) {
                        fell_through.push(self.symbols.snapshot_moves());
                    }
                    self.symbols.restore_moves(&move_snapshot);
                }

                if let Some(else_stmts) = else_block {
                    self.symbols.push_scope();
                    for stmt in else_stmts {
                        let _ = self.check_stmt(stmt);
                    }
                    self.symbols.pop_scope();
                    if !stmts_diverge(else_stmts) {
                        fell_through.push(self.symbols.snapshot_moves());
                    }
                    self.symbols.restore_moves(&move_snapshot);
                }
                self.symbols.join_moves(&fell_through);

                Some(())
            }

            Stmt::While {
                label,
                condition,
                body,
                span: _,
            } => {
                if let Some(cond_ty) = self.check_expr(condition, Some(&Type::Bool))
                    && !matches!(cond_ty, Type::Unknown)
                    && !cond_ty.is_bool()
                {
                    self.record_error(TypeError::Mismatch {
                        expected: Type::Bool,
                        found: cond_ty,
                        span: condition.span(),
                    });
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
                step,
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
                if let Some(step) = step {
                    self.check_range_step(step, &start_ty);
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
                    break_moves: Vec::new(),
                    continue_moves: Vec::new(),
                });
                self.symbols.push_scope();

                self.define_loop_index(index);
                if let Some(element_ty) = element_ty
                    && let Err(duplicate_name) =
                        self.symbols
                            .define(iterator.name.clone(), element_ty, false)
                {
                    self.record_error(TypeError::VariableAlreadyDefined {
                        name: duplicate_name,
                        span: iterator.span,
                    });
                }

                for stmt in body {
                    let _ = self.check_stmt(stmt);
                }

                self.symbols.pop_scope();
                let ctx = self.loop_stack.pop();
                self.close_loop_moves(&move_snapshot, body, ctx.as_ref());

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
                    break_moves: Vec::new(),
                    continue_moves: Vec::new(),
                });
                self.symbols.push_scope();

                self.define_loop_index(index);
                if let Some(element_ty) = element_ty
                    && let Err(duplicate_name) =
                        self.symbols
                            .define(iterator.name.clone(), element_ty, false)
                {
                    self.record_error(TypeError::VariableAlreadyDefined {
                        name: duplicate_name,
                        span: iterator.span,
                    });
                }

                for stmt in body {
                    let _ = self.check_stmt(stmt);
                }

                self.symbols.pop_scope();
                let ctx = self.loop_stack.pop();
                self.close_loop_moves(&move_snapshot, body, ctx.as_ref());

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
                    self.record_move(value_expr);
                }
                self.record_jump_moves(label.as_ref(), true);
                Some(())
            }

            Stmt::Continue { label, span } => {
                self.check_loop_control_label(label.as_ref(), *span, false);
                self.check_pool_loop_jump(
                    "continue",
                    label.as_ref().map(|l| l.name.as_str()),
                    *span,
                );
                self.record_jump_moves(label.as_ref(), false);
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

                if let Some(expr_ty) = self.check_expr(value, Some(&declared_ty))
                    && !expr_ty.is_compatible_with(&declared_ty)
                {
                    self.record_error(TypeError::Mismatch {
                        expected: declared_ty.clone(),
                        found: expr_ty,
                        span: *span,
                    });
                    return None;
                }
                self.check_const_value(value, &declared_ty);

                self.constants.insert(name.name.clone(), declared_ty);
                self.constant_values
                    .insert(name.name.clone(), value.clone());
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
