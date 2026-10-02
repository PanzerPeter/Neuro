// Outlining the tensor operations a GPU can run into functions that run where their
// operands live: an operation executes on the device its tensors are on.
//
// Nothing in a tensor's type says where it lives, so the choice is made per call at run
// time, and a backend can only make it at a call: it needs a body to hand the GPU and a
// body to run on the host, with the operands evaluated once in between. Each operation is
// therefore lifted into a `HirTarget::FollowsOperands` function whose parameters are the
// operation's operands exactly as written, owned or borrowed, so the call moves and borrows
// what the operation did. The operand expressions keep their spans in the body, so a host
// body refusing a device tensor still reports the operation's own position.
//
// The pass runs only in a program that moves a tensor to a device, since that is the one
// way a device tensor comes to exist; any other program is lowered exactly as before.

use ast_types::{BinaryOp, UnaryOp};
use neuro_hir::{
    AxisNames, HirCapture, HirExpr, HirExprKind, HirFunction, HirInterpPart, HirItem, HirParam,
    HirPlace, HirStmt, HirTarget, HirTensorApply, HirTensorAxis, HirType,
};
use shared_types::{Literal, Span};

/// The outlined functions are named with this and a counter. The checker forbids the `__`
/// prefix in user names, as it does for lifted closures (`__closure_N`).
const OUTLINED_PREFIX: &str = "__device_op_";

/// An outlined function's parameters are named with this and their position. The `__`
/// prefix keeps them apart from a closure capture, which is passed under its own name.
const OPERAND_PREFIX: &str = "__operand";

/// The local a slice position is bound to while the call site checks it.
const POSITION_LOCAL: &str = "__position";

/// What an out-of-range slice position reports: the host's own words for it, so a device
/// slice and a host one fail alike.
const INDEX_OUT_OF_BOUNDS: &str = "tensor index out of bounds";

/// Outline every device-capable tensor operation in the program's host code into a
/// `FollowsOperands` function, appended to `items`.
pub(crate) fn outline(items: &mut Vec<HirItem>) {
    let mut outliner = Outliner::default();
    for item in items.iter_mut() {
        match item {
            // A `@gpu` or `@kernel` body is device code already.
            HirItem::Function(function) if function.target == HirTarget::Host => {
                outliner.rewrite_stmts(&mut function.body)
            }
            HirItem::Impl(def) => {
                for method in &mut def.methods {
                    outliner.rewrite_stmts(&mut method.body);
                }
            }
            HirItem::Closure(closure) => outliner.rewrite_stmts(&mut closure.body),
            _ => {}
        }
    }
    items.extend(outliner.functions.into_iter().map(HirItem::Function));
}

#[derive(Default)]
struct Outliner {
    functions: Vec<HirFunction>,
}

/// Whether a GPU body can compute over a tensor of type `ty`: a numeric element (an integer
/// one keeps the host's checks there, reported back through the call), and a static shape of
/// rank 1 or more, since an operation over rank 0 has no parallel axis to launch across.
fn device_tensor(ty: &HirType) -> bool {
    matches!(
        ty.referent(),
        HirType::Tensor { element, shape, .. }
            if numeric(element)
                && !shape.is_empty()
                && shape.iter().all(Option::is_some)
    )
}

/// An element a GPU body computes with: `f16` / `bf16` carry no arithmetic in the HIR.
fn numeric(ty: &HirType) -> bool {
    matches!(
        ty,
        HirType::F32
            | HirType::F64
            | HirType::I8
            | HirType::I16
            | HirType::I32
            | HirType::I64
            | HirType::U8
            | HirType::U16
            | HirType::U32
            | HirType::U64
    )
}

/// A node of an outlinable operator tree: an operation producing a device-capable tensor
/// out of tensors the same function can compute or read. Element-wise arithmetic, `@` and a
/// permuting shape cast take their operands as written; elementwise math, a slice and an
/// `einsum` only read theirs, so each must be another node or a tensor that can be lent.
fn operator_node(expr: &HirExpr) -> bool {
    if !device_tensor(&expr.ty) {
        return false;
    }
    match &expr.kind {
        HirExprKind::Binary { op, .. } => matches!(
            op,
            BinaryOp::Add
                | BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::MatMul
        ),
        HirExprKind::TensorShapeCast {
            permutation: Some(_),
            ..
        } => true,
        HirExprKind::Math { operand, .. } => readable(operand),
        HirExprKind::TensorIndex { object, .. } => readable(object),
        HirExprKind::TensorEinsum { operands, .. } => operands.iter().all(readable),
        _ => false,
    }
}

/// Whether a node that only reads `operand` can take it: as a node of the same tree, or
/// as a device-capable tensor it is lent.
fn readable(operand: &HirExpr) -> bool {
    operator_node(operand) || (device_tensor(&operand.ty) && reducible(operand))
}

/// Whether a reduction or a sort may be outlined over `receiver`: a borrow (lent on as it
/// is), a named tensor (lent with `&`), or a temporary (moved in, since nothing else owns
/// it). A tensor inside another place, a field say, can be neither lent, since a backend
/// borrows only a binding, nor moved out of its owner, so its operation stays inline.
fn reducible(receiver: &HirExpr) -> bool {
    matches!(receiver.ty, HirType::Reference { .. })
        || matches!(receiver.kind, HirExprKind::Variable(_))
        || !is_place(receiver)
}

fn is_place(expr: &HirExpr) -> bool {
    matches!(
        expr.kind,
        HirExprKind::Variable(_)
            | HirExprKind::FieldAccess { .. }
            | HirExprKind::TupleIndex { .. }
            | HirExprKind::NewtypeAccess { .. }
            | HirExprKind::Index { .. }
            | HirExprKind::Deref { .. }
    )
}

/// Whether every capture of a closure is a scalar a device body can be handed, so a
/// traversal calling it may run on a GPU.
fn scalar_captures(captures: &[HirCapture]) -> bool {
    captures
        .iter()
        .all(|capture| numeric(&capture.ty) || capture.ty == HirType::Bool)
}

impl Outliner {
    /// Replace `expr` with a call to an outlined function when it is a device-capable
    /// operation, and report whether it did. Its operands are rewritten first, so an
    /// operation nested in one (a reduction's receiver) is outlined on its own.
    fn outline_operation(&mut self, expr: &mut HirExpr) -> bool {
        if operator_node(expr) {
            let mut operands = Vec::new();
            let body = self.take_tree(placeholder(expr), &mut operands);
            *expr = self.call(body, operands, &[], expr.span);
            return true;
        }
        match &mut expr.kind {
            // A sort's result is a fresh tensor (or `.topk`'s pair of them) of the receiver's
            // rank, so the receiver alone decides whether it can run on a device.
            HirExprKind::TensorSort { receiver, .. } => {
                if !device_tensor(&receiver.ty) || !reducible(receiver) {
                    return false;
                }
                self.rewrite(receiver);
                let span = expr.span;
                let mut sort = placeholder(expr);
                let HirExprKind::TensorSort { receiver, .. } = &mut sort.kind else {
                    return false;
                };
                let operand = lend_out(receiver, 0, span);
                *expr = self.call(sort, vec![operand], &[], span);
                true
            }
            HirExprKind::TensorReduce { receiver, axis, .. } => {
                if !device_tensor(&receiver.ty)
                    || !reducible(receiver)
                    || (axis.is_some() && !device_tensor(&expr.ty))
                {
                    return false;
                }
                self.rewrite(receiver);
                let whole = axis.is_none();
                let span = expr.span;
                let mut reduce = placeholder(expr);
                let HirExprKind::TensorReduce { receiver, .. } = &mut reduce.kind else {
                    return false;
                };
                let operand = lend_out(receiver, 0, span);
                *expr = match whole {
                    true => self.boxed_call(reduce, vec![operand], &[], span),
                    false => self.call(reduce, vec![operand], &[], span),
                };
                true
            }
            // A full contraction (`"ii->"`) yields a scalar; one with output letters is a
            // tensor, and so a node of an operator tree, taken above.
            HirExprKind::TensorEinsum { operands, .. } => {
                if device_tensor(&expr.ty) || !numeric(&expr.ty) || !operands.iter().all(readable) {
                    return false;
                }
                let span = expr.span;
                let mut einsum = placeholder(expr);
                let mut taken = Vec::new();
                if let HirExprKind::TensorEinsum { operands, .. } = &mut einsum.kind {
                    for operand in operands.iter_mut() {
                        *operand =
                            self.take_read(std::mem::replace(operand, unit(span)), &mut taken);
                    }
                }
                *expr = self.boxed_call(einsum, taken, &[], span);
                true
            }
            HirExprKind::TensorApply { .. } => self.outline_apply(expr),
            _ => false,
        }
    }

    /// Outline `.map(f)`, `.zip(other, f)` or `.reduce(init, f)` over a device-capable
    /// receiver whose function is a closure literal (a function passed by name is lowered to
    /// one forwarding to it). The closure stays in the body, so a GPU body knows what it
    /// calls; its captures become parameters under their own names, which is where the
    /// closure literal loads them from. A function passed through a local is a value only the
    /// run time knows, so that traversal stays inline.
    fn outline_apply(&mut self, expr: &mut HirExpr) -> bool {
        let HirExprKind::TensorApply {
            kind,
            receiver,
            operand,
            callee,
        } = &mut expr.kind
        else {
            return false;
        };
        let HirExprKind::Closure { captures, .. } = &callee.kind else {
            return false;
        };
        let admitted = device_tensor(&receiver.ty)
            && reducible(receiver)
            && scalar_captures(captures)
            && match (*kind, operand.as_deref()) {
                (HirTensorApply::Map, None) => device_tensor(&expr.ty),
                (HirTensorApply::Zip, Some(other)) => {
                    device_tensor(&expr.ty) && device_tensor(&other.ty) && reducible(other)
                }
                (HirTensorApply::Reduce, Some(_)) => numeric(&expr.ty),
                _ => false,
            };
        if !admitted {
            return false;
        }
        let captures = captures.clone();
        let reduce = *kind == HirTensorApply::Reduce;
        self.rewrite(receiver);
        if let Some(operand) = operand {
            self.rewrite(operand);
        }
        let span = expr.span;
        let mut apply = placeholder(expr);
        let HirExprKind::TensorApply {
            receiver, operand, ..
        } = &mut apply.kind
        else {
            return false;
        };
        let mut operands = vec![lend_out(receiver, 0, span)];
        if let Some(operand) = operand {
            // A `.zip`'s second tensor is read like the receiver; a `.reduce`'s seed is a
            // scalar, passed as it is.
            let taken = std::mem::replace(operand.as_mut(), unit(span));
            let taken = if reduce { taken } else { lend(taken) };
            **operand = parameter(1, &taken);
            operands.push(taken);
        }
        *expr = match reduce {
            true => self.boxed_call(apply, operands, &captures, span),
            false => self.call(apply, operands, &captures, span),
        };
        true
    }

    /// `place OP= value` on a device-capable tensor as a call to a function that updates
    /// its target through a `&mut` parameter, so the buffer the place addresses is the one
    /// written, wherever it lives. `None` for a place a backend cannot borrow mutably (a
    /// field, an element) or a value no GPU body takes.
    fn outline_compound(
        &mut self,
        place: &HirPlace,
        op: BinaryOp,
        value: &mut HirExpr,
        ty: &HirType,
        span: Span,
    ) -> Option<HirExpr> {
        let admitted = device_tensor(ty)
            && matches!(
                op,
                BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide
            )
            && (device_tensor(&value.ty) || value.ty == *element(ty)?);
        if !admitted {
            return None;
        }
        let target = mutable_borrow(place, ty, span)?;
        // The value is evaluated before the target is touched, as the statement orders it.
        let value = std::mem::replace(value, unit(span));
        let body = HirStmt::TensorCompoundAssign {
            place: HirPlace::Deref {
                pointer: Box::new(parameter(1, &target)),
                ty: ty.clone(),
            },
            op,
            value: parameter(0, &value),
            ty: ty.clone(),
            span,
        };
        Some(self.define(vec![body], HirType::Void, vec![value, target], &[], span))
    }

    /// The operator tree rooted at `expr` with each operand replaced by the parameter it
    /// becomes, pushing the operands in evaluation order.
    fn take_tree(&mut self, mut expr: HirExpr, operands: &mut Vec<HirExpr>) -> HirExpr {
        if !operator_node(&expr) {
            self.rewrite(&mut expr);
            let variable = parameter(operands.len(), &expr);
            operands.push(expr);
            return variable;
        }
        let span = expr.span;
        match &mut expr.kind {
            HirExprKind::Binary { left, right, .. } => {
                **left = self.take_tree(std::mem::replace(left, unit(span)), operands);
                **right = self.take_tree(std::mem::replace(right, unit(span)), operands);
            }
            HirExprKind::TensorShapeCast { receiver, .. } => {
                **receiver = self.take_tree(std::mem::replace(receiver, unit(span)), operands);
            }
            HirExprKind::Math {
                operand, exponent, ..
            } => {
                **operand = self.take_read(std::mem::replace(operand, unit(span)), operands);
                if let Some(exponent) = exponent {
                    **exponent = self.take_tree(std::mem::replace(exponent, unit(span)), operands);
                }
            }
            HirExprKind::TensorIndex { object, axes } => {
                let extents = match object.ty.referent() {
                    HirType::Tensor { shape, .. } => shape.clone(),
                    _ => Vec::new(),
                };
                **object = self.take_read(std::mem::replace(object, unit(span)), operands);
                for (axis, extent) in axes.iter_mut().zip(extents) {
                    if let HirTensorAxis::Position(position) = axis {
                        self.rewrite(position);
                        let checked =
                            checked_position(std::mem::replace(position, unit(span)), extent, span);
                        *position = parameter(operands.len(), &checked);
                        operands.push(checked);
                    }
                }
            }
            HirExprKind::TensorEinsum {
                operands: inputs, ..
            } => {
                for input in inputs.iter_mut() {
                    *input = self.take_read(std::mem::replace(input, unit(span)), operands);
                }
            }
            _ => {}
        }
        expr
    }

    /// An operand a node only reads: another node of the tree, or a leaf lent to the call.
    fn take_read(&mut self, expr: HirExpr, operands: &mut Vec<HirExpr>) -> HirExpr {
        if operator_node(&expr) {
            return self.take_tree(expr, operands);
        }
        let mut expr = expr;
        self.rewrite(&mut expr);
        let operand = lend(expr);
        let variable = parameter(operands.len(), &operand);
        operands.push(operand);
        variable
    }

    /// Define a `FollowsOperands` function computing `body` over `operands` and `captures`,
    /// and return the call to it.
    fn call(
        &mut self,
        body: HirExpr,
        operands: Vec<HirExpr>,
        captures: &[HirCapture],
        span: Span,
    ) -> HirExpr {
        let ret = body.ty.clone();
        self.define(vec![HirStmt::Expr(body)], ret, operands, captures, span)
    }

    /// [`Self::call`] for a body yielding a scalar, which a GPU body cannot return: its
    /// result is a buffer the caller allocates. So the body leaves it in a one-element
    /// tensor, and the call site reads that element back.
    fn boxed_call(
        &mut self,
        body: HirExpr,
        operands: Vec<HirExpr>,
        captures: &[HirCapture],
        span: Span,
    ) -> HirExpr {
        let element = body.ty.clone();
        let boxed = HirExpr::new(
            HirExprKind::TensorLiteral {
                elements: vec![body],
            },
            HirType::Tensor {
                element: Box::new(element.clone()),
                shape: vec![Some(1)],
                names: AxisNames::default(),
            },
            span,
        );
        let call = self.call(boxed, operands, captures, span);
        let first = HirExpr::new(
            HirExprKind::Literal(Literal::Integer(0, None)),
            HirType::U64,
            span,
        );
        HirExpr::new(
            HirExprKind::TensorIndex {
                object: Box::new(call),
                axes: vec![HirTensorAxis::Position(first)],
            },
            element,
            span,
        )
    }

    fn define(
        &mut self,
        body: Vec<HirStmt>,
        ret: HirType,
        operands: Vec<HirExpr>,
        captures: &[HirCapture],
        span: Span,
    ) -> HirExpr {
        let name = format!("{OUTLINED_PREFIX}{}", self.functions.len());
        let mut params: Vec<HirParam> = operands
            .iter()
            .enumerate()
            .map(|(index, operand)| HirParam {
                name: parameter_name(index),
                ty: operand.ty.clone(),
                span: operand.span,
            })
            .collect();
        let mut args = operands;
        for capture in captures {
            params.push(HirParam {
                name: capture.name.clone(),
                ty: capture.ty.clone(),
                span,
            });
            args.push(HirExpr::new(
                HirExprKind::Variable(capture.name.clone()),
                capture.ty.clone(),
                span,
            ));
        }
        let callee = HirExpr::new(
            HirExprKind::Variable(name.clone()),
            HirType::Function {
                params: params.iter().map(|param| param.ty.clone()).collect(),
                ret: Box::new(ret.clone()),
            },
            span,
        );
        self.functions.push(HirFunction {
            name,
            params,
            return_type: ret.clone(),
            body,
            target: HirTarget::FollowsOperands,
            span,
        });
        HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(callee),
                args,
            },
            ret,
            span,
        )
    }

    fn rewrite_stmts(&mut self, stmts: &mut [HirStmt]) {
        for stmt in stmts {
            self.rewrite_stmt(stmt);
        }
    }

    fn rewrite_stmt(&mut self, stmt: &mut HirStmt) {
        match stmt {
            HirStmt::VarDecl { init, .. } => self.rewrite_optional(init.as_mut()),
            HirStmt::Assign { place, value, .. } => {
                self.rewrite(value);
                self.rewrite_place(place);
            }
            HirStmt::TensorCompoundAssign {
                place,
                op,
                value,
                ty,
                span,
            } => {
                self.rewrite(value);
                self.rewrite_place(place);
                if let Some(call) = self.outline_compound(place, *op, value, ty, *span) {
                    *stmt = HirStmt::Expr(call);
                }
            }
            HirStmt::Return { value, .. } | HirStmt::Break { value, .. } => {
                self.rewrite_optional(value.as_mut())
            }
            HirStmt::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => self.rewrite_if(condition, then_block, else_if_blocks, else_block),
            HirStmt::While {
                condition, body, ..
            } => {
                self.rewrite(condition);
                self.rewrite_stmts(body);
            }
            HirStmt::ForRange {
                start,
                end,
                step,
                body,
                ..
            } => {
                self.rewrite(start);
                self.rewrite(end);
                if let Some(step) = step {
                    self.rewrite(step);
                }
                self.rewrite_stmts(body);
            }
            HirStmt::ForEach { iterable, body, .. } => {
                self.rewrite(iterable);
                self.rewrite_stmts(body);
            }
            HirStmt::ValElse {
                scrutinee,
                else_block,
                ..
            } => {
                self.rewrite(scrutinee);
                self.rewrite_stmts(else_block);
            }
            HirStmt::Const { value, .. } | HirStmt::Expr(value) => self.rewrite(value),
            HirStmt::Continue { .. } => {}
        }
    }

    fn rewrite_place(&mut self, place: &mut HirPlace) {
        match place {
            HirPlace::Var { .. } => {}
            HirPlace::Field { object, .. } => self.rewrite(object),
            HirPlace::Index { object, index, .. } => {
                self.rewrite(object);
                self.rewrite(index);
            }
            HirPlace::TensorIndex { object, axes, .. } => {
                self.rewrite(object);
                self.rewrite_axes(axes);
            }
            HirPlace::Deref { pointer, .. } => self.rewrite(pointer),
        }
    }

    fn rewrite_axes(&mut self, axes: &mut [HirTensorAxis]) {
        for axis in axes {
            if let HirTensorAxis::Position(position) = axis {
                self.rewrite(position);
            }
        }
    }

    fn rewrite_optional(&mut self, expr: Option<&mut HirExpr>) {
        if let Some(expr) = expr {
            self.rewrite(expr);
        }
    }

    fn rewrite_if(
        &mut self,
        condition: &mut HirExpr,
        then_block: &mut [HirStmt],
        else_if_blocks: &mut [(HirExpr, Vec<HirStmt>)],
        else_block: &mut Option<Vec<HirStmt>>,
    ) {
        self.rewrite(condition);
        self.rewrite_stmts(then_block);
        for (condition, block) in else_if_blocks {
            self.rewrite(condition);
            self.rewrite_stmts(block);
        }
        if let Some(block) = else_block {
            self.rewrite_stmts(block);
        }
    }

    /// Outline `expr` if it is an operation a GPU can run, and otherwise every such
    /// operation under it.
    fn rewrite(&mut self, expr: &mut HirExpr) {
        if self.outline_operation(expr) {
            return;
        }
        match &mut expr.kind {
            HirExprKind::Literal(_)
            | HirExprKind::Variable(_)
            | HirExprKind::Path { .. }
            | HirExprKind::GridPosition { .. }
            | HirExprKind::TensorIdentity
            | HirExprKind::CollectionNew
            | HirExprKind::Closure { .. } => {}
            HirExprKind::Binary { left, right, .. } => {
                self.rewrite(left);
                self.rewrite(right);
            }
            HirExprKind::Unary { operand, .. }
            | HirExprKind::Reference { operand, .. }
            | HirExprKind::Deref { operand }
            | HirExprKind::FieldAccess {
                object: operand, ..
            }
            | HirExprKind::TupleIndex {
                object: operand, ..
            }
            | HirExprKind::NewtypeAccess { object: operand }
            | HirExprKind::Cast { value: operand }
            | HirExprKind::DynCoerce { value: operand }
            | HirExprKind::SliceCoerce { value: operand }
            | HirExprKind::NewtypeConstruct { value: operand, .. }
            | HirExprKind::TensorFill { value: operand }
            | HirExprKind::ArrayRest { array: operand, .. }
            | HirExprKind::TensorShapeCast {
                receiver: operand, ..
            }
            | HirExprKind::TensorDetach { receiver: operand }
            | HirExprKind::TensorReduce {
                receiver: operand, ..
            }
            | HirExprKind::TensorSort {
                receiver: operand, ..
            } => self.rewrite(operand),
            HirExprKind::Call { callee, args } => {
                self.rewrite(callee);
                self.rewrite_all(args);
            }
            HirExprKind::StructLiteral { fields, base, .. } => {
                for field in fields {
                    self.rewrite(&mut field.value);
                }
                self.rewrite_optional(base.as_deref_mut());
            }
            HirExprKind::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
            } => self.rewrite_if(condition, then_block, else_if_blocks, else_block),
            HirExprKind::Block { stmts }
            | HirExprKind::Unsafe { stmts }
            | HirExprKind::Pool { stmts, .. }
            | HirExprKind::Loop { body: stmts, .. }
            | HirExprKind::KernelPartition { body: stmts, .. } => self.rewrite_stmts(stmts),
            HirExprKind::Range { start, end, .. } => {
                self.rewrite(start);
                self.rewrite(end);
            }
            HirExprKind::ArrayLiteral { elements }
            | HirExprKind::TensorLiteral { elements }
            | HirExprKind::TupleLiteral { elements }
            | HirExprKind::EnumConstruct {
                payload: elements, ..
            }
            | HirExprKind::TensorEinsum {
                operands: elements, ..
            } => self.rewrite_all(elements),
            HirExprKind::TensorRandomNormal { mean, std } => {
                self.rewrite(mean);
                self.rewrite(std);
            }
            HirExprKind::Index { object, index } => {
                self.rewrite(object);
                self.rewrite(index);
            }
            HirExprKind::TensorIndex { object, axes } => {
                self.rewrite(object);
                self.rewrite_axes(axes);
            }
            HirExprKind::TensorApply {
                receiver,
                operand,
                callee,
                ..
            } => {
                self.rewrite(receiver);
                self.rewrite_optional(operand.as_deref_mut());
                self.rewrite(callee);
            }
            HirExprKind::Math {
                operand, exponent, ..
            } => {
                self.rewrite(operand);
                self.rewrite_optional(exponent.as_deref_mut());
            }
            HirExprKind::Match { scrutinee, arms } => {
                self.rewrite(scrutinee);
                for arm in arms {
                    self.rewrite_optional(arm.guard.as_mut());
                    self.rewrite(&mut arm.body);
                }
            }
            HirExprKind::InterpString { parts } => {
                for part in parts {
                    if let HirInterpPart::Formatted { expr, .. } = part {
                        self.rewrite(expr);
                    }
                }
            }
        }
    }

    fn rewrite_all(&mut self, exprs: &mut [HirExpr]) {
        for expr in exprs {
            self.rewrite(expr);
        }
    }
}

fn parameter_name(index: usize) -> String {
    format!("{OPERAND_PREFIX}{index}")
}

fn element(ty: &HirType) -> Option<&HirType> {
    match ty.referent() {
        HirType::Tensor { element, .. } => Some(element),
        _ => None,
    }
}

/// The argument a reduction or a sort passes for its receiver. Both read the receiver and
/// leave it alive, so a named tensor is lent to the call rather than moved into it. A
/// temporary has no owner to outlive it, and a borrow is lent on as it is.
fn lend(operand: HirExpr) -> HirExpr {
    match &operand.ty {
        HirType::Tensor { .. } if is_place(&operand) => HirExpr::new(
            HirExprKind::Reference {
                operand: Box::new(operand.clone()),
                mutable: false,
            },
            HirType::Reference {
                inner: Box::new(operand.ty),
                mutable: false,
            },
            operand.span,
        ),
        _ => operand,
    }
}

/// Lend the receiver in `slot` out of the operation, leaving the parameter numbered
/// `index` in its place, and return the argument passed for it.
fn lend_out(slot: &mut HirExpr, index: usize, span: Span) -> HirExpr {
    let operand = lend(std::mem::replace(slot, unit(span)));
    *slot = parameter(index, &operand);
    operand
}

/// The `&mut` argument a compound assignment's outlined function writes its target
/// through: the binding itself when it already is a `&mut` tensor, or a fresh mutable
/// borrow of an owned one. A field or an element cannot be borrowed by a backend.
fn mutable_borrow(place: &HirPlace, ty: &HirType, span: Span) -> Option<HirExpr> {
    let reference = HirType::Reference {
        inner: Box::new(ty.clone()),
        mutable: true,
    };
    let (name, binding_ty) = match place {
        HirPlace::Var { name, ty } => (name, ty),
        HirPlace::Deref { pointer, .. } => match &pointer.kind {
            HirExprKind::Variable(name) => (name, &pointer.ty),
            _ => return None,
        },
        _ => return None,
    };
    let binding = HirExpr::new(
        HirExprKind::Variable(name.clone()),
        binding_ty.clone(),
        span,
    );
    match binding_ty {
        HirType::Reference { mutable: true, .. } => Some(binding),
        HirType::Tensor { .. } => Some(HirExpr::new(
            HirExprKind::Reference {
                operand: Box::new(binding),
                mutable: true,
            },
            reference,
            span,
        )),
        _ => None,
    }
}

/// `position` checked against `extent` where the call evaluates it, the host's guard
/// with the host's message: `{ val p = position; if !((p as u64) < extent) { panic } p }`.
/// A GPU body cannot stop the program, so a position reaches one only once it is known to
/// be in range. A signed position widens with its sign, so a negative one fails the same
/// unsigned test, as on the host.
fn checked_position(position: HirExpr, extent: Option<usize>, span: Span) -> HirExpr {
    let ty = position.ty.clone();
    let local = HirExpr::new(
        HirExprKind::Variable(POSITION_LOCAL.to_string()),
        ty.clone(),
        position.span,
    );
    let widened = match ty {
        HirType::U64 => local.clone(),
        _ => HirExpr::new(
            HirExprKind::Cast {
                value: Box::new(local.clone()),
            },
            HirType::U64,
            position.span,
        ),
    };
    let bound = HirExpr::new(
        HirExprKind::Literal(Literal::Integer(extent.unwrap_or(0) as i128, None)),
        HirType::U64,
        span,
    );
    let inside = HirExpr::new(
        HirExprKind::Binary {
            op: BinaryOp::Less,
            left: Box::new(widened),
            right: Box::new(bound),
        },
        HirType::Bool,
        span,
    );
    let outside = HirExpr::new(
        HirExprKind::Unary {
            op: UnaryOp::Not,
            operand: Box::new(inside),
        },
        HirType::Bool,
        span,
    );
    let message = HirExpr::new(
        HirExprKind::Literal(Literal::String(INDEX_OUT_OF_BOUNDS.to_string())),
        HirType::String,
        span,
    );
    let panic = HirExpr::new(
        HirExprKind::Call {
            callee: Box::new(HirExpr::new(
                HirExprKind::Variable("panic".to_string()),
                HirType::Function {
                    params: vec![HirType::String],
                    ret: Box::new(HirType::Void),
                },
                span,
            )),
            args: vec![message],
        },
        HirType::Void,
        span,
    );
    let block_span = position.span;
    HirExpr::new(
        HirExprKind::Block {
            stmts: vec![
                HirStmt::VarDecl {
                    name: POSITION_LOCAL.to_string(),
                    ty: ty.clone(),
                    init: Some(position),
                    mutable: false,
                    span: block_span,
                },
                HirStmt::If {
                    condition: outside,
                    then_block: vec![HirStmt::Expr(panic)],
                    else_if_blocks: Vec::new(),
                    else_block: None,
                    span,
                },
                HirStmt::Expr(local),
            ],
        },
        ty,
        block_span,
    )
}

/// The parameter an outlined body reads in place of `operand`, at the operand's own span.
fn parameter(index: usize, operand: &HirExpr) -> HirExpr {
    HirExpr::new(
        HirExprKind::Variable(parameter_name(index)),
        operand.ty.clone(),
        operand.span,
    )
}

/// `expr` taken out of its slot, leaving a unit literal until the slot is refilled.
fn placeholder(expr: &mut HirExpr) -> HirExpr {
    std::mem::replace(expr, unit(expr.span))
}

fn unit(span: Span) -> HirExpr {
    HirExpr::new(
        HirExprKind::TupleLiteral { elements: vec![] },
        HirType::Void,
        span,
    )
}
