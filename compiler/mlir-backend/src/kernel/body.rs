//! A `@kernel` body as the region of a `gpu.launch`: one thread's code.
//!
//! Every local lives in a `memref.alloca` slot hoisted to the region's entry, and control
//! flow is `cf` branches between blocks, so a loop needs no loop-carried SSA values and a
//! `break`, `continue` or `return` is a plain branch. LLVM promotes the slots to registers.
//! A value an `if` or `&&` produces goes through a slot too, which keeps every SSA value
//! used only in the block that defines it or one it dominates.

use std::fmt::Write as _;

use ast_types::{BinaryOp, UnaryOp};
use neuro_hir::{
    HirExpr, HirExprKind, HirFunction, HirGridIndex, HirPlace, HirStmt, HirTensorAxis, HirType,
};
use shared_types::{Literal, Span};

use super::GuardStyle;

/// The grid axes as `gpu` dialect dimensions, in field order.
const GRID_DIMENSIONS: [&str; 3] = ["x", "y", "z"];

const INDEX_OUT_OF_BOUNDS: &str = "index out of bounds in a `@kernel` body";
const DIVIDE_BY_ZERO: &str = "integer division by zero in a `@kernel` body";

/// The construct a body stopped at, and what it is, completing "cannot lower ...".
#[derive(Debug)]
pub(crate) struct Refused {
    pub(crate) span: Span,
    pub(crate) what: String,
}

impl Refused {
    pub(crate) fn new(span: Span, what: &str) -> Self {
        Refused {
            span,
            what: what.to_string(),
        }
    }
}

type Lowered<T> = Result<T, Refused>;

/// The MLIR spelling of a scalar a kernel computes with. `f16` / `bf16` carry no
/// arithmetic in the HIR, so a kernel has nothing to do with one.
pub(crate) fn scalar_type(ty: &HirType) -> Option<&'static str> {
    Some(match ty {
        HirType::I8 | HirType::U8 => "i8",
        HirType::I16 | HirType::U16 => "i16",
        HirType::I32 | HirType::U32 => "i32",
        HirType::I64 | HirType::U64 => "i64",
        HirType::F32 => "f32",
        HirType::F64 => "f64",
        HirType::Bool => "i1",
        _ => return None,
    })
}

/// The row-major `memref` a tensor of static shape is read and written through.
pub(crate) fn memref_type(ty: &HirType) -> Option<String> {
    let HirType::Tensor { element, shape, .. } = ty else {
        return None;
    };
    let element = scalar_type(element)?;
    let mut text = String::from("memref<");
    for extent in shape {
        write!(text, "{}x", (*extent)?).ok()?;
    }
    Some(format!("{text}{element}>"))
}

fn is_unsigned(ty: &HirType) -> bool {
    matches!(ty, HirType::U8 | HirType::U16 | HirType::U32 | HirType::U64)
}

fn is_float(ty: &HirType) -> bool {
    matches!(ty, HirType::F32 | HirType::F64)
}

fn int_width(ty: &HirType) -> Option<u32> {
    match ty {
        HirType::I8 | HirType::U8 => Some(8),
        HirType::I16 | HirType::U16 => Some(16),
        HirType::I32 | HirType::U32 => Some(32),
        HirType::I64 | HirType::U64 => Some(64),
        _ => None,
    }
}

/// A name the body can read: a local's slot, or a parameter's block argument.
#[derive(Clone)]
enum Binding {
    Slot { slot: String, ty: HirType },
    Param { value: String, ty: HirType },
}

/// Where `break` and `continue` go for one enclosing loop.
struct LoopTargets {
    label: Option<String>,
    next: String,
    exit: String,
}

/// Emits one kernel body as the text of a `gpu.launch` region.
pub(crate) struct BodyEmitter<'f> {
    function: &'f HirFunction,
    guard: GuardStyle,
    /// The launch's block shape, a constant the body multiplies block positions by.
    threads: [u32; 3],
    /// The slots, emitted at the top of the region's entry block.
    slots: String,
    /// Everything after them.
    text: String,
    next_value: u32,
    next_block: u32,
    scopes: Vec<Vec<(String, Binding)>>,
    loops: Vec<LoopTargets>,
    /// Whether the block being emitted already ends in a terminator. Code after one is
    /// unreachable and is not emitted.
    terminated: bool,
}

impl<'f> BodyEmitter<'f> {
    pub(crate) fn new(function: &'f HirFunction, guard: GuardStyle, threads: [u32; 3]) -> Self {
        let params = function
            .params
            .iter()
            .enumerate()
            .map(|(index, param)| {
                let binding = Binding::Param {
                    value: format!("%arg{index}"),
                    ty: param.ty.clone(),
                };
                (param.name.clone(), binding)
            })
            .collect();
        BodyEmitter {
            function,
            guard,
            threads,
            slots: String::new(),
            text: String::new(),
            next_value: 0,
            next_block: 0,
            scopes: vec![params],
            loops: Vec::new(),
            terminated: false,
        }
    }

    /// The region's text, ending in the block that terminates the launch.
    pub(crate) fn emit(mut self) -> Lowered<String> {
        let exit = self.block();
        let function = self.function;
        self.statements(&function.body, &exit)?;
        self.branch(&exit);
        self.start(&exit);
        self.line("gpu.terminator");
        Ok(format!("{}{}", self.slots, self.text))
    }

    fn value(&mut self) -> String {
        self.next_value += 1;
        format!("%v{}", self.next_value)
    }

    fn block(&mut self) -> String {
        self.next_block += 1;
        format!("^bb{}", self.next_block)
    }

    fn line(&mut self, line: &str) {
        if self.terminated {
            return;
        }
        let _ = writeln!(self.text, "      {line}");
    }

    /// `%v = <rhs>`, returning `%v`.
    fn assign(&mut self, rhs: &str) -> String {
        let value = self.value();
        self.line(&format!("{value} = {rhs}"));
        value
    }

    fn branch(&mut self, target: &str) {
        self.line(&format!("cf.br {target}"));
        self.terminated = true;
    }

    fn cond_branch(&mut self, condition: &str, then: &str, otherwise: &str) {
        self.line(&format!("cf.cond_br {condition}, {then}, {otherwise}"));
        self.terminated = true;
    }

    fn start(&mut self, block: &str) {
        let _ = writeln!(self.text, "    {block}:");
        self.terminated = false;
    }

    fn slot(&mut self, ty: &str) -> String {
        self.next_value += 1;
        let slot = format!("%s{}", self.next_value);
        let _ = writeln!(self.slots, "      {slot} = memref.alloca() : memref<{ty}>");
        slot
    }

    fn lookup(&self, name: &str) -> Option<Binding> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.iter().rev().find(|(n, _)| n == name))
            .map(|(_, binding)| binding.clone())
    }

    fn define(&mut self, name: &str, binding: Binding) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.push((name.to_string(), binding));
        }
    }

    /// Stop the thread unless `condition` holds.
    fn guard(&mut self, condition: &str, message: &str) {
        match self.guard {
            GuardStyle::Assert => self.line(&format!("cf.assert {condition}, \"{message}\"")),
            // `gpu.launch` wants every block that leaves the region to end in
            // `gpu.terminator`, so the trap block branches on rather than ending in
            // `unreachable`; the trap never returns to take it.
            GuardStyle::Trap => {
                let ok = self.block();
                let trap = self.block();
                self.cond_branch(condition, &ok, &trap);
                self.start(&trap);
                self.line("llvm.intr.trap");
                self.branch(&ok);
                self.start(&ok);
            }
        }
    }

    fn statements(&mut self, statements: &[HirStmt], exit: &str) -> Lowered<()> {
        self.scopes.push(Vec::new());
        for statement in statements {
            if self.terminated {
                break;
            }
            self.statement(statement, exit)?;
        }
        self.scopes.pop();
        Ok(())
    }

    fn statement(&mut self, statement: &HirStmt, exit: &str) -> Lowered<()> {
        match statement {
            HirStmt::VarDecl { name, ty, init, .. } => {
                let mlir = scalar_type(ty)
                    .ok_or_else(|| Refused::new(stmt_span(statement), "a local of this type"))?;
                let slot = self.slot(mlir);
                if let Some(init) = init {
                    let value = self.expr(init, exit)?;
                    self.line(&format!("memref.store {value}, {slot}[] : memref<{mlir}>"));
                }
                self.define(
                    name,
                    Binding::Slot {
                        slot,
                        ty: ty.clone(),
                    },
                );
                Ok(())
            }
            HirStmt::Assign { place, value, span } => self.store(place, value, *span, exit),
            HirStmt::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => self.if_chain(
                condition,
                then_block,
                else_if_blocks,
                else_block,
                None,
                exit,
            ),
            HirStmt::While {
                label,
                condition,
                body,
                ..
            } => self.while_loop(label, Some(condition), body, exit),
            HirStmt::ForRange {
                label,
                index: None,
                iterator,
                start,
                end,
                inclusive,
                reversed: false,
                step: None,
                body,
                ..
            } => self.for_range(label, iterator, start, end, *inclusive, body, exit),
            HirStmt::Break {
                label,
                value: None,
                span,
            } => {
                let target = self.loop_target(label, *span)?.exit.clone();
                self.branch(&target);
                Ok(())
            }
            HirStmt::Continue { label, span } => {
                let target = self.loop_target(label, *span)?.next.clone();
                self.branch(&target);
                Ok(())
            }
            HirStmt::Return { value: None, .. } => {
                self.branch(exit);
                Ok(())
            }
            HirStmt::Expr(expr) => self.expr(expr, exit).map(|_| ()),
            other => Err(Refused::new(stmt_span(other), statement_kind(other))),
        }
    }

    fn loop_target(&self, label: &Option<String>, span: Span) -> Lowered<&LoopTargets> {
        self.loops
            .iter()
            .rev()
            .find(|targets| label.is_none() || targets.label == *label)
            .ok_or_else(|| Refused::new(span, "a `break` or `continue` outside a loop"))
    }

    fn store(&mut self, place: &HirPlace, value: &HirExpr, span: Span, exit: &str) -> Lowered<()> {
        match place {
            HirPlace::Var { name, .. } => {
                let Some(Binding::Slot { slot, ty }) = self.lookup(name) else {
                    return Err(Refused::new(span, "an assignment to a parameter"));
                };
                let mlir = scalar_type(&ty).unwrap_or("i1");
                let value = self.expr(value, exit)?;
                self.line(&format!("memref.store {value}, {slot}[] : memref<{mlir}>"));
                Ok(())
            }
            HirPlace::TensorIndex { object, axes, .. } => {
                let (memref, ty, extents) = self.tensor(object)?;
                let indices = self.indices(axes, &extents, span, exit)?;
                let value = self.expr(value, exit)?;
                self.line(&format!("memref.store {value}, {memref}[{indices}] : {ty}"));
                Ok(())
            }
            HirPlace::Index { object, index, .. } => {
                let (memref, ty, extents) = self.tensor(object)?;
                let axes = [HirTensorAxis::Position((**index).clone())];
                let indices = self.indices(&axes, &extents, span, exit)?;
                let value = self.expr(value, exit)?;
                self.line(&format!("memref.store {value}, {memref}[{indices}] : {ty}"));
                Ok(())
            }
            _ => Err(Refused::new(span, "an assignment to this kind of place")),
        }
    }

    /// The parameter a tensor read or write goes through: its `memref`, the `memref`'s
    /// type, and its extents.
    fn tensor(&self, object: &HirExpr) -> Lowered<(String, String, Vec<usize>)> {
        let inner = match &object.kind {
            HirExprKind::Deref { operand } => operand.as_ref(),
            _ => object,
        };
        let HirExprKind::Variable(name) = &inner.kind else {
            return Err(Refused::new(
                object.span,
                "a tensor that is not a parameter",
            ));
        };
        let Some(Binding::Param { value, ty }) = self.lookup(name) else {
            return Err(Refused::new(
                object.span,
                "a tensor that is not a parameter",
            ));
        };
        let tensor = ty.referent();
        let (Some(memref), HirType::Tensor { shape, .. }) = (memref_type(tensor), tensor) else {
            return Err(Refused::new(
                object.span,
                "an index into something other than a tensor",
            ));
        };
        Ok((value, memref, shape.iter().flatten().copied().collect()))
    }

    /// Each position as a bounds-checked `index`. A signed position is widened with its
    /// sign, so a negative one reads as a huge unsigned value and fails the same check.
    fn indices(
        &mut self,
        axes: &[HirTensorAxis],
        extents: &[usize],
        span: Span,
        exit: &str,
    ) -> Lowered<String> {
        if axes.len() != extents.len() {
            return Err(Refused::new(span, "a tensor slice"));
        }
        let mut indices = Vec::with_capacity(axes.len());
        for (axis, extent) in axes.iter().zip(extents) {
            let HirTensorAxis::Position(position) = axis else {
                return Err(Refused::new(span, "a tensor slice"));
            };
            let raw = self.expr(position, exit)?;
            let wide = self.widen_to_i64(&raw, &position.ty, span)?;
            let bound = self.assign(&format!("arith.constant {extent} : i64"));
            let inside = self.assign(&format!("arith.cmpi ult, {wide}, {bound} : i64"));
            self.guard(&inside, INDEX_OUT_OF_BOUNDS);
            indices.push(self.assign(&format!("arith.index_cast {wide} : i64 to index")));
        }
        Ok(indices.join(", "))
    }

    fn widen_to_i64(&mut self, value: &str, ty: &HirType, span: Span) -> Lowered<String> {
        let width = int_width(ty).ok_or_else(|| Refused::new(span, "a non-integer index"))?;
        if width == 64 {
            return Ok(value.to_string());
        }
        let extend = if is_unsigned(ty) { "extui" } else { "extsi" };
        Ok(self.assign(&format!("arith.{extend} {value} : i{width} to i64")))
    }

    fn if_chain(
        &mut self,
        condition: &HirExpr,
        then_block: &[HirStmt],
        else_if_blocks: &[(HirExpr, Vec<HirStmt>)],
        else_block: &Option<Vec<HirStmt>>,
        result: Option<(&str, &str)>,
        exit: &str,
    ) -> Lowered<()> {
        let join = self.block();
        let mut condition = condition;
        let mut then_block = then_block;
        let mut rest = else_if_blocks;
        loop {
            let tested = self.expr(condition, exit)?;
            let then = self.block();
            let otherwise = self.block();
            self.cond_branch(&tested, &then, &otherwise);
            self.start(&then);
            self.arm(then_block, result, exit)?;
            self.branch(&join);
            self.start(&otherwise);
            let Some(((next_condition, next_block), remaining)) = rest.split_first() else {
                break;
            };
            condition = next_condition;
            then_block = next_block;
            rest = remaining;
        }
        if let Some(else_block) = else_block {
            self.arm(else_block, result, exit)?;
        }
        self.branch(&join);
        self.start(&join);
        Ok(())
    }

    /// One arm's statements, its tail stored into `result` (`(slot, type)`) when the `if`
    /// has a value.
    fn arm(
        &mut self,
        statements: &[HirStmt],
        result: Option<(&str, &str)>,
        exit: &str,
    ) -> Lowered<()> {
        let (Some((slot, ty)), Some((HirStmt::Expr(tail), leading))) =
            (result, statements.split_last())
        else {
            return self.statements(statements, exit);
        };
        self.scopes.push(Vec::new());
        for statement in leading {
            if self.terminated {
                break;
            }
            self.statement(statement, exit)?;
        }
        if !self.terminated {
            let value = self.expr(tail, exit)?;
            self.line(&format!("memref.store {value}, {slot}[] : memref<{ty}>"));
        }
        self.scopes.pop();
        Ok(())
    }

    fn while_loop(
        &mut self,
        label: &Option<String>,
        condition: Option<&HirExpr>,
        body: &[HirStmt],
        exit: &str,
    ) -> Lowered<()> {
        let head = self.block();
        let inside = self.block();
        let after = self.block();
        self.branch(&head);
        self.start(&head);
        match condition {
            Some(condition) => {
                let tested = self.expr(condition, exit)?;
                self.cond_branch(&tested, &inside, &after);
            }
            None => self.branch(&inside),
        }
        self.start(&inside);
        self.loops.push(LoopTargets {
            label: label.clone(),
            next: head.clone(),
            exit: after.clone(),
        });
        self.statements(body, exit)?;
        self.loops.pop();
        self.branch(&head);
        self.start(&after);
        Ok(())
    }

    /// `for i in start..end` (or `..=`): the bounds are read once, before the loop. The
    /// inclusive form leaves at `end` before stepping, so an `end` at the type's maximum
    /// does not wrap around into an endless loop.
    #[expect(
        clippy::too_many_arguments,
        reason = "the loop's pieces as the HIR hands them over"
    )]
    fn for_range(
        &mut self,
        label: &Option<String>,
        iterator: &str,
        start: &HirExpr,
        end: &HirExpr,
        inclusive: bool,
        body: &[HirStmt],
        exit: &str,
    ) -> Lowered<()> {
        let ty = start.ty.clone();
        let Some(width) = int_width(&ty) else {
            return Err(Refused::new(start.span, "a loop over a non-integer range"));
        };
        let mlir = format!("i{width}");
        let first = self.expr(start, exit)?;
        let last = self.expr(end, exit)?;
        let slot = self.slot(&mlir);
        self.line(&format!("memref.store {first}, {slot}[] : memref<{mlir}>"));

        let head = self.block();
        let inside = self.block();
        let step = self.block();
        let after = self.block();
        self.branch(&head);
        self.start(&head);
        let current = self.assign(&format!("memref.load {slot}[] : memref<{mlir}>"));
        let order = match (inclusive, is_unsigned(&ty)) {
            (false, false) => "slt",
            (false, true) => "ult",
            (true, false) => "sle",
            (true, true) => "ule",
        };
        let more = self.assign(&format!("arith.cmpi {order}, {current}, {last} : {mlir}"));
        self.cond_branch(&more, &inside, &after);

        self.start(&inside);
        self.scopes.push(vec![(
            iterator.to_string(),
            Binding::Slot {
                slot: slot.clone(),
                ty: ty.clone(),
            },
        )]);
        self.loops.push(LoopTargets {
            label: label.clone(),
            next: step.clone(),
            exit: after.clone(),
        });
        let walked = self.statements(body, exit);
        self.loops.pop();
        self.scopes.pop();
        walked?;
        self.branch(&step);

        self.start(&step);
        let current = self.assign(&format!("memref.load {slot}[] : memref<{mlir}>"));
        if inclusive {
            let advance = self.block();
            let at_end = self.assign(&format!("arith.cmpi eq, {current}, {last} : {mlir}"));
            self.cond_branch(&at_end, &after, &advance);
            self.start(&advance);
        }
        let one = self.assign(&format!("arith.constant 1 : {mlir}"));
        let next = self.assign(&format!("arith.addi {current}, {one} : {mlir}"));
        self.line(&format!("memref.store {next}, {slot}[] : memref<{mlir}>"));
        self.branch(&head);
        self.start(&after);
        Ok(())
    }

    /// Lower `expr`, returning the SSA value it produces (empty for one with no value).
    fn expr(&mut self, expr: &HirExpr, exit: &str) -> Lowered<String> {
        match &expr.kind {
            HirExprKind::Literal(literal) => self.literal(literal, &expr.ty, expr.span),
            HirExprKind::Variable(name) => match self.lookup(name) {
                Some(Binding::Slot { slot, ty }) => {
                    let mlir = scalar_type(&ty).unwrap_or("i1");
                    Ok(self.assign(&format!("memref.load {slot}[] : memref<{mlir}>")))
                }
                Some(Binding::Param { value, ty }) if scalar_type(&ty).is_some() => Ok(value),
                Some(Binding::Param { .. }) => {
                    Err(Refused::new(expr.span, "a tensor used as a whole"))
                }
                None => Err(Refused::new(expr.span, "a name from outside the kernel")),
            },
            HirExprKind::GridPosition { of, axis } => Ok(self.grid_position(*of, *axis)),
            HirExprKind::Binary { op, left, right } => self.binary(*op, left, right, expr, exit),
            HirExprKind::Unary { op, operand } => self.unary(*op, operand, expr, exit),
            HirExprKind::Cast { value } => self.cast(value, &expr.ty, exit),
            HirExprKind::TensorIndex { object, axes } if scalar_type(&expr.ty).is_some() => {
                let (memref, ty, extents) = self.tensor(object)?;
                let indices = self.indices(axes, &extents, expr.span, exit)?;
                Ok(self.assign(&format!("memref.load {memref}[{indices}] : {ty}")))
            }
            HirExprKind::Index { object, index } if scalar_type(&expr.ty).is_some() => {
                let (memref, ty, extents) = self.tensor(object)?;
                let axes = [HirTensorAxis::Position((**index).clone())];
                let indices = self.indices(&axes, &extents, expr.span, exit)?;
                Ok(self.assign(&format!("memref.load {memref}[{indices}] : {ty}")))
            }
            HirExprKind::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
            } => {
                let Some(mlir) = scalar_type(&expr.ty) else {
                    self.if_chain(
                        condition,
                        then_block,
                        else_if_blocks,
                        else_block,
                        None,
                        exit,
                    )?;
                    return Ok(String::new());
                };
                let slot = self.slot(mlir);
                self.if_chain(
                    condition,
                    then_block,
                    else_if_blocks,
                    else_block,
                    Some((&slot, mlir)),
                    exit,
                )?;
                Ok(self.assign(&format!("memref.load {slot}[] : memref<{mlir}>")))
            }
            HirExprKind::Block { stmts } | HirExprKind::Unsafe { stmts } => {
                self.block_value(stmts, &expr.ty, exit)
            }
            HirExprKind::Loop { label, body } if matches!(expr.ty, HirType::Void) => {
                self.while_loop(label, None, body, exit)?;
                Ok(String::new())
            }
            _ => Err(Refused::new(expr.span, expression_kind(&expr.kind))),
        }
    }

    fn block_value(&mut self, statements: &[HirStmt], ty: &HirType, exit: &str) -> Lowered<String> {
        let Some(mlir) = scalar_type(ty) else {
            self.statements(statements, exit)?;
            return Ok(String::new());
        };
        let slot = self.slot(mlir);
        self.arm(statements, Some((&slot, mlir)), exit)?;
        Ok(self.assign(&format!("memref.load {slot}[] : memref<{mlir}>")))
    }

    /// The block size is written as the constant it is rather than read with
    /// `gpu.block_dim`, which ROCDL lowers to a call into ROCm's device library.
    fn grid_position(&mut self, of: HirGridIndex, axis: u8) -> String {
        let axis = usize::from(axis).min(GRID_DIMENSIONS.len() - 1);
        let dimension = GRID_DIMENSIONS[axis];
        let block = self.assign(&format!("gpu.block_id {dimension}"));
        let position = match of {
            HirGridIndex::Block => block,
            HirGridIndex::Thread => {
                let size = self.assign(&format!("arith.constant {} : index", self.threads[axis]));
                let local = self.assign(&format!("gpu.thread_id {dimension}"));
                let first = self.assign(&format!("arith.muli {block}, {size} : index"));
                self.assign(&format!("arith.addi {first}, {local} : index"))
            }
        };
        self.assign(&format!("arith.index_cast {position} : index to i32"))
    }

    fn literal(&mut self, literal: &Literal, ty: &HirType, span: Span) -> Lowered<String> {
        let text = match (literal, ty) {
            (Literal::Boolean(value), HirType::Bool) => format!("arith.constant {value}"),
            (Literal::Integer(value, _), ty) if int_width(ty).is_some() => {
                let width = int_width(ty).unwrap_or(64);
                format!("arith.constant {} : i{width}", signed_bits(*value, width))
            }
            (Literal::Integer(value, _), HirType::F32) => float_constant(*value as f64, ty),
            (Literal::Integer(value, _), HirType::F64) => float_constant(*value as f64, ty),
            (Literal::Float(value, _), HirType::F32 | HirType::F64) => float_constant(*value, ty),
            _ => return Err(Refused::new(span, "a literal of this type")),
        };
        Ok(self.assign(&text))
    }

    fn binary(
        &mut self,
        op: BinaryOp,
        left: &HirExpr,
        right: &HirExpr,
        expr: &HirExpr,
        exit: &str,
    ) -> Lowered<String> {
        if matches!(op, BinaryOp::And | BinaryOp::Or) {
            return self.short_circuit(op, left, right, exit);
        }
        let operand_ty = &left.ty;
        let Some(mlir) = scalar_type(operand_ty) else {
            return Err(Refused::new(expr.span, "an operator on a non-scalar"));
        };
        let lhs = self.expr(left, exit)?;
        let rhs = self.expr(right, exit)?;
        if op.is_comparison() {
            let predicate = comparison(op, operand_ty)
                .ok_or_else(|| Refused::new(expr.span, "this comparison"))?;
            let family = if is_float(operand_ty) { "cmpf" } else { "cmpi" };
            return Ok(self.assign(&format!(
                "arith.{family} {predicate}, {lhs}, {rhs} : {mlir}"
            )));
        }
        if is_float(operand_ty) {
            let name = match op {
                BinaryOp::Add => "addf",
                BinaryOp::Subtract => "subf",
                BinaryOp::Multiply => "mulf",
                BinaryOp::Divide => "divf",
                BinaryOp::Modulo => "remf",
                _ => return Err(Refused::new(expr.span, "this operator on a float")),
            };
            return Ok(self.assign(&format!("arith.{name} {lhs}, {rhs} : {mlir}")));
        }
        if int_width(operand_ty).is_none() {
            return Err(Refused::new(expr.span, "this operator on a `bool`"));
        }
        // Integer arithmetic wraps, as it does in a release build; the kernel has no
        // debug tier to panic in.
        let name = match op {
            BinaryOp::Add => "addi",
            BinaryOp::Subtract => "subi",
            BinaryOp::Multiply => "muli",
            BinaryOp::BitAnd => "andi",
            BinaryOp::BitOr => "ori",
            BinaryOp::BitXor => "xori",
            BinaryOp::Divide | BinaryOp::Modulo => {
                return Ok(self.int_div_rem(op, &lhs, &rhs, operand_ty, mlir));
            }
            _ => return Err(Refused::new(expr.span, "this operator on an integer")),
        };
        Ok(self.assign(&format!("arith.{name} {lhs}, {rhs} : {mlir}")))
    }

    /// A zero divisor stops the thread. `MIN / -1` has no representable quotient, so it
    /// divides by 1 instead, which gives the wrapped answer (`MIN`, remainder 0) the host
    /// gives in a release build without handing the instruction its one undefined case.
    fn int_div_rem(
        &mut self,
        op: BinaryOp,
        lhs: &str,
        rhs: &str,
        ty: &HirType,
        mlir: &str,
    ) -> String {
        let zero = self.assign(&format!("arith.constant 0 : {mlir}"));
        let nonzero = self.assign(&format!("arith.cmpi ne, {rhs}, {zero} : {mlir}"));
        self.guard(&nonzero, DIVIDE_BY_ZERO);
        let remainder = matches!(op, BinaryOp::Modulo);
        if is_unsigned(ty) {
            let name = if remainder { "remui" } else { "divui" };
            return self.assign(&format!("arith.{name} {lhs}, {rhs} : {mlir}"));
        }
        let width = int_width(ty).unwrap_or(64);
        let min = self.assign(&format!(
            "arith.constant {} : {mlir}",
            signed_bits(1i128 << (width - 1), width)
        ));
        let minus_one = self.assign(&format!("arith.constant -1 : {mlir}"));
        let one = self.assign(&format!("arith.constant 1 : {mlir}"));
        let lhs_min = self.assign(&format!("arith.cmpi eq, {lhs}, {min} : {mlir}"));
        let rhs_minus_one = self.assign(&format!("arith.cmpi eq, {rhs}, {minus_one} : {mlir}"));
        let overflows = self.assign(&format!("arith.andi {lhs_min}, {rhs_minus_one} : i1"));
        let divisor = self.assign(&format!("arith.select {overflows}, {one}, {rhs} : {mlir}"));
        let name = if remainder { "remsi" } else { "divsi" };
        self.assign(&format!("arith.{name} {lhs}, {divisor} : {mlir}"))
    }

    /// `&&` / `||`: the right operand runs only when the left one leaves the answer open,
    /// which is what lets `row < M && a[row] > 0` guard its own index.
    fn short_circuit(
        &mut self,
        op: BinaryOp,
        left: &HirExpr,
        right: &HirExpr,
        exit: &str,
    ) -> Lowered<String> {
        let slot = self.slot("i1");
        let lhs = self.expr(left, exit)?;
        self.line(&format!("memref.store {lhs}, {slot}[] : memref<i1>"));
        let rest = self.block();
        let join = self.block();
        match op {
            BinaryOp::And => self.cond_branch(&lhs, &rest, &join),
            _ => self.cond_branch(&lhs, &join, &rest),
        }
        self.start(&rest);
        let rhs = self.expr(right, exit)?;
        self.line(&format!("memref.store {rhs}, {slot}[] : memref<i1>"));
        self.branch(&join);
        self.start(&join);
        Ok(self.assign(&format!("memref.load {slot}[] : memref<i1>")))
    }

    fn unary(
        &mut self,
        op: UnaryOp,
        operand: &HirExpr,
        expr: &HirExpr,
        exit: &str,
    ) -> Lowered<String> {
        let Some(mlir) = scalar_type(&operand.ty) else {
            return Err(Refused::new(expr.span, "an operator on a non-scalar"));
        };
        let value = self.expr(operand, exit)?;
        let text = match (op, &operand.ty) {
            (UnaryOp::Negate, ty) if is_float(ty) => format!("arith.negf {value} : {mlir}"),
            (UnaryOp::Negate, ty) if int_width(ty).is_some() => {
                let zero = self.assign(&format!("arith.constant 0 : {mlir}"));
                format!("arith.subi {zero}, {value} : {mlir}")
            }
            (UnaryOp::Not, HirType::Bool) => {
                let all = self.assign("arith.constant true");
                format!("arith.xori {value}, {all} : i1")
            }
            (UnaryOp::BitNot, ty) if int_width(ty).is_some() => {
                let all = self.assign(&format!("arith.constant -1 : {mlir}"));
                format!("arith.xori {value}, {all} : {mlir}")
            }
            _ => return Err(Refused::new(expr.span, "this unary operator")),
        };
        Ok(self.assign(&text))
    }

    fn cast(&mut self, value: &HirExpr, target: &HirType, exit: &str) -> Lowered<String> {
        let (Some(from), Some(to)) = (scalar_type(&value.ty), scalar_type(target)) else {
            return Err(Refused::new(value.span, "a cast between non-scalars"));
        };
        let source = self.expr(value, exit)?;
        let (from_ty, from_width, to_width) = (&value.ty, int_width(&value.ty), int_width(target));
        let text = match (from_width, to_width) {
            _ if from == to => return Ok(source),
            (Some(a), Some(b)) if a < b => {
                let extend = if is_unsigned(from_ty) {
                    "extui"
                } else {
                    "extsi"
                };
                format!("arith.{extend} {source} : {from} to {to}")
            }
            (Some(_), Some(_)) => format!("arith.trunci {source} : {from} to {to}"),
            (Some(_), None) if is_float(target) => {
                let convert = if is_unsigned(from_ty) {
                    "uitofp"
                } else {
                    "sitofp"
                };
                format!("arith.{convert} {source} : {from} to {to}")
            }
            // Saturating, as on the host: the plain conversion is undefined for a value
            // the integer type cannot hold.
            (None, Some(_)) if is_float(from_ty) => {
                let convert = if is_unsigned(target) {
                    "fptoui"
                } else {
                    "fptosi"
                };
                format!(
                    "llvm.call_intrinsic \"llvm.{convert}.sat.{to}.{from}\"({source}) : ({from}) -> {to}"
                )
            }
            (None, Some(_)) if matches!(from_ty, HirType::Bool) => {
                format!("arith.extui {source} : i1 to {to}")
            }
            (None, None) if is_float(from_ty) && is_float(target) => {
                let convert = if from == "f32" { "extf" } else { "truncf" };
                format!("arith.{convert} {source} : {from} to {to}")
            }
            _ => return Err(Refused::new(value.span, "this cast")),
        };
        Ok(self.assign(&text))
    }
}

fn comparison(op: BinaryOp, ty: &HirType) -> Option<&'static str> {
    if is_float(ty) {
        return Some(match op {
            BinaryOp::Equal => "oeq",
            BinaryOp::NotEqual => "one",
            BinaryOp::Less => "olt",
            BinaryOp::Greater => "ogt",
            BinaryOp::LessEqual => "ole",
            BinaryOp::GreaterEqual => "oge",
            _ => return None,
        });
    }
    let unsigned = is_unsigned(ty) || matches!(ty, HirType::Bool);
    Some(match (op, unsigned) {
        (BinaryOp::Equal, _) => "eq",
        (BinaryOp::NotEqual, _) => "ne",
        (BinaryOp::Less, false) => "slt",
        (BinaryOp::Less, true) => "ult",
        (BinaryOp::Greater, false) => "sgt",
        (BinaryOp::Greater, true) => "ugt",
        (BinaryOp::LessEqual, false) => "sle",
        (BinaryOp::LessEqual, true) => "ule",
        (BinaryOp::GreaterEqual, false) => "sge",
        (BinaryOp::GreaterEqual, true) => "uge",
        _ => return None,
    })
}

/// `value` truncated to `width` bits and read back as signed: MLIR parses an integer
/// constant against the signed range of its type, so a `u32` above `i32::MAX` is written
/// as the negative number with the same bits.
fn signed_bits(value: i128, width: u32) -> i128 {
    let shift = 128 - width;
    (value << shift) >> shift
}

/// A float constant as its bit pattern, which MLIR reads exactly and which spells
/// infinities and NaNs as well as any finite value.
fn float_constant(value: f64, ty: &HirType) -> String {
    match ty {
        HirType::F32 => format!("arith.constant 0x{:08X} : f32", (value as f32).to_bits()),
        _ => format!("arith.constant 0x{:016X} : f64", value.to_bits()),
    }
}

fn stmt_span(statement: &HirStmt) -> Span {
    match statement {
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

fn statement_kind(statement: &HirStmt) -> &'static str {
    match statement {
        HirStmt::TensorCompoundAssign { .. } => "a whole-tensor update",
        HirStmt::ForRange { .. } => "a reversed, stepped or enumerated range loop",
        HirStmt::ForEach { .. } => "a loop over a collection",
        HirStmt::Break { .. } => "a `break` with a value",
        HirStmt::Return { .. } => "a `return` with a value",
        HirStmt::ValElse { .. } => "a `val ... else`",
        HirStmt::Const { .. } => "a local constant",
        _ => "this statement",
    }
}

fn expression_kind(kind: &HirExprKind) -> &'static str {
    match kind {
        HirExprKind::Call { .. } => "a function call",
        HirExprKind::Closure { .. } => "a closure",
        HirExprKind::Math { .. } => "a math function",
        HirExprKind::Match { .. } => "a `match`",
        HirExprKind::TensorIndex { .. } | HirExprKind::Index { .. } => "a tensor slice",
        HirExprKind::Loop { .. } => "a `loop` with a value",
        HirExprKind::Pool { .. } => "a `pool` block",
        _ => "this expression",
    }
}
