//! `thread_id` and `block_id`, the grid positions a `@kernel` body reads.

use ast_types::Expr;
use neuro_hir::{HirExprKind, HirGridIndex};

use crate::Lowerer;

/// The grid axes in field order: `.x` is axis 0.
const GRID_AXES: [&str; 3] = ["x", "y", "z"];

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
}
