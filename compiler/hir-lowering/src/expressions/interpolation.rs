//! Lowering for interpolated string literals.

use ast_types::InterpPart;
use neuro_hir::{HirExpr, HirExprKind, HirInterpPart, HirType};
use shared_types::Span;

use crate::{Lowerer, LoweringError};

impl Lowerer {
    /// Lower each hole to a typed expression, normalizing a spec-less hole to
    /// [`FormatSpec::default`] so backends see one shape for every hole.
    ///
    /// A hole is lowered with no expected type: its own expression decides its
    /// type, and the rendering is chosen from that.
    pub(crate) fn lower_interp_string(
        &mut self,
        parts: &[InterpPart],
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let mut lowered = Vec::with_capacity(parts.len());

        for part in parts {
            match part {
                InterpPart::Text(text) => lowered.push(HirInterpPart::Text(text.clone())),
                InterpPart::Formatted { expr, spec, .. } => {
                    lowered.push(HirInterpPart::Formatted {
                        expr: self.lower_expr(expr, None)?,
                        spec: spec.clone().unwrap_or_default(),
                    })
                }
            }
        }

        Ok(HirExpr::new(
            HirExprKind::InterpString { parts: lowered },
            HirType::String,
            span,
        ))
    }
}
