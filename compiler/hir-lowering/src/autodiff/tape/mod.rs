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
use neuro_hir::{
    HirCapture, HirClosure, HirExpr, HirExprKind, HirFunction, HirItem, HirMathOp, HirParam,
    HirReduceOp, HirStmt, HirTensorAxis, HirType,
};
use shared_types::Span;

use crate::{is_numeric, LoweringError};

use super::emit::{self, tensor_parts};
use super::{FieldPath, WrtField};
use control::returns;
use leaf::{clone_call, cloned_tensor, is_arithmetic, is_array, is_copied_field};
use positions::{
    describe_expr, is_float_valued, is_scalar, is_slot_type, literal_index, repeats, slice_sources,
};

mod calls;
mod control;
mod leaf;
mod positions;

/// The prefix of every name the tape generates. User names may not contain `__`, so no
/// generated name can shadow a binding the body declared.
const ENTRY_PREFIX: &str = "__ad_v";

/// The tensor method that copies a buffer into a fresh handle, as every backend spells it.
pub(super) const CLONE_METHOD: &str = "clone";

/// The most elements a `.map` / `.zip` / `.reduce` in a `@grad` body may walk. Each element
/// is its own inlined call in the forward replay and the reverse sweep, so the derivative's
/// size grows with the tensor's.
const MAX_UNROLLED_ELEMENTS: usize = 1024;

/// What a function value chosen at run time is refused as.
const RUN_TIME_TARGET: &str =
    "a function value chosen at run time; call each function directly where the choice is made";

/// What a call to a `@gpu` function is refused as. Differentiating through a call inlines
/// the callee onto the tape, so its body would run on the host, which `@gpu` forbids.
const GPU_CALL: &str = "a call to a `@gpu` function, whose body would run on the host inside the derivative; mark it `@no_grad` to call it as a constant";

/// What a `.step(n)` whose stride is not a literal is refused as.
const RUN_TIME_STRIDE: &str =
    "a `.step(n)` whose stride is not an integer literal; write the stride as a literal";

/// A tape operand.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Leaf {
    /// A tape entry, a slot, a parameter, or a module constant, at the type it is read
    /// at: a parameter keeps its reference type.
    Var { name: String, ty: HirType },
    /// A scalar literal, which is `Copy` and can be written wherever it is needed.
    Const(HirExpr),
    /// A function value whose target is known here: a function or a lifted closure by
    /// name, each capture bound to the leaf it snapshots where the closure was written.
    /// It is never an operand. A call through it inlines the target, and a slot refuses
    /// it, which is what refuses a target chosen at run time.
    Function {
        target: String,
        captures: Vec<(String, Leaf)>,
        ty: HirType,
    },
}

impl Leaf {
    pub(super) fn ty(&self) -> &HirType {
        match self {
            Leaf::Var { ty, .. } | Leaf::Function { ty, .. } => ty,
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
            Leaf::Const(_) | Leaf::Function { .. } => None,
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
    /// One element read, at a position per axis that may be known only at run time.
    Read {
        object: Leaf,
        positions: Vec<Leaf>,
    },
    /// A slice at literal bounds. `sources[r]` is the row-major offset in `object` of
    /// the result's element `r`.
    Slice {
        object: Leaf,
        axes: Vec<HirTensorAxis>,
        sources: Vec<usize>,
    },
    /// `.t()`, `.permute(...)`, `.reshape(...)` or `.flatten(...)`. The node consumes its
    /// receiver, so the replay casts a copy.
    ShapeCast {
        receiver: Leaf,
        permutation: Option<Vec<usize>>,
    },
    /// `value as T` between two integer or float types.
    Convert(Leaf),
    /// An elementwise math function of a scalar or a tensor. The exponent of a `Pow` is
    /// read but never differentiated.
    Math {
        op: HirMathOp,
        operand: Leaf,
        exponent: Option<Leaf>,
    },
    /// An `einsum` contraction, with the HIR node's letter tables.
    Einsum {
        operands: Vec<Leaf>,
        inputs: Vec<Vec<usize>>,
        output: Vec<usize>,
        extents: Vec<usize>,
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

impl Slot {}

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
    /// The copy of each `wrt:` field, in the order `linearize` was given them: the value
    /// whose adjoint is that field's gradient.
    pub(super) fields: Vec<Leaf>,
}

/// Every lowered function and lifted closure of the program by name: what a call inlines.
pub(super) struct Functions<'f> {
    named: HashMap<&'f str, &'f HirFunction>,
    closures: HashMap<&'f str, &'f HirClosure>,
    /// The `@no_grad` functions, which a call runs as written instead of inlining.
    no_grad: &'f HashSet<String>,
}

impl<'f> Functions<'f> {
    pub(super) fn of(items: &'f [HirItem], no_grad: &'f HashSet<String>) -> Self {
        let mut functions = Functions {
            named: HashMap::new(),
            closures: HashMap::new(),
            no_grad,
        };
        for item in items {
            match item {
                HirItem::Function(function) => {
                    let _ = functions.named.insert(function.name.as_str(), function);
                }
                HirItem::Closure(closure) => {
                    let _ = functions.closures.insert(closure.name.as_str(), closure);
                }
                _ => {}
            }
        }
        functions
    }

    pub(super) fn function(&self, name: &str) -> Option<&'f HirFunction> {
        self.named.get(name).copied()
    }

    fn callee(&self, name: &str) -> Option<Callee<'f>> {
        if let Some(function) = self.function(name) {
            return Some(Callee {
                name: &function.name,
                captures: &[],
                params: &function.params,
                return_type: &function.return_type,
                body: &function.body,
            });
        }
        let closure = self.closures.get(name)?;
        Some(Callee {
            name: &closure.name,
            captures: &closure.captures,
            params: &closure.params,
            return_type: &closure.return_type,
            body: &closure.body,
        })
    }
}

/// What inlining reads of a function or a closure: a closure's captures are parameters
/// bound where the closure was written rather than where it is called.
struct Callee<'f> {
    name: &'f str,
    captures: &'f [HirCapture],
    params: &'f [HirParam],
    return_type: &'f HirType,
    body: &'f [HirStmt],
}

/// Linearize `primal`'s body. `differentiated` names the parameters the derivative is
/// taken with respect to; they seed the activity set. A call to one of `functions` is
/// linearized in place, its parameters bound to the arguments' leaves. `bound` gives a
/// function-typed parameter of `primal` the target a call site passed it; one it does not
/// name has no known target, and a call through it is refused.
///
/// Each of `fields` is copied once, ahead of the body, and that copy seeds the activity
/// set too: every read of the field, in a branch, a loop or an inlined callee, is that one
/// value, so all of their adjoints meet in it.
pub(super) fn linearize<'f>(
    primal: &'f HirFunction,
    differentiated: &[&str],
    functions: &'f Functions<'f>,
    bound: &HashMap<String, Leaf>,
    fields: &[WrtField],
) -> Result<Tape, LoweringError> {
    // Every function-typed parameter is bound in `params`, so it takes precedence over a
    // top-level function of the same name, as it does in the primal.
    let params = primal
        .params
        .iter()
        .filter(|param| matches!(param.ty, HirType::Function { .. }))
        .map(|param| {
            let leaf = bound
                .get(&param.name)
                .cloned()
                .unwrap_or_else(|| Leaf::Var {
                    name: param.name.clone(),
                    ty: param.ty.clone(),
                });
            (param.name.clone(), leaf)
        })
        .collect();
    let mut linearizer = Linearizer {
        function: &primal.name,
        functions,
        inlining: vec![primal.name.as_str()],
        nodes: Vec::new(),
        aliases: HashMap::new(),
        params,
        scopes: Vec::new(),
        active: differentiated.iter().map(|name| name.to_string()).collect(),
        slots: HashSet::new(),
        wrt: Vec::with_capacity(fields.len()),
        next: 0,
    };
    for field in fields {
        let copy = linearizer.copy_of(field.place.clone());
        // Active by name only: the entry itself stays inactive, so the reverse sweep leaves
        // its adjoint in place for the bundle to take, as it does a parameter's.
        if let Some(name) = copy.var_name() {
            let _ = linearizer.active.insert(name.to_string());
        }
        linearizer.wrt.push((field.path.clone(), copy));
    }
    let loss = linearizer.body_value(&primal.body, &primal.return_type)?;
    Ok(Tape {
        nodes: linearizer.nodes,
        active: linearizer.active,
        loss,
        slots: linearizer.slots,
        fields: linearizer.wrt.into_iter().map(|(_, copy)| copy).collect(),
    })
}

/// The head of a `for` over a range, as `HirStmt::ForRange` carries it.
struct ForRange<'a> {
    index: Option<&'a str>,
    iterator: &'a str,
    start: &'a HirExpr,
    end: &'a HirExpr,
    inclusive: bool,
    reversed: bool,
    step: Option<&'a HirExpr>,
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
    functions: &'f Functions<'f>,
    /// The functions being linearized, outermost first. A call to one of them is recursion,
    /// which inlining cannot unfold.
    inlining: Vec<&'f str>,
    nodes: Vec<Node>,
    /// Each body binding, resolved to the leaf that holds its current value. A binding is
    /// never a tape entry of its own: `val y = x` is `x` under a second name.
    aliases: HashMap<String, Leaf>,
    /// The parameters of an inlined callee, bound to its arguments. Kept apart from
    /// `aliases` so that assigning to one is refused, as it is for the `@grad` function's
    /// own: through a `&mut` it would write the caller's value.
    params: HashMap<String, Leaf>,
    scopes: Vec<Scope>,
    active: HashSet<String>,
    slots: HashSet<String>,
    /// The copy each `wrt:` field path is read through.
    wrt: Vec<(FieldPath, Leaf)>,
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
                Op::Read { object, .. } | Op::Slice { object, .. } => self.is_active(object),
                Op::ShapeCast { receiver, .. } => self.is_active(receiver),
                Op::Convert(operand) | Op::Math { operand, .. } => self.is_active(operand),
                Op::Einsum { operands, .. } => operands.iter().any(|leaf| self.is_active(leaf)),
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
        // A function value leaving a branch or a loop is one whose target the path decides.
        if matches!(ty, HirType::Function { .. }) {
            return Err(self.refuse(RUN_TIME_TARGET, span));
        }
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

    fn leaf(&mut self, expr: &HirExpr) -> Result<Leaf, LoweringError> {
        // A tape value is never written after it is made (mutation is versioning), so a
        // copy of one is the value itself. Through a field, the copy is the field's.
        if let Some(receiver) = cloned_tensor(expr) {
            return self.leaf(receiver);
        }
        match &expr.kind {
            HirExprKind::Literal(_) if is_scalar(&expr.ty) => Ok(Leaf::Const(expr.clone())),
            HirExprKind::Variable(name) => {
                if let Some(leaf) = self.aliases.get(name).or_else(|| self.params.get(name)) {
                    return Ok(leaf.clone());
                }
                if matches!(expr.ty, HirType::Function { .. })
                    && self.functions.callee(name).is_some()
                {
                    return Ok(Leaf::Function {
                        target: name.clone(),
                        captures: Vec::new(),
                        ty: expr.ty.clone(),
                    });
                }
                Ok(Leaf::Var {
                    name: name.clone(),
                    ty: expr.ty.clone(),
                })
            }
            HirExprKind::Closure { name, captures } => {
                let mut bound = Vec::with_capacity(captures.len());
                for capture in captures {
                    let read = HirExprKind::Variable(capture.name.clone());
                    let read = HirExpr::new(read, capture.ty.clone(), expr.span);
                    bound.push((capture.name.clone(), self.leaf(&read)?));
                }
                Ok(Leaf::Function {
                    target: name.clone(),
                    captures: bound,
                    ty: expr.ty.clone(),
                })
            }
            // `x |> |v| ...` desugars to a block binding the closure and calling it.
            HirExprKind::Block { stmts } => self.block_value(stmts, &expr.ty),
            HirExprKind::TensorApply {
                kind,
                receiver,
                operand,
                callee,
            } => self.traverse(*kind, receiver, operand.as_deref(), callee, expr),
            // Reading through `&x` reads `x`; the replay borrows every tensor operand anyway.
            HirExprKind::Reference {
                operand,
                mutable: false,
            } if matches!(operand.kind, HirExprKind::Variable(_)) => self.leaf(operand),
            HirExprKind::FieldAccess { .. } if is_copied_field(&expr.ty) => {
                let place = self.rebase_place(expr)?;
                Ok(self.push(&expr.ty, expr.span, Op::Constant(place)))
            }
            HirExprKind::FieldAccess { .. } if matches!(expr.ty, HirType::Tensor { .. }) => {
                self.field_copy(expr)
            }
            HirExprKind::Index { object, .. } if is_array(&object.ty) => {
                if is_copied_field(&expr.ty) {
                    let place = self.rebase_place(expr)?;
                    return Ok(self.push(&expr.ty, expr.span, Op::Constant(place)));
                }
                if !matches!(expr.ty, HirType::Tensor { .. }) {
                    return Err(self.refuse(describe_expr(expr), expr.span));
                }
                self.field_copy(expr)
            }
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
                let mut positions = Vec::with_capacity(axes.len());
                for axis in axes {
                    let HirTensorAxis::Position(position) = axis else {
                        return Err(self.malformed("an element read with a range axis"));
                    };
                    positions.push(self.leaf(position)?);
                }
                let object = self.leaf(object)?;
                let op = Op::Read { object, positions };
                Ok(self.push(&expr.ty, expr.span, op))
            }
            HirExprKind::TensorIndex { object, axes } if is_float_valued(&expr.ty) => {
                let Some(sources) = slice_sources(&object.ty, axes) else {
                    return Err(self.refuse("a tensor slice at a computed position", expr.span));
                };
                let object = self.leaf(object)?;
                let op = Op::Slice {
                    object,
                    axes: axes.clone(),
                    sources,
                };
                Ok(self.push(&expr.ty, expr.span, op))
            }
            HirExprKind::TensorShapeCast {
                receiver,
                permutation,
            } if is_float_valued(&expr.ty)
                && tensor_parts(&expr.ty).is_some()
                && tensor_parts(&receiver.ty).is_some() =>
            {
                let receiver = self.leaf(receiver)?;
                let op = Op::ShapeCast {
                    receiver,
                    permutation: permutation.clone(),
                };
                Ok(self.push(&expr.ty, expr.span, op))
            }
            HirExprKind::Cast { value } if is_numeric(&expr.ty) && is_numeric(&value.ty) => {
                let operand = self.leaf(value)?;
                Ok(self.push(&expr.ty, expr.span, Op::Convert(operand)))
            }
            HirExprKind::Math {
                op,
                operand,
                exponent,
            } if is_float_valued(&expr.ty) => {
                let operand = self.leaf(operand)?;
                let exponent = exponent
                    .as_deref()
                    .map(|exponent| self.leaf(exponent))
                    .transpose()?;
                let op = Op::Math {
                    op: *op,
                    operand,
                    exponent,
                };
                Ok(self.push(&expr.ty, expr.span, op))
            }
            HirExprKind::TensorEinsum {
                operands,
                inputs,
                output,
                extents,
            } if is_float_valued(&expr.ty) => {
                // The adjoint of an operand walked along its diagonal would have to be
                // written back along that diagonal, which no contraction can express.
                if inputs.iter().any(|letters| repeats(letters)) {
                    return Err(self.refuse("an `einsum` operand that repeats a letter", expr.span));
                }
                let mut leaves = Vec::with_capacity(operands.len());
                for operand in operands {
                    leaves.push(self.leaf(operand)?);
                }
                let op = Op::Einsum {
                    operands: leaves,
                    inputs: inputs.clone(),
                    output: output.clone(),
                    extents: extents.clone(),
                };
                Ok(self.push(&expr.ty, expr.span, op))
            }
            // The value is the receiver's, but the entry is a constant, so no adjoint reaches
            // the receiver through it. The replay detaches a copy: the reverse pass reads the
            // receiver again, and a second pass over this derivative must meet the fence too.
            HirExprKind::TensorDetach { receiver } => {
                let receiver = self.leaf(receiver)?;
                let copy = clone_call(emit::operand_owned(&receiver, expr.span), &expr.ty);
                let kind = HirExprKind::TensorDetach {
                    receiver: Box::new(copy),
                };
                let detached = HirExpr::new(kind, expr.ty.clone(), expr.span);
                Ok(self.push(&expr.ty, expr.span, Op::Constant(detached)))
            }
            HirExprKind::Call { callee, args } => self.call(callee, args, expr.span),
            _ => Err(self.refuse(describe_expr(expr), expr.span)),
        }
    }
}

/// Where the first active element read at a position known only at run time is in
/// `nodes`. Its adjoint is an element store into a zero tensor, which a second derivative
/// would have to read back, and a tape has no element store.
pub(super) fn run_time_read(nodes: &[Node]) -> Option<Span> {
    nodes.iter().find_map(|node| match node {
        Node::Entry(Entry {
            op: Op::Read { object, positions },
            active: true,
            span,
            ..
        }) if literal_offset(object.value_ty(), positions).is_none() => Some(*span),
        Node::Entry(_) => None,
        Node::Branch(branch) => branch.arms.iter().find_map(|arm| run_time_read(&arm.nodes)),
        Node::Loop(looped) => {
            run_time_read(&looped.condition).or_else(|| run_time_read(&looped.body))
        }
    })
}

/// The row-major offset an element read names, when every position is a literal inside
/// its extent. `None` for a read placed only at run time.
pub(super) fn literal_offset(object_ty: &HirType, positions: &[Leaf]) -> Option<usize> {
    let (_, extents) = tensor_parts(object_ty)?;
    if extents.len() != positions.len() {
        return None;
    }
    let mut flat = 0usize;
    for (position, extent) in positions.iter().zip(&extents) {
        let index = literal_index(position)?.filter(|index| index < extent)?;
        flat = flat * extent + index;
    }
    Some(flat)
}
