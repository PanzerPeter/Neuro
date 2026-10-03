// Codegen for elementwise math on a scalar: `.exp()`, `.log()`, `.sqrt()`, `.tanh()`,
// `.abs()`, `.pow(p)`, and the `sign` the derivative of `.abs()` is written with. On a tensor
// each is computed in MLIR.
//
// Each function is the LLVM intrinsic of its name, which becomes one instruction (`sqrt`,
// `fabs`) or a call into the C math library the driver links; `sign` has no intrinsic and is
// two compares.
//
// A half-precision value is widened to `f32` for the function and narrowed back. The
// intrinsics' `half` / `bfloat` overloads are not ones every target can lower, and the
// widened computation rounds once, on the way back.

use inkwell::FloatPredicate;
use inkwell::intrinsics::Intrinsic;
use inkwell::values::{BasicValueEnum, FloatValue};
use neuro_hir::{HirExpr, HirMathOp};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};

impl<'ctx> CodegenContext<'ctx> {
    /// Lower one math call on a scalar.
    pub(crate) fn codegen_math(
        &mut self,
        op: HirMathOp,
        operand: &HirExpr,
        exponent: Option<&HirExpr>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let value = self.codegen_float(operand)?;
        let exponent = exponent.map(|e| self.codegen_float(e)).transpose()?;
        Ok(self.apply_math(op, value, exponent)?.into())
    }

    fn codegen_float(&mut self, expr: &HirExpr) -> CodegenResult<FloatValue<'ctx>> {
        match self.codegen_expr(expr)? {
            BasicValueEnum::FloatValue(value) => Ok(value),
            _ => Err(CodegenError::InternalError(
                "elementwise math on a value that is not a float".to_string(),
            )),
        }
    }

    /// `op` on one float, widening a half-precision one to `f32` around it.
    fn apply_math(
        &mut self,
        op: HirMathOp,
        value: FloatValue<'ctx>,
        exponent: Option<FloatValue<'ctx>>,
    ) -> CodegenResult<FloatValue<'ctx>> {
        let narrow = value.get_type();
        let (f32_type, f64_type) = (self.context.f32_type(), self.context.f64_type());
        let half = narrow != f32_type && narrow != f64_type;
        let widen = |ctx: &Self, v: FloatValue<'ctx>| -> CodegenResult<FloatValue<'ctx>> {
            if !half {
                return Ok(v);
            }
            Ok(ctx.builder.build_float_ext(v, f32_type, "math.widen")?)
        };
        let value = widen(self, value)?;
        let exponent = exponent.map(|e| widen(self, e)).transpose()?;

        let result = match op {
            HirMathOp::Sign => self.float_sign(value)?,
            _ => self.call_float_intrinsic(op, value, exponent)?,
        };
        if !half {
            return Ok(result);
        }
        Ok(self
            .builder
            .build_float_trunc(result, narrow, "math.narrow")?)
    }

    fn call_float_intrinsic(
        &self,
        op: HirMathOp,
        value: FloatValue<'ctx>,
        exponent: Option<FloatValue<'ctx>>,
    ) -> CodegenResult<FloatValue<'ctx>> {
        let name = match op {
            HirMathOp::Exp => "llvm.exp",
            HirMathOp::Log => "llvm.log",
            HirMathOp::Sqrt => "llvm.sqrt",
            HirMathOp::Tanh => "llvm.tanh",
            HirMathOp::Abs => "llvm.fabs",
            HirMathOp::Pow => "llvm.pow",
            HirMathOp::Sign => {
                return Err(CodegenError::InternalError(
                    "`sign` has no intrinsic".to_string(),
                ));
            }
        };
        let intrinsic = Intrinsic::find(name)
            .ok_or_else(|| CodegenError::InternalError(format!("no `{name}` intrinsic")))?;
        let declaration = intrinsic
            .get_declaration(&self.module, &[value.get_type().into()])
            .ok_or_else(|| CodegenError::InternalError(format!("`{name}` has no overload")))?;
        let mut args = vec![value.into()];
        if let Some(exponent) = exponent {
            args.push(exponent.into());
        }
        self.builder
            .build_call(declaration, &args, "math")?
            .try_as_basic_value()
            .basic()
            .map(BasicValueEnum::into_float_value)
            .ok_or_else(|| CodegenError::InternalError(format!("`{name}` returned void")))
    }

    /// `1` above zero, `-1` below, and the value itself otherwise: `0` for either zero and
    /// NaN for NaN, since both compares are ordered and fail on NaN.
    fn float_sign(&self, value: FloatValue<'ctx>) -> CodegenResult<FloatValue<'ctx>> {
        let ty = value.get_type();
        let zero = ty.const_zero();
        let above =
            self.builder
                .build_float_compare(FloatPredicate::OGT, value, zero, "sign.above")?;
        let below =
            self.builder
                .build_float_compare(FloatPredicate::OLT, value, zero, "sign.below")?;
        let negative =
            self.builder
                .build_select(below, ty.const_float(-1.0), value, "sign.below.value")?;
        Ok(self
            .builder
            .build_select(above, ty.const_float(1.0).into(), negative, "sign")?
            .into_float_value())
    }
}
