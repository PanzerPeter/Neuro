//! Loop control: the loop stack, `break` / `continue` targets and labels, value-carrying
//! `break`s, and the move state each exit settles when a loop body closes.

use ast_types::Stmt;
use shared_types::Identifier;

use crate::errors::TypeError;
use crate::symbol_table::MoveState;
use crate::type_checkers::{LoopContext, TypeChecker};
use crate::types::Type;

/// How a checked loop body is left: the agreed type of its value-carrying
/// `break`s (`None` when it has none), and whether any `break` targeted it at all.
pub(crate) struct LoopExit {
    pub(crate) value_ty: Option<Type>,
    pub(crate) has_break: bool,
}

impl TypeChecker {
    /// Validate a `break` / `continue` against the active loop stack.
    ///
    /// An unlabeled control statement requires any enclosing loop; a labeled one
    /// requires an enclosing loop carrying that exact label. `is_break`
    /// distinguishes the two error codes for an out-of-loop unlabeled statement.
    pub(super) fn check_loop_control_label(
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
            break_moves: Vec::new(),
            continue_moves: Vec::new(),
        });
        self.symbols.push_scope();
        for stmt in body {
            let _ = self.check_stmt(stmt);
        }
        self.symbols.pop_scope();
        let ctx = self.loop_stack.pop();
        self.close_loop_moves(&move_snapshot, body, ctx.as_ref());
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
    pub(super) fn record_break_target(&mut self, label: Option<&Identifier>) {
        if let Some(ctx) = self.loop_target_mut(label) {
            ctx.has_break = true;
        }
    }

    /// The loop a `break` or `continue` with `label` targets: the one wearing the
    /// label, or the innermost loop when unlabeled.
    pub(super) fn loop_target_mut(
        &mut self,
        label: Option<&Identifier>,
    ) -> Option<&mut LoopContext> {
        match label {
            Some(label) => self
                .loop_stack
                .iter_mut()
                .rev()
                .find(|ctx| ctx.label.as_deref() == Some(label.name.as_str())),
            None => self.loop_stack.last_mut(),
        }
    }

    /// Keep the move state at a `break` (`leaves`) or a `continue` for the loop it
    /// targets to settle when the loop closes: see [`Self::close_loop_moves`].
    pub(super) fn record_jump_moves(&mut self, label: Option<&Identifier>, leaves: bool) {
        let state = self.symbols.snapshot_moves();
        if let Some(ctx) = self.loop_target_mut(label) {
            if leaves {
                ctx.break_moves.push(state);
            } else {
                ctx.continue_moves.push(state);
            }
        }
    }

    /// Settle a loop's moves once its body is checked, with the scope stack the
    /// snapshot was taken on. The body's end and every `continue` start another
    /// iteration, so a move still outstanding at any of them is reported. Past the
    /// loop the state is the one before it (a `while` or `for` may run zero times),
    /// joined with what every `break` had moved.
    pub(super) fn close_loop_moves(
        &mut self,
        snapshot: &[MoveState],
        body: &[Stmt],
        ctx: Option<&LoopContext>,
    ) {
        self.report_loop_body_moves(snapshot, body);
        if let Some(ctx) = ctx {
            let mut reported = self.symbols.moves_since(snapshot);
            for state in &ctx.continue_moves {
                self.symbols.restore_moves(state);
                for (name, span) in self.symbols.moves_since(snapshot) {
                    if !reported.iter().any(|(n, s)| *n == name && *s == span) {
                        self.record_error(TypeError::MovedInLoopBody {
                            name: name.clone(),
                            span,
                        });
                        reported.push((name, span));
                    }
                }
            }
        }
        self.symbols.restore_moves(snapshot);
        if let Some(ctx) = ctx {
            self.symbols.join_moves(&ctx.break_moves);
        }
    }

    /// The expected type of the loop a `break` targets, so `break v` checks `v`
    /// against the same annotation the loop expression is checked against. Without
    /// it a literal in a value loop is typed on its own and then fails to match an
    /// annotation an `if` arm or a block tail in the same position would satisfy.
    pub(super) fn break_target_expected(&self, label: Option<&Identifier>) -> Option<Type> {
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
    pub(super) fn record_break_value(
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

    /// Define an enumerated loop's position binding in the already-pushed loop
    /// scope. It is `u64` whatever the sequence holds, matching `.len()` and the
    /// index expression, so `xs[i]` works on the binding directly.
    pub(super) fn define_loop_index(&mut self, index: &Option<Identifier>) {
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
