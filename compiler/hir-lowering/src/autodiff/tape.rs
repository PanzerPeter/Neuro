//! Linearization of a `@grad` body into a tape of single-operation bindings.
//!
//! Every value the body computes becomes one [`Entry`] whose operands are [`Leaf`]s: a
//! named value or a scalar constant written in place. The reverse sweep then needs no
//! expression walking at all, only the tape backwards.
//!
//! The rule set is closed on purpose. A construct outside it is refused with its span
//! even when it would be inactive, because the forward replay re-emits the whole body
//! with every tensor operand BORROWED (the reverse pass reads operands again after the
//! forward ops that, in the primal, might have consumed them), and only a construct this
//! module rebuilds can be re-emitted that way.

use std::collections::{HashMap, HashSet};

use ast_types::{BinaryOp, UnaryOp};
use neuro_hir::{HirExpr, HirExprKind, HirReduceOp, HirStmt, HirTensorAxis, HirType};
use shared_types::{Literal, Span};

use crate::LoweringError;

/// The prefix of a tape entry's generated name. User names may not contain `__`, so no
/// entry can shadow a binding the body declared.
const ENTRY_PREFIX: &str = "__ad_v";

/// A tape operand.
#[derive(Debug, Clone)]
pub(super) enum Leaf {
    /// A tape entry, a parameter, or a module constant, at the type it is read at: a
    /// parameter keeps its reference type.
    Var { name: String, ty: HirType },
    /// A scalar literal, which is `Copy` and can be written wherever it is needed.
    Const(HirExpr),
}

impl Leaf {
    pub(super) fn ty(&self) -> &HirType {
        match self {
            Leaf::Var { ty, .. } => ty,
            Leaf::Const(expr) => &expr.ty,
        }
    }

    /// The type of the value itself, looking through a borrowed parameter.
    pub(super) fn value_ty(&self) -> &HirType {
        self.ty().referent()
    }

    pub(super) fn var_name(&self) -> Option<&str> {
        match self {
            Leaf::Var { name, .. } => Some(name),
            Leaf::Const(_) => None,
        }
    }
}

/// The one operation a tape entry performs.
#[derive(Debug, Clone)]
pub(super) enum Op {
    Binary {
        op: BinaryOp,
        left: Leaf,
        right: Leaf,
    },
    Negate(Leaf),
    Reduce {
        receiver: Leaf,
        op: HirReduceOp,
        axis: Option<usize>,
    },
    /// A tensor built from scalar elements, `Tensor::scalar(v)` among them.
    Literal(Vec<Leaf>),
    /// One element read at literal positions; `flat` is its row-major offset.
    Read {
        object: Leaf,
        axes: Vec<HirTensorAxis>,
        flat: usize,
    },
    /// A value with no operand at all (`zeros()`, `ones()`, `identity()`), replayed as
    /// written.
    Constant(HirExpr),
}

#[derive(Debug, Clone)]
pub(super) struct Entry {
    pub(super) name: String,
    pub(super) ty: HirType,
    pub(super) op: Op,
    pub(super) span: Span,
    /// Whether the value depends on a differentiated parameter, and so has an adjoint.
    pub(super) active: bool,
}

pub(super) struct Tape {
    pub(super) entries: Vec<Entry>,
    pub(super) active: HashSet<String>,
    pub(super) loss: Leaf,
}

/// Linearize `body`. `differentiated` names the parameters the derivative is taken with
/// respect to; they seed the activity set.
pub(super) fn linearize(
    function: &str,
    body: &[HirStmt],
    differentiated: &[&str],
) -> Result<Tape, LoweringError> {
    let mut linearizer = Linearizer {
        function,
        entries: Vec::new(),
        aliases: HashMap::new(),
        active: differentiated.iter().map(|name| name.to_string()).collect(),
    };
    let mut loss = None;
    for (index, stmt) in body.iter().enumerate() {
        let is_last = index + 1 == body.len();
        match stmt {
            HirStmt::VarDecl {
                name,
                init: Some(init),
                mutable: false,
                ..
            } => {
                let leaf = linearizer.leaf(init)?;
                let _ = linearizer.aliases.insert(name.clone(), leaf);
            }
            HirStmt::Return {
                value: Some(value), ..
            }
            | HirStmt::Expr(value)
                if is_last =>
            {
                loss = Some(linearizer.leaf(value)?);
            }
            other => {
                return Err(linearizer.refuse(describe_stmt(other), stmt_span(other)));
            }
        }
    }
    let loss = loss.ok_or_else(|| LoweringError::Malformed {
        detail: format!("`@grad` function '{function}' has no final loss expression"),
    })?;
    Ok(Tape {
        entries: linearizer.entries,
        active: linearizer.active,
        loss,
    })
}

struct Linearizer<'f> {
    function: &'f str,
    entries: Vec<Entry>,
    /// Each body binding, resolved to the leaf that holds its value. A binding is never a
    /// tape entry of its own: `val y = x` is `x` under a second name.
    aliases: HashMap<String, Leaf>,
    active: HashSet<String>,
}

impl Linearizer<'_> {
    fn refuse(&self, construct: &str, span: Span) -> LoweringError {
        LoweringError::NotDifferentiable {
            function: self.function.to_string(),
            construct: construct.to_string(),
            span,
        }
    }

    fn is_active(&self, leaf: &Leaf) -> bool {
        leaf.var_name()
            .is_some_and(|name| self.active.contains(name))
    }

    fn push(&mut self, expr: &HirExpr, op: Op) -> Leaf {
        let active = match &op {
            Op::Binary { left, right, .. } => self.is_active(left) || self.is_active(right),
            Op::Negate(operand) => self.is_active(operand),
            Op::Reduce { receiver, .. } => self.is_active(receiver),
            Op::Literal(elements) => elements.iter().any(|leaf| self.is_active(leaf)),
            Op::Read { object, .. } => self.is_active(object),
            Op::Constant(_) => false,
        };
        let name = format!("{ENTRY_PREFIX}{}", self.entries.len());
        if active {
            let _ = self.active.insert(name.clone());
        }
        self.entries.push(Entry {
            name: name.clone(),
            ty: expr.ty.clone(),
            op,
            span: expr.span,
            active,
        });
        Leaf::Var {
            name,
            ty: expr.ty.clone(),
        }
    }

    fn leaf(&mut self, expr: &HirExpr) -> Result<Leaf, LoweringError> {
        match &expr.kind {
            HirExprKind::Literal(_) if is_scalar(&expr.ty) => Ok(Leaf::Const(expr.clone())),
            HirExprKind::Variable(name) => {
                Ok(self
                    .aliases
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| Leaf::Var {
                        name: name.clone(),
                        ty: expr.ty.clone(),
                    }))
            }
            // Reading through `&x` reads `x`; the replay borrows every tensor operand anyway.
            HirExprKind::Reference {
                operand,
                mutable: false,
            } if matches!(operand.kind, HirExprKind::Variable(_)) => self.leaf(operand),
            HirExprKind::Binary { op, left, right }
                if is_arithmetic(*op) && is_float_valued(&expr.ty) =>
            {
                let left = self.leaf(left)?;
                let right = self.leaf(right)?;
                Ok(self.push(
                    expr,
                    Op::Binary {
                        op: *op,
                        left,
                        right,
                    },
                ))
            }
            HirExprKind::Unary {
                op: UnaryOp::Negate,
                operand,
            } if is_float_valued(&expr.ty) => {
                let operand = self.leaf(operand)?;
                Ok(self.push(expr, Op::Negate(operand)))
            }
            HirExprKind::TensorReduce {
                receiver,
                op: op @ (HirReduceOp::Sum | HirReduceOp::Mean),
                axis,
            } if is_float_valued(&expr.ty) => {
                let receiver = self.leaf(receiver)?;
                Ok(self.push(
                    expr,
                    Op::Reduce {
                        receiver,
                        op: *op,
                        axis: *axis,
                    },
                ))
            }
            HirExprKind::TensorLiteral { elements } if is_float_valued(&expr.ty) => {
                let elements = elements
                    .iter()
                    .map(|element| self.leaf(element))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(self.push(expr, Op::Literal(elements)))
            }
            HirExprKind::TensorFill { value } if matches!(value.kind, HirExprKind::Literal(_)) => {
                Ok(self.push(expr, Op::Constant(expr.clone())))
            }
            HirExprKind::TensorIdentity => Ok(self.push(expr, Op::Constant(expr.clone()))),
            HirExprKind::TensorIndex { object, axes } if is_scalar(&expr.ty) => {
                let Some(flat) = literal_offset(&object.ty, axes) else {
                    return Err(self.refuse("an element read at a computed position", expr.span));
                };
                let object = self.leaf(object)?;
                Ok(self.push(
                    expr,
                    Op::Read {
                        object,
                        axes: axes.clone(),
                        flat,
                    },
                ))
            }
            _ => Err(self.refuse(describe_expr(expr), expr.span)),
        }
    }
}

fn is_arithmetic(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::MatMul
    )
}

fn is_float(ty: &HirType) -> bool {
    matches!(ty, HirType::F32 | HirType::F64)
}

fn is_scalar(ty: &HirType) -> bool {
    !matches!(
        ty,
        HirType::Tensor { .. }
            | HirType::Reference { .. }
            | HirType::String
            | HirType::Void
            | HirType::Struct(_)
            | HirType::Enum(_)
            | HirType::Tuple(_)
            | HirType::Array { .. }
            | HirType::Collection { .. }
    )
}

/// Whether a value of `ty` has a derivative: a float, or a tensor of floats.
fn is_float_valued(ty: &HirType) -> bool {
    match ty.referent() {
        HirType::Tensor { element, .. } => is_float(element),
        other => is_float(other),
    }
}

/// The row-major offset an element read names, when every axis is a literal position
/// inside its extent. A read the compiler cannot place has no single element to send the
/// adjoint back to.
fn literal_offset(object_ty: &HirType, axes: &[HirTensorAxis]) -> Option<usize> {
    let HirType::Tensor { shape, .. } = object_ty.referent() else {
        return None;
    };
    if shape.len() != axes.len() {
        return None;
    }
    let mut flat = 0usize;
    for (axis, extent) in axes.iter().zip(shape) {
        let HirTensorAxis::Position(position) = axis else {
            return None;
        };
        let HirExprKind::Literal(Literal::Integer(value, _)) = position.kind else {
            return None;
        };
        let extent = (*extent)?;
        let index = usize::try_from(value)
            .ok()
            .filter(|index| *index < extent)?;
        flat = flat * extent + index;
    }
    Some(flat)
}

fn stmt_span(stmt: &HirStmt) -> Span {
    match stmt {
        HirStmt::VarDecl { span, .. }
        | HirStmt::Assign { span, .. }
        | HirStmt::TensorCompoundAssign { span, .. }
        | HirStmt::Return { span, .. }
        | HirStmt::If { span, .. }
        | HirStmt::While { span, .. }
        | HirStmt::ForRange { span, .. }
        | HirStmt::ForEach { span, .. }
        | HirStmt::Break { span, .. }
        | HirStmt::Continue { span, .. }
        | HirStmt::ValElse { span, .. }
        | HirStmt::Const { span, .. } => *span,
        HirStmt::Expr(expr) => expr.span,
    }
}

fn describe_stmt(stmt: &HirStmt) -> &'static str {
    match stmt {
        HirStmt::VarDecl { mutable: true, .. } => "a `mut` binding",
        HirStmt::VarDecl { .. } => "a binding without an initializer",
        HirStmt::Assign { .. } | HirStmt::TensorCompoundAssign { .. } => "an assignment",
        HirStmt::Return { .. } => "a `return` before the end of the body",
        HirStmt::If { .. } => "an `if`",
        HirStmt::While { .. } | HirStmt::ForRange { .. } | HirStmt::ForEach { .. } => "a loop",
        HirStmt::Break { .. } | HirStmt::Continue { .. } => "a loop jump",
        HirStmt::ValElse { .. } => "a `val ... else` binding",
        HirStmt::Const { .. } => "a local `const`",
        HirStmt::Expr(_) => "an expression statement",
    }
}

fn describe_expr(expr: &HirExpr) -> &'static str {
    match &expr.kind {
        HirExprKind::Call { .. } => "a call",
        HirExprKind::Binary { .. } | HirExprKind::Unary { .. } => {
            "an operator on a value that has no derivative"
        }
        HirExprKind::TensorReduce { .. } => "a `.max()` / `.min()` reduction",
        HirExprKind::TensorEinsum { .. } => "an `einsum` contraction",
        HirExprKind::TensorShapeCast { .. } => "a shape change",
        HirExprKind::TensorIndex { .. } => "a tensor slice",
        HirExprKind::TensorApply { .. } => "a `.map` / `.zip` / `.reduce` traversal",
        HirExprKind::TensorSort { .. } => "a sort",
        HirExprKind::TensorRandomNormal { .. } => "a random tensor",
        HirExprKind::Cast { .. } => "a cast",
        HirExprKind::If { .. } | HirExprKind::Match { .. } => "a branch",
        HirExprKind::Loop { .. } => "a loop",
        HirExprKind::Block { .. } | HirExprKind::Unsafe { .. } | HirExprKind::Pool { .. } => {
            "a block"
        }
        HirExprKind::Reference { .. } | HirExprKind::Deref { .. } => {
            "a borrow of anything but a binding"
        }
        _ => "this expression",
    }
}
