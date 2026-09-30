// Outlining the tensor operations a GPU can run into functions that run where their
// operands live (§6.2: operations execute on the tensor's device).
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

use ast_types::BinaryOp;
use neuro_hir::{
    AxisNames, HirExpr, HirExprKind, HirFunction, HirInterpPart, HirItem, HirParam, HirPlace,
    HirStmt, HirTarget, HirTensorAxis, HirType,
};
use shared_types::{Literal, Span};

/// The outlined functions are named with this and a counter. The checker forbids the `__`
/// prefix in user names, as it does for lifted closures (`__closure_N`).
const OUTLINED_PREFIX: &str = "__device_op_";

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

/// Whether a GPU body can compute over a tensor of type `ty`: a floating-point element, as
/// the MLIR path requires (it has none of the integer guards), and a static shape of rank 1
/// or more, since an operation over rank 0 has no parallel axis to launch across.
fn device_tensor(ty: &HirType) -> bool {
    matches!(
        ty.referent(),
        HirType::Tensor { element, shape, .. }
            if matches!(**element, HirType::F32 | HirType::F64)
                && !shape.is_empty()
                && shape.iter().all(Option::is_some)
    )
}

/// A node of an outlinable operator tree: element-wise arithmetic or a matrix product
/// producing a device-capable tensor.
fn operator_node(expr: &HirExpr) -> bool {
    matches!(
        &expr.kind,
        HirExprKind::Binary {
            op: BinaryOp::Add
                | BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::MatMul,
            ..
        }
    ) && device_tensor(&expr.ty)
}

/// Whether a reduction may be outlined over `receiver`: a borrow (lent on as it is), a
/// named tensor (lent with `&`), or a temporary (moved in, since nothing else owns it).
/// A tensor inside another place, a field say, can be neither lent, since a backend borrows
/// only a binding, nor moved out of its owner, so its reduction stays inline.
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

impl Outliner {
    /// Replace `expr` with a call to an outlined function when it is a device-capable
    /// operation, and report whether it did. Its operands are rewritten first, so an
    /// operation nested in one (a reduction's receiver) is outlined on its own.
    fn outline_operation(&mut self, expr: &mut HirExpr) -> bool {
        if operator_node(expr) {
            let mut operands = Vec::new();
            let body = self.take_tree(placeholder(expr), &mut operands);
            *expr = self.call(body, operands, expr.span);
            return true;
        }
        let HirExprKind::TensorReduce { receiver, axis, .. } = &mut expr.kind else {
            return false;
        };
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
        let operand = std::mem::replace(receiver.as_mut(), unit(span));
        // A reduction reads its receiver and leaves it alive, so a named tensor is lent to
        // the call rather than moved into it. A temporary has no owner to outlive it.
        let operand = match &operand.ty {
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
        };
        **receiver = parameter(0, &operand);
        if !whole {
            *expr = self.call(reduce, vec![operand], span);
            return true;
        }
        // A whole-tensor reduction yields a scalar, which a GPU body cannot return: its
        // result is a buffer the caller allocates. So the body leaves it in a one-element
        // tensor, and the call site reads that element back.
        let element = reduce.ty.clone();
        let boxed = HirExpr::new(
            HirExprKind::TensorLiteral {
                elements: vec![reduce],
            },
            HirType::Tensor {
                element: Box::new(element.clone()),
                shape: vec![Some(1)],
                names: AxisNames::default(),
            },
            span,
        );
        let call = self.call(boxed, vec![operand], span);
        let first = HirExpr::new(
            HirExprKind::Literal(Literal::Integer(0, None)),
            HirType::U64,
            span,
        );
        *expr = HirExpr::new(
            HirExprKind::TensorIndex {
                object: Box::new(call),
                axes: vec![HirTensorAxis::Position(first)],
            },
            element,
            span,
        );
        true
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
        if let HirExprKind::Binary { left, right, .. } = &mut expr.kind {
            **left = self.take_tree(std::mem::replace(left, unit(expr.span)), operands);
            **right = self.take_tree(std::mem::replace(right, unit(expr.span)), operands);
        }
        expr
    }

    /// Define a `FollowsOperands` function computing `body` over `operands`, and return
    /// the call to it.
    fn call(&mut self, body: HirExpr, operands: Vec<HirExpr>, span: Span) -> HirExpr {
        let name = format!("{OUTLINED_PREFIX}{}", self.functions.len());
        let params: Vec<HirParam> = operands
            .iter()
            .enumerate()
            .map(|(index, operand)| HirParam {
                name: parameter_name(index),
                ty: operand.ty.clone(),
                span: operand.span,
            })
            .collect();
        let ret = body.ty.clone();
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
            body: vec![HirStmt::Expr(body)],
            target: HirTarget::FollowsOperands,
            span,
        });
        HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(callee),
                args: operands,
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
            HirStmt::Assign { place, value, .. }
            | HirStmt::TensorCompoundAssign { place, value, .. } => {
                self.rewrite(value);
                self.rewrite_place(place);
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
    format!("operand{index}")
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
