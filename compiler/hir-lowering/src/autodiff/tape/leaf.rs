//! An expression reduced to a leaf: every operation it performs pushed as one entry,
//! and the copies a place read needs so the forward replay can borrow its operands.

use ast_types::BinaryOp;
use neuro_hir::{HirExpr, HirExprKind, HirType};

use crate::autodiff::{FieldPath, PathStep, RECEIVER};
use crate::{is_numeric, LoweringError};

use super::positions::literal_position;
use super::{Leaf, Linearizer, Op, Slot, CLONE_METHOD};

pub(super) fn is_arithmetic(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::MatMul
    )
}

/// Whether a field read of `ty` is a copy the replay can take as the primal did: a
/// number, a `bool` or a `char`, all `Copy`.
/// The receiver of `expr` when `expr` is `receiver.clone()` on a tensor.
pub(super) fn cloned_tensor(expr: &HirExpr) -> Option<&HirExpr> {
    let HirExprKind::Call { callee, args } = &expr.kind else {
        return None;
    };
    match &callee.kind {
        HirExprKind::FieldAccess { object, field }
            if field == CLONE_METHOD
                && args.is_empty()
                && matches!(object.ty.referent(), HirType::Tensor { .. }) =>
        {
            Some(object)
        }
        _ => None,
    }
}

pub(super) fn is_array(ty: &HirType) -> bool {
    matches!(ty.referent(), HirType::Array { .. })
}

/// The steps from the receiver to `place`, when `place` is rooted at the receiver and every
/// array position in it is a literal.
pub(super) fn receiver_path(place: &HirExpr) -> Option<FieldPath> {
    match &place.kind {
        HirExprKind::Variable(name) if name == RECEIVER => Some(Vec::new()),
        HirExprKind::FieldAccess { object, field } => {
            let mut path = receiver_path(object)?;
            path.push(PathStep::Field(field.clone()));
            Some(path)
        }
        HirExprKind::Index { object, index } => {
            let position = literal_position(index)?;
            let mut path = receiver_path(object)?;
            path.push(PathStep::Element(position));
            Some(path)
        }
        _ => None,
    }
}

/// `place.clone()`, a fresh copy of the tensor at `place`, owned at `ty`.
pub(super) fn clone_call(place: HirExpr, ty: &HirType) -> HirExpr {
    let span = place.span;
    let method = HirExpr::new(
        HirExprKind::FieldAccess {
            object: Box::new(place),
            field: CLONE_METHOD.to_string(),
        },
        ty.clone(),
        span,
    );
    let call = HirExprKind::Call {
        callee: Box::new(method),
        args: Vec::new(),
    };
    HirExpr::new(call, ty.clone(), span)
}

pub(super) fn is_copied_field(ty: &HirType) -> bool {
    is_numeric(ty) || matches!(ty, HirType::Bool | HirType::Char)
}

impl Slot {
    pub(super) fn leaf(&self) -> Leaf {
        Leaf::Var {
            name: self.name.clone(),
            ty: self.ty.clone(),
        }
    }
}

impl<'f> Linearizer<'f> {
    /// A copy of the tensor field `place` names, taken once where the body reads it. The
    /// primal can only read such a field in place (a reduction's receiver, an element
    /// read's object), since it is reached through a borrow. The replay and the reverse
    /// pass read an operand wherever their rules need it, and only a binding of the
    /// derivative's own can be read that way without moving the field out of the receiver.
    // ponytail: one buffer copy per read of the field; a borrow of the field instead once
    // `&self.field` is a place the checker and backends accept (BUG-033).
    pub(super) fn field_copy(&mut self, place: &HirExpr) -> Result<Leaf, LoweringError> {
        let place = self.rebase_place(place)?;
        let listed = receiver_path(&place)
            .and_then(|path| self.wrt.iter().find(|(listed, _)| *listed == path));
        if let Some((_, copy)) = listed {
            return Ok(copy.clone());
        }
        Ok(self.copy_of(place))
    }

    /// A fresh copy of the tensor at `place`, which is already rooted at a binding of the
    /// derivative function.
    pub(super) fn copy_of(&mut self, place: HirExpr) -> Leaf {
        let (ty, span) = (place.ty.clone(), place.span);
        let copy = clone_call(place, &ty);
        self.push(&ty, span, Op::Constant(copy))
    }

    /// The field chain `place` with its root renamed to the binding that holds it in the
    /// derivative function: `self` or a parameter as written, or, inside an inlined callee,
    /// the caller's value the parameter stands for.
    ///
    /// The root is a constant by construction. The tape builds no struct or array value and
    /// never differentiates one, so one it can reach is the receiver or a parameter, and the
    /// body cannot assign to either; every field it reads is the same value at every read.
    /// An array position must be a literal, so that a read of a `wrt:` element is known to
    /// be that element.
    pub(super) fn rebase_place(&mut self, place: &HirExpr) -> Result<HirExpr, LoweringError> {
        let kind = match &place.kind {
            HirExprKind::FieldAccess { object, field } => HirExprKind::FieldAccess {
                object: Box::new(self.rebase_place(object)?),
                field: field.clone(),
            },
            HirExprKind::Index { object, index } => {
                if literal_position(index).is_none() {
                    return Err(self.refuse("an array element at a computed position", index.span));
                }
                HirExprKind::Index {
                    object: Box::new(self.rebase_place(object)?),
                    index: index.clone(),
                }
            }
            HirExprKind::Variable(_) => match self.leaf(place)? {
                Leaf::Var { name, ty }
                    if matches!(ty.referent(), HirType::Struct(_) | HirType::Array { .. }) =>
                {
                    return Ok(HirExpr::new(HirExprKind::Variable(name), ty, place.span));
                }
                _ => {
                    return Err(self.refuse(
                        "a field or element of a value that is neither a struct nor an array",
                        place.span,
                    ))
                }
            },
            _ => return Err(self.refuse("a field of a computed value", place.span)),
        };
        Ok(HirExpr::new(kind, place.ty.clone(), place.span))
    }
}
