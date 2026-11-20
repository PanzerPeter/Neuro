// Outlining every tensor operation into a function whose body `mlir-backend` lowers, so the
// operation runs where its operands live: on the host, or on the device its tensors are on.
//
// Each operation is lifted into a `HirTarget::FollowsOperands` function whose parameters are
// the operation's operands exactly as written, owned or borrowed, so the call moves and
// borrows what the operation did. Nothing in a tensor's type says where it lives, so a
// backend chooses per call at run time between the body it was given for the host and the
// one for a GPU, with the operands evaluated once in between. The operand expressions keep
// their spans in the body, so a check the body reports still names the operation's own
// position.
//
// The LLVM backend has no tensor loops of its own: this pass is how every operation over
// tensor elements reaches MLIR.

use ast_types::{BinaryOp, UnaryOp};
use neuro_hir::{
    AxisNames, HirCapture, HirExpr, HirExprKind, HirExtent, HirFunction, HirInterpPart, HirItem,
    HirParam, HirPlace, HirReduceOp, HirSortKind, HirStmt, HirTarget, HirTensorApply,
    HirTensorAxis, HirType,
};
use shared_types::{Literal, Span};

/// The outlined functions are named with this and a counter. The checker forbids the `__`
/// prefix in user names, as it does for lifted closures (`__closure_N`).
const OUTLINED_PREFIX: &str = "__tensor_op_";

/// An outlined function's parameters are named with this and their position. The `__`
/// prefix keeps them apart from a closure capture, which is passed under its own name.
const OPERAND_PREFIX: &str = "__operand";

/// The binding an index over a temporary object is read through, so its checks see the same tensor.
const TEMPORARY_PREFIX: &str = "__index_source_";

/// The parameter a traversal's function value is passed through, after every other one.
const CALLEE_PARAMETER: &str = "__callee";

/// The locals of a `.reduce` unfolded into a loop: the tensor it walks and its accumulator.
const FOLD_RECEIVER: &str = "__receiver";
const FOLD_ACCUMULATOR: &str = "__acc";

/// An unfolded `.reduce`'s position along `axis`.
fn position_name(axis: usize) -> String {
    format!("__position{axis}")
}

fn index_literal(value: usize, span: Span) -> HirExpr {
    HirExpr::new(
        HirExprKind::Literal(Literal::Integer(value as i128, None)),
        HirType::U64,
        span,
    )
}

/// The local a slice position is bound to while the call site checks it.
const POSITION_LOCAL: &str = "__position";

use crate::extents::{
    INDEX_OUT_OF_BOUNDS, binary, borrow, extent_read, guard, panic_text, u64_value,
};
use crate::tensor_index::dynamic_axis_check;

/// Outline every tensor operation in the program's host code into a function of `target`,
/// appended to `items`: `FollowsOperands` in a program that moves a tensor to a device,
/// `HostOperation` in one that does not.
pub(crate) fn outline(items: &mut Vec<HirItem>, target: HirTarget) {
    let mut outliner = Outliner {
        functions: Vec::new(),
        target,
        temporaries: 0,
    };
    for item in items.iter_mut() {
        match item {
            // A `@gpu` body is lowered whole, for the host as for the device, and a
            // `@kernel` one is device code.
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

struct Outliner {
    functions: Vec<HirFunction>,
    target: HirTarget,
    /// Counts the temporaries bound for a checked index's object, so each has its own name.
    temporaries: usize,
}

/// Whether an outlined body computes over a tensor of type `ty`: an element with arithmetic
/// (a half-precision one computes in `f32`). A dynamic axis is computed too: the body reads
/// its extent from the operand at run time, and the call site sizes the result from
/// [`HirFunction::result_extents`].
fn computed_tensor(ty: &HirType) -> bool {
    matches!(ty.referent(), HirType::Tensor { element, .. } if arithmetic(element))
}

/// An element the language gives arithmetic: a number, or a half-precision float.
fn arithmetic(ty: &HirType) -> bool {
    matches!(
        ty,
        HirType::F16
            | HirType::BF16
            | HirType::F32
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

/// A value a body passes to and from a traversal's function as itself: an element with
/// arithmetic, or a `bool`.
fn scalar(ty: &HirType) -> bool {
    arithmetic(ty) || *ty == HirType::Bool
}

/// Whether a traversal walks a tensor of type `ty`: a static shape of scalars.
fn walked_tensor(ty: &HirType) -> bool {
    matches!(ty.referent(), HirType::Tensor { element, .. } if scalar(element))
}

/// Whether an index over `object_ty` checks a run-time extent: a position or a constant range
/// on a dynamic axis. Such an index names its object as a place, so the check can borrow it.
fn index_is_checked(object_ty: &HirType, axes: &[HirTensorAxis]) -> bool {
    let HirType::Tensor { shape, .. } = object_ty.referent() else {
        return false;
    };
    axes.iter()
        .zip(shape)
        .any(|(axis, extent)| extent.is_none() && !matches!(axis, HirTensorAxis::Whole))
}

/// A node of an outlinable operator tree: an operation producing a tensor out of tensors the
/// same function can compute or read. Element-wise arithmetic, `@` and a permuting shape cast
/// take their operands as written; elementwise math, a slice and an `einsum` only read
/// theirs, so each must be another node or a tensor that is lent. A slice or a permutation
/// moves elements without computing them, so it takes a `bool` tensor too.
fn operator_node(expr: &HirExpr) -> bool {
    let layout = matches!(
        expr.kind,
        HirExprKind::TensorShapeCast {
            permutation: Some(_),
            ..
        } | HirExprKind::TensorIndex { .. }
    );
    if !computed_tensor(&expr.ty) && !(layout && walked_tensor(&expr.ty)) {
        return false;
    }
    // A checked index borrows its object for the check, which only a place can be read
    // twice; any other object is bound by `outline_operation` first.
    if let HirExprKind::TensorIndex { object, axes } = &expr.kind
        && index_is_checked(&object.ty, axes)
        && !is_place(object)
    {
        return false;
    }
    match &expr.kind {
        HirExprKind::Binary { op, .. } => matches!(
            op,
            BinaryOp::Add
                | BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::Modulo
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
/// as a tensor it is lent.
fn readable(operand: &HirExpr) -> bool {
    operator_node(operand) || walked_tensor(&operand.ty)
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
    captures.iter().all(|capture| scalar(&capture.ty))
}

impl Outliner {
    /// Replace `expr` with a call to an outlined function when it is a tensor operation,
    /// and report whether it did. Its operands are rewritten first, so an operation nested
    /// in one (a reduction's receiver) is outlined on its own.
    fn outline_operation(&mut self, expr: &mut HirExpr) -> bool {
        if let HirExprKind::TensorIndex { object, axes } = &expr.kind
            && index_is_checked(&object.ty, axes)
            && !is_place(object)
        {
            return self.outline_temporary_index(expr);
        }
        if operator_node(expr) {
            let mut operands = Vec::new();
            let body = self.take_tree(placeholder(expr), &mut operands);
            *expr = self.call(body, operands, &[], None, expr.span);
            return true;
        }
        match &mut expr.kind {
            // A sort's result is a fresh tensor (or `.topk`'s pair of them) of the receiver's
            // shape, so the receiver alone decides whether it is computed.
            HirExprKind::TensorSort { receiver, .. } => {
                if !computed_tensor(&receiver.ty) {
                    return false;
                }
                self.rewrite(receiver);
                let span = expr.span;
                let mut sort = placeholder(expr);
                let HirExprKind::TensorSort {
                    receiver,
                    kind,
                    axis,
                    ..
                } = &mut sort.kind
                else {
                    return false;
                };
                let axis = *axis;
                let top = match kind {
                    HirSortKind::TopK(k) => Some(*k),
                    _ => None,
                };
                let mut operand = lend_out(receiver, 0, span);
                // `.topk(k)` past a dynamic axis's run-time extent is refused at the call.
                if let Some(k) = top
                    && is_dynamic_axis(&operand.ty, axis)
                {
                    operand = guarded_operand(operand, span, |tensor| {
                        let within = binary(
                            BinaryOp::LessEqual,
                            u64_value(k as u64, span),
                            extent_read(borrow(tensor), axis, span),
                            HirType::Bool,
                            span,
                        );
                        vec![guard(within, panic_text(INDEX_OUT_OF_BOUNDS, span), span)]
                    });
                }
                *expr = self.call(sort, vec![operand], &[], None, span);
                true
            }
            HirExprKind::TensorReduce { receiver, axis, op } => {
                if !computed_tensor(&receiver.ty) || (axis.is_some() && !computed_tensor(&expr.ty))
                {
                    return false;
                }
                self.rewrite(receiver);
                let whole = axis.is_none();
                let span = expr.span;
                let method = reduce_method(*op);
                // The run a reduction folds is every axis, or the one it names. A static empty
                // run was refused before lowering; a dynamic one is refused here, at run time.
                let run: Vec<usize> = match axis {
                    Some(axis) => vec![*axis],
                    None => (0..tensor_rank(&receiver.ty)).collect(),
                };
                let mut reduce = placeholder(expr);
                let HirExprKind::TensorReduce { receiver, .. } = &mut reduce.kind else {
                    return false;
                };
                let mut operand = lend_out(receiver, 0, span);
                let dynamic: Vec<usize> = run
                    .into_iter()
                    .filter(|&axis| is_dynamic_axis(&operand.ty, axis))
                    .collect();
                if !dynamic.is_empty() {
                    operand = guarded_operand(operand, span, |tensor| {
                        dynamic
                            .iter()
                            .map(|&axis| {
                                let filled = binary(
                                    BinaryOp::Greater,
                                    extent_read(borrow(tensor.clone()), axis, span),
                                    u64_value(0, span),
                                    HirType::Bool,
                                    span,
                                );
                                let message = format!("`.{method}()` reduces over no elements");
                                guard(filled, panic_text(&message, span), span)
                            })
                            .collect()
                    });
                }
                *expr = match whole {
                    true => self.boxed_call(reduce, vec![operand], &[], None, span),
                    false => self.call(reduce, vec![operand], &[], None, span),
                };
                true
            }
            // A full contraction (`"ii->"`) yields a scalar; one with output letters is a
            // tensor, and so a node of an operator tree, taken above.
            HirExprKind::TensorEinsum { operands, .. } => {
                if computed_tensor(&expr.ty)
                    || !arithmetic(&expr.ty)
                    || !operands.iter().all(readable)
                {
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
                *expr = self.boxed_call(einsum, taken, &[], None, span);
                true
            }
            HirExprKind::TensorApply { .. } => self.outline_apply(expr),
            _ => false,
        }
    }

    /// Outline `.map(f)`, `.zip(other, f)` or `.reduce(init, f)` over a tensor of scalars
    /// whose answer is a scalar too. The function value is the last parameter, which is how
    /// a host body calls it. A closure literal whose captures are all scalars also stays in
    /// the body with its captures passed under their own names, which is where the literal
    /// loads them from and how a GPU body calls it.
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
        let admitted = walked_tensor(&receiver.ty)
            && match (*kind, operand.as_deref()) {
                (HirTensorApply::Map, None) => computed_tensor(&expr.ty),
                (HirTensorApply::Zip, Some(other)) => {
                    computed_tensor(&expr.ty) && walked_tensor(&other.ty)
                }
                (HirTensorApply::Reduce, Some(_)) => scalar(&expr.ty),
                _ => false,
            };
        if !admitted {
            if *kind == HirTensorApply::Reduce && walked_tensor(&receiver.ty) {
                self.unfold_reduce(expr);
                return true;
            }
            return false;
        }
        let captures = match &callee.kind {
            HirExprKind::Closure { captures, .. } if scalar_captures(captures) => {
                Some(captures.clone())
            }
            _ => None,
        };
        let reduce = *kind == HirTensorApply::Reduce;
        self.rewrite(receiver);
        if let Some(operand) = operand {
            self.rewrite(operand);
        }
        self.rewrite(callee);
        let span = expr.span;
        let mut apply = placeholder(expr);
        let HirExprKind::TensorApply {
            receiver,
            operand,
            callee,
            ..
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
        let function = match captures {
            Some(_) => (**callee).clone(),
            None => {
                let parameter = HirExpr::new(
                    HirExprKind::Variable(CALLEE_PARAMETER.to_string()),
                    callee.ty.clone(),
                    callee.span,
                );
                std::mem::replace(callee.as_mut(), parameter)
            }
        };
        let captures = captures.unwrap_or_default();
        *expr = match reduce {
            true => self.boxed_call(apply, operands, &captures, Some(function), span),
            false => self.call(apply, operands, &captures, Some(function), span),
        };
        true
    }

    /// `.reduce(init, f)` whose accumulator is no scalar, which no outlined body can carry, as
    /// the loop it is: `{ val r = receiver; mut acc = init; val f = f; for each position in
    /// row-major order { acc = f(acc, r[position]) }; acc }`. The operands are evaluated in
    /// the order the call writes them, and every element read is scalar code.
    fn unfold_reduce(&mut self, expr: &mut HirExpr) {
        let span = expr.span;
        let accumulator = expr.ty.clone();
        let HirExprKind::TensorApply {
            receiver,
            operand: Some(seed),
            callee,
            ..
        } = &mut expr.kind
        else {
            return;
        };
        self.rewrite(receiver);
        self.rewrite(seed);
        self.rewrite(callee);
        let receiver = lend(std::mem::replace(receiver.as_mut(), unit(span)));
        let seed = std::mem::replace(seed.as_mut(), unit(span));
        let callee = std::mem::replace(callee.as_mut(), unit(span));
        let extents: Vec<Option<usize>> = match receiver.ty.referent() {
            HirType::Tensor { shape, .. } => shape.clone(),
            _ => Vec::new(),
        };
        let receiver_local = receiver.ty.clone();
        let element = element(&receiver.ty).cloned().unwrap_or(HirType::Void);
        let local = |name: &str, ty: &HirType| {
            HirExpr::new(HirExprKind::Variable(name.to_string()), ty.clone(), span)
        };
        let declare = |name: &str, init: HirExpr, mutable: bool| HirStmt::VarDecl {
            name: name.to_string(),
            ty: init.ty.clone(),
            init: Some(init),
            mutable,
            span,
        };
        let positions = (0..extents.len())
            .map(|axis| HirTensorAxis::Position(local(&position_name(axis), &HirType::U64)))
            .collect();
        let read = HirExpr::new(
            HirExprKind::TensorIndex {
                object: Box::new(local(FOLD_RECEIVER, &receiver.ty)),
                axes: positions,
            },
            element,
            span,
        );
        let step = HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(local(CALLEE_PARAMETER, &callee.ty)),
                args: vec![local(FOLD_ACCUMULATOR, &accumulator), read],
            },
            accumulator.clone(),
            span,
        );
        let mut body = vec![HirStmt::Assign {
            place: HirPlace::Var {
                name: FOLD_ACCUMULATOR.to_string(),
                ty: accumulator.clone(),
            },
            value: step,
            span,
        }];
        for (axis, extent) in extents.iter().enumerate().rev() {
            // A dynamic axis runs to the receiver's run-time extent, read from the local the
            // receiver is declared as.
            let end = match extent {
                Some(value) => index_literal(*value, span),
                None => extent_read(borrow(local(FOLD_RECEIVER, &receiver_local)), axis, span),
            };
            body = vec![HirStmt::ForRange {
                label: None,
                index: None,
                iterator: position_name(axis),
                start: index_literal(0, span),
                end,
                inclusive: false,
                reversed: false,
                step: None,
                body,
                span,
            }];
        }
        let mut stmts = vec![
            declare(FOLD_RECEIVER, receiver, false),
            declare(FOLD_ACCUMULATOR, seed, true),
            declare(CALLEE_PARAMETER, callee, false),
        ];
        stmts.extend(body);
        stmts.push(HirStmt::Expr(local(FOLD_ACCUMULATOR, &accumulator)));
        *expr = HirExpr::new(HirExprKind::Block { stmts }, accumulator, span);
    }

    /// `place OP= value` on a computed tensor as a call to a function that updates its
    /// target through a `&mut` parameter, so the buffer the place addresses is the one
    /// written, wherever it lives. `None` for a value no body takes.
    fn outline_compound(
        &mut self,
        place: &HirPlace,
        op: BinaryOp,
        value: &mut HirExpr,
        ty: &HirType,
        span: Span,
    ) -> Option<HirExpr> {
        let admitted = computed_tensor(ty)
            && matches!(
                op,
                BinaryOp::Add
                    | BinaryOp::Subtract
                    | BinaryOp::Multiply
                    | BinaryOp::Divide
                    | BinaryOp::Modulo
            )
            && (computed_tensor(&value.ty) || value.ty == *element(ty)?);
        if !admitted {
            return None;
        }
        let target = mutable_borrow(place, ty, span);
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
        Some(self.define(
            vec![body],
            HirType::Void,
            vec![value, target],
            (&[], None),
            span,
        ))
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
                // Only a place reaches here as a checked object (see `operator_node`), so the
                // checks can borrow it where the call evaluates them.
                let source = (**object).clone();
                let before = operands.len();
                **object = self.take_read(std::mem::replace(object, unit(span)), operands);
                // A constant range over a dynamic axis is checked before the object is lent.
                let mut range_checks = Vec::new();
                for (index, (axis, extent)) in axes.iter().zip(extents.iter()).enumerate() {
                    if let (HirTensorAxis::Range { .. }, None) = (axis, extent)
                        && let Some(check) = dynamic_axis_check(axis, &source, index, span)
                    {
                        range_checks.push(check);
                    }
                }
                if !range_checks.is_empty() && operands.len() == before + 1 {
                    let lent = operands.pop().unwrap_or_else(|| unit(span));
                    let mut stmts = range_checks;
                    stmts.push(HirStmt::Expr(lent.clone()));
                    operands.push(HirExpr::new(HirExprKind::Block { stmts }, lent.ty, span));
                }
                for (index, (axis, extent)) in axes.iter_mut().zip(extents).enumerate() {
                    if let HirTensorAxis::Position(position) = axis {
                        self.rewrite(position);
                        let bound = match extent {
                            Some(value) => index_literal(value, span),
                            None => extent_read(borrow(source.clone()), index, span),
                        };
                        let checked =
                            checked_position(std::mem::replace(position, unit(span)), bound, span);
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

    /// Define an outlined function computing `body` over `operands`, `captures` and
    /// a traversal's function value, and return the call to it.
    fn call(
        &mut self,
        body: HirExpr,
        operands: Vec<HirExpr>,
        captures: &[HirCapture],
        callee: Option<HirExpr>,
        span: Span,
    ) -> HirExpr {
        let ret = body.ty.clone();
        let extents = result_extents(&body);
        let call = self.define(
            vec![HirStmt::Expr(body)],
            ret,
            operands,
            (captures, callee),
            span,
        );
        if let Some(function) = self.functions.last_mut() {
            function.result_extents = extents;
        }
        call
    }

    /// A checked index whose object is a temporary: the temporary is bound first, so the
    /// checks and the read see one tensor. The object is outlined on its own, and the index
    /// over the binding is then outlined as any place's would be.
    fn outline_temporary_index(&mut self, expr: &mut HirExpr) -> bool {
        let span = expr.span;
        let ty = expr.ty.clone();
        let HirExprKind::TensorIndex { object, .. } = &mut expr.kind else {
            return false;
        };
        self.rewrite(object);
        let temp = format!("{TEMPORARY_PREFIX}{}", self.temporaries);
        self.temporaries += 1;
        let object_ty = object.ty.clone();
        let taken = std::mem::replace(
            object.as_mut(),
            HirExpr::new(HirExprKind::Variable(temp.clone()), object_ty.clone(), span),
        );
        let mut index = std::mem::replace(expr, unit(span));
        self.outline_operation(&mut index);
        *expr = HirExpr::new(
            HirExprKind::Block {
                stmts: vec![
                    HirStmt::VarDecl {
                        name: temp,
                        ty: object_ty,
                        init: Some(taken),
                        mutable: false,
                        span,
                    },
                    HirStmt::Expr(index),
                ],
            },
            ty,
            span,
        );
        true
    }

    /// [`Self::call`] for a body yielding a scalar, which a GPU body cannot return: its
    /// result is a buffer the caller allocates. So the body leaves it in a one-element
    /// tensor, and the call site reads that element back.
    fn boxed_call(
        &mut self,
        body: HirExpr,
        operands: Vec<HirExpr>,
        captures: &[HirCapture],
        callee: Option<HirExpr>,
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
        let call = self.call(boxed, operands, captures, callee, span);
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
        (captures, callee): (&[HirCapture], Option<HirExpr>),
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
        if let Some(callee) = callee {
            params.push(HirParam {
                name: CALLEE_PARAMETER.to_string(),
                ty: callee.ty.clone(),
                span: callee.span,
            });
            args.push(callee);
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
            result_extents: Vec::new(),
            name,
            params,
            return_type: ret.clone(),
            body,
            target: self.target,
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
            HirPlace::Field { object, .. } | HirPlace::TupleIndex { object, .. } => {
                self.rewrite(object)
            }
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
            | HirExprKind::CollectionNew { capacity: None }
            | HirExprKind::Closure { .. } => {}
            HirExprKind::TensorIdentity { dims } => self.rewrite_all(dims),
            HirExprKind::TensorExtent { tensor, .. } => self.rewrite(tensor),
            HirExprKind::CollectionNew {
                capacity: Some(capacity),
            } => self.rewrite(capacity),
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
            HirExprKind::TensorRandomNormal { mean, std, dims } => {
                self.rewrite(mean);
                self.rewrite(std);
                self.rewrite_all(dims);
            }
            HirExprKind::TensorFill { value, dims } => {
                self.rewrite(value);
                self.rewrite_all(dims);
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

/// Whether axis `axis` of the tensor type `ty` has a run-time extent.
fn tensor_rank(ty: &HirType) -> usize {
    match ty.referent() {
        HirType::Tensor { shape, .. } => shape.len(),
        _ => 0,
    }
}

/// The method a reduction was written as, for its run-time diagnostic.
fn reduce_method(op: HirReduceOp) -> &'static str {
    match op {
        HirReduceOp::Sum => "sum",
        HirReduceOp::Mean => "mean",
        HirReduceOp::Max => "max",
        HirReduceOp::Min => "min",
    }
}

fn is_dynamic_axis(ty: &HirType, axis: usize) -> bool {
    matches!(
        ty.referent(),
        HirType::Tensor { shape, .. } if shape.get(axis).is_some_and(Option::is_none)
    )
}

/// `operand` bound to a local for the length of `checks`, which read that local, and then
/// handed on as the operand itself: the checks run where the call evaluates its operands.
fn guarded_operand(
    operand: HirExpr,
    span: Span,
    checks: impl FnOnce(HirExpr) -> Vec<HirStmt>,
) -> HirExpr {
    let ty = operand.ty.clone();
    let local = format!("{TEMPORARY_PREFIX}operand");
    let read = HirExpr::new(HirExprKind::Variable(local.clone()), ty.clone(), span);
    let mut stmts = vec![HirStmt::VarDecl {
        name: local,
        ty: ty.clone(),
        init: Some(operand),
        mutable: false,
        span,
    }];
    stmts.extend(checks(read.clone()));
    stmts.push(HirStmt::Expr(read));
    HirExpr::new(HirExprKind::Block { stmts }, ty, span)
}

/// The run-time extent of each dynamic axis of an outlined body's result, in the order the
/// result type lists them (a tuple result's parts in order). Every such axis is walked by
/// some operand, so [`extent_of`] finds it.
fn result_extents(body: &HirExpr) -> Vec<HirExtent> {
    let parts: Vec<&HirType> = match &body.ty {
        HirType::Tuple(parts) => parts.iter().collect(),
        other => vec![other],
    };
    let mut extents = Vec::new();
    for part in parts {
        let HirType::Tensor { shape, .. } = part.referent() else {
            continue;
        };
        for (axis, extent) in shape.iter().enumerate() {
            if extent.is_none()
                && let Some(found) = extent_of(body, axis)
            {
                extents.push(found);
            }
        }
    }
    extents
}

/// The run-time extent of axis `axis` of the tensor `expr` computes, as a function of the
/// operands the outlined body names (`__operandN`). An element-wise operation takes the
/// extent of an operand that walks the axis itself, counted from the trailing axis, so a
/// dynamic axis is never stretched.
fn extent_of(expr: &HirExpr, axis: usize) -> Option<HirExtent> {
    match &expr.kind {
        HirExprKind::Variable(name) => {
            let operand = name.strip_prefix(OPERAND_PREFIX)?.parse::<usize>().ok()?;
            Some(HirExtent::Axis { operand, axis })
        }
        HirExprKind::Binary {
            op: BinaryOp::MatMul,
            left,
            right,
        } => {
            let rank = rank_of(&expr.ty)?;
            if axis + 2 == rank {
                return extent_of(left, rank_of(&left.ty)?.checked_sub(2)?);
            }
            if axis + 1 == rank {
                return extent_of(right, rank_of(&right.ty)?.checked_sub(1)?);
            }
            // A batch axis is walked by whichever operand has it, counted from the trailing one.
            aligned_extent(&[&**left, &**right], rank, axis, 2)
        }
        HirExprKind::Binary { left, right, .. } => {
            let rank = rank_of(&expr.ty)?;
            aligned_extent(&[&**left, &**right], rank, axis, 0)
        }
        HirExprKind::TensorShapeCast {
            receiver,
            permutation: Some(order),
            ..
        } => extent_of(receiver, *order.get(axis)?),
        HirExprKind::TensorIndex { object, axes } => {
            let mut survivor = 0;
            for (position, index) in axes.iter().enumerate() {
                if matches!(index, HirTensorAxis::Position(_)) {
                    continue;
                }
                if survivor == axis {
                    return match index {
                        HirTensorAxis::Whole => extent_of(object, position),
                        _ => None,
                    };
                }
                survivor += 1;
            }
            None
        }
        HirExprKind::Math { operand, .. } => extent_of(operand, axis),
        HirExprKind::TensorEinsum {
            operands,
            inputs,
            output,
            ..
        } => {
            let letter = *output.get(axis)?;
            for (operand, subscript) in operands.iter().zip(inputs) {
                if let Some(position) = subscript.iter().position(|index| *index == letter) {
                    return extent_of(operand, position);
                }
            }
            None
        }
        HirExprKind::TensorReduce {
            receiver,
            axis: Some(reduced),
            ..
        } => extent_of(receiver, if axis < *reduced { axis } else { axis + 1 }),
        HirExprKind::TensorSort { receiver, .. } | HirExprKind::TensorApply { receiver, .. } => {
            extent_of(receiver, axis)
        }
        _ => None,
    }
}

/// The rank of a tensor type, or of a tensor a tuple part names.
fn rank_of(ty: &HirType) -> Option<usize> {
    match ty.referent() {
        HirType::Tensor { shape, .. } => Some(shape.len()),
        _ => None,
    }
}

/// The extent of result axis `axis` (of a result of rank `rank`) from the first operand
/// that walks it: operand axes align on their trailing edge, and an operand walks an axis
/// only when it has one there. `keep` leaves that many trailing axes of each operand out
/// (a matrix product's two matrix axes, which are not batch axes).
fn aligned_extent(
    operands: &[&HirExpr],
    rank: usize,
    axis: usize,
    keep: usize,
) -> Option<HirExtent> {
    for operand in operands {
        let HirType::Tensor { shape, .. } = operand.ty.referent() else {
            continue;
        };
        let depth = shape.len();
        let Some(offset) = rank.checked_sub(depth) else {
            continue;
        };
        let Some(position) = axis.checked_sub(offset) else {
            continue;
        };
        if position + keep >= depth {
            continue;
        }
        if shape[position].is_none() {
            return extent_of(operand, position);
        }
    }
    None
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
/// through: the binding itself when it already is a `&mut` tensor, and otherwise a fresh
/// mutable borrow of the place.
fn mutable_borrow(place: &HirPlace, ty: &HirType, span: Span) -> HirExpr {
    let reference = HirType::Reference {
        inner: Box::new(ty.clone()),
        mutable: true,
    };
    match place {
        HirPlace::Var { name, ty } if matches!(ty, HirType::Reference { mutable: true, .. }) => {
            return HirExpr::new(HirExprKind::Variable(name.clone()), ty.clone(), span);
        }
        HirPlace::Deref { pointer, .. }
            if matches!(pointer.kind, HirExprKind::Variable(_))
                && matches!(pointer.ty, HirType::Reference { mutable: true, .. }) =>
        {
            return HirExpr::new(pointer.kind.clone(), pointer.ty.clone(), span);
        }
        _ => {}
    }
    HirExpr::new(
        HirExprKind::Reference {
            operand: Box::new(place.to_expr(span)),
            mutable: true,
        },
        reference,
        span,
    )
}

/// `position` checked against `extent` where the call evaluates it, the host's guard
/// with the host's message: `{ val p = position; if !((p as u64) < extent) { panic } p }`.
/// A GPU body cannot stop the program, so a position reaches one only once it is known to
/// be in range. A signed position widens with its sign, so a negative one fails the same
/// unsigned test, as on the host.
fn checked_position(position: HirExpr, bound: HirExpr, span: Span) -> HirExpr {
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
    let inside = binary(BinaryOp::Less, widened, bound, HirType::Bool, span);
    let outside = HirExpr::new(
        HirExprKind::Unary {
            op: UnaryOp::Not,
            operand: Box::new(inside),
        },
        HirType::Bool,
        span,
    );
    let panic = panic_text(INDEX_OUT_OF_BOUNDS, span);
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
