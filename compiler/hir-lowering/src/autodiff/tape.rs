//! Linearization of a `@grad` body into a tape of single-operation bindings.
//!
//! Every value the body computes becomes one [`Entry`] whose operands are [`Leaf`]s: a
//! named value or a scalar constant written in place. Control flow keeps its structure:
//! an `if` becomes a [`Branch`] holding one tape per arm, a `while` a [`Loop`] holding the
//! tapes of its condition and body. The reverse sweep then walks tapes backwards and
//! never has to walk an expression.
//!
//! Mutation is versioning. Each assignment rebinds the name to the leaf of its new value,
//! so within one straight-line tape every value keeps its own binding. A binding an arm
//! or a loop body reassigns leaves the construct through a [`Slot`], the one mutable
//! binding the derivative function declares per such value.
//!
//! The rule set is closed on purpose. A construct outside it is refused with its span
//! even when it would be inactive, because the forward replay re-emits the whole body
//! with every tensor operand BORROWED (the reverse pass reads operands again after the
//! forward ops that, in the primal, might have consumed them), and only a construct this
//! module rebuilds can be re-emitted that way.

use std::collections::{HashMap, HashSet};

use ast_types::{BinaryOp, UnaryOp};
use neuro_hir::{HirExpr, HirExprKind, HirPlace, HirReduceOp, HirStmt, HirTensorAxis, HirType};
use shared_types::{Literal, Span};

use crate::LoweringError;

/// The prefix of every name the tape generates. User names may not contain `__`, so no
/// generated name can shadow a binding the body declared.
const ENTRY_PREFIX: &str = "__ad_v";

/// A tape operand.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Leaf {
    /// A tape entry, a slot, a parameter, or a module constant, at the type it is read
    /// at: a parameter keeps its reference type.
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
    Unary {
        op: UnaryOp,
        operand: Leaf,
    },
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

/// A value a branch or a loop leaves behind: the value of an `if` expression, or a
/// binding an arm or the loop body reassigns. It lives in a mutable binding of its own,
/// declared before the construct, because its value is decided inside it.
#[derive(Debug, Clone)]
pub(super) struct Slot {
    pub(super) name: String,
    pub(super) ty: HirType,
    pub(super) active: bool,
}

impl Slot {
    pub(super) fn leaf(&self) -> Leaf {
        Leaf::Var {
            name: self.name.clone(),
            ty: self.ty.clone(),
        }
    }
}

/// One arm of a [`Branch`]: its tape, and the value it gives each of the branch's merges.
#[derive(Debug)]
pub(super) struct Arm {
    pub(super) nodes: Vec<Node>,
    pub(super) outs: Vec<Leaf>,
}

/// An `if`. An `else if` chain is an `else` arm holding the next branch, and a missing
/// `else` is an empty arm.
#[derive(Debug)]
pub(super) struct Branch {
    pub(super) condition: Leaf,
    pub(super) merges: Vec<Slot>,
    pub(super) arms: [Arm; 2],
    pub(super) span: Span,
}

/// A binding a loop body reassigns: its slot, and the value it held before the loop.
#[derive(Debug)]
pub(super) struct Carried {
    pub(super) slot: Slot,
    pub(super) entry: Leaf,
}

/// A `while`. The body reads each carried value through its slot and leaves the next
/// iteration's value in `outs`; `count` names the iteration counter the forward pass
/// keeps, which is how the reverse pass knows how many iterations to undo.
#[derive(Debug)]
pub(super) struct Loop {
    pub(super) carried: Vec<Carried>,
    pub(super) condition: Vec<Node>,
    pub(super) test: Leaf,
    pub(super) body: Vec<Node>,
    pub(super) outs: Vec<Leaf>,
    pub(super) count: String,
    pub(super) span: Span,
}

#[derive(Debug)]
pub(super) enum Node {
    Entry(Entry),
    Branch(Branch),
    Loop(Loop),
}

pub(super) struct Tape {
    pub(super) nodes: Vec<Node>,
    pub(super) active: HashSet<String>,
    pub(super) loss: Leaf,
    /// Every slot's name. A slot is reassigned after the forward pass (a loop's reverse
    /// pass rebuilds its carried values in place), so a loss held in one must be copied.
    pub(super) slots: HashSet<String>,
}

/// Linearize `body`, whose final value has type `loss_ty`. `differentiated` names the
/// parameters the derivative is taken with respect to; they seed the activity set.
pub(super) fn linearize(
    function: &str,
    body: &[HirStmt],
    differentiated: &[&str],
    loss_ty: &HirType,
) -> Result<Tape, LoweringError> {
    let mut linearizer = Linearizer {
        function,
        nodes: Vec::new(),
        aliases: HashMap::new(),
        scopes: Vec::new(),
        active: differentiated.iter().map(|name| name.to_string()).collect(),
        slots: HashSet::new(),
        next: 0,
    };
    let loss = linearizer.body_value(body, loss_ty)?;
    Ok(Tape {
        nodes: linearizer.nodes,
        active: linearizer.active,
        loss,
        slots: linearizer.slots,
    })
}

/// The bindings one arm or loop body declares, so that leaving it can undo them.
#[derive(Default)]
struct Scope {
    declared: HashSet<String>,
    /// The value an outer binding had when a binding of this scope shadowed it.
    shadowed: HashMap<String, Leaf>,
}

/// A nested statement list, linearized: its tape, its value when it has one, and what
/// each binding visible outside it holds at its end.
struct Scoped {
    nodes: Vec<Node>,
    value: Option<Leaf>,
    exit: HashMap<String, Leaf>,
}

/// The body of an arm, as `fork` runs it.
type ArmBody<'a, 'f> =
    &'a mut dyn FnMut(&mut Linearizer<'f>) -> Result<Option<Leaf>, LoweringError>;

struct Linearizer<'f> {
    function: &'f str,
    nodes: Vec<Node>,
    /// Each body binding, resolved to the leaf that holds its current value. A binding is
    /// never a tape entry of its own: `val y = x` is `x` under a second name.
    aliases: HashMap<String, Leaf>,
    scopes: Vec<Scope>,
    active: HashSet<String>,
    slots: HashSet<String>,
    next: usize,
}

impl<'f> Linearizer<'f> {
    fn refuse(&self, construct: &str, span: Span) -> LoweringError {
        LoweringError::NotDifferentiable {
            function: self.function.to_string(),
            construct: construct.to_string(),
            span,
        }
    }

    fn malformed(&self, detail: &str) -> LoweringError {
        LoweringError::Malformed {
            detail: format!("`@grad` function '{}': {detail}", self.function),
        }
    }

    fn fresh(&mut self) -> String {
        let name = format!("{ENTRY_PREFIX}{}", self.next);
        self.next += 1;
        name
    }

    fn is_active(&self, leaf: &Leaf) -> bool {
        leaf.var_name()
            .is_some_and(|name| self.active.contains(name))
    }

    fn push(&mut self, ty: &HirType, span: Span, op: Op) -> Leaf {
        let active = is_float_valued(ty)
            && match &op {
                Op::Binary { left, right, .. } => self.is_active(left) || self.is_active(right),
                Op::Unary { operand, .. } => self.is_active(operand),
                Op::Reduce { receiver, .. } => self.is_active(receiver),
                Op::Literal(elements) => elements.iter().any(|leaf| self.is_active(leaf)),
                Op::Read { object, .. } => self.is_active(object),
                Op::Constant(_) => false,
            };
        let name = self.fresh();
        if active {
            let _ = self.active.insert(name.clone());
        }
        self.nodes.push(Node::Entry(Entry {
            name: name.clone(),
            ty: ty.clone(),
            op,
            span,
            active,
        }));
        Leaf::Var {
            name,
            ty: ty.clone(),
        }
    }

    /// Bind a declared name, remembering what it shadows so the enclosing scope gets the
    /// outer binding back.
    fn declare(&mut self, name: &str, leaf: Leaf) {
        if let Some(scope) = self.scopes.last_mut() {
            if scope.declared.insert(name.to_string()) {
                if let Some(outer) = self.aliases.get(name) {
                    let _ = scope.shadowed.insert(name.to_string(), outer.clone());
                }
            }
        }
        let _ = self.aliases.insert(name.to_string(), leaf);
    }

    /// A new slot of type `ty` holding one of `values`, active when any of them is.
    fn slot(&mut self, ty: &HirType, values: &[&Leaf], span: Span) -> Result<Slot, LoweringError> {
        if !is_slot_type(ty) {
            return Err(self.refuse(
                "a value of this type carried out of a branch or a loop",
                span,
            ));
        }
        let name = self.fresh();
        let active = is_float_valued(ty) && values.iter().any(|value| self.is_active(value));
        if active {
            let _ = self.active.insert(name.clone());
        }
        let _ = self.slots.insert(name.clone());
        Ok(Slot {
            name,
            ty: ty.clone(),
            active,
        })
    }

    /// Run `body` as a nested statement list starting from the bindings `before`.
    fn scoped(
        &mut self,
        before: &HashMap<String, Leaf>,
        body: ArmBody<'_, 'f>,
    ) -> Result<Scoped, LoweringError> {
        self.aliases = before.clone();
        let outer = std::mem::take(&mut self.nodes);
        self.scopes.push(Scope::default());
        let value = body(self);
        let mut scope = self.scopes.pop().unwrap_or_default();
        let nodes = std::mem::replace(&mut self.nodes, outer);
        let value = value?;
        let mut exit = std::mem::take(&mut self.aliases);
        for name in scope.declared {
            match scope.shadowed.remove(&name) {
                Some(outer) => {
                    let _ = exit.insert(name, outer);
                }
                None => {
                    let _ = exit.remove(&name);
                }
            }
        }
        Ok(Scoped { nodes, value, exit })
    }

    /// The value a function-level statement list returns. `if c { ...; return a }`
    /// followed by the rest of the body is `if c { ...; a } else { rest }`, so an early
    /// return is a branch whose value is the loss.
    fn body_value(&mut self, stmts: &[HirStmt], ty: &HirType) -> Result<Leaf, LoweringError> {
        for (index, stmt) in stmts.iter().enumerate() {
            match stmt {
                HirStmt::Return {
                    value: Some(value), ..
                } => return self.leaf(value),
                HirStmt::Expr(value) if index + 1 == stmts.len() => return self.leaf(value),
                HirStmt::If {
                    condition,
                    then_block,
                    else_if_blocks,
                    else_block,
                    span,
                } if else_if_blocks.is_empty() && returns(then_block) => {
                    let mut rest = else_block.clone().unwrap_or_default();
                    rest.extend_from_slice(&stmts[index + 1..]);
                    let condition = self.leaf(condition)?;
                    let value = self.fork(
                        condition,
                        Some(ty),
                        [
                            &mut |this: &mut Self| this.body_value(then_block, ty).map(Some),
                            &mut |this: &mut Self| this.body_value(&rest, ty).map(Some),
                        ],
                        *span,
                    )?;
                    return value.ok_or_else(|| self.malformed("an early return has no value"));
                }
                other => self.stmt(other)?,
            }
        }
        Err(self.malformed("the body has no final loss expression"))
    }

    fn block(&mut self, stmts: &[HirStmt]) -> Result<(), LoweringError> {
        stmts.iter().try_for_each(|stmt| self.stmt(stmt))
    }

    /// The statements of an arm; when the `if` is an expression of type `value_ty`, the
    /// arm's trailing expression is its value.
    fn arm(
        &mut self,
        stmts: &[HirStmt],
        value_ty: Option<&HirType>,
    ) -> Result<Option<Leaf>, LoweringError> {
        let Some(_) = value_ty else {
            self.block(stmts)?;
            return Ok(None);
        };
        let Some((HirStmt::Expr(value), leading)) = stmts.split_last() else {
            return Err(self.malformed("an `if` expression arm has no trailing value"));
        };
        self.block(leading)?;
        self.leaf(value).map(Some)
    }

    fn stmt(&mut self, stmt: &HirStmt) -> Result<(), LoweringError> {
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
            other => Err(self.refuse(describe_stmt(other), stmt_span(other))),
        }
    }

    /// `if condition { then } else if ... else { otherwise }`, of type `value_ty` when it
    /// is an expression.
    fn if_chain(
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
    fn fork(
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
    fn while_loop(
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
            HirExprKind::Binary {
                op: op @ (BinaryOp::And | BinaryOp::Or),
                left,
                right,
            } => self.short_circuit(*op, left, right, expr),
            HirExprKind::Binary { op, left, right }
                if (is_arithmetic(*op) && is_float_valued(&expr.ty))
                    || (is_scalar(&expr.ty) && is_scalar(&left.ty) && is_scalar(&right.ty)) =>
            {
                let left = self.leaf(left)?;
                let right = self.leaf(right)?;
                let op = Op::Binary {
                    op: *op,
                    left,
                    right,
                };
                Ok(self.push(&expr.ty, expr.span, op))
            }
            HirExprKind::Unary { op, operand }
                if (*op == UnaryOp::Negate && is_float_valued(&expr.ty)) || is_scalar(&expr.ty) =>
            {
                let operand = self.leaf(operand)?;
                let op = Op::Unary { op: *op, operand };
                Ok(self.push(&expr.ty, expr.span, op))
            }
            HirExprKind::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
            } => {
                let value = self.if_chain(
                    condition,
                    then_block,
                    else_if_blocks,
                    else_block.as_deref(),
                    Some(&expr.ty),
                    expr.span,
                )?;
                value.ok_or_else(|| self.malformed("an `if` expression has no value"))
            }
            HirExprKind::TensorReduce {
                receiver,
                op: op @ (HirReduceOp::Sum | HirReduceOp::Mean),
                axis,
            } if is_float_valued(&expr.ty) => {
                let receiver = self.leaf(receiver)?;
                let op = Op::Reduce {
                    receiver,
                    op: *op,
                    axis: *axis,
                };
                Ok(self.push(&expr.ty, expr.span, op))
            }
            HirExprKind::TensorLiteral { elements } if is_float_valued(&expr.ty) => {
                let elements = elements
                    .iter()
                    .map(|element| self.leaf(element))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(self.push(&expr.ty, expr.span, Op::Literal(elements)))
            }
            HirExprKind::TensorFill { value } if matches!(value.kind, HirExprKind::Literal(_)) => {
                Ok(self.push(&expr.ty, expr.span, Op::Constant(expr.clone())))
            }
            HirExprKind::TensorIdentity => {
                Ok(self.push(&expr.ty, expr.span, Op::Constant(expr.clone())))
            }
            HirExprKind::TensorIndex { object, axes } if is_scalar(&expr.ty) => {
                let Some(flat) = literal_offset(&object.ty, axes) else {
                    return Err(self.refuse("an element read at a computed position", expr.span));
                };
                let object = self.leaf(object)?;
                let op = Op::Read {
                    object,
                    axes: axes.clone(),
                    flat,
                };
                Ok(self.push(&expr.ty, expr.span, op))
            }
            _ => Err(self.refuse(describe_expr(expr), expr.span)),
        }
    }

    /// `a && b` is `if a { b } else { false }` and `a || b` is `if a { true } else { b }`,
    /// so the right operand runs only when the primal would run it.
    fn short_circuit(
        &mut self,
        op: BinaryOp,
        left: &HirExpr,
        right: &HirExpr,
        expr: &HirExpr,
    ) -> Result<Leaf, LoweringError> {
        let condition = self.leaf(left)?;
        let decided = Leaf::Const(HirExpr::new(
            HirExprKind::Literal(Literal::Boolean(op == BinaryOp::Or)),
            HirType::Bool,
            expr.span,
        ));
        let mut evaluate = |this: &mut Self| this.leaf(right).map(Some);
        let mut known = |_: &mut Self| Ok(Some(decided.clone()));
        let arms: [ArmBody<'_, 'f>; 2] = if op == BinaryOp::And {
            [&mut evaluate, &mut known]
        } else {
            [&mut known, &mut evaluate]
        };
        let value = self.fork(condition, Some(&expr.ty), arms, expr.span)?;
        value.ok_or_else(|| self.malformed("a short-circuit operator has no value"))
    }
}

/// The bindings of `before`, in name order, that some exit rebinds, with the leaf each
/// held before.
fn changed<const N: usize>(
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

fn returns(block: &[HirStmt]) -> bool {
    matches!(block.last(), Some(HirStmt::Return { value: Some(_), .. }))
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

fn is_integer(ty: &HirType) -> bool {
    matches!(
        ty,
        HirType::I8
            | HirType::I16
            | HirType::I32
            | HirType::I64
            | HirType::U8
            | HirType::U16
            | HirType::U32
            | HirType::U64
    )
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

/// Whether a slot can hold a value of `ty`: something the derivative function can give a
/// placeholder to and copy, which is an owned float tensor or a plain scalar.
fn is_slot_type(ty: &HirType) -> bool {
    match ty {
        HirType::Tensor { element, shape, .. } => {
            is_float(element) && shape.iter().all(Option::is_some)
        }
        other => is_float(other) || is_integer(other) || *other == HirType::Bool,
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
        HirStmt::VarDecl { .. } => "a binding without an initializer",
        HirStmt::Assign { .. } | HirStmt::TensorCompoundAssign { .. } => {
            "an assignment to anything but a local binding"
        }
        HirStmt::Return { .. } => "a `return` that does not end the body or an `if` arm",
        HirStmt::If { .. } | HirStmt::While { .. } => "this statement",
        HirStmt::ForRange { .. } | HirStmt::ForEach { .. } => "a `for` loop",
        HirStmt::Break { .. } | HirStmt::Continue { .. } => "a `break` or `continue`",
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
        HirExprKind::Match { .. } => "a `match`",
        HirExprKind::Loop { .. } => "a `loop`",
        HirExprKind::Block { .. } | HirExprKind::Unsafe { .. } | HirExprKind::Pool { .. } => {
            "a block"
        }
        HirExprKind::Reference { .. } | HirExprKind::Deref { .. } => {
            "a borrow of anything but a binding"
        }
        _ => "this expression",
    }
}
