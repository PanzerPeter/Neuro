//! The `.map(f)` / `.filter(p)` desugar for a `for` head.
//!
//! An adapter chain is folded into the loop it decorates rather than materialized
//! as an iterator value, so it works over every head shape — a range, an array, a
//! `Vec`, a borrowed slice, and a protocol iterator alike:
//!
//! ```text
//! for y in xs.map(f).filter(p) { body }
//!
//! =>  val __adapt_fn_N_0 = f
//!     val __adapt_fn_N_1 = p
//!     for __adapt_elem_N in xs {
//!         val __adapt_v_N_0 = __adapt_fn_N_0(__adapt_elem_N)
//!         if !__adapt_fn_N_1(__adapt_v_N_0) { continue }
//!         val y = __adapt_v_N_0
//!         body
//!     }
//! ```
//!
//! Each adapter function is bound ONCE, outside the loop: `xs.map(make_rule())`
//! must not rebuild its rule per element.
//!
//! An enumerated head counts what the chain YIELDS, not what the source produced, so
//! `.enumerate()` over a filtered chain gets its own cursor instead of the counted
//! loop's index — which counts source steps and would leave gaps.

use ast_types::{LoopAdapter, LoopAdapterKind, Stmt, UnaryOp};
use neuro_hir::{HirExpr, HirExprKind, HirStmt, HirType};
use shared_types::{Identifier, Literal, Span};

use crate::{Lowerer, LoweringError};

/// The type of a yielded-position binding, matching the counted loops' index.
const LOOP_INDEX_TYPE: HirType = HirType::U64;

/// One adapter, resolved to the binding holding its function.
struct AdapterStep {
    kind: LoopAdapterKind,
    /// The binding the function was evaluated into, outside the loop.
    binding: String,
    /// The function's return type, which `.map` makes the new element type.
    result: HirType,
}

/// The desugar of one head's adapter chain: what to emit around the loop, and what
/// the loop itself must bind.
pub(crate) struct AdapterPlan {
    /// Bindings that precede the loop: one per adapter function, plus the yielded
    /// -position cursor of an enumerated head.
    pub(crate) prelude: Vec<HirStmt>,
    /// The name the loop binds instead of the user's, holding the SOURCE element.
    pub(crate) element: String,
    steps: Vec<AdapterStep>,
    cursor: Option<String>,
    id: usize,
}

impl Lowerer {
    /// Evaluate an adapter chain's functions and reserve the loop's own bindings.
    ///
    /// Must run in the scope that encloses the loop: the bindings it defines are
    /// referenced from inside the body and must outlive one iteration.
    pub(crate) fn plan_loop_adapters(
        &mut self,
        adapters: &[LoopAdapter],
        index: &Option<Identifier>,
        span: Span,
    ) -> Result<AdapterPlan, LoweringError> {
        self.protocol_counter += 1;
        let id = self.protocol_counter;

        let mut prelude = Vec::with_capacity(adapters.len() + 1);
        let mut steps = Vec::with_capacity(adapters.len());
        for (position, adapter) in adapters.iter().enumerate() {
            let callee = self.lower_expr(&adapter.callee, None)?;
            let HirType::Function { ret, .. } = &callee.ty else {
                return Err(LoweringError::Malformed {
                    detail: format!(
                        "`.{}()` in a `for` head is not a function",
                        adapter_name(adapter.kind)
                    ),
                });
            };
            let result = (**ret).clone();
            let binding = format!("__adapt_fn_{}_{}", id, position);
            self.define(binding.clone(), callee.ty.clone());
            prelude.push(HirStmt::VarDecl {
                name: binding.clone(),
                ty: callee.ty.clone(),
                init: Some(callee),
                mutable: false,
                span: adapter.span,
            });
            steps.push(AdapterStep {
                kind: adapter.kind,
                binding,
                result,
            });
        }

        let cursor = index.as_ref().map(|_| format!("__adapt_pos_{}", id));
        if let Some(cursor) = &cursor {
            self.define(cursor.clone(), LOOP_INDEX_TYPE);
            prelude.push(HirStmt::VarDecl {
                name: cursor.clone(),
                ty: LOOP_INDEX_TYPE,
                init: Some(int_literal(0, span)),
                mutable: true,
                span,
            });
        }

        Ok(AdapterPlan {
            prelude,
            element: format!("__adapt_elem_{}", id),
            steps,
            cursor,
            id,
        })
    }

    /// The loop body an adapted head runs: the chain, then the user's binding, then
    /// the user's statements.
    ///
    /// Must run inside the loop's own scope, with [`AdapterPlan::element`] already
    /// defined as `element_ty`.
    pub(crate) fn apply_loop_adapters(
        &mut self,
        plan: &AdapterPlan,
        element_ty: &HirType,
        index: &Option<Identifier>,
        iterator: &Identifier,
        body: &[Stmt],
        span: Span,
    ) -> Result<Vec<HirStmt>, LoweringError> {
        let mut stmts = Vec::new();
        let mut current = variable(&plan.element, element_ty.clone(), span);

        for (position, step) in plan.steps.iter().enumerate() {
            let call = self.adapter_call(step, current.clone(), span);
            match step.kind {
                LoopAdapterKind::Filter => {
                    stmts.push(HirStmt::If {
                        condition: HirExpr::new(
                            HirExprKind::Unary {
                                op: UnaryOp::Not,
                                operand: Box::new(call),
                            },
                            HirType::Bool,
                            span,
                        ),
                        then_block: vec![HirStmt::Continue { label: None, span }],
                        else_if_blocks: Vec::new(),
                        else_block: None,
                        span,
                    });
                }
                LoopAdapterKind::Map => {
                    let name = format!("__adapt_v_{}_{}", plan.id, position);
                    self.define(name.clone(), step.result.clone());
                    stmts.push(HirStmt::VarDecl {
                        name: name.clone(),
                        ty: step.result.clone(),
                        init: Some(call),
                        mutable: false,
                        span,
                    });
                    current = variable(&name, step.result.clone(), span);
                }
            }
        }

        // The position is read out and advanced here — after the filters, so it counts
        // yielded elements, and before the user's statements, so a `continue` in the
        // body cannot skip the advance and repeat an index.
        if let (Some(index), Some(cursor)) = (index, &plan.cursor) {
            self.define(index.name.clone(), LOOP_INDEX_TYPE);
            stmts.push(HirStmt::VarDecl {
                name: index.name.clone(),
                ty: LOOP_INDEX_TYPE,
                init: Some(variable(cursor, LOOP_INDEX_TYPE, span)),
                mutable: false,
                span,
            });
            stmts.push(HirStmt::Assignment {
                target: cursor.clone(),
                value: HirExpr::new(
                    HirExprKind::Binary {
                        op: ast_types::BinaryOp::Add,
                        left: Box::new(variable(cursor, LOOP_INDEX_TYPE, span)),
                        right: Box::new(int_literal(1, span)),
                    },
                    LOOP_INDEX_TYPE,
                    span,
                ),
                span,
            });
        }

        self.define(iterator.name.clone(), current.ty.clone());
        stmts.push(HirStmt::VarDecl {
            name: iterator.name.clone(),
            ty: current.ty.clone(),
            init: Some(current),
            mutable: false,
            span,
        });
        stmts.extend(self.lower_stmt_list(body)?);
        Ok(stmts)
    }

    fn adapter_call(&self, step: &AdapterStep, argument: HirExpr, span: Span) -> HirExpr {
        let callee_ty = self
            .lookup(&step.binding)
            .unwrap_or_else(|| HirType::Function {
                params: vec![argument.ty.clone()],
                ret: Box::new(step.result.clone()),
            });
        HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(variable(&step.binding, callee_ty, span)),
                args: vec![argument],
            },
            step.result.clone(),
            span,
        )
    }

    /// Lower `for v in a..b` wearing an adapter chain.
    ///
    /// The chain's function bindings sit in a scope of their own, wrapped around the
    /// loop, so they are evaluated once and are invisible to everything after it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lower_adapted_for_range(
        &mut self,
        label: &Option<Identifier>,
        index: &Option<Identifier>,
        iterator: &Identifier,
        start: HirExpr,
        end: HirExpr,
        inclusive: bool,
        adapters: &[LoopAdapter],
        body: &[Stmt],
        span: Span,
    ) -> Result<HirStmt, LoweringError> {
        self.in_adapter_scope(|lo| {
            let element_ty = start.ty.clone();
            let plan = lo.plan_loop_adapters(adapters, index, span)?;
            let element = plan.element.clone();
            let loop_body = lo.lower_loop_body_with(label, false, |lo| {
                lo.define(element.clone(), element_ty.clone());
                lo.apply_loop_adapters(&plan, &element_ty, index, iterator, body, span)
            })?;
            let loop_stmt = HirStmt::ForRange {
                label: label.as_ref().map(|l| l.name.clone()),
                index: None,
                iterator: plan.element.clone(),
                start,
                end,
                inclusive,
                body: loop_body,
                span,
            };
            Ok(Self::adapted_loop(plan, loop_stmt, span))
        })
    }

    /// Lower `for x in xs` over a counted sequence wearing an adapter chain.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lower_adapted_for_each(
        &mut self,
        label: &Option<Identifier>,
        index: &Option<Identifier>,
        iterator: &Identifier,
        iterable: HirExpr,
        element_ty: HirType,
        adapters: &[LoopAdapter],
        body: &[Stmt],
        span: Span,
    ) -> Result<HirStmt, LoweringError> {
        self.in_adapter_scope(|lo| {
            let plan = lo.plan_loop_adapters(adapters, index, span)?;
            let element = plan.element.clone();
            let loop_body = lo.lower_loop_body_with(label, false, |lo| {
                lo.define(element.clone(), element_ty.clone());
                lo.apply_loop_adapters(&plan, &element_ty, index, iterator, body, span)
            })?;
            let loop_stmt = HirStmt::ForEach {
                label: label.as_ref().map(|l| l.name.clone()),
                index: None,
                iterator: plan.element.clone(),
                iterable,
                body: loop_body,
                span,
            };
            Ok(Self::adapted_loop(plan, loop_stmt, span))
        })
    }

    /// Run `build` in a scope of its own, popped even when it fails, so the bindings
    /// an abandoned lowering defined cannot leak into the statements after it.
    fn in_adapter_scope(
        &mut self,
        build: impl FnOnce(&mut Self) -> Result<HirStmt, LoweringError>,
    ) -> Result<HirStmt, LoweringError> {
        self.push_scope();
        let lowered = build(self);
        self.pop_scope();
        lowered
    }

    /// Wrap a lowered loop in the bindings its adapter chain needs.
    pub(crate) fn adapted_loop(plan: AdapterPlan, loop_stmt: HirStmt, span: Span) -> HirStmt {
        let mut stmts = plan.prelude;
        stmts.push(loop_stmt);
        HirStmt::Expr(HirExpr::new(
            HirExprKind::Block { stmts },
            HirType::Void,
            span,
        ))
    }
}

/// The method spelling of an adapter, as a lowering diagnostic names it.
fn adapter_name(kind: LoopAdapterKind) -> &'static str {
    match kind {
        LoopAdapterKind::Map => "map",
        LoopAdapterKind::Filter => "filter",
    }
}

fn variable(name: &str, ty: HirType, span: Span) -> HirExpr {
    HirExpr::new(HirExprKind::Variable(name.to_string()), ty, span)
}

fn int_literal(value: i64, span: Span) -> HirExpr {
    HirExpr::new(
        HirExprKind::Literal(Literal::Integer(value as i128, None)),
        LOOP_INDEX_TYPE,
        span,
    )
}
