// Codegen for expressions: Unary operators.

use ast_types::UnaryOp;
use inkwell::values::*;
use neuro_hir::HirExpr;

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::type_mapping::TypeMapper;
use crate::types::Type;

impl<'ctx> CodegenContext<'ctx> {
    /// Generate code for a unary expression
    pub(crate) fn codegen_unary(
        &mut self,
        op: UnaryOp,
        operand: &HirExpr,
        operand_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let val = self.codegen_expr(operand)?;

        match op {
            UnaryOp::Negate => {
                if TypeMapper::is_float_type(operand_ty) {
                    Ok(self
                        .builder
                        .build_float_neg(val.into_float_value(), "negtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_int_neg(val.into_int_value(), "negtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
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
