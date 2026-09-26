// Codegen for elementwise math: `.exp()`, `.log()`, `.sqrt()`, `.tanh()`, `.abs()`,
// `.pow(p)`, and the `sign` the derivative of `.abs()` is written with.
//
// A scalar is one application of the function. A tensor is one counted loop over its flat
// buffer writing a fresh result, the walk `.map` does with the function inlined, so the
// receiver is read and never consumed. Each function is the LLVM intrinsic of its name,
// which becomes one instruction (`sqrt`, `fabs`) or a call into the C math library the
// driver links; `sign` has no intrinsic and is two compares.
//
// A half-precision element is widened to `f32` for the function and narrowed back. The
// intrinsics' `half` / `bfloat` overloads are not ones every target can lower, and the
// widened computation rounds once, on the way back.

use inkwell::intrinsics::Intrinsic;
use inkwell::values::{BasicValueEnum, FloatValue};
use inkwell::{FloatPredicate, IntPredicate};
use neuro_hir::{HirExpr, HirMathOp};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

impl<'ctx> CodegenContext<'ctx> {
    /// Lower one math call: the function's value for a scalar, a fresh tensor for a tensor.
    pub(crate) fn codegen_math(
        &mut self,
        op: HirMathOp,
        operand: &HirExpr,
        exponent: Option<&HirExpr>,
        result_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        if !matches!(result_ty, Type::Tensor { .. }) {
            let value = self.codegen_float(operand)?;
            let exponent = exponent.map(|e| self.codegen_float(e)).transpose()?;
            return Ok(self.apply_math(op, value, exponent)?.into());
        }

        let source = self.walk_tensor(operand)?;
        let count = self.element_count(operand)?;
        // Evaluated once, after the receiver, which is the order the call is written in.
        let exponent = exponent.map(|e| self.codegen_float(e)).transpose()?;
        let handle = self.alloc_dlpack_tensor(result_ty, "tensor.math")?;
        let written = self.load_dlpack_data(handle)?;

        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("elementwise math outside a function".to_string())
        })?;
        let i64_type = self.context.i64_type();
        let cursor = self.entry_alloca(i64_type, "tensor.math.i")?;
        self.builder.build_store(cursor, i64_type.const_zero())?;
        let head = self
            .context
            .append_basic_block(function, "tensor.math.head");
        let body = self
            .context
            .append_basic_block(function, "tensor.math.body");
        let done = self
            .context
            .append_basic_block(function, "tensor.math.done");

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let index = self
            .builder
            .build_load(i64_type, cursor, "tensor.math.idx")?
            .into_int_value();
        let more = self.builder.build_int_compare(
            IntPredicate::ULT,
            index,
            i64_type.const_int(count as u64, false),
            "tensor.math.more",
        )?;
        self.builder.build_conditional_branch(more, body, done)?;

        self.builder.position_at_end(body);
        let element = self.load_walked(&source, index)?.into_float_value();
        let value = self.apply_math(op, element, exponent)?;
        let slot = self.buffer_slot(source.elem_llvm, written, index)?;
        self.builder.build_store(slot, value)?;
        let next =
            self.builder
                .build_int_add(index, i64_type.const_int(1, false), "tensor.math.next")?;
        self.builder.build_store(cursor, next)?;
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(done);
        self.release_receiver_temporary(operand, source.handle)?;
        Ok(handle.into())
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
                ))
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
