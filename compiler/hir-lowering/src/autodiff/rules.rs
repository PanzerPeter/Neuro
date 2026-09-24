//! The adjoint of each tape operation, and the accumulation of adjoints per value.
//!
//! A value used in several places receives one contribution per use, and its adjoint is
//! their sum. Contributions are collected until the sweep reaches the value's own entry,
//! which comes after every use in reverse order, so each adjoint is summed exactly once.

use std::collections::{HashMap, HashSet};

use ast_types::{BinaryOp, UnaryOp};
use neuro_hir::{HirReduceOp, HirType};

use super::emit::{tensor_parts, tensor_type, Emitter};
use super::tape::{Entry, Leaf, Op};
use crate::LoweringError;

#[derive(Default)]
pub(super) struct Adjoints {
    whole: HashMap<String, Vec<Leaf>>,
    /// The value type of every target that has received a contribution.
    types: HashMap<String, HirType>,
    /// Contributions to single elements of a tensor, from element reads, by offset.
    elements: HashMap<String, Vec<(usize, Leaf)>>,
    /// How many values each contribution was handed to. An add passes its own adjoint
    /// through to both operands, and a tensor given to two owners must be copied before
    /// either can consume it.
    owners: HashMap<String, usize>,
}

impl Adjoints {
    pub(super) fn add(&mut self, target: &Leaf, contribution: Leaf) {
        let Some(name) = target.var_name() else {
            return;
        };
        self.note(target);
        if let Some(held) = contribution.var_name() {
            *self.owners.entry(held.to_string()).or_default() += 1;
        }
        self.whole
            .entry(name.to_string())
            .or_default()
            .push(contribution);
    }

    fn owners_of(&self, contribution: &Leaf) -> usize {
        contribution
            .var_name()
            .and_then(|name| self.owners.get(name).copied())
            .unwrap_or(0)
    }

    fn release(&mut self, contribution: &Leaf) {
        if let Some(count) = contribution
            .var_name()
            .and_then(|name| self.owners.get_mut(name))
        {
            *count = count.saturating_sub(1);
        }
    }

    fn note(&mut self, target: &Leaf) {
        if let Some(name) = target.var_name() {
            let _ = self
                .types
                .entry(name.to_string())
                .or_insert_with(|| target.value_ty().clone());
        }
    }

    /// Every target still holding a contribution, with its value type, in name order.
    /// After a nested sweep these are exactly the values defined outside it.
    pub(super) fn pending(&self) -> Vec<(String, HirType)> {
        let mut pending: Vec<(String, HirType)> = self
            .whole
            .keys()
            .chain(self.elements.keys())
            .filter_map(|name| Some((name.clone(), self.types.get(name)?.clone())))
            .collect();
        pending.sort_by(|a, b| a.0.cmp(&b.0));
        pending.dedup_by(|a, b| a.0 == b.0);
        pending
    }

    fn add_element(&mut self, target: &Leaf, flat: usize, contribution: Leaf) {
        self.note(target);
        let Some(target) = target.var_name() else {
            return;
        };
        self.elements
            .entry(target.to_string())
            .or_default()
            .push((flat, contribution));
    }

    /// The adjoint of `target`, a value of type `ty`, summed from its contributions and
    /// owned by `target` alone. `None` when nothing reached it: its adjoint is zero and
    /// nothing upstream of it needs visiting.
    pub(super) fn materialize(
        &mut self,
        em: &mut Emitter,
        target: &str,
        ty: &HirType,
    ) -> Result<Option<Leaf>, LoweringError> {
        let mut whole = self.whole.remove(target).unwrap_or_default();
        if let Some(elements) = self.elements.remove(target) {
            whole.push(scatter(em, &elements, ty)?);
        }
        // Whether another value still holds the lone contribution, read before this one
        // lets go of its share.
        let shared = matches!(whole.as_slice(), [only] if self.owners_of(only) > 1);
        for contribution in &whole {
            self.release(contribution);
        }
        let mut contributions = whole.into_iter();
        let Some(first) = contributions.next() else {
            return Ok(None);
        };
        let Some(second) = contributions.next() else {
            if shared && tensor_parts(ty).is_some() {
                return em.copy(&first).map(Some);
            }
            return Ok(Some(first));
        };
        let mut sum = em.binary(BinaryOp::Add, &first, &second)?;
        for next in contributions {
            sum = em.binary(BinaryOp::Add, &sum, &next)?;
        }
        Ok(Some(sum))
    }
}

/// A tensor of type `ty` holding the summed element contributions at their offsets and
/// zero everywhere else.
// ponytail: one HIR element per tensor element, so a read of a large tensor costs a
// literal its size; a scatter node would make it O(reads) if large parameters need it.
fn scatter(
    em: &mut Emitter,
    elements: &[(usize, Leaf)],
    ty: &HirType,
) -> Result<Leaf, LoweringError> {
    let (element, extents) = tensor_parts(ty).ok_or_else(|| LoweringError::Malformed {
        detail: "derivative transform: an element read of a non-tensor".to_string(),
    })?;
    let count = extents.iter().product::<usize>();
    let zero = em.float(0.0, element);
    let mut slots = Vec::with_capacity(count);
    for offset in 0..count {
        let mut here = elements.iter().filter(|(flat, _)| *flat == offset);
        let Some((_, first)) = here.next() else {
            slots.push(zero.clone());
            continue;
        };
        let mut sum = first.clone();
        for (_, next) in here {
            sum = em.binary(BinaryOp::Add, &sum, next)?;
        }
        slots.push(sum);
    }
    Ok(em.literal(&slots, ty))
}

/// Send `adjoint`, the adjoint of `entry`'s value, back to each active operand.
pub(super) fn propagate(
    em: &mut Emitter,
    entry: &Entry,
    adjoint: &Leaf,
    active: &HashSet<String>,
    adjoints: &mut Adjoints,
) -> Result<(), LoweringError> {
    let is_active = |leaf: &Leaf| leaf.var_name().is_some_and(|name| active.contains(name));
    match &entry.op {
        Op::Binary { op, left, right } => {
            binary(em, *op, [left, right], adjoint, is_active, adjoints)
        }
        Op::Unary {
            op: UnaryOp::Negate,
            operand,
        } => {
            if is_active(operand) {
                let contribution = em.negate(adjoint)?;
                adjoints.add(operand, contribution);
            }
            Ok(())
        }
        Op::Reduce { receiver, op, axis } if is_active(receiver) => {
            let contribution = reduce(em, receiver.value_ty(), *op, *axis, adjoint)?;
            adjoints.add(receiver, contribution);
            Ok(())
        }
        Op::Reduce { .. } => Ok(()),
        Op::Literal(elements) => {
            for (offset, element) in elements.iter().enumerate() {
                if is_active(element) {
                    let contribution = em.element(adjoint, offset)?;
                    adjoints.add(element, contribution);
                }
            }
            Ok(())
        }
        Op::Read { object, flat, .. } => {
            if is_active(object) {
                adjoints.add_element(object, *flat, adjoint.clone());
            }
            Ok(())
        }
        Op::Unary { .. } | Op::Constant(_) => Ok(()),
    }
}

fn binary(
    em: &mut Emitter,
    op: BinaryOp,
    [left, right]: [&Leaf; 2],
    adjoint: &Leaf,
    is_active: impl Fn(&Leaf) -> bool,
    adjoints: &mut Adjoints,
) -> Result<(), LoweringError> {
    if op == BinaryOp::MatMul {
        return matmul(em, [left, right], adjoint, is_active, adjoints);
    }
    if is_active(left) {
        let toward = match op {
            BinaryOp::Multiply => em.binary(BinaryOp::Multiply, adjoint, right)?,
            BinaryOp::Divide => em.binary(BinaryOp::Divide, adjoint, right)?,
            _ => adjoint.clone(),
        };
        let contribution = unbroadcast(em, toward, left.value_ty())?;
        adjoints.add(left, contribution);
    }
    if is_active(right) {
        let toward = match op {
            BinaryOp::Subtract => em.negate(adjoint)?,
            BinaryOp::Multiply => em.binary(BinaryOp::Multiply, adjoint, left)?,
            // d(a / b)/db = -a / b^2.
            BinaryOp::Divide => {
                let scaled = em.binary(BinaryOp::Multiply, adjoint, left)?;
                let squared = em.binary(BinaryOp::Multiply, right, right)?;
                let quotient = em.binary(BinaryOp::Divide, &scaled, &squared)?;
                em.negate(&quotient)?
            }
            _ => adjoint.clone(),
        };
        let contribution = unbroadcast(em, toward, right.value_ty())?;
        adjoints.add(right, contribution);
    }
    Ok(())
}

/// `C = A @ B` with `A: [m, k]`, `B: [k, n]`: `dA = dC @ Bᵀ` and `dB = Aᵀ @ dC`, written as
/// contractions so that neither operand has to be transposed, which would consume it.
fn matmul(
    em: &mut Emitter,
    [left, right]: [&Leaf; 2],
    adjoint: &Leaf,
    is_active: impl Fn(&Leaf) -> bool,
    adjoints: &mut Adjoints,
) -> Result<(), LoweringError> {
    let (Some((element, a)), Some((_, b))) = (tensor_parts(left.ty()), tensor_parts(right.ty()))
    else {
        return Err(LoweringError::Malformed {
            detail: "derivative transform: `@` on a non-tensor".to_string(),
        });
    };
    let ([m, k], [_, n]) = (a.as_slice(), b.as_slice()) else {
        return Err(LoweringError::Malformed {
            detail: "derivative transform: `@` on operands that are not matrices".to_string(),
        });
    };
    // Letters: 0 = the rows of A, 1 = the columns of B, 2 = the contracted axis.
    let extents = vec![*m, *n, *k];
    if is_active(left) {
        let ty = tensor_type(element, &[*m, *k]);
        let contribution = em.einsum(
            [adjoint, right],
            vec![vec![0, 1], vec![2, 1]],
            vec![0, 2],
            extents.clone(),
            ty,
        );
        adjoints.add(left, contribution);
    }
    if is_active(right) {
        let ty = tensor_type(element, &[*k, *n]);
        let contribution = em.einsum(
            [left, adjoint],
            vec![vec![0, 2], vec![0, 1]],
            vec![2, 1],
            extents,
            ty,
        );
        adjoints.add(right, contribution);
    }
    Ok(())
}

/// The adjoint of a reduction's receiver, of type `receiver`: the result's adjoint spread
/// back over every element it summarised.
fn reduce(
    em: &mut Emitter,
    receiver: &HirType,
    op: HirReduceOp,
    axis: Option<usize>,
    adjoint: &Leaf,
) -> Result<Leaf, LoweringError> {
    let (element, extents) = tensor_parts(receiver).ok_or_else(|| LoweringError::Malformed {
        detail: "derivative transform: a reduction of a non-tensor".to_string(),
    })?;
    let Some(axis) = axis else {
        let spread = if op == HirReduceOp::Mean {
            let count = em.float(extents.iter().product::<usize>() as f64, element);
            em.binary(BinaryOp::Divide, adjoint, &count)?
        } else {
            adjoint.clone()
        };
        return em.fill(&spread, receiver);
    };
    // Put the reduced axis back at extent 1, then let broadcasting stretch it.
    let mut kept = extents.clone();
    kept[axis] = 1;
    let restored = em.reshape(adjoint, tensor_type(element, &kept))?;
    let one = em.float(1.0, element);
    let ones = em.fill(&one, receiver)?;
    let spread = em.binary(BinaryOp::Multiply, &ones, &restored)?;
    if op != HirReduceOp::Mean {
        return Ok(spread);
    }
    let count = em.float(extents[axis] as f64, element);
    em.binary(BinaryOp::Divide, &spread, &count)
}

/// Sum `contribution` down to the shape of the operand it belongs to, undoing the
/// broadcast that stretched the operand to the result's shape.
fn unbroadcast(
    em: &mut Emitter,
    contribution: Leaf,
    target: &HirType,
) -> Result<Leaf, LoweringError> {
    let Some((current_element, current)) = tensor_parts(contribution.ty()) else {
        return Ok(contribution);
    };
    let Some((_, wanted)) = tensor_parts(target) else {
        // A scalar operand was stretched over every element.
        return Ok(em.sum_all(&contribution));
    };
    if current == wanted {
        return Ok(contribution);
    }
    let element = current_element.clone();
    if wanted.is_empty() {
        let total = em.sum_all(&contribution);
        return Ok(em.literal(&[total], target));
    }
    let mut value = contribution;
    for _ in wanted.len()..current.len() {
        value = em.sum_axis(&value, 0)?;
    }
    let mut stretched = false;
    for (axis, extent) in wanted.iter().enumerate().rev() {
        let Some((_, now)) = tensor_parts(value.ty()) else {
            break;
        };
        if *extent == 1 && now[axis] != 1 {
            value = em.sum_axis(&value, axis)?;
            stretched = true;
        }
    }
    if !stretched {
        return Ok(value);
    }
    em.reshape(&value, tensor_type(&element, &wanted))
}
