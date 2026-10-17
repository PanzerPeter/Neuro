//! A `@kernel` body as the region of a `gpu.launch`: one thread's code.
//!
//! Every local lives in a `memref.alloca` slot hoisted to the region's entry, and control
//! flow is `cf` branches between blocks, so a loop needs no loop-carried SSA values and a
//! `break`, `continue` or `return` is a plain branch. LLVM promotes the slots to registers.
//! A value an `if` or `&&` produces goes through a slot too, which keeps every SSA value
//! used only in the block that defines it or one it dominates.

mod traversal;

use std::fmt::Write as _;

use ast_types::{BinaryOp, UnaryOp};
use neuro_hir::{
    HirBindingSource, HirExpr, HirExprKind, HirFunction, HirGridIndex, HirItem, HirMatchArm,
    HirMatchTest, HirMathOp, HirPlace, HirProgram, HirStmt, HirTensorAxis, HirType,
};
use shared_types::{Literal, Span};

use super::GuardStyle;
use crate::guards::{Guard, GuardKind, Overflow, SITE_BITS, STATUS_BITS};

pub(super) use traversal::{Launch, outlined_launch};

/// The grid axes as `gpu` dialect dimensions, in field order.
const GRID_DIMENSIONS: [&str; 3] = ["x", "y", "z"];

const INDEX_OUT_OF_BOUNDS: &str = "index out of bounds in a `@kernel` body";
const DIVIDE_BY_ZERO: &str = "integer division by zero in a `@kernel` body";
const REMAINDER_BY_ZERO: &str = "integer remainder by zero in a `@kernel` body";
const OVERFLOW: &str = "integer overflow in a `@kernel` body";
const SHIFT_OVERFLOW: &str = "shift overflow in a `@kernel` body";
const RANGE_STEP: &str = "range step must be positive in a `@kernel` body";

/// The launcher parameter a checked body stores a failed check's number into.
pub(super) const STATUS: &str = "%status";

/// Why a math function has no device form in this compile.
const NO_DEVICE_MATH: &str = "a math function, which needs the GPU vendor's device math library (libdevice from the CUDA toolkit, or ROCm's) when compiling";

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

/// [`scalar_type`], and a `char` as the 32-bit code point the host holds it in. A `char` is
/// only a local's or a `match`'s type here: no kernel parameter or tensor element is one.
fn value_type(ty: &HirType) -> Option<&'static str> {
    match ty {
        HirType::Char => Some("i32"),
        other => scalar_type(other),
    }
}

/// The width a `match` tests a scrutinee at, or a cast converts it at: an integer's, or a
/// `char`'s 32 bits. `None` for a `bool`, whose one bit `match_value` supplies itself.
fn int_like_width(ty: &HirType) -> Option<u32> {
    match ty {
        HirType::Char => Some(32),
        other => int_width(other),
    }
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

/// A name the body can read: a local's slot, a parameter's block argument, or the
/// `slice` of a `partition`.
#[derive(Clone)]
enum Binding {
    Slot { slot: String, ty: HirType },
    Param { value: String, ty: HirType },
    Slice(SliceView),
}

/// One thread's run of a partitioned output: `chunk` elements from the row-major
/// position `base`, an `i64` value, of the tensor behind `memref`.
#[derive(Clone)]
struct SliceView {
    memref: String,
    ty: String,
    extents: Vec<usize>,
    base: String,
    chunk: usize,
}

/// How a thread that failed an integer check stops. A `@kernel` stops the kernel, as an
/// index past an extent does ([`GuardStyle`]). An outlined body lowers [`STATUS`] to the
/// check's key and leaves the launch, so its caller panics as the host would.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Failure {
    Stop,
    Report,
}

/// Where `break` and `continue` go for one enclosing loop, and, for a `loop` with a value,
/// the slot a `break` stores it into and its type.
struct LoopTargets {
    label: Option<String>,
    next: String,
    exit: String,
    value: Option<(String, &'static str)>,
}

/// Emits one kernel body as the text of a `gpu.launch` region.
pub(crate) struct BodyEmitter<'f> {
    function: &'f HirFunction,
    guard: GuardStyle,
    /// The launch's block shape, a constant the body multiplies block positions by.
    threads: [u32; 3],
    /// The grid tensor's extents: which threads own an element, and so a `partition` run.
    grid: Vec<usize>,
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
    /// Whether the vendor's device math library is there to call. Without it a math
    /// function has no device form.
    math: bool,
    /// Whether integer overflow is a failed check, the debug tier, or wraps. A zero divisor
    /// fails on both.
    overflow: Overflow,
    /// What a failed integer check does to the thread.
    pub(super) failure: Failure,
    /// The checks emitted so far, in the order their numbers count.
    sites: Vec<Guard>,
    /// The block a thread leaves the launch through, which a failed check branches to.
    stop: Option<String>,
    /// The row-major position, an `i64`, of the element this thread computes, where a failed
    /// check's key needs it: a thread stops at its first failure, so the earliest element's
    /// is the one the host meets first.
    element: Option<String>,
    /// Where a `return` with a value stores it: the slot of the function being called, and
    /// its type. A `@kernel` returns nothing.
    returns: Option<(String, &'static str)>,
    /// The program's module constants, a read of one lowering to its value.
    program: &'f HirProgram,
}

impl<'f> BodyEmitter<'f> {
    pub(crate) fn new(
        (program, function): (&'f HirProgram, &'f HirFunction),
        (guard, overflow): (GuardStyle, Overflow),
        threads: [u32; 3],
        grid: Vec<usize>,
        math: bool,
    ) -> Self {
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
            grid,
            slots: String::new(),
            text: String::new(),
            next_value: 0,
            next_block: 0,
            scopes: vec![params],
            loops: Vec::new(),
            terminated: false,
            math,
            overflow,
            failure: Failure::Stop,
            sites: Vec::new(),
            stop: None,
            element: None,
            returns: None,
            program,
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
                self.local(name, ty, init.as_ref(), stmt_span(statement), exit)
            }
            // A local `const` is a scalar local nothing reassigns.
            HirStmt::Const {
                name,
                ty,
                value,
                span,
            } => self.local(name, ty, Some(value), *span, exit),
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
            } => self.while_loop(label, Some(condition), body, None, exit),
            HirStmt::ForRange { index: None, .. } => self.for_range(statement, exit),
            HirStmt::Break { label, value, span } => {
                let target = self.loop_target(label, *span)?;
                let (target, slot) = (target.exit.clone(), target.value.clone());
                match (value, slot) {
                    (None, _) => {}
                    (Some(value), Some((slot, ty))) => {
                        let value = self.expr(value, exit)?;
                        self.line(&format!("memref.store {value}, {slot}[] : memref<{ty}>"));
                    }
                    (Some(_), None) => return Err(Refused::new(*span, "a `break` with a value")),
                }
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
            HirStmt::Return {
                value: Some(value), ..
            } if self.returns.is_some() => {
                let value = self.expr(value, exit)?;
                if let Some((slot, ty)) = &self.returns {
                    let store = format!("memref.store {value}, {slot}[] : memref<{ty}>");
                    self.line(&store);
                }
                self.branch(exit);
                Ok(())
            }
            HirStmt::Expr(expr) => self.expr(expr, exit).map(|_| ()),
            other => Err(Refused::new(stmt_span(other), statement_kind(other))),
        }
    }

    /// The value of the module constant `name` when it is a scalar, which a read of it in
    /// the body computes in place.
    fn module_constant(&self, name: &str) -> Option<&'f HirExpr> {
        self.program.items.iter().find_map(|item| match item {
            HirItem::Const(constant)
                if constant.name == name && scalar_type(&constant.ty).is_some() =>
            {
                Some(&constant.value)
            }
            _ => None,
        })
    }

    /// A scalar local in a slot of its own, stored from `init` when it has one.
    fn local(
        &mut self,
        name: &str,
        ty: &HirType,
        init: Option<&HirExpr>,
        span: Span,
        exit: &str,
    ) -> Lowered<()> {
        let mlir = value_type(ty).ok_or_else(|| Refused::new(span, "a local of this type"))?;
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
                let mlir = value_type(&ty).unwrap_or("i1");
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
                let (memref, ty, indices) = self.indexed(object, index, span, exit)?;
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

    /// `while`, or `loop` when there is no `condition`. `value` is a valued `loop`'s result
    /// slot, which its `break`s store into.
    fn while_loop(
        &mut self,
        label: &Option<String>,
        condition: Option<&HirExpr>,
        body: &[HirStmt],
        value: Option<(String, &'static str)>,
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
            value,
        });
        self.statements(body, exit)?;
        self.loops.pop();
        self.branch(&head);
        self.start(&after);
        Ok(())
    }

    /// `for i in start..end` (or `..=`), `.rev()` and `.step(n)` included, walked as the host
    /// walks it. The bounds, then the stride, are read once before the loop, and a stride
    /// that is not positive fails on every tier. The induction variable counts up from
    /// `start` even when reversed: the binding is then `start + last - k`, stored at the top
    /// of the body, so nothing steps below `start` (which wraps at an unsigned zero). A
    /// stepped or inclusive loop leaves when the distance still to `end` is short of the
    /// stride, rather than adding it first, which wraps near the type's maximum.
    fn for_range(&mut self, range: &HirStmt, exit: &str) -> Lowered<()> {
        let HirStmt::ForRange {
            label,
            index: None,
            iterator,
            start,
            end,
            inclusive,
            reversed,
            step,
            body,
            span,
        } = range
        else {
            return Err(Refused::new(stmt_span(range), statement_kind(range)));
        };
        let ty = start.ty.clone();
        let Some(width) = int_width(&ty) else {
            return Err(Refused::new(*span, "a loop over a non-integer range"));
        };
        let mlir = format!("i{width}");
        let first = self.expr(start, exit)?;
        let last = self.expr(end, exit)?;
        let stride = match step {
            Some(step) => Some(self.range_stride(step, &ty, exit)?),
            None => None,
        };
        let induction = self.slot(&mlir);
        self.line(&format!(
            "memref.store {first}, {induction}[] : memref<{mlir}>"
        ));
        let (binding, origin) = if *reversed {
            let top = match inclusive {
                true => last.clone(),
                false => {
                    let one = self.assign(&format!("arith.constant 1 : {mlir}"));
                    self.assign(&format!("arith.subi {last}, {one} : {mlir}"))
                }
            };
            let origin = self.assign(&format!("arith.addi {first}, {top} : {mlir}"));
            (self.slot(&mlir), Some(origin))
        } else {
            (induction.clone(), None)
        };

        let head = self.block();
        let inside = self.block();
        let next = self.block();
        let after = self.block();
        self.branch(&head);
        self.start(&head);
        let current = self.assign(&format!("memref.load {induction}[] : memref<{mlir}>"));
        let order = match (inclusive, is_unsigned(&ty)) {
            (false, false) => "slt",
            (false, true) => "ult",
            (true, false) => "sle",
            (true, true) => "ule",
        };
        let more = self.assign(&format!("arith.cmpi {order}, {current}, {last} : {mlir}"));
        self.cond_branch(&more, &inside, &after);

        self.start(&inside);
        if let Some(origin) = origin {
            let mirrored = self.assign(&format!("arith.subi {origin}, {current} : {mlir}"));
            self.line(&format!(
                "memref.store {mirrored}, {binding}[] : memref<{mlir}>"
            ));
        }
        self.scopes.push(vec![(
            iterator.to_string(),
            Binding::Slot {
                slot: binding,
                ty: ty.clone(),
            },
        )]);
        self.loops.push(LoopTargets {
            label: label.clone(),
            next: next.clone(),
            exit: after.clone(),
            value: None,
        });
        let walked = self.statements(body, exit);
        self.loops.pop();
        self.scopes.pop();
        walked?;
        self.branch(&next);

        self.start(&next);
        let current = self.assign(&format!("memref.load {induction}[] : memref<{mlir}>"));
        let increment = match &stride {
            Some(stride) => stride.clone(),
            None => self.assign(&format!("arith.constant 1 : {mlir}")),
        };
        if stride.is_some() || *inclusive {
            // An inclusive range may still yield `end`, so a stride landing on it goes on.
            let overshoots = if *inclusive { "ult" } else { "ule" };
            let left = self.assign(&format!("arith.subi {last}, {current} : {mlir}"));
            let done = self.assign(&format!(
                "arith.cmpi {overshoots}, {left}, {increment} : {mlir}"
            ));
            let advance = self.block();
            self.cond_branch(&done, &after, &advance);
            self.start(&advance);
        }
        let stepped = self.assign(&format!("arith.addi {current}, {increment} : {mlir}"));
        self.line(&format!(
            "memref.store {stepped}, {induction}[] : memref<{mlir}>"
        ));
        self.branch(&head);
        self.start(&after);
        Ok(())
    }

    /// A `.step(n)` stride, read once: a zero one would never leave the loop and a negative
    /// one has no meaning, so either fails, on every tier, as on the host.
    fn range_stride(&mut self, step: &HirExpr, ty: &HirType, exit: &str) -> Lowered<String> {
        let mlir = scalar_type(ty).unwrap_or("i64");
        let stride = self.expr(step, exit)?;
        let zero = self.assign(&format!("arith.constant 0 : {mlir}"));
        let not_positive = if is_unsigned(ty) { "eq" } else { "sle" };
        let failed = self.assign(&format!(
            "arith.cmpi {not_positive}, {stride}, {zero} : {mlir}"
        ));
        self.fail_if(&failed, GuardKind::RangeStep, step.span.start);
        Ok(stride)
    }

    /// Lower `expr`, returning the SSA value it produces (empty for one with no value).
    fn expr(&mut self, expr: &HirExpr, exit: &str) -> Lowered<String> {
        match &expr.kind {
            HirExprKind::Literal(literal) => self.literal(literal, &expr.ty, expr.span),
            HirExprKind::Variable(name) => match self.lookup(name) {
                Some(Binding::Slot { slot, ty }) => {
                    let mlir = value_type(&ty).unwrap_or("i1");
                    Ok(self.assign(&format!("memref.load {slot}[] : memref<{mlir}>")))
                }
                Some(Binding::Param { value, ty }) if value_type(&ty).is_some() => Ok(value),
                Some(Binding::Param { .. }) => {
                    Err(Refused::new(expr.span, "a tensor used as a whole"))
                }
                Some(Binding::Slice(_)) => Err(Refused::new(
                    expr.span,
                    "a `partition` slice used other than by index or `.len()`",
                )),
                None => match self.module_constant(name) {
                    Some(value) => self.expr(value, exit),
                    None => Err(Refused::new(expr.span, "a name from outside the kernel")),
                },
            },
            HirExprKind::GridPosition { of, axis } => Ok(self.grid_position(*of, *axis)),
            HirExprKind::Binary { op, left, right } => self.binary(*op, left, right, expr, exit),
            HirExprKind::Unary { op, operand } => self.unary(*op, operand, expr, exit),
            HirExprKind::Cast { value } => self.cast(value, &expr.ty, exit),
            HirExprKind::Math {
                op,
                operand,
                exponent,
            } => self.math(*op, operand, exponent.as_deref(), expr, exit),
            HirExprKind::TensorIndex { object, axes } if scalar_type(&expr.ty).is_some() => {
                let (memref, ty, extents) = self.tensor(object)?;
                let indices = self.indices(axes, &extents, expr.span, exit)?;
                Ok(self.assign(&format!("memref.load {memref}[{indices}] : {ty}")))
            }
            HirExprKind::Index { object, index } if scalar_type(&expr.ty).is_some() => {
                let (memref, ty, indices) = self.indexed(object, index, expr.span, exit)?;
                Ok(self.assign(&format!("memref.load {memref}[{indices}] : {ty}")))
            }
            HirExprKind::Call { callee, args } => self.call(callee, args, expr.span, exit),
            HirExprKind::KernelPartition {
                out,
                base,
                slice,
                body,
            } => {
                self.partition(out, base, slice, body, expr.span)?;
                Ok(String::new())
            }
            HirExprKind::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
            } => {
                let Some(mlir) = value_type(&expr.ty) else {
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
                self.while_loop(label, None, body, None, exit)?;
                Ok(String::new())
            }
            HirExprKind::Loop { label, body } => {
                let Some(mlir) = value_type(&expr.ty) else {
                    return Err(Refused::new(expr.span, expression_kind(&expr.kind)));
                };
                let slot = self.slot(mlir);
                self.while_loop(label, None, body, Some((slot.clone(), mlir)), exit)?;
                Ok(self.assign(&format!("memref.load {slot}[] : memref<{mlir}>")))
            }
            HirExprKind::Match { scrutinee, arms } => self.match_value(scrutinee, arms, expr, exit),
            _ => Err(Refused::new(expr.span, expression_kind(&expr.kind))),
        }
    }

    fn block_value(&mut self, statements: &[HirStmt], ty: &HirType, exit: &str) -> Lowered<String> {
        let Some(mlir) = value_type(ty) else {
            self.statements(statements, exit)?;
            return Ok(String::new());
        };
        let slot = self.slot(mlir);
        self.arm(statements, Some((&slot, mlir)), exit)?;
        Ok(self.assign(&format!("memref.load {slot}[] : memref<{mlir}>")))
    }

    fn grid_position(&mut self, of: HirGridIndex, axis: u8) -> String {
        let axis = usize::from(axis).min(GRID_DIMENSIONS.len() - 1);
        let position = match of {
            HirGridIndex::Block => self.assign(&format!("gpu.block_id {}", GRID_DIMENSIONS[axis])),
            HirGridIndex::Thread => self.thread_position(axis),
        };
        self.assign(&format!("arith.index_cast {position} : index to i32"))
    }

    /// The thread's global position along grid axis `axis`, as an `index`. The block size
    /// is written as the constant it is rather than read with `gpu.block_dim`, which ROCDL
    /// lowers to a call into ROCm's device library.
    fn thread_position(&mut self, axis: usize) -> String {
        let dimension = GRID_DIMENSIONS[axis];
        let block = self.assign(&format!("gpu.block_id {dimension}"));
        let size = self.assign(&format!("arith.constant {} : index", self.threads[axis]));
        let local = self.assign(&format!("gpu.thread_id {dimension}"));
        let first = self.assign(&format!("arith.muli {block}, {size} : index"));
        self.assign(&format!("arith.addi {first}, {local} : index"))
    }

    /// `out.partition(|base, slice| body)`. A thread's number is the row-major position
    /// of the grid element it owns, so thread `n` gets elements `n * chunk` up to
    /// `(n + 1) * chunk` of `out`: every run disjoint, and every element in one. A thread
    /// in the overhang owns no grid element and skips the body. The body's `return`
    /// leaves the closure, so it ends at the partition's own exit.
    fn partition(
        &mut self,
        out: &HirExpr,
        base: &str,
        slice: &str,
        body: &[HirStmt],
        span: Span,
    ) -> Lowered<()> {
        let (memref, ty, extents) = self.tensor(out)?;
        let threads: usize = self.grid.iter().product();
        let elements: usize = extents.iter().product();
        if threads == 0 || !elements.is_multiple_of(threads) {
            return Err(Refused::new(
                span,
                &format!(
                    "a `partition` of {elements} elements, which the grid's {threads} threads cannot share equally,"
                ),
            ));
        }
        let chunk = elements / threads;

        let mut owned = None;
        let mut number = None;
        for (axis, extent) in self.grid.clone().into_iter().enumerate() {
            let position = self.thread_position(axis);
            let bound = self.assign(&format!("arith.constant {extent} : index"));
            let inside = self.assign(&format!("arith.cmpi ult, {position}, {bound} : index"));
            owned = Some(match owned {
                None => inside,
                Some(all) => self.assign(&format!("arith.andi {all}, {inside} : i1")),
            });
            number = Some(match number {
                None => position,
                Some(outer) => {
                    let scaled = self.assign(&format!("arith.muli {outer}, {bound} : index"));
                    self.assign(&format!("arith.addi {scaled}, {position} : index"))
                }
            });
        }
        let (Some(owned), Some(number)) = (owned, number) else {
            return Err(Refused::new(span, "a `partition` in a kernel with no grid"));
        };
        let run = self.block();
        let after = self.block();
        self.cond_branch(&owned, &run, &after);
        self.start(&run);
        let number = self.assign(&format!("arith.index_cast {number} : index to i64"));
        let size = self.assign(&format!("arith.constant {chunk} : i64"));
        let first = self.assign(&format!("arith.muli {number}, {size} : i64"));
        let slot = self.slot("i64");
        self.line(&format!("memref.store {first}, {slot}[] : memref<i64>"));
        let view = SliceView {
            memref,
            ty,
            extents,
            base: first,
            chunk,
        };
        self.scopes.push(vec![
            (
                base.to_string(),
                Binding::Slot {
                    slot,
                    ty: HirType::U64,
                },
            ),
            (slice.to_string(), Binding::Slice(view)),
        ]);
        let loops = std::mem::take(&mut self.loops);
        let walked = self.statements(body, &after);
        self.loops = loops;
        self.scopes.pop();
        walked?;
        self.branch(&after);
        self.start(&after);
        Ok(())
    }

    /// The partition slice `object` names, if it names one.
    fn slice(&self, object: &HirExpr) -> Option<SliceView> {
        let inner = match &object.kind {
            HirExprKind::Deref { operand } => operand.as_ref(),
            _ => object,
        };
        let HirExprKind::Variable(name) = &inner.kind else {
            return None;
        };
        match self.lookup(name) {
            Some(Binding::Slice(view)) => Some(view),
            _ => None,
        }
    }

    /// Where `object[index]` lives: an element of a partition slice or of a rank-1 tensor
    /// parameter, as the `memref`, its type and the bounds-checked indices into it.
    fn indexed(
        &mut self,
        object: &HirExpr,
        index: &HirExpr,
        span: Span,
        exit: &str,
    ) -> Lowered<(String, String, String)> {
        let Some(view) = self.slice(object) else {
            let (memref, ty, extents) = self.tensor(object)?;
            let axes = [HirTensorAxis::Position(index.clone())];
            let indices = self.indices(&axes, &extents, span, exit)?;
            return Ok((memref, ty, indices));
        };
        let raw = self.expr(index, exit)?;
        let wide = self.widen_to_i64(&raw, &index.ty, span)?;
        let bound = self.assign(&format!("arith.constant {} : i64", view.chunk));
        let inside = self.assign(&format!("arith.cmpi ult, {wide}, {bound} : i64"));
        self.guard(&inside, INDEX_OUT_OF_BOUNDS);
        let flat = self.assign(&format!("arith.addi {}, {wide} : i64", view.base));
        let indices = self.delinearize(&flat, &view.extents);
        Ok((view.memref, view.ty, indices))
    }

    /// `flat`, an `i64` row-major position already known to be in range, as one `index`
    /// per axis of `extents`. An empty axis gets the unwrapped quotient, as `.flat` does.
    fn delinearize(&mut self, flat: &str, extents: &[usize]) -> String {
        self.delinearize_each(flat, extents).join(", ")
    }

    /// [`Self::delinearize`], one `index` value per axis.
    fn delinearize_each(&mut self, flat: &str, extents: &[usize]) -> Vec<String> {
        let mut indices = Vec::with_capacity(extents.len());
        for (axis, &extent) in extents.iter().enumerate() {
            let stride: usize = extents[axis + 1..].iter().map(|&e| e.max(1)).product();
            let mut value = flat.to_string();
            if stride > 1 {
                let divisor = self.assign(&format!("arith.constant {stride} : i64"));
                value = self.assign(&format!("arith.divui {value}, {divisor} : i64"));
            }
            if axis > 0 && extent > 0 {
                let modulus = self.assign(&format!("arith.constant {extent} : i64"));
                value = self.assign(&format!("arith.remui {value}, {modulus} : i64"));
            }
            indices.push(self.assign(&format!("arith.index_cast {value} : i64 to index")));
        }
        indices
    }

    /// The one call a body makes: `slice.len()` on a partition slice, its run length.
    fn call(
        &mut self,
        callee: &HirExpr,
        args: &[HirExpr],
        span: Span,
        exit: &str,
    ) -> Lowered<String> {
        let HirExprKind::FieldAccess { object, field } = &callee.kind else {
            return Err(Refused::new(span, "a function call"));
        };
        if field == "len"
            && args.is_empty()
            && let Some(view) = self.slice(object)
        {
            return Ok(self.assign(&format!("arith.constant {} : i64", view.chunk)));
        }
        match self.scalar_method(object, field, args, exit)? {
            Some(value) => Ok(value),
            None => Err(Refused::new(span, "a function call")),
        }
    }

    /// A method of a scalar receiver that the LLVM backend computes as plain arithmetic, as
    /// it computes it: `.wrapping_add` / `_sub` / `_mul`, `.saturating_add` / `_sub` / `_mul`
    /// and `.shr(n)` on an integer, and `.is_nan()` on a float. `None`, with nothing
    /// emitted, for any other method.
    fn scalar_method(
        &mut self,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
        exit: &str,
    ) -> Lowered<Option<String>> {
        let ty = &object.ty;
        let Some(mlir) = scalar_type(ty) else {
            return Ok(None);
        };
        if method == "is_nan" && is_float(ty) && args.is_empty() {
            let value = self.expr(object, exit)?;
            return Ok(Some(
                self.assign(&format!("arith.cmpf uno, {value}, {value} : {mlir}")),
            ));
        }
        let ([arg], Some(width)) = (args, int_width(ty)) else {
            return Ok(None);
        };
        let integer_methods = [
            "wrapping_add",
            "wrapping_sub",
            "wrapping_mul",
            "saturating_add",
            "saturating_sub",
            "saturating_mul",
            "shr",
        ];
        if !integer_methods.contains(&method) || arg.ty != *ty {
            return Ok(None);
        }
        let lhs = self.expr(object, exit)?;
        let rhs = self.expr(arg, exit)?;
        let unsigned = is_unsigned(ty);
        let sign = if unsigned { "u" } else { "s" };
        let text = match method {
            "wrapping_add" => format!("arith.addi {lhs}, {rhs} : {mlir}"),
            "wrapping_sub" => format!("arith.subi {lhs}, {rhs} : {mlir}"),
            "wrapping_mul" => format!("arith.muli {lhs}, {rhs} : {mlir}"),
            "saturating_add" | "saturating_sub" => {
                let op = &method["saturating_".len()..];
                format!(
                    "llvm.call_intrinsic \"llvm.{sign}{op}.sat.{mlir}\"({lhs}, {rhs}) : ({mlir}, {mlir}) -> {mlir}"
                )
            }
            "saturating_mul" => {
                return Ok(Some(self.saturating_mul((&lhs, &rhs), unsigned, width)));
            }
            // The host's diagnostic for `.shr` points at the receiver.
            _ => {
                let op = if unsigned { "shrui" } else { "shrsi" };
                return Ok(Some(self.shift(op, (&lhs, &rhs), ty, object.span.start)));
            }
        };
        Ok(Some(self.assign(&text)))
    }

    /// `lhs.saturating_mul(rhs)`: the wrapped product unless it overflowed, and then the
    /// bound on the side the exact product lies, which the operands' signs give.
    fn saturating_mul(&mut self, (lhs, rhs): (&str, &str), unsigned: bool, width: u32) -> String {
        let mlir = format!("i{width}");
        let sign = if unsigned { "u" } else { "s" };
        let pair = format!("!llvm.struct<({mlir}, i1)>");
        let both = self.assign(&format!(
            "\"llvm.intr.{sign}mul.with.overflow\"({lhs}, {rhs}) : ({mlir}, {mlir}) -> {pair}"
        ));
        let product = self.assign(&format!("llvm.extractvalue {both}[0] : {pair}"));
        let overflowed = self.assign(&format!("llvm.extractvalue {both}[1] : {pair}"));
        let bound = if unsigned {
            self.assign(&format!("arith.constant -1 : {mlir}"))
        } else {
            let max = signed_bits((1i128 << (width - 1)) - 1, width);
            let min = signed_bits(1i128 << (width - 1), width);
            let max = self.assign(&format!("arith.constant {max} : {mlir}"));
            let min = self.assign(&format!("arith.constant {min} : {mlir}"));
            let zero = self.assign(&format!("arith.constant 0 : {mlir}"));
            let signs = self.assign(&format!("arith.xori {lhs}, {rhs} : {mlir}"));
            let negative = self.assign(&format!("arith.cmpi slt, {signs}, {zero} : {mlir}"));
            self.assign(&format!("arith.select {negative}, {min}, {max} : {mlir}"))
        };
        self.assign(&format!(
            "arith.select {overflowed}, {bound}, {product} : {mlir}"
        ))
    }

    /// `match` on an integer, `bool` or `char`, as the host tests it: each arm's
    /// alternatives in turn, then its guard, and the first arm passing both runs, with a
    /// bare binding naming the scrutinee. The checker has proven the arms exhaustive, so
    /// the fall-through past the last one is never taken.
    fn match_value(
        &mut self,
        scrutinee: &HirExpr,
        arms: &[HirMatchArm],
        expr: &HirExpr,
        exit: &str,
    ) -> Lowered<String> {
        let ty = &scrutinee.ty;
        let width = match ty {
            HirType::Bool => Some(1),
            other => int_like_width(other),
        };
        let (Some(mlir), Some(width)) = (value_type(ty), width) else {
            return Err(Refused::new(expr.span, "a `match` on a non-scalar"));
        };
        let unsigned = is_unsigned(ty) || matches!(ty, HirType::Char);
        let value = self.expr(scrutinee, exit)?;
        let result = value_type(&expr.ty).map(|ty| (self.slot(ty), ty));
        let join = self.block();
        for arm in arms {
            let mut matched = None;
            for test in &arm.tests {
                let one = self.match_test(test, &value, (mlir, width), unsigned, expr.span)?;
                matched = Some(match matched {
                    None => one,
                    Some(all) => self.assign(&format!("arith.ori {all}, {one} : i1")),
                });
            }
            let matched = match matched {
                Some(matched) => matched,
                None => self.assign("arith.constant false"),
            };
            let body = self.block();
            let next = self.block();
            self.cond_branch(&matched, &body, &next);
            self.start(&body);
            let mut bindings = Vec::with_capacity(arm.bindings.len());
            for binding in &arm.bindings {
                if !matches!(binding.source, HirBindingSource::Scrutinee) {
                    return Err(Refused::new(expr.span, "a `match` binding of a payload"));
                }
                let bound = Binding::Param {
                    value: value.clone(),
                    ty: ty.clone(),
                };
                bindings.push((binding.name.clone(), bound));
            }
            self.scopes.push(bindings);
            let walked = self.match_arm(arm, result.as_ref(), &next, exit);
            self.scopes.pop();
            walked?;
            self.branch(&join);
            self.start(&next);
        }
        self.branch(&join);
        self.start(&join);
        Ok(match result {
            Some((slot, ty)) => self.assign(&format!("memref.load {slot}[] : memref<{ty}>")),
            None => String::new(),
        })
    }

    /// One arm past its tests: the guard, which falls through to `next` when it fails, then
    /// the body, its value stored into `result` when the `match` has one.
    fn match_arm(
        &mut self,
        arm: &HirMatchArm,
        result: Option<&(String, &'static str)>,
        next: &str,
        exit: &str,
    ) -> Lowered<()> {
        if let Some(guard) = &arm.guard {
            let passed = self.expr(guard, exit)?;
            let run = self.block();
            self.cond_branch(&passed, &run, next);
            self.start(&run);
        }
        let value = self.expr(&arm.body, exit)?;
        if let Some((slot, ty)) = result
            && !self.terminated
        {
            self.line(&format!("memref.store {value}, {slot}[] : memref<{ty}>"));
        }
        Ok(())
    }

    /// One alternative of a `match` arm as an `i1`. A pattern's value is the low `width`
    /// bits of an `i64`, read against the scrutinee's own signedness for a range.
    fn match_test(
        &mut self,
        test: &HirMatchTest,
        value: &str,
        (mlir, width): (&str, u32),
        unsigned: bool,
        span: Span,
    ) -> Lowered<String> {
        let constant = |bits: i64| match width {
            1 => format!("arith.constant {}", bits & 1 != 0),
            _ => format!(
                "arith.constant {} : {mlir}",
                signed_bits(i128::from(bits), width)
            ),
        };
        Ok(match test {
            HirMatchTest::Wildcard => self.assign("arith.constant true"),
            HirMatchTest::IntEq { value: want } => {
                let want = self.assign(&constant(*want));
                self.assign(&format!("arith.cmpi eq, {value}, {want} : {mlir}"))
            }
            HirMatchTest::IntRange { lo, hi } => {
                let (at_least, at_most) = if unsigned {
                    ("uge", "ule")
                } else {
                    ("sge", "sle")
                };
                let lo = self.assign(&constant(*lo));
                let hi = self.assign(&constant(*hi));
                let above = self.assign(&format!("arith.cmpi {at_least}, {value}, {lo} : {mlir}"));
                let below = self.assign(&format!("arith.cmpi {at_most}, {value}, {hi} : {mlir}"));
                self.assign(&format!("arith.andi {above}, {below} : i1"))
            }
            HirMatchTest::Tag { .. } => {
                return Err(Refused::new(span, "a `match` on an enum"));
            }
        })
    }

    fn literal(&mut self, literal: &Literal, ty: &HirType, span: Span) -> Lowered<String> {
        let text = match (literal, ty) {
            (Literal::Boolean(value), HirType::Bool) => format!("arith.constant {value}"),
            (Literal::Char(value), HirType::Char) => {
                format!("arith.constant {} : i32", u32::from(*value))
            }
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
        let Some(mlir) = value_type(operand_ty) else {
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
        let name = match op {
            BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::Modulo => {
                return Ok(self.checked_int(op, (&lhs, &rhs), operand_ty, expr.span.start));
            }
            BinaryOp::BitAnd => "andi",
            BinaryOp::BitOr => "ori",
            BinaryOp::BitXor => "xori",
            BinaryOp::Shl => {
                return Ok(self.shift("shli", (&lhs, &rhs), operand_ty, expr.span.start));
            }
            _ => return Err(Refused::new(expr.span, "this operator on an integer")),
        };
        Ok(self.assign(&format!("arith.{name} {lhs}, {rhs} : {mlir}")))
    }

    /// Integer `op` as the host computes it, with its checks: an overflow on the debug tier
    /// through LLVM's `with.overflow` intrinsics, a zero divisor on every tier, `MIN / -1` on
    /// the debug tier. A divisor that failed is replaced by 1, so nothing undefined runs
    /// before the thread stops; on the release tier that is also `MIN / -1`'s wrap.
    fn checked_int(
        &mut self,
        op: BinaryOp,
        (lhs, rhs): (&str, &str),
        ty: &HirType,
        offset: usize,
    ) -> String {
        let overflow = self.overflow;
        let mlir = scalar_type(ty).unwrap_or("i64");
        let unsigned = is_unsigned(ty);
        let intrinsic = match op {
            BinaryOp::Add => "add",
            BinaryOp::Subtract => "sub",
            BinaryOp::Multiply => "mul",
            _ => return self.checked_div_rem(op, (lhs, rhs), ty, offset),
        };
        if overflow == Overflow::Wrapping {
            return self.assign(&format!("arith.{intrinsic}i {lhs}, {rhs} : {mlir}"));
        }
        let sign = if unsigned { "u" } else { "s" };
        let pair = format!("!llvm.struct<({mlir}, i1)>");
        let both = self.assign(&format!(
            "\"llvm.intr.{sign}{intrinsic}.with.overflow\"({lhs}, {rhs}) : ({mlir}, {mlir}) -> {pair}"
        ));
        let value = self.assign(&format!("llvm.extractvalue {both}[0] : {pair}"));
        let overflowed = self.assign(&format!("llvm.extractvalue {both}[1] : {pair}"));
        self.fail_if(&overflowed, GuardKind::Overflow, offset);
        value
    }

    /// `lhs << rhs`, or `.shr` when `op` is `shrsi` / `shrui`, as the host shifts: on the
    /// debug tier an amount of at least the operand's width stops the thread, and from `-O1`
    /// up the amount is taken modulo the width.
    fn shift(&mut self, op: &str, (lhs, rhs): (&str, &str), ty: &HirType, offset: usize) -> String {
        let mlir = scalar_type(ty).unwrap_or("i64");
        let width = int_width(ty).unwrap_or(64);
        let amount = if self.overflow == Overflow::Wrapping {
            let mask = self.assign(&format!("arith.constant {} : {mlir}", width - 1));
            self.assign(&format!("arith.andi {rhs}, {mask} : {mlir}"))
        } else {
            let limit = self.assign(&format!("arith.constant {width} : {mlir}"));
            let past = self.assign(&format!("arith.cmpi uge, {rhs}, {limit} : {mlir}"));
            self.fail_if(&past, GuardKind::ShiftOverflow, offset);
            rhs.to_string()
        };
        self.assign(&format!("arith.{op} {lhs}, {amount} : {mlir}"))
    }

    fn checked_div_rem(
        &mut self,
        op: BinaryOp,
        (lhs, rhs): (&str, &str),
        ty: &HirType,
        offset: usize,
    ) -> String {
        let overflow = self.overflow;
        let mlir = scalar_type(ty).unwrap_or("i64");
        let remainder = matches!(op, BinaryOp::Modulo);
        let zero = self.assign(&format!("arith.constant 0 : {mlir}"));
        let one = self.assign(&format!("arith.constant 1 : {mlir}"));
        let by_zero = self.assign(&format!("arith.cmpi eq, {rhs}, {zero} : {mlir}"));
        let kind = match remainder {
            true => GuardKind::RemainderByZero,
            false => GuardKind::DivisionByZero,
        };
        self.fail_if(&by_zero, kind, offset);
        let divisor = self.assign(&format!("arith.select {by_zero}, {one}, {rhs} : {mlir}"));
        if is_unsigned(ty) {
            let name = if remainder { "remui" } else { "divui" };
            return self.assign(&format!("arith.{name} {lhs}, {divisor} : {mlir}"));
        }
        let width = int_width(ty).unwrap_or(64);
        let min = self.assign(&format!(
            "arith.constant {} : {mlir}",
            signed_bits(1i128 << (width - 1), width)
        ));
        let minus_one = self.assign(&format!("arith.constant -1 : {mlir}"));
        let lhs_min = self.assign(&format!("arith.cmpi eq, {lhs}, {min} : {mlir}"));
        let rhs_minus_one = self.assign(&format!("arith.cmpi eq, {divisor}, {minus_one} : {mlir}"));
        let overflows = self.assign(&format!("arith.andi {lhs_min}, {rhs_minus_one} : i1"));
        if overflow == Overflow::Checked {
            self.fail_if(&overflows, GuardKind::Overflow, offset);
        }
        let safe = self.assign(&format!(
            "arith.select {overflows}, {one}, {divisor} : {mlir}"
        ));
        let name = if remainder { "remsi" } else { "divsi" };
        self.assign(&format!("arith.{name} {lhs}, {safe} : {mlir}"))
    }

    /// Stop the thread when `failed` holds: a `@kernel`'s whole kernel, or, for an outlined
    /// body, after recording the check at `offset` and lowering [`STATUS`] to its key (an
    /// atomic unsigned min, see `crate::guards`: the element's position above the check's
    /// number, so the earliest element any thread failed at is the one reported).
    fn fail_if(&mut self, failed: &str, kind: GuardKind, offset: usize) {
        if self.failure == Failure::Stop {
            let message = match kind {
                GuardKind::Overflow => OVERFLOW,
                GuardKind::DivisionByZero => DIVIDE_BY_ZERO,
                GuardKind::RemainderByZero => REMAINDER_BY_ZERO,
                GuardKind::ShiftOverflow => SHIFT_OVERFLOW,
                GuardKind::RangeStep => RANGE_STEP,
            };
            let all = self.assign("arith.constant true");
            let passed = self.assign(&format!("arith.xori {failed}, {all} : i1"));
            self.guard(&passed, message);
            return;
        }
        self.sites.push(Guard { kind, offset });
        let number = self.sites.len();
        let stop = self.stop.clone().unwrap_or_default();
        let report = self.block();
        let ok = self.block();
        self.cond_branch(failed, &report, &ok);
        self.start(&report);
        let word = format!("i{STATUS_BITS}");
        let mut key = self.assign(&format!("arith.constant {number} : {word}"));
        if let Some(element) = self.element.clone() {
            let shift = self.assign(&format!("arith.constant {SITE_BITS} : {word}"));
            let shifted = self.assign(&format!("arith.shli {element}, {shift} : {word}"));
            key = self.assign(&format!("arith.ori {shifted}, {key} : {word}"));
        }
        let first = self.assign("arith.constant 0 : index");
        self.assign(&format!(
            "memref.atomic_rmw minu {key}, {STATUS}[{first}] : ({word}, memref<1x{word}>) -> {word}"
        ));
        self.branch(&stop);
        self.start(&ok);
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
        // A negated integer literal is the constant it denotes, as on the host: `-128i8` is
        // written as a magnitude past `i8::MAX`, and `0 - MIN` would overflow.
        if let (UnaryOp::Negate, HirExprKind::Literal(Literal::Integer(magnitude, suffix))) =
            (op, &operand.kind)
            && int_width(&operand.ty).is_some()
        {
            let negated = Literal::Integer(magnitude.wrapping_neg(), *suffix);
            return self.literal(&negated, &operand.ty, expr.span);
        }
        let value = self.expr(operand, exit)?;
        let text = match (op, &operand.ty) {
            (UnaryOp::Negate, ty) if is_float(ty) => format!("arith.negf {value} : {mlir}"),
            (UnaryOp::Negate, ty) if int_width(ty).is_some() => {
                let zero = self.assign(&format!("arith.constant 0 : {mlir}"));
                // `-x` is `0 - x` on the host, so it fails where that subtraction does.
                return Ok(self.checked_int(
                    BinaryOp::Subtract,
                    (&zero, &value),
                    ty,
                    expr.span.start,
                ));
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

    /// A math function on one float: the `math` dialect op of its name, which the GPU
    /// conversion turns into a call into the vendor's device math library, `sign`'s two
    /// compares, as the LLVM backend writes it, or `abs`'s cleared sign bit, which is
    /// `fabs` exactly.
    fn math(
        &mut self,
        op: HirMathOp,
        operand: &HirExpr,
        exponent: Option<&HirExpr>,
        expr: &HirExpr,
        exit: &str,
    ) -> Lowered<String> {
        let Some(mlir) = scalar_type(&expr.ty).filter(|_| is_float(&expr.ty)) else {
            return Err(Refused::new(expr.span, "a math function on a non-float"));
        };
        // `sign` is two compares and `abs` clears the sign bit, so neither needs the device
        // library.
        if !self.math && !matches!(op, HirMathOp::Sign | HirMathOp::Abs) {
            return Err(Refused::new(expr.span, NO_DEVICE_MATH));
        }
        let value = self.expr(operand, exit)?;
        let name = match op {
            HirMathOp::Exp => "exp",
            HirMathOp::Log => "log",
            HirMathOp::Sqrt => "sqrt",
            HirMathOp::Tanh => "tanh",
            HirMathOp::Abs => {
                let (bits, mask) = match mlir {
                    "f32" => ("i32", i32::MAX as i64),
                    _ => ("i64", i64::MAX),
                };
                let raw = self.assign(&format!("arith.bitcast {value} : {mlir} to {bits}"));
                let mask = self.assign(&format!("arith.constant {mask} : {bits}"));
                let magnitude = self.assign(&format!("arith.andi {raw}, {mask} : {bits}"));
                return Ok(self.assign(&format!("arith.bitcast {magnitude} : {bits} to {mlir}")));
            }
            HirMathOp::Pow => {
                let Some(exponent) = exponent else {
                    return Err(Refused::new(expr.span, "a `.pow` without an exponent"));
                };
                let power = self.expr(exponent, exit)?;
                return Ok(self.assign(&format!("math.powf {value}, {power} : {mlir}")));
            }
            HirMathOp::Sign => {
                let zero = self.assign(&format!("arith.constant 0.0 : {mlir}"));
                let one = self.assign(&format!("arith.constant 1.0 : {mlir}"));
                let minus_one = self.assign(&format!("arith.constant -1.0 : {mlir}"));
                let above = self.assign(&format!("arith.cmpf ogt, {value}, {zero} : {mlir}"));
                let below = self.assign(&format!("arith.cmpf olt, {value}, {zero} : {mlir}"));
                let negative = self.assign(&format!(
                    "arith.select {below}, {minus_one}, {value} : {mlir}"
                ));
                return Ok(
                    self.assign(&format!("arith.select {above}, {one}, {negative} : {mlir}"))
                );
            }
        };
        Ok(self.assign(&format!("math.{name} {value} : {mlir}")))
    }

    fn cast(&mut self, value: &HirExpr, target: &HirType, exit: &str) -> Lowered<String> {
        let (Some(from), Some(to)) = (value_type(&value.ty), value_type(target)) else {
            return Err(Refused::new(value.span, "a cast between non-scalars"));
        };
        let source = self.expr(value, exit)?;
        let (from_ty, from_width, to_width) =
            (&value.ty, int_like_width(&value.ty), int_like_width(target));
        let text = match (from_width, to_width) {
            _ if from == to => return Ok(source),
            // A `char` widens as the unsigned code point it is.
            (Some(a), Some(b)) if a < b => {
                let extend = if is_unsigned(from_ty) || matches!(from_ty, HirType::Char) {
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
    let unsigned = is_unsigned(ty) || matches!(ty, HirType::Bool | HirType::Char);
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
        HirStmt::ForRange { .. } => "an enumerated range loop",
        HirStmt::ForEach { .. } => "a loop over a collection",
        HirStmt::Return { .. } => "a `return` with a value",
        HirStmt::ValElse { .. } => "a `val ... else`",
        HirStmt::Const { .. } => "a local constant",
        _ => "this statement",
    }
}

fn expression_kind(kind: &HirExprKind) -> &'static str {
    match kind {
        HirExprKind::Closure { .. } => "a closure",
        HirExprKind::TensorIndex { .. } | HirExprKind::Index { .. } => "a tensor slice",
        HirExprKind::Loop { .. } => "a `loop` with a value of this type",
        HirExprKind::Pool { .. } => "a `pool` block",
        _ => "this expression",
    }
}
