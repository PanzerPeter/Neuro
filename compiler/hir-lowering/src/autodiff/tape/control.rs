//! Statements and control flow on the tape: a statement list, an `if` chain forked into
//! one tape per arm, and `while` / `for` loops with the slots their bodies reassign.

use std::collections::HashMap;

use ast_types::{BinaryOp, UnaryOp};
use neuro_hir::{HirExpr, HirExprKind, HirPlace, HirStmt, HirType};
use shared_types::{Literal, Span};

use crate::LoweringError;

use super::leaf::is_arithmetic;
use super::positions::{constant_stride, describe_stmt, integer_min, is_float_valued, stmt_span};
use super::{
    Arm, ArmBody, Branch, Carried, ForRange, Leaf, Linearizer, Loop, Node, Op, Slot,
    RUN_TIME_STRIDE,
};

/// `loop { C; if !t { break }; B }` as the `while { C; t } { B }` it is: the condition, a
/// block value when `C` is not empty, and the body `B`. It is the form a derivative's own
/// forward replay gives a `while`, so a second derivative reads one back through it. Any
/// other `loop` is `None`.
pub(super) fn guarded_loop(body: &[HirStmt], span: Span) -> Option<(HirExpr, &[HirStmt])> {
    let exit = body.iter().position(is_loop_exit)?;
    let HirStmt::If { condition, .. } = &body[exit] else {
        return None;
    };
    let HirExprKind::Unary { operand: test, .. } = &condition.kind else {
        return None;
    };
    let prelude = &body[..exit];
    if prelude.is_empty() {
        return Some(((**test).clone(), &body[exit + 1..]));
    }
    let mut stmts = prelude.to_vec();
    stmts.push(HirStmt::Expr((**test).clone()));
    let condition = HirExpr::new(HirExprKind::Block { stmts }, HirType::Bool, span);
    Some((condition, &body[exit + 1..]))
}

/// `if !t { break }`, with no label, value or other arm.
pub(super) fn is_loop_exit(stmt: &HirStmt) -> bool {
    let HirStmt::If {
        condition,
        then_block,
        else_if_blocks,
        else_block: None,
        ..
    } = stmt
    else {
        return false;
    };
    matches!(
        condition.kind,
        HirExprKind::Unary {
            op: UnaryOp::Not,
            ..
        }
    ) && else_if_blocks.is_empty()
        && matches!(
            then_block.as_slice(),
            [HirStmt::Break {
                label: None,
                value: None,
                ..
            }]
        )
}

/// The bindings of `before`, in name order, that some exit rebinds, with the leaf each
/// held before.
pub(super) fn changed<const N: usize>(
    before: &HashMap<String, Leaf>,
    exits: [&HashMap<String, Leaf>; N],
) -> Vec<(String, Leaf)> {
    let mut names: Vec<(String, Leaf)> = before
        .iter()
        .filter(|(name, leaf)| exits.iter().any(|exit| exit.get(*name) != Some(*leaf)))
        .map(|(name, leaf)| (name.clone(), leaf.clone()))
        .collect();
    names.sort_by(|a, b| a.0.cmp(&b.0));
    names
}

pub(super) fn returns(block: &[HirStmt]) -> bool {
    matches!(block.last(), Some(HirStmt::Return { value: Some(_), .. }))
}

impl<'f> Linearizer<'f> {
    pub(super) fn stmt(&mut self, stmt: &HirStmt) -> Result<(), LoweringError> {
        match stmt {
            HirStmt::VarDecl {
                name,
                init: Some(init),
                ..
            } => {
                let leaf = self.leaf(init)?;
                self.declare(name, leaf);
                Ok(())
            }
            HirStmt::Assign {
                place: HirPlace::Var { name, .. },
                value,
                ..
            } if self.aliases.contains_key(name) => {
                let leaf = self.leaf(value)?;
                let _ = self.aliases.insert(name.clone(), leaf);
                Ok(())
            }
            HirStmt::TensorCompoundAssign {
                place: HirPlace::Var { name, .. },
                op,
                value,
                ty,
                span,
            } if is_arithmetic(*op) && is_float_valued(ty) => {
                let Some(target) = self.aliases.get(name).cloned() else {
                    return Err(self.refuse(describe_stmt(stmt), *span));
                };
                let right = self.leaf(value)?;
                let op = Op::Binary {
                    op: *op,
                    left: target,
                    right,
                };
                let leaf = self.push(ty, *span, op);
                let _ = self.aliases.insert(name.clone(), leaf);
                Ok(())
            }
            HirStmt::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                span,
            } => self
                .if_chain(
                    condition,
                    then_block,
                    else_if_blocks,
                    else_block.as_deref(),
                    None,
                    *span,
                )
                .map(drop),
            HirStmt::Expr(HirExpr {
                kind:
                    HirExprKind::If {
                        condition,
                        then_block,
                        else_if_blocks,
                        else_block,
                    },
                ty: HirType::Void,
                span,
            }) => self
                .if_chain(
                    condition,
                    then_block,
                    else_if_blocks,
                    else_block.as_deref(),
                    None,
                    *span,
                )
                .map(drop),
            HirStmt::While {
                condition,
                body,
                span,
                ..
            } => self.while_loop(condition, body, *span),
            HirStmt::ForRange {
                index,
                iterator,
                start,
                end,
                inclusive,
                reversed,
                step,
                body,
                span,
                ..
            } => self.for_range(
                ForRange {
                    index: index.as_deref(),
                    iterator,
                    start,
                    end,
                    inclusive: *inclusive,
                    reversed: *reversed,
                    step: step.as_deref(),
                },
                body,
                *span,
            ),
            HirStmt::Expr(HirExpr {
                kind: HirExprKind::Loop { label: None, body },
                ty: HirType::Void,
                span,
            }) => {
                let Some((condition, body)) = guarded_loop(body, *span) else {
                    return Err(self.refuse("a `loop`", *span));
                };
                self.while_loop(&condition, body, *span)
            }
            other => Err(self.refuse(describe_stmt(other), stmt_span(other))),
        }
    }

    /// `if condition { then } else if ... else { otherwise }`, of type `value_ty` when it
    /// is an expression.
    pub(super) fn if_chain(
        &mut self,
        condition: &HirExpr,
        then_block: &[HirStmt],
        else_ifs: &[(HirExpr, Vec<HirStmt>)],
        otherwise: Option<&[HirStmt]>,
        value_ty: Option<&HirType>,
        span: Span,
    ) -> Result<Option<Leaf>, LoweringError> {
        let condition = self.leaf(condition)?;
        self.fork(
            condition,
            value_ty,
            [
                &mut |this: &mut Self| this.arm(then_block, value_ty),
                &mut |this: &mut Self| match (else_ifs.split_first(), otherwise) {
                    (Some(((next, block), rest)), _) => {
                        this.if_chain(next, block, rest, otherwise, value_ty, span)
                    }
                    (None, Some(block)) => this.arm(block, value_ty),
                    (None, None) => Ok(None),
                },
            ],
            span,
        )
    }

    /// A branch on `condition` between two arms. Every binding either arm reassigns, and
    /// the value when the branch has one, leaves through a slot.
    pub(super) fn fork(
        &mut self,
        condition: Leaf,
        value_ty: Option<&HirType>,
        [then, otherwise]: [ArmBody<'_, 'f>; 2],
        span: Span,
    ) -> Result<Option<Leaf>, LoweringError> {
        let before = self.aliases.clone();
        let then = self.scoped(&before, then)?;
        let otherwise = self.scoped(&before, otherwise)?;
        self.aliases = before;

        let mut merges = Vec::new();
        let mut outs = [Vec::new(), Vec::new()];
        if let Some(ty) = value_ty {
            let (Some(a), Some(b)) = (then.value, otherwise.value) else {
                return Err(self.malformed("an `if` expression arm has no value"));
            };
            merges.push(self.slot(ty, &[&a, &b], span)?);
            outs[0].push(a);
            outs[1].push(b);
        }
        let value = merges.first().map(Slot::leaf);
        for (name, before) in changed(&self.aliases, [&then.exit, &otherwise.exit]) {
            let (Some(a), Some(b)) = (then.exit.get(&name), otherwise.exit.get(&name)) else {
                return Err(self.malformed("an arm lost a binding it did not declare"));
            };
            let slot = self.slot(before.ty(), &[a, b], span)?;
            outs[0].push(a.clone());
            outs[1].push(b.clone());
            let _ = self.aliases.insert(name, slot.leaf());
            merges.push(slot);
        }
        let [then_outs, otherwise_outs] = outs;
        self.nodes.push(Node::Branch(Branch {
            condition,
            merges,
            arms: [
                Arm {
                    nodes: then.nodes,
                    outs: then_outs,
                },
                Arm {
                    nodes: otherwise.nodes,
                    outs: otherwise_outs,
                },
            ],
            span,
        }));
        Ok(value)
    }

    /// `while condition { body }`. A first pass finds the bindings the body reassigns;
    /// the body is then linearized reading each through its slot, again whenever a slot
    /// turns out to be active only because the body makes it so.
    pub(super) fn while_loop(
        &mut self,
        condition: &HirExpr,
        body: &[HirStmt],
        span: Span,
    ) -> Result<(), LoweringError> {
        let before = self.aliases.clone();
        let discovery = self.scoped(&before, &mut |this: &mut Self| {
            let _ = this.leaf(condition)?;
            this.block(body).map(|()| None)
        })?;
        let mut carried = Vec::new();
        for (name, entry) in changed(&before, [&discovery.exit]) {
            let slot = self.slot(entry.ty(), &[&entry], span)?;
            carried.push((name, Carried { slot, entry }));
        }

        loop {
            let mut state = before.clone();
            for (name, carried) in &carried {
                let _ = state.insert(name.clone(), carried.slot.leaf());
            }
            let mut header = None;
            let pass = self.scoped(&state, &mut |this: &mut Self| {
                let test = this.leaf(condition)?;
                header = Some((std::mem::take(&mut this.nodes), test));
                this.block(body).map(|()| None)
            })?;
            let mut outs = Vec::with_capacity(carried.len());
            let mut widened = false;
            for (name, carried) in &mut carried {
                let Some(out) = pass.exit.get(name) else {
                    return Err(self.malformed("a loop body lost a binding it reassigns"));
                };
                let slot = &mut carried.slot;
                if !slot.active && is_float_valued(&slot.ty) && self.is_active(out) {
                    slot.active = true;
                    let _ = self.active.insert(slot.name.clone());
                    widened = true;
                }
                outs.push(out.clone());
            }
            if widened {
                continue;
            }
            let Some((condition, test)) = header else {
                return Err(self.malformed("a loop condition was never linearized"));
            };
            self.aliases = before;
            for (name, carried) in &carried {
                let _ = self.aliases.insert(name.clone(), carried.slot.leaf());
            }
            let count = self.fresh();
            self.nodes.push(Node::Loop(Loop {
                carried: carried.into_iter().map(|(_, carried)| carried).collect(),
                condition,
                test,
                body: pass.nodes,
                outs,
                count,
                span,
            }));
            return Ok(());
        }
    }

    /// `for iterator in start..end { body }` as the counted `while` it is. The bounds are
    /// read once, as the primal reads them. The counter never steps past `end`, so an
    /// inclusive range that ends at its type's maximum stops on a flag instead, and a
    /// reversed one counts up and mirrors the counter onto the binding, as the backend does.
    /// A `.step(n)` head also stops on the flag, because a stride can carry the counter
    /// past the type's maximum on either kind of range.
    pub(super) fn for_range(
        &mut self,
        head: ForRange<'_>,
        body: &[HirStmt],
        span: Span,
    ) -> Result<(), LoweringError> {
        let ty = &head.start.ty;
        let stride = head
            .step
            .map(|step| constant_stride(step).ok_or_else(|| self.refuse(RUN_TIME_STRIDE, span)))
            .transpose()?;
        let var = |name: &str, ty: &HirType| {
            HirExpr::new(HirExprKind::Variable(name.to_string()), ty.clone(), span)
        };
        let literal = |value: Literal, ty: &HirType| {
            HirExpr::new(HirExprKind::Literal(value), ty.clone(), span)
        };
        let one = |ty: &HirType| literal(Literal::Integer(1, None), ty);
        let binary = |op: BinaryOp, left: HirExpr, right: HirExpr, ty: &HirType| {
            HirExpr::new(
                HirExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                ty.clone(),
                span,
            )
        };
        let declare = |name: &str, init: HirExpr, mutable: bool| HirStmt::VarDecl {
            name: name.to_string(),
            ty: init.ty.clone(),
            init: Some(init),
            mutable,
            span,
        };
        let assign = |name: &str, value: HirExpr| HirStmt::Assign {
            place: HirPlace::Var {
                name: name.to_string(),
                ty: value.ty.clone(),
            },
            value,
            span,
        };

        let (low, high, counter) = (self.fresh(), self.fresh(), self.fresh());
        self.stmt(&declare(&low, head.start.clone(), false))?;
        self.stmt(&declare(&high, head.end.clone(), false))?;
        self.stmt(&declare(&counter, var(&low, ty), true))?;
        let current = if head.reversed {
            let walked = binary(BinaryOp::Subtract, var(&counter, ty), var(&low, ty), ty);
            let top = if head.inclusive {
                var(&high, ty)
            } else {
                binary(BinaryOp::Subtract, var(&high, ty), one(ty), ty)
            };
            binary(BinaryOp::Subtract, top, walked, ty)
        } else {
            var(&counter, ty)
        };
        let mut looped = vec![declare(head.iterator, current, false)];
        let mut position = None;
        if let Some(index) = head.index {
            let name = self.fresh();
            self.stmt(&declare(
                &name,
                literal(Literal::Integer(0, None), &HirType::U64),
                true,
            ))?;
            looped.push(declare(index, var(&name, &HirType::U64), false));
            position = Some(name);
        }
        looped.extend_from_slice(body);
        if let Some(name) = &position {
            let next = binary(
                BinaryOp::Add,
                var(name, &HirType::U64),
                one(&HirType::U64),
                &HirType::U64,
            );
            looped.push(assign(name, next));
        }
        let step = assign(
            &counter,
            binary(BinaryOp::Add, var(&counter, ty), one(ty), ty),
        );
        let within = match head.inclusive {
            true => BinaryOp::LessEqual,
            false => BinaryOp::Less,
        };
        let when =
            |condition: HirExpr, then_block: Vec<HirStmt>, else_block: Vec<HirStmt>| HirStmt::If {
                condition,
                then_block,
                else_if_blocks: Vec::new(),
                else_block: Some(else_block),
                span,
            };
        let condition = if head.inclusive || stride.is_some() {
            let more = self.fresh();
            let nonempty = binary(within, var(&low, ty), var(&high, ty), &HirType::Bool);
            self.stmt(&declare(&more, nonempty, true))?;
            let stop = || assign(&more, literal(Literal::Boolean(false), &HirType::Bool));
            let advance = match stride {
                None => when(
                    binary(
                        BinaryOp::Less,
                        var(&counter, ty),
                        var(&high, ty),
                        &HirType::Bool,
                    ),
                    vec![step],
                    vec![stop()],
                ),
                // `counter + n` overflows where the range ends near the type's top, so the
                // test is `counter <= high - n`. That subtraction underflows when `high`
                // lies within `n` of the type's minimum, and there no second value exists
                // anyway, so it is formed only once `high` is known to clear that floor.
                Some(n) => {
                    let floor = integer_min(ty)
                        .map(|min| literal(Literal::Integer(min + n, None), ty))
                        .ok_or_else(|| self.malformed("a stepped range over a non-integer"))?;
                    let n = literal(Literal::Integer(n, None), ty);
                    let last_start = binary(BinaryOp::Subtract, var(&high, ty), n.clone(), ty);
                    let stride_fits = when(
                        binary(within, var(&counter, ty), last_start, &HirType::Bool),
                        vec![assign(
                            &counter,
                            binary(BinaryOp::Add, var(&counter, ty), n, ty),
                        )],
                        vec![stop()],
                    );
                    when(
                        binary(
                            BinaryOp::GreaterEqual,
                            var(&high, ty),
                            floor,
                            &HirType::Bool,
                        ),
                        vec![stride_fits],
                        vec![stop()],
                    )
                }
            };
            looped.push(advance);
            var(&more, &HirType::Bool)
        } else {
            looped.push(step);
            binary(
                BinaryOp::Less,
                var(&counter, ty),
                var(&high, ty),
                &HirType::Bool,
            )
        };
        self.while_loop(&condition, &looped, span)
    }
}
