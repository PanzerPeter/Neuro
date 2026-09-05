// Codegen for expressions: Unary operators.

use ast_types::{BinaryOp, UnaryOp};
use inkwell::values::*;
use neuro_hir::{HirExpr, HirExprKind};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::type_mapping::TypeMapper;
use crate::types::Type;

impl<'ctx> CodegenContext<'ctx> {
    /// Generate code for a unary expression.
    ///
    /// `offset` keys the panic diagnostic's source location for the integer-negation
    /// overflow guard.
    pub(crate) fn codegen_unary(
        &mut self,
        op: UnaryOp,
        operand: &HirExpr,
        operand_ty: &Type,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        // A negation directly over an integer literal is a constant, and the checker
        // has already range-checked it against the value it DENOTES — so it is in range
        // for its type by the time codegen runs, and there is nothing to guard. It must
        // be materialized rather than computed: the most negative value of a signed type
        // is written as a magnitude one past that type's maximum, which narrows to the
        // target width as MIN's own bit pattern, and `0 - MIN` overflows.
        if let (
            UnaryOp::Negate,
            HirExprKind::Literal(shared_types::Literal::Integer(magnitude, suffix)),
        ) = (op, &operand.kind)
        {
            let negated = shared_types::Literal::Integer(magnitude.wrapping_neg(), *suffix);
            return self.codegen_literal(&negated, operand_ty);
        }

        let val = self.codegen_expr(operand)?;

        match op {
            UnaryOp::Negate => {
                if TypeMapper::is_float_type(operand_ty) {
                    return Ok(self
                        .builder
                        .build_float_neg(val.into_float_value(), "negtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into());
                }
                // Integer negation IS `0 - x`, so it overflows exactly where that
                // subtraction does: at a signed type's `MIN`, and at every nonzero
                // value of an unsigned type. Routing it through the same guard is what
                // makes `-x` and `0 - x` agree — emitted separately, `build_int_neg`
                // wrapped silently on the debug tier while the subtraction panicked.
                let int_val = val.into_int_value();
                let zero = int_val.get_type().const_zero();
                let unsigned = TypeMapper::is_unsigned_int(operand_ty);
                Ok(self
                    .codegen_int_arith(
                        BinaryOp::Subtract,
                        zero,
                        int_val,
                        unsigned,
                        offset,
                        "negtmp",
                    )?
                    .into())
            }
            UnaryOp::Not => Ok(self
                .builder
                .build_not(val.into_int_value(), "nottmp")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into()),
            UnaryOp::BitNot => Ok(self
                .builder
                .build_not(val.into_int_value(), "bnottmp")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into()),
        }
    }
}
