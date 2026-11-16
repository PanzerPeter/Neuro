//! `T::member(args)` inside a generic instance: an associated function reached through a
//! bound on `T`.
//!
//! The instance knows `T`, so the call is the type argument's own function. A scalar has
//! no `impl` to call: the three it gets from the numeric families (`Numeric::zero`,
//! `Numeric::one`, `Float::from_f64`) are a literal and a cast.

use ast_types::Expr;
use neuro_hir::{HirExpr, HirExprKind, HirType};
use shared_types::{Identifier, Literal, Span};

use crate::{Lowerer, LoweringError};

impl Lowerer {
    pub(crate) fn lower_bound_assoc_call(
        &mut self,
        param: &str,
        member: &Identifier,
        type_args: &[ast_types::GenericArg],
        args: &[Expr],
        expected: Option<&HirType>,
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let concrete =
            self.type_subst
                .get(param)
                .cloned()
                .ok_or_else(|| LoweringError::UnresolvedCall {
                    target: format!("{param}::{}", member.name),
                })?;
        if let Some(call) = self.numeric_family_call(&concrete, &member.name, args, span)? {
            return Ok(call);
        }
        let name = match &concrete {
            HirType::Struct(name) | HirType::Enum(name) | HirType::Newtype { name, .. } => {
                name.clone()
            }
            _ => {
                return Err(LoweringError::UnresolvedCall {
                    target: format!("{concrete}::{}", member.name),
                });
            }
        };
        let callee = Expr::Path {
            type_name: Identifier {
                name,
                span: member.span,
            },
            member: member.clone(),
            span,
        };
        self.lower_call(&callee, type_args, args, expected, span)
    }

    /// `zero()`, `one()` or `from_f64(x)` on a scalar of the family that declares it, or
    /// `None` for anything else.
    fn numeric_family_call(
        &mut self,
        ty: &HirType,
        member: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<Option<HirExpr>, LoweringError> {
        let float = matches!(
            ty,
            HirType::F16 | HirType::BF16 | HirType::F32 | HirType::F64
        );
        let integer = matches!(
            ty,
            HirType::I8
                | HirType::I16
                | HirType::I32
                | HirType::I64
                | HirType::U8
                | HirType::U16
                | HirType::U32
                | HirType::U64
        );
        let literal = |value: u8| {
            let lit = if float {
                Literal::Float(f64::from(value), None)
            } else {
                Literal::Integer(i128::from(value), None)
            };
            HirExpr::new(HirExprKind::Literal(lit), ty.clone(), span)
        };
        Ok(match member {
            "zero" if float || integer => Some(literal(0)),
            "one" if float || integer => Some(literal(1)),
            "from_f64" if float => {
                let value = match args {
                    [value] => self.lower_expr(value, Some(&HirType::F64))?,
                    _ => {
                        return Err(LoweringError::UnresolvedCall {
                            target: format!("{ty}::from_f64"),
                        });
                    }
                };
                Some(HirExpr::new(
                    HirExprKind::Cast {
                        value: Box::new(value),
                    },
                    ty.clone(),
                    span,
                ))
            }
            _ => None,
        })
    }
}
