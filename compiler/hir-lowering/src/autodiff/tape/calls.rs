//! Calls in a `@grad` body: a user function or a function value inlined onto the tape,
//! a call folded at compile time, and the short-circuit operators.

use std::collections::HashMap;

use ast_types::BinaryOp;
use neuro_hir::{HirExpr, HirExprKind, HirStmt, HirTensorApply, HirType};
use shared_types::{Literal, Span};

use crate::autodiff::emit::{self, tensor_parts};
use crate::LoweringError;

use super::leaf::{clone_call, is_copied_field};
use super::positions::coordinates;
use super::{ArmBody, Leaf, Linearizer, Op, MAX_UNROLLED_ELEMENTS, RUN_TIME_TARGET};

impl<'f> Linearizer<'f> {
    /// A call to a user function, or through a function value whose target is known here,
    /// linearized in place.
    pub(super) fn call(
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
            target,
            captures,
            ty,
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
        if self.functions.no_grad.contains(&target) {
            return self.constant_call(&target, &ty, args, span);
        }
        let mut leaves = Vec::with_capacity(args.len());
        for arg in args {
            leaves.push(self.leaf(arg)?);
        }
        self.inline(&target, captures, leaves, span)
    }

    /// A call to the `@no_grad` function `target`, run as written and a constant to the
    /// derivative. Its arguments are read, never consumed or written: the reverse pass reads
    /// a tape value again after the call, so an owned tensor is passed as a copy, a borrow
    /// as a shared one, a number as it is, and anything else is refused.
    pub(super) fn constant_call(
        &mut self,
        target: &str,
        ty: &HirType,
        args: &[HirExpr],
        span: Span,
    ) -> Result<Leaf, LoweringError> {
        let Some(function) = self.functions.function(target) else {
            return Err(self.malformed("a `@no_grad` call names no function"));
        };
        if function.return_type == HirType::Void {
            return Err(self.refuse("a call to a function that returns nothing", span));
        }
        if function.params.len() != args.len() {
            return Err(self.malformed("a call whose argument count is not its callee's"));
        }
        let mut operands = Vec::with_capacity(args.len());
        for (param, arg) in function.params.iter().zip(args) {
            let leaf = self.leaf(arg)?;
            let read = emit::operand_owned(&leaf, arg.span);
            let operand = match &param.ty {
                HirType::Tensor { .. } => clone_call(read, &param.ty),
                HirType::Reference { mutable: false, .. } if !matches!(leaf.ty(), HirType::Reference { .. }) => {
                    HirExpr::new(
                        HirExprKind::Reference {
                            operand: Box::new(read),
                            mutable: false,
                        },
                        param.ty.clone(),
                        arg.span,
                    )
                }
                HirType::Reference { mutable: false, .. } => read,
                other if is_copied_field(other) => read,
                _ => {
                    return Err(self.refuse(
                        "an argument a `@no_grad` call borrows mutably, or takes by value when it is neither a number nor a tensor",
                        arg.span,
                    ))
                }
            };
            operands.push(operand);
        }
        let callee = HirExpr::new(HirExprKind::Variable(target.to_string()), ty.clone(), span);
        let call = HirExprKind::Call {
            callee: Box::new(callee),
            args: operands,
        };
        let call = HirExpr::new(call, function.return_type.clone(), span);
        Ok(self.push(&function.return_type, span, Op::Constant(call)))
    }

    /// The body of the function or closure `target` run at the call on `args`, under
    /// empty aliases and scopes so nothing of the caller leaks in or out. A closure's
    /// `captures` are bound beside its parameters.
    // ponytail: every call site gets its own copy of the callee's tape, so the derivative
    // grows with the call tree; a per-callee reverse function chained at each call is the
    // upgrade if code size starts to matter.
    pub(super) fn inline(
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
    pub(super) fn block_value(
        &mut self,
        stmts: &[HirStmt],
        ty: &HirType,
    ) -> Result<Leaf, LoweringError> {
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
    pub(super) fn traverse(
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
    pub(super) fn short_circuit(
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
