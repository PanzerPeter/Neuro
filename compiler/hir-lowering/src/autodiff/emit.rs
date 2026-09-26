//! HIR builders for the derivative function's body.
//!
//! Every builder binds its result to a fresh `val` and hands back a [`Leaf`] naming it,
//! so no emitted expression nests another. That keeps ownership trivial for the backend:
//! each temporary is a binding it drops at the function's exit, and every tensor operand
//! is read through a borrow and never consumed. The one exception is [`Emitter::reshape`],
//! whose HIR node consumes its receiver by definition.

use ast_types::{BinaryOp, UnaryOp};
use neuro_hir::{
    static_shape, AxisNames, HirExpr, HirExprKind, HirMathOp, HirPlace, HirReduceOp, HirStmt,
    HirTensorAxis, HirType,
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
        // No rule reads a function value (a call through one is inlined, and a slot
        // refuses one), so this only keeps the match total. It names the target.
        Leaf::Function { target, ty, .. } => {
            HirExpr::new(HirExprKind::Variable(target.clone()), ty.clone(), span)
        }
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
        Leaf::Function { target, ty, .. } => {
            HirExpr::new(HirExprKind::Variable(target.clone()), ty.clone(), span)
        }
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

    pub(super) fn push(&mut self, stmt: HirStmt) {
        self.stmts.push(stmt);
    }

    /// Run `body` against an empty statement list and hand back what it emitted: the
    /// statements of a nested block.
    pub(super) fn nested<R>(
        &mut self,
        body: impl FnOnce(&mut Self) -> Result<R, LoweringError>,
    ) -> Result<(Vec<HirStmt>, R), LoweringError> {
        let outer = std::mem::take(&mut self.stmts);
        let result = body(self);
        let inner = std::mem::replace(&mut self.stmts, outer);
        Ok((inner, result?))
    }

    /// Declare `name` bound to `init`, the form every value of the function takes.
    pub(super) fn declare(&mut self, name: String, init: HirExpr) {
        self.declare_as(name, init, false);
    }

    /// Declare the mutable binding `name`: a slot, a running adjoint, or a counter.
    pub(super) fn declare_mut(&mut self, name: String, init: HirExpr) {
        self.declare_as(name, init, true);
    }

    fn declare_as(&mut self, name: String, init: HirExpr, mutable: bool) {
        self.stmts.push(HirStmt::VarDecl {
            name,
            ty: init.ty.clone(),
            init: Some(init),
            mutable,
            span: self.span,
        });
    }

    /// `name = value`, for a binding `declare_mut` made.
    pub(super) fn assign(&mut self, name: &str, ty: &HirType, value: HirExpr) {
        self.stmts.push(HirStmt::Assign {
            place: HirPlace::Var {
                name: name.to_string(),
                ty: ty.clone(),
            },
            value,
            span: self.span,
        });
    }

    pub(super) fn fresh(&mut self) -> String {
        let name = format!("{TEMP_PREFIX}{}", self.counter);
        self.counter += 1;
        name
    }

    fn bind(&mut self, kind: HirExprKind, ty: HirType) -> Leaf {
        let name = self.fresh();
        self.declare(name.clone(), HirExpr::new(kind, ty.clone(), self.span));
        Leaf::Var { name, ty }
    }

    /// A fresh binding holding `leaf`'s current value, for a value about to be moved or
    /// about to change: a tensor is copied, a scalar read, a constant kept as written.
    pub(super) fn snapshot(&mut self, leaf: &Leaf) -> Result<Leaf, LoweringError> {
        match leaf {
            Leaf::Const(_) | Leaf::Function { .. } => Ok(leaf.clone()),
            Leaf::Var { .. } if tensor_parts(leaf.ty()).is_some() => self.copy(leaf),
            Leaf::Var { name, ty } => {
                let read = HirExprKind::Variable(name.clone());
                Ok(self.bind(read, ty.clone()))
            }
        }
    }

    /// The zero of `ty`: the placeholder a slot starts from and the adjoint of a value
    /// nothing reached.
    pub(super) fn zero(&self, ty: &HirType) -> Result<HirExpr, LoweringError> {
        let literal = |value: Literal, ty: &HirType| {
            HirExpr::new(HirExprKind::Literal(value), ty.clone(), self.span)
        };
        let kind = match ty {
            HirType::Tensor { element, .. } => HirExprKind::TensorFill {
                value: Box::new(literal(Literal::Float(0.0, None), element)),
            },
            HirType::F32 | HirType::F64 => return Ok(literal(Literal::Float(0.0, None), ty)),
            HirType::Bool => return Ok(literal(Literal::Boolean(false), ty)),
            HirType::I8
            | HirType::I16
            | HirType::I32
            | HirType::I64
            | HirType::U8
            | HirType::U16
            | HirType::U32
            | HirType::U64 => return Ok(literal(Literal::Integer(0, None), ty)),
            _ => return Err(malformed("a zero of a type no slot holds")),
        };
        Ok(HirExpr::new(kind, ty.clone(), self.span))
    }

    /// An iteration counter's value `value`.
    pub(super) fn count(&self, value: i128) -> HirExpr {
        HirExpr::new(
            HirExprKind::Literal(Literal::Integer(value, None)),
            HirType::I64,
            self.span,
        )
    }

    fn counter(&self, name: &str) -> Box<HirExpr> {
        Box::new(HirExpr::new(
            HirExprKind::Variable(name.to_string()),
            HirType::I64,
            self.span,
        ))
    }

    /// `name = name + step` on an iteration counter.
    pub(super) fn step(&mut self, name: &str, step: i128) {
        let next = HirExpr::new(
            HirExprKind::Binary {
                op: BinaryOp::Add,
                left: self.counter(name),
                right: Box::new(self.count(step)),
            },
            HirType::I64,
            self.span,
        );
        self.assign(name, &HirType::I64, next);
    }

    /// `left op right` on two iteration counters, or on one and a constant.
    pub(super) fn compare(&self, op: BinaryOp, left: &str, right: Option<&str>) -> HirExpr {
        let right = match right {
            Some(name) => self.counter(name),
            None => Box::new(self.count(0)),
        };
        HirExpr::new(
            HirExprKind::Binary {
                op,
                left: self.counter(left),
                right,
            },
            HirType::Bool,
            self.span,
        )
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
        self.shape_cast(tensor, ty, None)
    }

    /// [`Emitter::reshape`], reordering the axes when `permutation` is given: result axis
    /// `d` is `tensor`'s axis `permutation[d]`. Consumes `tensor` the same way.
    pub(super) fn shape_cast(
        &mut self,
        tensor: &Leaf,
        ty: HirType,
        permutation: Option<Vec<usize>>,
    ) -> Result<Leaf, LoweringError> {
        let Leaf::Var { name, ty: from } = tensor else {
            return Err(malformed("a reshape of a constant"));
        };
        let receiver = HirExpr::new(HirExprKind::Variable(name.clone()), from.clone(), self.span);
        let kind = HirExprKind::TensorShapeCast {
            receiver: Box::new(receiver),
            permutation,
        };
        Ok(self.bind(kind, ty))
    }

    /// `op` applied to `operand`, a float scalar or a float tensor, raised to `exponent`
    /// for a `Pow`. The result has the operand's value type.
    pub(super) fn math(&mut self, op: HirMathOp, operand: &Leaf, exponent: Option<&Leaf>) -> Leaf {
        let kind = HirExprKind::Math {
            op,
            operand: self.read(operand),
            exponent: exponent.map(|exponent| self.read(exponent)),
        };
        self.bind(kind, operand.value_ty().clone())
    }

    /// `value as ty`, between two integer or float types.
    pub(super) fn convert(&mut self, value: &Leaf, ty: &HirType) -> Leaf {
        let kind = HirExprKind::Cast {
            value: self.read(value),
        };
        self.bind(kind, ty.clone())
    }

    /// A zero tensor of type `ty` holding `value` at `positions`, which may be known only
    /// at run time: the adjoint of an element read the compiler cannot place.
    pub(super) fn scatter(
        &mut self,
        value: &Leaf,
        positions: &[Leaf],
        ty: &HirType,
    ) -> Result<Leaf, LoweringError> {
        let (element, _) =
            tensor_parts(ty).ok_or_else(|| malformed("an element write into a non-tensor"))?;
        let name = self.fresh();
        let zero = self.zero(ty)?;
        self.declare_mut(name.clone(), zero);
        let axes = positions
            .iter()
            .map(|leaf| HirTensorAxis::Position(operand(leaf, self.span)))
            .collect();
        self.stmts.push(HirStmt::Assign {
            place: HirPlace::TensorIndex {
                object: Box::new(HirExpr::new(
                    HirExprKind::Variable(name.clone()),
                    ty.clone(),
                    self.span,
                )),
                axes,
                ty: element.clone(),
            },
            value: operand(value, self.span),
            span: self.span,
        });
        Ok(Leaf::Var {
            name,
            ty: ty.clone(),
        })
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
        operands: &[&Leaf],
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
