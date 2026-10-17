//! What only a `@kernel` body has: `thread_id` and `block_id`, the grid positions it
//! reads, and `out.partition(...)`, the write form that hands each thread its own run.

use ast_types::{Expr, Stmt};
use neuro_hir::{HirExpr, HirExprKind, HirGridIndex, HirStmt, HirType};
use shared_types::Span;

use crate::{Lowerer, LoweringError};

/// The grid axes in field order: `.x` is axis 0.
const GRID_AXES: [&str; 3] = ["x", "y", "z"];

/// The safe write form on a `KernelOut` handle.
pub(super) const PARTITION_METHOD: &str = "partition";

impl Lowerer {
    /// `thread_id.x` or `block_id.z` in a kernel body, where no local of that name
    /// shadows it, or `t.x` on a local bound to either whole. The checker has already
    /// refused every other field of either name, and the two names outside a kernel body.
    pub(crate) fn grid_position(&self, object: &Expr, field: &str) -> Option<HirExprKind> {
        let Expr::Identifier(name) = object else {
            return None;
        };
        let of = self.grid_named(&name.name)?;
        let axis = GRID_AXES.iter().position(|axis| *axis == field)?;
        Some(HirExprKind::GridPosition {
            of,
            axis: axis as u8,
        })
    }

    /// The grid position `name` names in a kernel body: `thread_id` or `block_id` where no
    /// local hides it, or a local bound to one whole.
    fn grid_named(&self, name: &str) -> Option<HirGridIndex> {
        if !self.in_kernel {
            return None;
        }
        let local = self
            .scopes
            .iter()
            .rposition(|scope| scope.contains_key(name))
            .map_or(0, |at| at + 1);
        // The deepest alias of a name is its last: a deeper one is gone with its scope
        // before a shallower one can follow it.
        if let Some((at, _, of)) = self.grid_aliases.iter().rev().find(|(_, n, _)| n == name)
            && *at > local
        {
            return Some(*of);
        }
        match name {
            _ if local > 0 => None,
            "thread_id" => Some(HirGridIndex::Thread),
            "block_id" => Some(HirGridIndex::Block),
            _ => None,
        }
    }

    /// `val t = thread_id`, or `val u = t` for such a `t`, in a kernel body: the checker
    /// lets only an immutable, unannotated local name a position whole, and reads it only
    /// by field, so `t` becomes an alias each `t.x` resolves and the binding lowers to
    /// nothing. `None` for any other binding.
    pub(crate) fn grid_binding(
        &mut self,
        name: &str,
        init: Option<&Expr>,
        (mutable, annotated): (bool, bool),
        span: Span,
    ) -> Option<HirStmt> {
        let Some(Expr::Identifier(source)) = init else {
            return None;
        };
        if mutable || annotated {
            return None;
        }
        let of = self.grid_named(&source.name)?;
        if let Some(scope) = self.scopes.last_mut() {
            scope.remove(name);
        }
        let depth = self.scopes.len();
        self.grid_aliases
            .retain(|(at, alias, _)| *at != depth || alias != name);
        self.grid_aliases.push((depth, name.to_string(), of));
        let nothing = HirExprKind::Block { stmts: Vec::new() };
        Some(HirStmt::Expr(HirExpr::new(nothing, HirType::Void, span)))
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
