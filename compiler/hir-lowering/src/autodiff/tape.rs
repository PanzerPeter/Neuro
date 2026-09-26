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
    HirPlace, HirReduceOp, HirStmt, HirTensorApply, HirTensorAxis, HirType,
};
use shared_types::{Literal, Span};

use super::emit::tensor_parts;
use super::{FieldPath, PathStep, WrtField, RECEIVER};
use crate::LoweringError;

/// The prefix of every name the tape generates. User names may not contain `__`, so no
/// generated name can shadow a binding the body declared.
const ENTRY_PREFIX: &str = "__ad_v";

/// The tensor method that copies a buffer into a fresh handle, as every backend spells it.
const CLONE_METHOD: &str = "clone";

/// The most elements a `.map` / `.zip` / `.reduce` in a `@grad` body may walk. Each element
/// is its own inlined call in the forward replay and the reverse sweep, so the derivative's
/// size grows with the tensor's.
const MAX_UNROLLED_ELEMENTS: usize = 1024;

/// What a function value chosen at run time is refused as.
const RUN_TIME_TARGET: &str =
    "a function value chosen at run time; call each function directly where the choice is made";

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
    /// The copy of each `wrt:` field, in the order `linearize` was given them: the value
    /// whose adjoint is that field's gradient.
    pub(super) fields: Vec<Leaf>,
}

/// Every lowered function and lifted closure of the program by name: what a call inlines.
pub(super) struct Functions<'f> {
    named: HashMap<&'f str, &'f HirFunction>,
    closures: HashMap<&'f str, &'f HirClosure>,
}

impl<'f> Functions<'f> {
    pub(super) fn of(items: &'f [HirItem]) -> Self {
        let mut functions = Functions {
            named: HashMap::new(),
            closures: HashMap::new(),
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
            HirStmt::ForRange {
                index,
                iterator,
                start,
                end,
                inclusive,
                reversed,
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
                },
                body,
                *span,
            ),
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

    /// `for iterator in start..end { body }` as the counted `while` it is. The bounds are
    /// read once, as the primal reads them. The counter never steps past `end`, so an
    /// inclusive range that ends at its type's maximum stops on a flag instead, and a
    /// reversed one counts up and mirrors the counter onto the binding, as the backend does.
    fn for_range(
        &mut self,
        head: ForRange<'_>,
        body: &[HirStmt],
        span: Span,
    ) -> Result<(), LoweringError> {
        let ty = &head.start.ty;
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
        let condition = if head.inclusive {
            let more = self.fresh();
            let nonempty = binary(
                BinaryOp::LessEqual,
                var(&low, ty),
                var(&high, ty),
                &HirType::Bool,
            );
            self.stmt(&declare(&more, nonempty, true))?;
            looped.push(HirStmt::If {
                condition: binary(
                    BinaryOp::Less,
                    var(&counter, ty),
                    var(&high, ty),
                    &HirType::Bool,
                ),
                then_block: vec![step],
                else_if_blocks: Vec::new(),
                else_block: Some(vec![assign(
                    &more,
                    literal(Literal::Boolean(false), &HirType::Bool),
                )]),
                span,
            });
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
            HirExprKind::Call { callee, args } => self.call(callee, args, expr.span),
            _ => Err(self.refuse(describe_expr(expr), expr.span)),
        }
    }

    /// A copy of the tensor field `place` names, taken once where the body reads it. The
    /// primal can only read such a field in place (a reduction's receiver, an element
    /// read's object), since it is reached through a borrow. The replay and the reverse
    /// pass read an operand wherever their rules need it, and only a binding of the
    /// derivative's own can be read that way without moving the field out of the receiver.
    // ponytail: one buffer copy per read of the field; a borrow of the field instead once
    // `&self.field` is a place the checker and backends accept (BUG-033).
    fn field_copy(&mut self, place: &HirExpr) -> Result<Leaf, LoweringError> {
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
    fn copy_of(&mut self, place: HirExpr) -> Leaf {
        let (ty, span) = (place.ty.clone(), place.span);
        let method = HirExpr::new(
            HirExprKind::FieldAccess {
                object: Box::new(place),
                field: CLONE_METHOD.to_string(),
            },
            ty.clone(),
            span,
        );
        let copy = HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(method),
                args: Vec::new(),
            },
            ty.clone(),
            span,
        );
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
    fn rebase_place(&mut self, place: &HirExpr) -> Result<HirExpr, LoweringError> {
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

    /// A call to a user function, or through a function value whose target is known here,
    /// linearized in place.
    fn call(
        &mut self,
        callee: &HirExpr,
        args: &[HirExpr],
        span: Span,
    ) -> Result<Leaf, LoweringError> {
        let target = match &callee.kind {
            HirExprKind::Variable(_) | HirExprKind::Closure { .. } => self.leaf(callee)?,
            _ => return Err(self.refuse("a method call", span)),
        };
        let Leaf::Function {
            target, captures, ..
        } = target
        else {
            // A local or a parameter of function type with no known target.
            let bound = matches!(&callee.kind, HirExprKind::Variable(name)
                if self.aliases.contains_key(name) || self.params.contains_key(name));
            if bound {
                return Err(self.refuse(RUN_TIME_TARGET, span));
            }
            return Err(self.refuse("a call to a builtin or a method", span));
        };
        let mut leaves = Vec::with_capacity(args.len());
        for arg in args {
            leaves.push(self.leaf(arg)?);
        }
        self.inline(&target, captures, leaves, span)
    }

    /// The body of the function or closure `target` run at the call on `args`, under
    /// empty aliases and scopes so nothing of the caller leaks in or out. A closure's
    /// `captures` are bound beside its parameters.
    // ponytail: every call site gets its own copy of the callee's tape, so the derivative
    // grows with the call tree; a per-callee reverse function chained at each call is the
    // upgrade if code size starts to matter.
    fn inline(
        &mut self,
        target: &str,
        captures: Vec<(String, Leaf)>,
        args: Vec<Leaf>,
        span: Span,
    ) -> Result<Leaf, LoweringError> {
        let Some(callee) = self.functions.callee(target) else {
            return Err(self.malformed("a function value names no function"));
        };
        if self.inlining.contains(&callee.name) {
            return Err(self.refuse("a recursive call", span));
        }
        if *callee.return_type == HirType::Void {
            return Err(self.refuse("a call to a function that returns nothing", span));
        }
        if callee.params.len() != args.len() || callee.captures.len() != captures.len() {
            return Err(self.malformed("a call whose argument count is not its callee's"));
        }
        let mut params: HashMap<String, Leaf> = captures.into_iter().collect();
        for (param, arg) in callee.params.iter().zip(args) {
            let _ = params.insert(param.name.clone(), arg);
        }
        let aliases = std::mem::take(&mut self.aliases);
        let params = std::mem::replace(&mut self.params, params);
        let scopes = std::mem::take(&mut self.scopes);
        self.inlining.push(callee.name);
        let value = self.body_value(callee.body, callee.return_type);
        let _ = self.inlining.pop();
        self.aliases = aliases;
        self.params = params;
        self.scopes = scopes;
        value
    }

    /// A block's value: its statements in a scope of their own, then its tail.
    fn block_value(&mut self, stmts: &[HirStmt], ty: &HirType) -> Result<Leaf, LoweringError> {
        let before = self.aliases.clone();
        let inner = self.scoped(&before, &mut |this: &mut Self| this.arm(stmts, Some(ty)))?;
        self.nodes.extend(inner.nodes);
        self.aliases = inner.exit;
        inner
            .value
            .ok_or_else(|| self.malformed("a block value has no tail"))
    }

    /// `.map(f)`, `.zip(other, f)` or `.reduce(init, f)`, unrolled: one element read per
    /// position and one inlined call of `f` per element, in the row-major order the backend
    /// walks, so a `.reduce` folds in the same order and rounds the same way.
    // ponytail: unrolled per element, hence MAX_UNROLLED_ELEMENTS; a rule per traversal,
    // with `f`'s derivative as a traversal of its own, is the upgrade for large tensors.
    fn traverse(
        &mut self,
        kind: HirTensorApply,
        receiver: &HirExpr,
        operand: Option<&HirExpr>,
        callee: &HirExpr,
        expr: &HirExpr,
    ) -> Result<Leaf, LoweringError> {
        let span = expr.span;
        let Some((element, extents)) = tensor_parts(&receiver.ty) else {
            return Err(self.refuse(
                "a `.map` / `.zip` / `.reduce` over a tensor of dynamic shape",
                span,
            ));
        };
        let element = element.clone();
        let count = extents
            .iter()
            .try_fold(1usize, |count, extent| count.checked_mul(*extent))
            .filter(|count| *count <= MAX_UNROLLED_ELEMENTS);
        let Some(count) = count else {
            let construct = format!(
                "a `.map` / `.zip` / `.reduce` over more than the {MAX_UNROLLED_ELEMENTS} elements a traversal is unrolled for"
            );
            return Err(self.refuse(&construct, span));
        };
        let Leaf::Function {
            target, captures, ..
        } = self.leaf(callee)?
        else {
            return Err(self.refuse(RUN_TIME_TARGET, span));
        };
        let object = self.leaf(receiver)?;
        let operand = operand.map(|operand| self.leaf(operand)).transpose()?;
        let read = |this: &mut Self, object: &Leaf, element: &HirType, flat: usize| {
            let positions = coordinates(flat, &extents, span);
            let object = object.clone();
            this.push(element, span, Op::Read { object, positions })
        };

        if kind == HirTensorApply::Reduce {
            let mut acc = operand.ok_or_else(|| self.malformed("a `.reduce` with no seed"))?;
            for flat in 0..count {
                let value = read(self, &object, &element, flat);
                acc = self.inline(&target, captures.clone(), vec![acc, value], span)?;
            }
            return Ok(acc);
        }
        let other = match operand {
            Some(other) => match tensor_parts(other.ty()) {
                Some((element, _)) => Some((element.clone(), other)),
                None => return Err(self.malformed("a `.zip` over a non-tensor operand")),
            },
            None => None,
        };
        let mut results = Vec::with_capacity(count);
        for flat in 0..count {
            let mut args = vec![read(self, &object, &element, flat)];
            if let Some((element, other)) = &other {
                args.push(read(self, other, element, flat));
            }
            results.push(self.inline(&target, captures.clone(), args, span)?);
        }
        Ok(self.push(&expr.ty, span, Op::Literal(results)))
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

fn is_numeric(ty: &HirType) -> bool {
    is_float(ty) || is_integer(ty)
}

/// Whether a field read of `ty` is a copy the replay can take as the primal did: a
/// number, a `bool` or a `char`, all `Copy`.
/// The receiver of `expr` when `expr` is `receiver.clone()` on a tensor.
fn cloned_tensor(expr: &HirExpr) -> Option<&HirExpr> {
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

fn is_array(ty: &HirType) -> bool {
    matches!(ty.referent(), HirType::Array { .. })
}

/// The steps from the receiver to `place`, when `place` is rooted at the receiver and every
/// array position in it is a literal.
fn receiver_path(place: &HirExpr) -> Option<FieldPath> {
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

fn is_copied_field(ty: &HirType) -> bool {
    is_numeric(ty) || matches!(ty, HirType::Bool | HirType::Char)
}

fn repeats(letters: &[usize]) -> bool {
    letters
        .iter()
        .enumerate()
        .any(|(at, letter)| letters[at + 1..].contains(letter))
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

/// The literal position, one per axis, of the element at row-major offset `flat`.
fn coordinates(mut flat: usize, extents: &[usize], span: Span) -> Vec<Leaf> {
    let mut positions = vec![0usize; extents.len()];
    for (position, extent) in positions.iter_mut().zip(extents).rev() {
        *position = flat % extent;
        flat /= extent;
    }
    positions
        .into_iter()
        .map(|position| {
            let literal = Literal::Integer(position as i128, None);
            Leaf::Const(HirExpr::new(
                HirExprKind::Literal(literal),
                HirType::U64,
                span,
            ))
        })
        .collect()
}

/// The value of an integer literal array position.
fn literal_position(index: &HirExpr) -> Option<usize> {
    match &index.kind {
        HirExprKind::Literal(Literal::Integer(value, _)) => usize::try_from(*value).ok(),
        _ => None,
    }
}

fn literal_index(position: &Leaf) -> Option<Option<usize>> {
    let Leaf::Const(HirExpr {
        kind: HirExprKind::Literal(Literal::Integer(value, _)),
        ..
    }) = position
    else {
        return None;
    };
    Some(usize::try_from(*value).ok())
}

/// For a slice whose every axis is a literal position or a range, the offset in the
/// object of each result element, in the result's row-major order.
fn slice_sources(object_ty: &HirType, axes: &[HirTensorAxis]) -> Option<Vec<usize>> {
    let (_, extents) = tensor_parts(object_ty)?;
    if extents.len() != axes.len() {
        return None;
    }
    // Per object axis, the coordinates the result visits along it, in result order.
    let mut visits: Vec<Vec<usize>> = Vec::with_capacity(axes.len());
    for (axis, extent) in axes.iter().zip(&extents) {
        let along = match axis {
            HirTensorAxis::Position(HirExpr {
                kind: HirExprKind::Literal(Literal::Integer(value, _)),
                ..
            }) => vec![usize::try_from(*value)
                .ok()
                .filter(|index| index < extent)?],
            HirTensorAxis::Position(_) => return None,
            HirTensorAxis::Range {
                start,
                end,
                reversed,
            } => {
                if start > end || end > extent {
                    return None;
                }
                let mut along: Vec<usize> = (*start..*end).collect();
                if *reversed {
                    along.reverse();
                }
                along
            }
        };
        visits.push(along);
    }
    let mut sources = vec![0usize];
    for (along, extent) in visits.iter().zip(&extents) {
        sources = sources
            .iter()
            .flat_map(|base| along.iter().map(move |at| base * extent + at))
            .collect();
    }
    Some(sources)
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
        HirStmt::ForRange { .. } | HirStmt::ForEach { .. } => "a `for` loop over a collection",
        HirStmt::Break { .. } | HirStmt::Continue { .. } => "a `break` or `continue`",
        HirStmt::ValElse { .. } => "a `val ... else` binding",
        HirStmt::Const { .. } => "a local `const`",
        HirStmt::Expr(_) => "an expression statement",
    }
}

fn describe_expr(expr: &HirExpr) -> &'static str {
    match &expr.kind {
        HirExprKind::Binary { .. } | HirExprKind::Unary { .. } => {
            "an operator on a value that has no derivative"
        }
        HirExprKind::TensorReduce { .. } => "a `.max()` / `.min()` reduction",
        HirExprKind::TensorEinsum { .. } => "an `einsum` contraction",
        HirExprKind::TensorShapeCast { .. } => "a shape change",
        HirExprKind::TensorIndex { .. } => "a tensor slice at a computed position",
        HirExprKind::TensorSort { .. } => "a sort",
        HirExprKind::TensorRandomNormal { .. } => "a random tensor",
        HirExprKind::Cast { .. } => "a cast",
        HirExprKind::Math { .. } => "elementwise math on half-precision elements",
        HirExprKind::FieldAccess { .. } => "a field that is neither a number nor a tensor",
        HirExprKind::Match { .. } => "a `match`",
        HirExprKind::Loop { .. } => "a `loop`",
        HirExprKind::Unsafe { .. } | HirExprKind::Pool { .. } => "an `unsafe` or `pool` block",
        HirExprKind::Reference { .. } | HirExprKind::Deref { .. } => {
            "a borrow of anything but a binding"
        }
        _ => "this expression",
    }
}
