use ast_types::{BinaryOp, Expr, Stmt};

use crate::errors::TypeError;
use crate::symbol_table::pending_key;
use crate::types::Type;

use super::backward::gradient_view_root;
use super::val_else::{expr_diverges, stmts_diverge};
use super::{LoopContext, TypeChecker};

mod loops;
mod places;
mod returns;
pub(crate) use returns::tail_expr;

fn peel_parens(expr: &Expr) -> &Expr {
    match expr {
        Expr::Paren(inner, _) => peel_parens(inner),
        other => other,
    }
}

/// The span a scope-bound function value may occupy when `value` is checked whole: its
/// own, if it is a closure literal or a binding, through any parentheses. Those are the
/// two shapes that hand the value on intact rather than storing it inside another.
pub(crate) fn function_value_position(value: &Expr) -> Option<shared_types::Span> {
    match peel_parens(value) {
        inner @ (Expr::Closure { .. } | Expr::Identifier(_)) => Some(inner.span()),
        _ => None,
    }
}

/// The span of the borrow a call argument is, when it is one: the position a borrow of a
/// temporary may take.
pub(crate) fn temporary_borrow_position(arg: &Expr) -> Option<shared_types::Span> {
    match peel_parens(arg) {
        borrow @ Expr::Reference { .. } => Some(borrow.span()),
        _ => None,
    }
}

/// The span of the closure literal a block yields as its last expression, through nested
/// blocks: a binding's initializer may end in a closure that only copies references, whose
/// borrows `hold_carried_borrows` follows out of the block.
pub(crate) fn block_tail_closure(value: &Expr) -> Option<shared_types::Span> {
    match peel_parens(value) {
        Expr::Block { stmts, .. } => match stmts.last()? {
            Stmt::Expr(tail) => match peel_parens(tail) {
                closure @ Expr::Closure { .. } => Some(closure.span()),
                nested => block_tail_closure(nested),
            },
            _ => None,
        },
        _ => None,
    }
}

/// If `expr` is a direct borrow of a named place (`&x` / `&mut x`, a borrow of a field or
/// an element rooted in `x`, or a `x.slice(range)` view into `x`), return the root
/// binding's name and whether the borrow is exclusive. A borrow of a part of `x` borrows
/// `x`, as moving out of a part of it moves it. A borrow wrapped in any other expression
/// (a block, an `if`, a call result) is not tracked as persistent: only a direct
/// initializer creates a held borrow, which keeps the analysis free of false positives
/// at the cost of missing some borrows that escape through compound expressions.
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
        Expr::FieldAccess { .. } | Expr::Index { .. } | Expr::TupleIndex { .. } => {
            TypeChecker::place_root_name(place).map(|root| (root, *mutable))
        }
        _ => None,
    }
}

/// The borrows a block, `if` or `match` initializer may yield, found through each tail
/// that produces its value. A tail naming a binding the block declared is followed to that
/// binding's initializer. Without this the borrow ended with the block that took it, while
/// the value carrying it lived on in the binding, and its borrowee could be freed under it.
///
/// A name the walk cannot follow to a declaration, and every name a closure tail reads,
/// lands in `live`: the value copies whatever that binding holds, a reference or a closure
/// over one, so its borrows are the caller's to inherit.
fn tail_borrow_targets<'a>(
    expr: &'a Expr,
    blocks: &mut Vec<&'a [Stmt]>,
    followed: &mut Vec<&'a str>,
    out: &mut Vec<(String, bool)>,
    live: &mut Vec<String>,
) {
    let mut expr = expr;
    while let Expr::Paren(inner, _) = expr {
        expr = inner;
    }
    if let Some(target) = borrow_target_of(expr) {
        out.push(target);
        return;
    }
    match expr {
        Expr::Block { stmts, .. } => block_tail_borrow_targets(stmts, blocks, followed, out, live),
        Expr::If {
            then_block,
            else_if_blocks,
            else_block,
            ..
        } => {
            let branches = std::iter::once(then_block)
                .chain(else_if_blocks.iter().map(|(_, block)| block))
                .chain(else_block.iter());
            for block in branches {
                block_tail_borrow_targets(block, blocks, followed, out, live);
            }
        }
        // An arm may yield what its pattern bound, a part of the scrutinee.
        Expr::Match {
            scrutinee, arms, ..
        } => {
            tail_borrow_targets(scrutinee, blocks, followed, out, live);
            for arm in arms {
                tail_borrow_targets(&arm.body, blocks, followed, out, live);
            }
        }
        // A part of a value carries that value's borrows: a destructuring `val (a, b) = e`
        // reaches its parts through a generated binding and `.0` / `.1`.
        Expr::TupleIndex { object, .. }
        | Expr::FieldAccess { object, .. }
        | Expr::Index { object, .. } => {
            tail_borrow_targets(object, blocks, followed, out, live);
        }
        Expr::Try { operand, .. } => tail_borrow_targets(operand, blocks, followed, out, live),
        Expr::Binary {
            left,
            op: BinaryOp::NullCoalesce,
            right,
            ..
        } => {
            tail_borrow_targets(left, blocks, followed, out, live);
            tail_borrow_targets(right, blocks, followed, out, live);
        }
        // A value built around a borrow holds it: `H { r: &s }`, `(&s, 1)`, `[&s]`.
        Expr::TupleLiteral { elements, .. } | Expr::ArrayLiteral { elements, .. } => {
            for element in elements {
                tail_borrow_targets(element, blocks, followed, out, live);
            }
        }
        Expr::StructLiteral { fields, base, .. } => {
            for field in fields {
                tail_borrow_targets(&field.value, blocks, followed, out, live);
            }
            if let Some(base) = base {
                tail_borrow_targets(base, blocks, followed, out, live);
            }
        }
        Expr::EnumStructLiteral { fields, .. } => {
            for field in fields {
                tail_borrow_targets(&field.value, blocks, followed, out, live);
            }
        }
        // A call's result may hold any borrow its arguments hand it (lifetime elision),
        // `Some(&s)` and `wrap(&s)` alike, and a method's may hold its receiver's: both a
        // borrow of the receiver and whatever the receiver itself holds.
        Expr::Call { func, args, .. } => {
            for arg in args {
                tail_borrow_targets(arg, blocks, followed, out, live);
            }
            if let Expr::FieldAccess { object, .. } = func.as_ref()
                && let Some(root) = TypeChecker::place_root_name(object)
            {
                out.push((root.clone(), false));
                live.push(root);
            }
        }
        Expr::Identifier(id) => follow_binding(&id.name, blocks, followed, out, live),
        // A closure carries the borrows of what it captures.
        Expr::Closure { params, body, .. } => {
            for name in super::closures::closure_reads(params, body) {
                follow_binding(&name, blocks, followed, out, live);
            }
        }
        _ => {}
    }
}

/// Follow a name a tail yields to the block declaration that bound it, or hand it to
/// `live` when no walked block declares it.
fn follow_binding<'a>(
    name: &str,
    blocks: &mut Vec<&'a [Stmt]>,
    followed: &mut Vec<&'a str>,
    out: &mut Vec<(String, bool)>,
    live: &mut Vec<String>,
) {
    if followed.contains(&name) {
        return;
    }
    let decl = blocks.iter().rev().copied().find_map(|stmts: &'a [Stmt]| {
        stmts.iter().rev().find_map(|stmt| match stmt {
            Stmt::VarDecl {
                name: declared,
                init: Some(init),
                ..
            } if declared.name == name => Some((declared.name.as_str(), init)),
            _ => None,
        })
    });
    match decl {
        Some((declared, init)) => {
            followed.push(declared);
            tail_borrow_targets(init, blocks, followed, out, live);
        }
        None => live.push(name.to_string()),
    }
}

fn block_tail_borrow_targets<'a>(
    stmts: &'a [Stmt],
    blocks: &mut Vec<&'a [Stmt]>,
    followed: &mut Vec<&'a str>,
    out: &mut Vec<(String, bool)>,
    live: &mut Vec<String>,
) {
    blocks.push(stmts);
    if let Some(Stmt::Expr(tail)) = stmts.last() {
        tail_borrow_targets(tail, blocks, followed, out, live);
    }
    blocks.pop();
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
    /// Make `holder` hold the borrows `value`, of type `ty`, carries into it: a struct,
    /// enum, tuple or array built around a borrow, a call's result, a block, `if` or
    /// `match` value, a copied binding, or a closure. Each is a way for a borrow to
    /// outlive the binding or the statement that took it: the block's binding dies with
    /// the block, and an inner reference copied (or captured) into an outer binding dies
    /// before it.
    pub(crate) fn hold_carried_borrows(&mut self, holder: &str, value: &Expr, ty: &Type) {
        if !self.type_holds_reference(ty) && !self.holds_function_value(ty) {
            return;
        }
        let value = peel_parens(value);
        if let Expr::Closure { span, .. } = value
            && let Some(borrows) = self.scope_bound_closures.get(span).cloned()
        {
            for (place, exclusive) in borrows {
                self.symbols.attach_borrow(holder, &place, exclusive);
            }
        }
        let (mut targets, mut live) = (Vec::new(), Vec::new());
        match value {
            // `hold_returned_borrows` takes the receiver of a call returning a
            // reference, by how the method takes it.
            Expr::Call { args, .. } if matches!(ty, Type::Reference { .. }) => {
                for arg in args {
                    tail_borrow_targets(
                        arg,
                        &mut Vec::new(),
                        &mut Vec::new(),
                        &mut targets,
                        &mut live,
                    );
                }
            }
            _ => tail_borrow_targets(
                value,
                &mut Vec::new(),
                &mut Vec::new(),
                &mut targets,
                &mut live,
            ),
        }
        for (place, exclusive) in targets {
            self.symbols.attach_borrow(holder, &place, exclusive);
        }
        for name in live {
            self.symbols.inherit_borrows(holder, &name);
        }
    }

    /// Whether `value` yields a value that may borrow: a scope-bound closure literal, a
    /// block ending in one, or a binding already holding one.
    pub(crate) fn yields_scope_bound_closure(&self, value: &Expr) -> bool {
        if let Some(span) = block_tail_closure(value) {
            return self.scope_bound_closures.contains_key(&span);
        }
        match peel_parens(value) {
            Expr::Closure { span, .. } => self.scope_bound_closures.contains_key(span),
            Expr::Identifier(ident) => self
                .symbols
                .lookup(&ident.name)
                .is_some_and(|info| info.scope_bound),
            _ => false,
        }
    }

    /// Make `holder`, a binding of reference type `ty` that `init` just initialized,
    /// hold a borrow of the receiver a method call's returned reference may come from.
    ///
    /// The returned reference borrows one of the call's borrowed inputs (lifetime
    /// elision): `hold_carried_borrows` takes the arguments, and this the receiver, as
    /// shared or exclusive as the method takes it. Without this the borrow reached the
    /// binding attached to nothing, and the borrowee rules let the source be moved or
    /// freed while the reference still read it.
    pub(crate) fn hold_returned_borrows(&mut self, holder: &str, init: &Expr, ty: &Type) {
        let Expr::Call { func, .. } = peel_parens(init) else {
            return;
        };
        let Expr::FieldAccess { object, field, .. } = func.as_ref() else {
            return;
        };
        // `m.get_mut(k)` hands out a `&mut` inside its `Option`, so the receiver is held
        // exclusively, not by the shared borrow every method result carries.
        if self.collection_lends_mutably(object, &field.name)
            && let Some(root) = Self::place_root_name(object)
        {
            self.symbols.attach_borrow(holder, &root, true);
            return;
        }
        if !matches!(ty, Type::Reference { .. }) {
            return;
        }
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

    /// The binding type of a counted loop over an array, slice or `Vec` of `element`.
    ///
    /// The head's receiver mode is the binding's: a by-value head hands the loop
    /// the elements, so it is a consuming position and owns nothing after the loop (a
    /// `Vec` of `Copy` elements included; an array of them is itself `Copy` and is
    /// copied). A borrowed head moves nothing and binds `&T`, or `&mut T` through a
    /// mutable borrow, for every element type, so a generic body and its instances
    /// agree on what the loop binds.
    fn counted_loop_item(&mut self, iterable: &Expr, iterable_ty: &Type, element: Type) -> Type {
        match iterable_ty {
            Type::Reference { mutable, .. } => Type::Reference {
                inner: Box::new(element),
                mutable: *mutable,
            },
            _ => {
                if self.is_type_move_tracked(iterable_ty) {
                    self.record_move(iterable);
                }
                element
            }
        }
    }

    /// What one step of a `for` over the nominal type `head` binds, through the
    /// `IntoIterator` / `Iterator` protocol.
    ///
    /// A by-value head is consumed into the loop's iterator. `&mut it` over an `Iterator`
    /// drives `it` in place, as `next(&mut self)` already would, and leaves it usable
    /// after the loop. Any other borrow has no protocol impl to resolve against: one on
    /// `&Items` has no syntax yet.
    fn protocol_loop_item(
        &mut self,
        iterable: &Expr,
        iterable_ty: &Type,
        head: &Type,
    ) -> Option<Type> {
        let item = match iterable_ty {
            Type::Reference { mutable: true, .. } => self.iterator_item(head),
            Type::Reference { .. } => None,
            _ => self.iteration_item(head),
        };
        if let Some(item) = item {
            if !matches!(iterable_ty, Type::Reference { .. }) {
                self.record_move(iterable);
            }
            return Some(item);
        }
        // The referent peel is what makes a borrow of a protocol type look resolvable,
        // so a borrowed head that misses names the borrow rather than the type.
        let error = match iterable_ty {
            Type::Reference { .. } if self.iteration_item(head).is_some() => {
                TypeError::BorrowedIterableHead {
                    found: iterable_ty.clone(),
                    span: iterable.span(),
                }
            }
            _ => TypeError::NotIterable {
                found: head.clone(),
                span: iterable.span(),
            },
        };
        self.record_error(error);
        None
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
                if self.in_kernel
                    && self.check_grid_binding(name, init.as_ref(), (*mutable, ty.is_some()))
                {
                    return Some(());
                }
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
                    self.function_value_slot = function_value_position(init_expr);
                    // The block's own bindings are checked inside it, so the outer slot is
                    // put back rather than cleared.
                    let outer_tail =
                        std::mem::replace(&mut self.block_tail_slot, block_tail_closure(init_expr));
                    let init_ty = self.check_expr(init_expr, declared_ty.as_ref());
                    self.function_value_slot = None;
                    self.block_tail_slot = outer_tail;
                    init_ty
                } else {
                    None
                };
                let init_errored = self.errors.len() > errors_before;
                // An annotation that failed to resolve was reported already. The
                // `::new()` it was written to type then has nothing to infer from and
                // would ask for an annotation the binding visibly has.
                if ty.is_some() && declared_ty.is_none() {
                    let reported = self.errors.split_off(errors_before);
                    self.errors.extend(reported.into_iter().filter(|error| {
                        !matches!(error, TypeError::CollectionTypeNotInferable { .. })
                    }));
                }

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
                //
                // In a program already refused, an `Unknown` that no divergence explains
                // came from a value an earlier error left at `Unknown` (`val c = b.sum()`
                // after `b`'s initializer failed), so it is bound the same way.
                let poisoned = !self.errors.is_empty()
                    && init
                        .as_ref()
                        .is_some_and(|init_expr| !expr_diverges(init_expr));
                if (init_errored || poisoned) && matches!(final_ty, Type::Unknown) {
                    self.symbols.define_pending(&name.name, final_ty, *mutable);
                    self.symbols.commit_pending(&name.name);
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
                let holder = pending_key(&name.name);
                self.symbols.define_pending(&name.name, final_ty, *mutable);

                // Binding the initializer moves it out of its source.
                if let Some(init_expr) = init {
                    self.record_move(init_expr);

                    // A direct `&place` / `&mut place` initializer makes this
                    // binding hold a persistent borrow of that place, live until
                    // the binding leaves scope.
                    if let Some((place, exclusive)) = borrow_target_of(init_expr) {
                        self.symbols.attach_borrow(&holder, &place, exclusive);
                        // A `mut` reference may be pointed elsewhere later.
                        if exclusive && !*mutable {
                            self.note_block_referent(&holder, &place);
                        }
                    } else if let Some(ty) = self.symbols.lookup(&holder).map(|s| s.ty.clone()) {
                        self.hold_carried_borrows(&holder, init_expr, &ty);
                    }
                    if let Some(place) = view_root {
                        self.symbols.attach_borrow(&holder, &place, false);
                    }
                    if let Some(ty) = self.symbols.lookup(&holder).map(|s| s.ty.clone()) {
                        self.hold_returned_borrows(&holder, init_expr, &ty);
                    }
                    if self.yields_scope_bound_closure(init_expr) {
                        self.symbols.mark_scope_bound(&holder);
                    }
                    // A `mut` loss could be reassigned, and the `.backward()` would then
                    // run the derivative of a call its value no longer came from.
                    if !*mutable {
                        self.hold_grad_call_borrows(&holder, &name.name, init_expr);
                    }
                }
                self.symbols.commit_pending(&name.name);

                Some(())
            }

            Stmt::Assign {
                place,
                op,
                value,
                span,
            } => self.check_assign(place, *op, value, *span),

            Stmt::Return { value, span } => {
                if self.inferring_closure_return && self.current_function_return_type.is_none() {
                    self.record_error(TypeError::ClosureBlockNeedsReturnType { span: *span });
                }
                // Cloned to release the borrow on `self` before `check_expr`.
                let expected_return = self.current_function_return_type.clone();
                let return_ty = if let Some(expr) = value {
                    self.check_expr(expr, expected_return.as_ref())
                        .unwrap_or(Type::Unknown)
                } else {
                    Type::Void
                };
                self.check_pool_return("return", &return_ty, *span);

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
                    if expected_return
                        .as_ref()
                        .is_some_and(|ty| self.type_holds_reference(ty))
                    {
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
                    // Every path ran one arm, so the arms alone say what is moved.
                    self.symbols.settle_moves(&fell_through);
                } else {
                    self.symbols.join_moves(&fell_through);
                }

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
                // An unsuffixed literal start (`0..v.len()`) takes the end's type, so the
                // end is checked first; otherwise the start leads, as before.
                let (start_ty, end_ty) = if unsuffixed_integer(start) && !unsuffixed_integer(end) {
                    let end_ty = self.check_expr(end, None).unwrap_or(Type::Unknown);
                    let start_ty = self
                        .check_expr(start, Some(&end_ty))
                        .unwrap_or(Type::Unknown);
                    (start_ty, end_ty)
                } else {
                    let start_ty = self.check_expr(start, None).unwrap_or(Type::Unknown);
                    let end_ty = self
                        .check_expr(end, Some(&start_ty))
                        .unwrap_or(Type::Unknown);
                    (start_ty, end_ty)
                };
                if !matches!(start_ty, Type::Unknown) && !start_ty.is_integer() {
                    self.record_error(TypeError::InvalidForRangeType {
                        found: start_ty.clone(),
                        span: start.span(),
                    });
                }

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
                self.hold_adapter_borrows(adapters);

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
                    Some(element) => Some(self.counted_loop_item(iterable, &iterable_ty, element)),
                    None => match iterable_ty.referent() {
                        Type::Array { element, .. } | Type::Slice(element) => Some(
                            self.counted_loop_item(iterable, &iterable_ty, (**element).clone()),
                        ),
                        Type::Unknown => None,
                        other => {
                            let other = other.clone();
                            self.protocol_loop_item(iterable, &iterable_ty, &other)
                        }
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
                self.hold_adapter_borrows(adapters);

                self.define_loop_index(index);
                if let Some(element_ty) = element_ty {
                    match self
                        .symbols
                        .define(iterator.name.clone(), element_ty.clone(), false)
                    {
                        // Each element is a part of the iterable, so it holds what the
                        // iterable borrows: `&v` itself, or what a `[&T; N]` was built from.
                        // A borrowed head is held for the whole loop even when what a step
                        // yields holds nothing (`&mut it` over an iterator of `i32`): the
                        // loop goes on reading the head between steps.
                        Ok(()) => {
                            let held = match iterable_ty {
                                Type::Reference { .. } => &iterable_ty,
                                _ => &element_ty,
                            };
                            self.hold_carried_borrows(&iterator.name, iterable, held);
                        }
                        Err(duplicate_name) => {
                            self.record_error(TypeError::VariableAlreadyDefined {
                                name: duplicate_name,
                                span: iterator.span,
                            })
                        }
                    }
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
                self.record_break_target(label.as_ref());
                if let Some(value_expr) = value {
                    let expected = self.break_target_expected(label.as_ref());
                    let value_ty = self
                        .check_expr(value_expr, expected.as_ref())
                        .unwrap_or(Type::Unknown);
                    self.check_pool_loop_jump(
                        "break",
                        label.as_ref().map(|l| l.name.as_str()),
                        &value_ty,
                        *span,
                    );
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

/// An integer literal with no suffix, whose type comes from its context.
fn unsuffixed_integer(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Literal(shared_types::Literal::Integer(_, None), _)
    )
}
