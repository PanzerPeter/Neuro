//! What only a `@kernel` body has: `thread_id` and `block_id`, the grid positions it
//! reads, and `out.partition(...)`, the write form that hands each thread its own run.

use ast_types::{Expr, Stmt};
use neuro_hir::{HirExpr, HirExprKind, HirGridIndex, HirType};
use shared_types::Span;

use crate::{Lowerer, LoweringError};

/// The grid axes in field order: `.x` is axis 0.
const GRID_AXES: [&str; 3] = ["x", "y", "z"];

/// The safe write form on a `KernelOut` handle.
pub(super) const PARTITION_METHOD: &str = "partition";

impl Lowerer {
    /// `thread_id.x` or `block_id.z` in a kernel body, where no local of that name
    /// shadows it. The checker has already refused every other field of either name, and
    /// the two names outside a kernel body.
    pub(crate) fn grid_position(&self, object: &Expr, field: &str) -> Option<HirExprKind> {
        let Expr::Identifier(name) = object else {
            return None;
        };
        if !self.in_kernel || self.lookup_local(&name.name).is_some() {
            return None;
        }
        let of = match name.name.as_str() {
            "thread_id" => HirGridIndex::Thread,
            "block_id" => HirGridIndex::Block,
            _ => return None,
        };
        let axis = GRID_AXES.iter().position(|axis| *axis == field)?;
        Some(HirExprKind::GridPosition {
            of,
            axis: axis as u8,
        })
    }

    /// `out.partition(|base, slice| { ... })`, `out` already lowered: the closure's body
    /// inlined with its two parameters as locals, since a kernel has nothing to call a
    /// closure with. The checker has already fixed the closure's shape and types.
    pub(super) fn lower_kernel_partition(
        &mut self,
        out: HirExpr,
        args: &[Expr],
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let malformed = || LoweringError::Malformed {
            detail: "`partition` expects a closure of two parameters".to_string(),
        };
        let [Expr::Closure { params, body, .. }] = args else {
            return Err(malformed());
        };
        let [base, slice] = params.as_slice() else {
            return Err(malformed());
        };
        let HirType::Tensor { element, .. } = out.ty.referent() else {
            return Err(malformed());
        };
        let slice_ty = HirType::Reference {
            inner: Box::new(HirType::Slice(element.clone())),
            mutable: true,
        };
        let single;
        let stmts: &[Stmt] = match body.as_ref() {
            Expr::Block { stmts, .. } => stmts,
            other => {
                single = [Stmt::Expr(other.clone())];
                &single
            }
        };

        self.push_scope();
        self.define(base.name.name.clone(), HirType::U64);
        self.define(slice.name.name.clone(), slice_ty);
        // A `break` in the closure cannot reach a loop around the call.
        let saved_loops = std::mem::take(&mut self.loop_stack);
        let lowered = self.lower_body(stmts, &HirType::Void);
        self.loop_stack = saved_loops;
        self.pop_scope();

        let kind = HirExprKind::KernelPartition {
            out: Box::new(out),
            base: base.name.name.clone(),
            slice: slice.name.name.clone(),
            body: lowered?,
        };
        Ok(HirExpr::new(kind, HirType::Void, span))
    }
}
