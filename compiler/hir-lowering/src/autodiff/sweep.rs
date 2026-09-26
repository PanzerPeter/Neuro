//! The two passes of the derivative function over a tape: the forward replay, and the
//! reverse sweep that sends each value's adjoint back to its operands.
//!
//! Control flow is where the passes stop being a plain walk, and both constructs share
//! one idea: nothing a branch arm or a loop iteration computes survives it, so the reverse
//! pass RECOMPUTES what it needs rather than keeping it. The reverse of a branch re-runs
//! the arm the forward pass took, then sweeps it. The reverse of a loop walks its
//! iterations backwards; for each one it rebuilds the carried values from the loop's
//! entry by replaying the iterations before it, replays the iteration itself, and sweeps
//! that. The forward pass keeps an iteration count and nothing else, so no value is
//! recorded at run time: the no-tape rule holds through control flow too.
//!
//! What a nested reverse pass sends to a value defined outside it leaves through a
//! running sum, a mutable binding declared before the construct and handed to the
//! enclosing pass as that value's contribution once the construct is done.

use std::collections::{BTreeMap, HashSet};

use ast_types::{BinaryOp, UnaryOp};
use neuro_hir::{HirExpr, HirExprKind, HirStmt, HirTensorAxis, HirType};

use super::emit::{self, Emitter};
use super::rules::{self, Adjoints};
use super::tape::{Branch, Entry, Leaf, Loop, Node, Op, Slot};
use crate::LoweringError;

/// Emit the forward computation of `nodes`, with every operand borrowed.
pub(super) fn forward(em: &mut Emitter, nodes: &[Node]) -> Result<(), LoweringError> {
    for node in nodes {
        match node {
            Node::Entry(entry) => {
                let init = replay(em, entry)?;
                em.declare(entry.name.clone(), init);
            }
            Node::Branch(branch) => forward_branch(em, branch)?,
            Node::Loop(looped) => forward_loop(em, looped)?,
        }
    }
    Ok(())
}

/// Sweep `nodes` backwards. `adjoints` holds the contributions reaching their values and
/// receives those their operands are sent.
pub(super) fn reverse(
    em: &mut Emitter,
    nodes: &[Node],
    adjoints: &mut Adjoints,
    active: &HashSet<String>,
) -> Result<(), LoweringError> {
    for node in nodes.iter().rev() {
        match node {
            Node::Entry(entry) if entry.active => {
                let Some(adjoint) = adjoints.materialize(em, &entry.name, &entry.ty)? else {
                    continue;
                };
                rules::propagate(em, entry, &adjoint, active, adjoints)?;
            }
            Node::Entry(_) => {}
            Node::Branch(branch) => reverse_branch(em, branch, adjoints, active)?,
            Node::Loop(looped) => reverse_loop(em, looped, adjoints, active)?,
        }
    }
    Ok(())
}

/// The forward computation of one tape entry, with every operand borrowed.
fn replay(em: &mut Emitter, entry: &Entry) -> Result<HirExpr, LoweringError> {
    let span = em.span();
    let read = |leaf: &Leaf| Box::new(emit::operand(leaf, span));
    let kind = match &entry.op {
        Op::Binary { op, left, right } => HirExprKind::Binary {
            op: *op,
            left: read(left),
            right: read(right),
        },
        Op::Unary { op, operand } => HirExprKind::Unary {
            op: *op,
            operand: read(operand),
        },
        Op::Reduce { receiver, op, axis } => HirExprKind::TensorReduce {
            receiver: read(receiver),
            op: *op,
            axis: *axis,
        },
        Op::Literal(elements) => HirExprKind::TensorLiteral {
            elements: elements
                .iter()
                .map(|leaf| emit::operand(leaf, span))
                .collect(),
        },
        Op::Read { object, positions } => HirExprKind::TensorIndex {
            object: read(object),
            axes: positions
                .iter()
                .map(|leaf| HirTensorAxis::Position(emit::operand(leaf, span)))
                .collect(),
        },
        Op::Slice { object, axes, .. } => HirExprKind::TensorIndex {
            object: read(object),
            axes: axes.clone(),
        },
        Op::ShapeCast {
            receiver,
            permutation,
        } => {
            let copy = em.copy(receiver)?;
            HirExprKind::TensorShapeCast {
                receiver: Box::new(emit::operand_owned(&copy, span)),
                permutation: permutation.clone(),
            }
        }
        Op::Convert(operand) => HirExprKind::Cast {
            value: read(operand),
        },
        Op::Math {
            op,
            operand,
            exponent,
        } => HirExprKind::Math {
            op: *op,
            operand: read(operand),
            exponent: exponent.as_ref().map(read),
        },
        Op::Einsum {
            operands,
            inputs,
            output,
            extents,
        } => HirExprKind::TensorEinsum {
            operands: operands
                .iter()
                .map(|leaf| emit::operand(leaf, span))
                .collect(),
            inputs: inputs.clone(),
            output: output.clone(),
            extents: extents.clone(),
        },
        Op::Constant(expr) => return Ok(expr.clone()),
    };
    Ok(HirExpr::new(kind, entry.ty.clone(), entry.span))
}

fn forward_branch(em: &mut Emitter, branch: &Branch) -> Result<(), LoweringError> {
    for slot in &branch.merges {
        let placeholder = em.zero(&slot.ty)?;
        em.declare_mut(slot.name.clone(), placeholder);
    }
    let merges: Vec<&Slot> = branch.merges.iter().collect();
    let mut blocks = Vec::with_capacity(branch.arms.len());
    for arm in &branch.arms {
        let (block, ()) = em.nested(|em| {
            forward(em, &arm.nodes)?;
            store(em, &merges, &arm.outs, &arm.nodes)
        })?;
        blocks.push(block);
    }
    push_if(em, &branch.condition, blocks, branch.span)
}

fn forward_loop(em: &mut Emitter, looped: &Loop) -> Result<(), LoweringError> {
    let span = em.span();
    for carried in &looped.carried {
        let initial = em.snapshot(&carried.entry)?;
        em.declare_mut(
            carried.slot.name.clone(),
            emit::operand_owned(&initial, span),
        );
    }
    em.declare_mut(looped.count.clone(), em.count(0));
    let slots: Vec<&Slot> = looped.carried.iter().map(|carried| &carried.slot).collect();
    let (body, ()) = em.nested(|em| {
        forward(em, &looped.condition)?;
        let stop = HirStmt::Break {
            label: None,
            value: None,
            span,
        };
        let finished = HirExpr::new(
            HirExprKind::Unary {
                op: UnaryOp::Not,
                operand: Box::new(emit::operand(&looped.test, span)),
            },
            HirType::Bool,
            span,
        );
        em.push(HirStmt::If {
            condition: finished,
            then_block: vec![stop],
            else_if_blocks: Vec::new(),
            else_block: None,
            span,
        });
        forward(em, &looped.body)?;
        store(em, &slots, &looped.outs, &looped.body)?;
        em.step(&looped.count, 1);
        Ok(())
    })?;
    em.push(HirStmt::Expr(HirExpr::new(
        HirExprKind::Loop { label: None, body },
        HirType::Void,
        looped.span,
    )));
    Ok(())
}

/// Give each slot its value in `outs`, as a parallel assignment: every value is taken
/// before any slot changes, because one slot's new value may be another's old one.
///
/// A value `nodes` declared, used by no later slot, is moved; anything else is copied,
/// since it is still read after this block (a value of the enclosing tape) or is about
/// to be overwritten (a slot of the same loop).
fn store(
    em: &mut Emitter,
    slots: &[&Slot],
    outs: &[Leaf],
    nodes: &[Node],
) -> Result<(), LoweringError> {
    let span = em.span();
    let locals = declared(nodes);
    let mut values = Vec::with_capacity(outs.len());
    for (index, (slot, out)) in slots.iter().zip(outs).enumerate() {
        let name = out.var_name();
        if name == Some(slot.name.as_str()) {
            values.push(None);
            continue;
        }
        let moved = name.is_some_and(|name| {
            locals.contains(name)
                && !outs[index + 1..]
                    .iter()
                    .any(|later| later.var_name() == Some(name))
        });
        let owned = if moved {
            out.clone()
        } else {
            em.snapshot(out)?
        };
        values.push(Some(emit::operand_owned(&owned, span)));
    }
    for (slot, value) in slots.iter().zip(values) {
        if let Some(value) = value {
            em.assign(&slot.name, &slot.ty, value);
        }
    }
    Ok(())
}

/// The names a tape binds at its own level: its entries and the slots of the branches
/// and loops it holds.
fn declared(nodes: &[Node]) -> HashSet<&str> {
    let mut names = HashSet::new();
    for node in nodes {
        match node {
            Node::Entry(entry) => {
                let _ = names.insert(entry.name.as_str());
            }
            Node::Branch(branch) => {
                names.extend(branch.merges.iter().map(|slot| slot.name.as_str()));
            }
            Node::Loop(looped) => {
                names.extend(looped.carried.iter().map(|c| c.slot.name.as_str()));
            }
        }
    }
    names
}

fn push_if(
    em: &mut Emitter,
    condition: &Leaf,
    blocks: Vec<Vec<HirStmt>>,
    span: shared_types::Span,
) -> Result<(), LoweringError> {
    let Ok([then_block, else_block]) = <[Vec<HirStmt>; 2]>::try_from(blocks) else {
        return Err(LoweringError::Malformed {
            detail: "derivative transform: a branch without two arms".to_string(),
        });
    };
    em.push(HirStmt::If {
        condition: emit::operand(condition, span),
        then_block,
        else_if_blocks: Vec::new(),
        else_block: Some(else_block),
        span,
    });
    Ok(())
}

/// Seed `inner` with `seed`, the adjoint of a value an arm or a loop body leaves in
/// `out`. The seed belongs to the enclosing pass, so the nested one works on a copy it
/// may consume.
fn seed(
    em: &mut Emitter,
    inner: &mut Adjoints,
    out: &Leaf,
    seed: &Leaf,
    active: &HashSet<String>,
) -> Result<(), LoweringError> {
    if !out.var_name().is_some_and(|name| active.contains(name)) {
        return Ok(());
    }
    let copy = em.snapshot(seed)?;
    inner.add(out, copy);
    Ok(())
}

fn reverse_branch(
    em: &mut Emitter,
    branch: &Branch,
    adjoints: &mut Adjoints,
    active: &HashSet<String>,
) -> Result<(), LoweringError> {
    let mut seeds = Vec::new();
    for (index, slot) in branch.merges.iter().enumerate() {
        if !slot.active {
            continue;
        }
        if let Some(adjoint) = adjoints.materialize(em, &slot.name, &slot.ty)? {
            seeds.push((index, adjoint));
        }
    }
    if seeds.is_empty() {
        return Ok(());
    }
    let mut sums = Sums::default();
    let mut blocks = Vec::with_capacity(branch.arms.len());
    for arm in &branch.arms {
        let (block, ()) = em.nested(|em| {
            forward(em, &arm.nodes)?;
            let mut inner = Adjoints::default();
            for (index, adjoint) in &seeds {
                seed(em, &mut inner, &arm.outs[*index], adjoint, active)?;
            }
            reverse(em, &arm.nodes, &mut inner, active)?;
            sums.collect(em, &mut inner)
        })?;
        blocks.push(block);
    }
    sums.declare(em)?;
    push_if(em, &branch.condition, blocks, branch.span)?;
    sums.hand_back(adjoints);
    Ok(())
}

// ponytail: iteration k is rebuilt by replaying k iterations from the loop's entry, so a
// loop of n iterations costs O(n^2) body evaluations in its reverse pass and O(1) extra
// memory. Checkpointing every sqrt(n) iterations is the upgrade if long loops need it.
fn reverse_loop(
    em: &mut Emitter,
    looped: &Loop,
    adjoints: &mut Adjoints,
    active: &HashSet<String>,
) -> Result<(), LoweringError> {
    let mut after = Vec::new();
    for (index, carried) in looped.carried.iter().enumerate() {
        let slot = &carried.slot;
        if slot.active {
            after.push((index, adjoints.materialize(em, &slot.name, &slot.ty)?));
        }
    }
    if after.iter().all(|(_, adjoint)| adjoint.is_none()) {
        return Ok(());
    }
    let span = em.span();
    // The adjoint of each active carried value at the iteration boundary the sweep has
    // reached, starting from the loop's exit.
    let mut running = Vec::with_capacity(after.len());
    for (index, adjoint) in after {
        let init = match adjoint {
            Some(adjoint) => emit::operand_owned(&adjoint, span),
            None => em.zero(&looped.carried[index].slot.ty)?,
        };
        let name = em.fresh();
        em.declare_mut(name.clone(), init);
        running.push((index, name));
    }

    let countdown = em.fresh();
    let remaining = HirExpr::new(
        HirExprKind::Variable(looped.count.clone()),
        HirType::I64,
        span,
    );
    em.declare_mut(countdown.clone(), remaining);
    let slots: Vec<&Slot> = looped.carried.iter().map(|carried| &carried.slot).collect();
    let mut sums = Sums::default();
    let (body, ()) = em.nested(|em| {
        em.step(&countdown, -1);
        rebuild(em, looped, &slots, &countdown)?;
        forward(em, &looped.body)?;
        let mut inner = Adjoints::default();
        for (index, name) in &running {
            let slot = &looped.carried[*index].slot;
            let adjoint = Leaf::Var {
                name: name.clone(),
                ty: slot.ty.clone(),
            };
            seed(em, &mut inner, &looped.outs[*index], &adjoint, active)?;
        }
        reverse(em, &looped.body, &mut inner, active)?;
        for (index, name) in &running {
            let slot = &looped.carried[*index].slot;
            let before = match inner.materialize(em, &slot.name, &slot.ty)? {
                Some(adjoint) => emit::operand_owned(&adjoint, span),
                None => em.zero(&slot.ty)?,
            };
            em.assign(name, &slot.ty, before);
        }
        sums.collect(em, &mut inner)
    })?;
    sums.declare(em)?;
    em.push(HirStmt::While {
        label: None,
        condition: em.compare(BinaryOp::Greater, &countdown, None),
        body,
        span,
    });
    for (index, name) in running {
        let carried = &looped.carried[index];
        if carried
            .entry
            .var_name()
            .is_some_and(|entry| active.contains(entry))
        {
            let adjoint = Leaf::Var {
                name,
                ty: carried.slot.ty.clone(),
            };
            adjoints.add(&carried.entry, adjoint);
        }
    }
    sums.hand_back(adjoints);
    Ok(())
}

/// Put every carried value back to what it held at the start of iteration `countdown`:
/// its entry value, then that many iterations of the body.
fn rebuild(
    em: &mut Emitter,
    looped: &Loop,
    slots: &[&Slot],
    countdown: &str,
) -> Result<(), LoweringError> {
    let span = em.span();
    for carried in &looped.carried {
        let initial = em.snapshot(&carried.entry)?;
        em.assign(
            &carried.slot.name,
            &carried.slot.ty,
            emit::operand_owned(&initial, span),
        );
    }
    let step = em.fresh();
    em.declare_mut(step.clone(), em.count(0));
    let (replayed, ()) = em.nested(|em| {
        forward(em, &looped.body)?;
        store(em, slots, &looped.outs, &looped.body)?;
        em.step(&step, 1);
        Ok(())
    })?;
    em.push(HirStmt::While {
        label: None,
        condition: em.compare(BinaryOp::Less, &step, Some(countdown)),
        body: replayed,
        span,
    });
    Ok(())
}

/// The running sums a nested reverse pass accumulates into, one per value outside it
/// that it sends a contribution to, keyed by that value's name.
#[derive(Default)]
struct Sums {
    by_target: BTreeMap<String, (String, HirType)>,
}

impl Sums {
    /// Add what `inner` still holds, which is everything sent outside the nested tape,
    /// to the running sums.
    fn collect(&mut self, em: &mut Emitter, inner: &mut Adjoints) -> Result<(), LoweringError> {
        let span = em.span();
        for (target, ty) in inner.pending() {
            let Some(contribution) = inner.materialize(em, &target, &ty)? else {
                continue;
            };
            let (name, ty) = self
                .by_target
                .entry(target)
                .or_insert_with(|| (em.fresh(), ty))
                .clone();
            let sum = Leaf::Var {
                name: name.clone(),
                ty: ty.clone(),
            };
            let total = em.binary(BinaryOp::Add, &sum, &contribution)?;
            em.assign(&name, &ty, emit::operand_owned(&total, span));
        }
        Ok(())
    }

    fn declare(&self, em: &mut Emitter) -> Result<(), LoweringError> {
        for (name, ty) in self.by_target.values() {
            let zero = em.zero(ty)?;
            em.declare_mut(name.clone(), zero);
        }
        Ok(())
    }

    fn hand_back(self, adjoints: &mut Adjoints) {
        for (target, (name, ty)) in self.by_target {
            let target = Leaf::Var {
                name: target,
                ty: ty.clone(),
            };
            adjoints.add(&target, Leaf::Var { name, ty });
        }
    }
}
