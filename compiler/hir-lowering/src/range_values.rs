//! A range `start..end` or `start..=end` written as a value, and the two intrinsics the
//! prelude's `Range` walks with.
//!
//! The value is a literal of the prelude struct, so no backend learns ranges exist:
//!
//! ```text
//! a..b    =>  Range<T> { start: a, end: b, inclusive: false, stride: 1,
//!                        reversed: false, done: false }
//! ```
//!
//! The intrinsics are only ever called on an integer, since only this literal builds a
//! `Range`, and become what a body could write if it knew its type:
//!
//! ```text
//! x.__to_u64()   =>  x as u64
//! x.__offset(k)  =>  (x as u64).wrapping_add(k) as T
//! ```

use ast_types::Expr;
use neuro_hir::{HirExpr, HirExprKind, HirFieldInit, HirType};
use shared_types::{Literal, Span};

use crate::{Lowerer, LoweringError, MonoArg, is_integer};

const RANGE_STRUCT: &str = "Range";
const TO_U64_INTRINSIC: &str = "__to_u64";
const OFFSET_INTRINSIC: &str = "__offset";
const WRAPPING_ADD_METHOD: &str = "wrapping_add";

/// An integer literal with no suffix, whose type comes from its context.
fn unsuffixed_integer(expr: &Expr) -> bool {
    matches!(expr, Expr::Literal(Literal::Integer(_, None), _))
}

impl Lowerer {
    /// Lower a range's two bounds as the checker typed them: an unsuffixed literal start
    /// takes the end's type, and otherwise the start leads, typed by `hint` if given.
    pub(crate) fn lower_range_bounds(
        &mut self,
        start: &Expr,
        end: &Expr,
        hint: Option<&HirType>,
    ) -> Result<(HirExpr, HirExpr), LoweringError> {
        if unsuffixed_integer(start) && !unsuffixed_integer(end) {
            let end = self.lower_expr(end, None)?;
            Ok((self.lower_expr(start, Some(&end.ty))?, end))
        } else {
            let start = self.lower_expr(start, hint)?;
            let end = self.lower_expr(end, Some(&start.ty))?;
            Ok((start, end))
        }
    }

    /// Lower a range value to the `Range<T>` literal over its bounds. `expected` is the
    /// type the value is wanted as, whose element type an unsuffixed literal takes.
    pub(crate) fn lower_range_value(
        &mut self,
        start: &Expr,
        end: &Expr,
        inclusive: bool,
        expected: Option<&HirType>,
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let hint = match expected {
            Some(HirType::Struct(name)) => match self.instance_args(name) {
                Some((RANGE_STRUCT, [MonoArg::Type(element)])) => Some(element.clone()),
                _ => None,
            },
            _ => None,
        };
        let (start, end) = self.lower_range_bounds(start, end, hint.as_ref())?;
        let name =
            self.instantiate_generic_struct(RANGE_STRUCT, &[MonoArg::Type(start.ty.clone())])?;
        let flag = |value| {
            HirExpr::new(
                HirExprKind::Literal(Literal::Boolean(value)),
                HirType::Bool,
                span,
            )
        };
        let one = HirExpr::new(
            HirExprKind::Literal(Literal::Integer(1, None)),
            HirType::U64,
            span,
        );
        let fields = [
            ("start", start),
            ("end", end),
            ("inclusive", flag(inclusive)),
            ("stride", one),
            ("reversed", flag(false)),
            ("done", flag(false)),
        ]
        .into_iter()
        .map(|(field, value)| HirFieldInit {
            name: field.to_string(),
            value: Box::new(value),
            span,
        })
        .collect();
        Ok(HirExpr::new(
            HirExprKind::StructLiteral {
                name: name.clone(),
                fields,
                base: None,
            },
            HirType::Struct(name),
            span,
        ))
    }

    /// Lower `object.__to_u64()` or `object.__offset(k)` on an integer, or answer `None`
    /// for every other call.
    pub(crate) fn lower_range_intrinsic(
        &mut self,
        object: &HirExpr,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<Option<HirExpr>, LoweringError> {
        if !matches!(method, TO_U64_INTRINSIC | OFFSET_INTRINSIC) {
            return Ok(None);
        }
        let value = crate::expressions::coercion::auto_deref(object.clone());
        let element = value.ty.clone();
        if !is_integer(&element) {
            return Ok(None);
        }
        let cast = |value: HirExpr, ty: HirType| {
            HirExpr::new(
                HirExprKind::Cast {
                    value: Box::new(value),
                },
                ty,
                span,
            )
        };
        let wide = cast(value, HirType::U64);
        if method == TO_U64_INTRINSIC {
            return Ok(Some(wide));
        }
        let offset = self.lower_args(args, &[HirType::U64])?;
        let callee = HirExpr::new(
            HirExprKind::FieldAccess {
                object: Box::new(wide),
                field: WRAPPING_ADD_METHOD.to_string(),
            },
            HirType::U64,
            span,
        );
        let sum = HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(callee),
                args: offset,
            },
            HirType::U64,
            span,
        );
        Ok(Some(cast(sum, element)))
    }
}
