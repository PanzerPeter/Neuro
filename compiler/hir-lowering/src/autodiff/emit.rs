//! HIR builders for the derivative function's body.
//!
//! Every builder binds its result to a fresh `val` and hands back a [`Leaf`] naming it,
//! so no emitted expression nests another. That keeps ownership trivial for the backend:
//! each temporary is a binding it drops at the function's exit, and every tensor operand
//! is read through a borrow and never consumed. The one exception is [`Emitter::reshape`],
//! whose HIR node consumes its receiver by definition.

use ast_types::{BinaryOp, UnaryOp};
use neuro_hir::{
    static_shape, AxisNames, HirExpr, HirExprKind, HirReduceOp, HirStmt, HirTensorAxis, HirType,
};
use shared_types::{Literal, Span};

use super::tape::Leaf;
use crate::LoweringError;

/// The prefix of a reverse-pass temporary, distinct from the tape's own entries.
const TEMP_PREFIX: &str = "__ad_t";

/// The operand expression that reads `leaf` without consuming it: an owned tensor binding
/// is borrowed, a borrowed parameter and a scalar are read as they are.
pub(super) fn operand(leaf: &Leaf, span: Span) -> HirExpr {
    match leaf {
        Leaf::Const(expr) => expr.clone(),
        Leaf::Var { name, ty } => {
            let read = HirExpr::new(HirExprKind::Variable(name.clone()), ty.clone(), span);
            if !matches!(ty, HirType::Tensor { .. }) {
                return read;
            }
            HirExpr::new(
                HirExprKind::Reference {
                    operand: Box::new(read),
                    mutable: false,
                },
                HirType::Reference {
                    inner: Box::new(ty.clone()),
                    mutable: false,
                },
                span,
            )
        }
    }
}

/// The expression that MOVES `leaf` out of its binding, for the one place a value
/// changes owner: the returned bundle.
pub(super) fn operand_owned(leaf: &Leaf, span: Span) -> HirExpr {
    match leaf {
        Leaf::Const(expr) => expr.clone(),
        Leaf::Var { name, ty } => {
            HirExpr::new(HirExprKind::Variable(name.clone()), ty.clone(), span)
        }
    }
}

pub(super) fn tensor_type(element: &HirType, extents: &[usize]) -> HirType {
    HirType::Tensor {
        element: Box::new(element.clone()),
        shape: static_shape(extents),
        names: AxisNames::default(),
    }
}

/// A tensor type's element type and static extents.
pub(super) fn tensor_parts(ty: &HirType) -> Option<(&HirType, Vec<usize>)> {
    let HirType::Tensor { element, shape, .. } = ty.referent() else {
        return None;
    };
    let extents = shape.iter().copied().collect::<Option<Vec<_>>>()?;
    Some((element, extents))
}

/// The element type of a float value or of a float tensor.
pub(super) fn element_type(ty: &HirType) -> &HirType {
    match ty.referent() {
        HirType::Tensor { element, .. } => element,
        other => other,
    }
}

fn malformed(detail: &str) -> LoweringError {
    LoweringError::Malformed {
        detail: format!("derivative transform: {detail}"),
    }
}

/// The result type of an arithmetic operator the tape accepted, re-derived here because
/// the reverse pass builds operator applications the primal never wrote.
fn arithmetic_type(
    op: BinaryOp,
    left: &HirType,
    right: &HirType,
) -> Result<HirType, LoweringError> {
    let (l, r) = (tensor_parts(left), tensor_parts(right));
    match (l, r) {
        (Some((element, a)), Some((_, b))) if op == BinaryOp::MatMul => {
            let ([rows, _], [_, columns]) = (a.as_slice(), b.as_slice()) else {
                return Err(malformed("`@` on operands that are not matrices"));
            };
            Ok(tensor_type(element, &[*rows, *columns]))
        }
        (Some((element, a)), Some((_, b))) => {
            let rank = a.len().max(b.len());
            let mut joined = vec![0usize; rank];
            for (axis, slot) in joined.iter_mut().enumerate() {
                let pick = |extents: &[usize]| {
                    (axis + extents.len())
                        .checked_sub(rank)
                        .map_or(1, |k| extents[k])
                };
                let (x, y) = (pick(&a), pick(&b));
                *slot = match (x, y) {
                    _ if x == y => x,
                    (1, other) | (other, 1) => other,
                    _ => return Err(malformed("operands whose shapes do not broadcast")),
                };
            }
            Ok(tensor_type(element, &joined))
        }
        (Some(_), None) => Ok(left.referent().clone()),
        (None, Some(_)) => Ok(right.referent().clone()),
        (None, None) => Ok(left.referent().clone()),
    }
}

pub(super) struct Emitter {
    pub(super) stmts: Vec<HirStmt>,
    counter: usize,
    span: Span,
}

impl Emitter {
    pub(super) fn new(span: Span) -> Self {
        Self {
            stmts: Vec::new(),
            counter: 0,
            span,
        }
    }

    pub(super) fn span(&self) -> Span {
        self.span
    }

    /// Declare `name` bound to `init`, the form every value of the function takes.
    pub(super) fn declare(&mut self, name: String, init: HirExpr) {
        self.stmts.push(HirStmt::VarDecl {
            name,
            ty: init.ty.clone(),
            init: Some(init),
            mutable: false,
            span: self.span,
        });
    }

    fn bind(&mut self, kind: HirExprKind, ty: HirType) -> Leaf {
        let name = format!("{TEMP_PREFIX}{}", self.counter);
        self.counter += 1;
        self.declare(name.clone(), HirExpr::new(kind, ty.clone(), self.span));
        Leaf::Var { name, ty }
    }

    fn read(&self, leaf: &Leaf) -> Box<HirExpr> {
        Box::new(operand(leaf, self.span))
    }

    /// A float constant at `ty`, which is always a scalar element type.
    pub(super) fn float(&self, value: f64, ty: &HirType) -> Leaf {
        Leaf::Const(HirExpr::new(
            HirExprKind::Literal(Literal::Float(value, None)),
            ty.clone(),
            self.span,
        ))
    }

    pub(super) fn binary(
        &mut self,
        op: BinaryOp,
        left: &Leaf,
        right: &Leaf,
    ) -> Result<Leaf, LoweringError> {
        let ty = arithmetic_type(op, left.ty(), right.ty())?;
        let kind = HirExprKind::Binary {
            op,
            left: self.read(left),
            right: self.read(right),
        };
        Ok(self.bind(kind, ty))
    }

    /// `-operand`. The language has no unary minus on a tensor, so a tensor is scaled by
    /// `-1`, which is exact.
    pub(super) fn negate(&mut self, operand: &Leaf) -> Result<Leaf, LoweringError> {
        if tensor_parts(operand.ty()).is_some() {
            let minus_one = self.float(-1.0, element_type(operand.ty()));
            return self.binary(BinaryOp::Multiply, operand, &minus_one);
        }
        let kind = HirExprKind::Unary {
            op: UnaryOp::Negate,
            operand: self.read(operand),
        };
        Ok(self.bind(kind, operand.value_ty().clone()))
    }

    /// A fresh copy of a tensor, for an adjoint two owners would otherwise share.
    /// Multiplying by one is exact for every float, `-0.0` and NaN included.
    pub(super) fn copy(&mut self, tensor: &Leaf) -> Result<Leaf, LoweringError> {
        let one = self.float(1.0, element_type(tensor.ty()));
        self.binary(BinaryOp::Multiply, tensor, &one)
    }

    pub(super) fn sum_all(&mut self, tensor: &Leaf) -> Leaf {
        let kind = HirExprKind::TensorReduce {
            receiver: self.read(tensor),
            op: HirReduceOp::Sum,
            axis: None,
        };
        self.bind(kind, element_type(tensor.ty()).clone())
    }

    pub(super) fn sum_axis(&mut self, tensor: &Leaf, axis: usize) -> Result<Leaf, LoweringError> {
        let (element, mut extents) =
            tensor_parts(tensor.ty()).ok_or_else(|| malformed("an axis sum of a non-tensor"))?;
        if axis >= extents.len() {
            return Err(malformed("an axis sum past the tensor's rank"));
        }
        let _ = extents.remove(axis);
        let ty = tensor_type(element, &extents);
        let kind = HirExprKind::TensorReduce {
            receiver: self.read(tensor),
            op: HirReduceOp::Sum,
            axis: Some(axis),
        };
        Ok(self.bind(kind, ty))
    }

    /// Re-describe `tensor`'s buffer at `ty`, CONSUMING it: the node moves the buffer into
    /// the result. Callers pass an adjoint no other rule reads again.
    pub(super) fn reshape(&mut self, tensor: &Leaf, ty: HirType) -> Result<Leaf, LoweringError> {
        let Leaf::Var { name, ty: from } = tensor else {
            return Err(malformed("a reshape of a constant"));
        };
        let receiver = HirExpr::new(HirExprKind::Variable(name.clone()), from.clone(), self.span);
        let kind = HirExprKind::TensorShapeCast {
            receiver: Box::new(receiver),
            permutation: None,
        };
        Ok(self.bind(kind, ty))
    }

    /// A tensor of type `ty` with `value` at every element. The fill node takes a
    /// constant only, so a computed value is broadcast over a fill of ones instead.
    pub(super) fn fill(&mut self, value: &Leaf, ty: &HirType) -> Result<Leaf, LoweringError> {
        if let Leaf::Const(_) = value {
            let kind = HirExprKind::TensorFill {
                value: self.read(value),
            };
            return Ok(self.bind(kind, ty.clone()));
        }
        let one = self.float(1.0, element_type(ty));
        let ones = self.fill(&one, ty)?;
        self.binary(BinaryOp::Multiply, &ones, value)
    }

    pub(super) fn zeros(&mut self, ty: &HirType) -> Leaf {
        let zero = self.float(0.0, element_type(ty));
        let kind = HirExprKind::TensorFill {
            value: self.read(&zero),
        };
        self.bind(kind, ty.clone())
    }

    pub(super) fn literal(&mut self, elements: &[Leaf], ty: &HirType) -> Leaf {
        let kind = HirExprKind::TensorLiteral {
            elements: elements
                .iter()
                .map(|leaf| operand(leaf, self.span))
                .collect(),
        };
        self.bind(kind, ty.clone())
    }

    /// The element of `tensor` at row-major offset `flat`.
    pub(super) fn element(&mut self, tensor: &Leaf, flat: usize) -> Result<Leaf, LoweringError> {
        let (element, extents) =
            tensor_parts(tensor.ty()).ok_or_else(|| malformed("an element of a non-tensor"))?;
        // A rank-0 tensor has no axis to name a position on; its only element is its sum.
        if extents.is_empty() {
            return Ok(self.sum_all(tensor));
        }
        let mut positions = vec![0usize; extents.len()];
        let mut rest = flat;
        for (slot, extent) in positions.iter_mut().zip(&extents).rev() {
            *slot = rest % extent;
            rest /= extent;
        }
        let axes = positions
            .into_iter()
            .map(|position| {
                HirTensorAxis::Position(HirExpr::new(
                    HirExprKind::Literal(Literal::Integer(position as i128, None)),
                    HirType::I64,
                    self.span,
                ))
            })
            .collect();
        let kind = HirExprKind::TensorIndex {
            object: self.read(tensor),
            axes,
        };
        Ok(self.bind(kind, element.clone()))
    }

    pub(super) fn einsum(
        &mut self,
        operands: [&Leaf; 2],
        inputs: Vec<Vec<usize>>,
        output: Vec<usize>,
        extents: Vec<usize>,
        ty: HirType,
    ) -> Leaf {
        let kind = HirExprKind::TensorEinsum {
            operands: operands
                .iter()
                .map(|leaf| operand(leaf, self.span))
                .collect(),
            inputs,
            output,
            extents,
        };
        self.bind(kind, ty)
    }
}
