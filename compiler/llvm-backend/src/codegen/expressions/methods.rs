// Neuro Programming Language - LLVM Backend
// Codegen for expressions: Builtin method calls and integer intrinsics.

use ast_types::*;
use inkwell::intrinsics::Intrinsic;
use inkwell::values::*;
use inkwell::IntPredicate;

use crate::codegen::context::{BuiltinMethod, CodegenContext};
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

impl<'ctx> CodegenContext<'ctx> {
    /// Lower a compiler-known intrinsic method call on a builtin receiver. `recv_ty` is the
    /// receiver's type as resolved during the type pass — used for the integer intrinsics'
    /// signedness rather than `expr_types`, which an enclosing cast can clobber.
    pub(crate) fn codegen_builtin_method(
        &mut self,
        kind: BuiltinMethod,
        recv_ty: &Type,
        receiver: &Expr,
        args: &[Expr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        match kind {
            // `string.len()` reads field 1 of the fat pointer `{ ptr, i64 }` — the
            // stored byte length. O(1), no scan; the value is already the u64 length.
            BuiltinMethod::StringLen => {
                let recv_val = self.codegen_expr(receiver)?;
                let struct_val = match recv_val {
                    BasicValueEnum::StructValue(sv) => sv,
                    other => {
                        return Err(CodegenError::InternalError(format!(
                            "string receiver did not lower to a fat pointer: {:?}",
                            other
                        )))
                    }
                };
                self.builder
                    .build_extract_value(struct_val, 1, "str.len")
                    .map_err(|e| {
                        CodegenError::LlvmError(format!("failed to extract string length: {}", e))
                    })
            }
            // `string.clone()` (§2.7) — explicit deep copy of an owned string.
            // String literals live in immutable `.rodata` and no heap-backed string type
            // exists yet (Phase 1.7), so duplicating the `{ ptr, len }` fat-pointer value is
            // observationally a deep copy: the pointee bytes are immutable and shared safely.
            // When runtime heap strings land this must duplicate the underlying buffer.
            BuiltinMethod::StringClone => self.codegen_expr(receiver),
            // `struct.clone()` (§2.3) — structs are stack-allocated aggregates with no heap
            // backing yet, so loading the receiver's value is a faithful deep copy. When a
            // struct gains a heap-owning field this must recurse into that field's clone.
            BuiltinMethod::StructClone => self.codegen_expr(receiver),
            BuiltinMethod::WrappingAdd
            | BuiltinMethod::WrappingSub
            | BuiltinMethod::WrappingMul
            | BuiltinMethod::SaturatingAdd
            | BuiltinMethod::SaturatingSub
            | BuiltinMethod::SaturatingMul
            | BuiltinMethod::Shr => self.codegen_int_intrinsic(kind, recv_ty, receiver, args),
        }
    }

    /// Lower an integer intrinsic (`wrapping_*`, `saturating_*`, `.shr`) on a builtin
    /// integer receiver. Signedness is read from the receiver's recorded type, selecting
    /// the arithmetic (`ashr`) vs. logical (`lshr`) shift and the `s`/`u` saturating path.
    fn codegen_int_intrinsic(
        &mut self,
        kind: BuiltinMethod,
        recv_ty: &Type,
        receiver: &Expr,
        args: &[Expr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let unsigned = recv_ty.is_unsigned_int();

        // Both operands must share the receiver's exact integer type. The arg literal can
        // arrive widened (the backend's literal default is i32), so coerce it down/up to the
        // receiver type — semantic analysis has already proven the two types compatible.
        let target_llvm = self.type_mapper.map_type(recv_ty)?;
        let lhs_raw = self.codegen_expr(receiver)?;
        let lhs = self
            .coerce_if_needed(lhs_raw, target_llvm, recv_ty)?
            .into_int_value();
        let rhs_expr = args.first().ok_or_else(|| {
            CodegenError::InternalError("integer intrinsic is missing its argument".into())
        })?;
        let rhs_raw = self.codegen_expr(rhs_expr)?;
        let rhs = self
            .coerce_if_needed(rhs_raw, target_llvm, recv_ty)?
            .into_int_value();

        let value = match kind {
            BuiltinMethod::WrappingAdd => self
                .builder
                .build_int_add(lhs, rhs, "wrap.add")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
            BuiltinMethod::WrappingSub => self
                .builder
                .build_int_sub(lhs, rhs, "wrap.sub")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
            BuiltinMethod::WrappingMul => self
                .builder
                .build_int_mul(lhs, rhs, "wrap.mul")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
            BuiltinMethod::Shr => self
                .builder
                .build_right_shift(lhs, rhs, !unsigned, "shr")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
            BuiltinMethod::SaturatingAdd | BuiltinMethod::SaturatingSub => {
                let intrinsic_name = match (kind, unsigned) {
                    (BuiltinMethod::SaturatingAdd, false) => "llvm.sadd.sat",
                    (BuiltinMethod::SaturatingAdd, true) => "llvm.uadd.sat",
                    (BuiltinMethod::SaturatingSub, false) => "llvm.ssub.sat",
                    (BuiltinMethod::SaturatingSub, true) => "llvm.usub.sat",
                    _ => unreachable!("guarded by the outer match"),
                };
                self.emit_saturating_sat_intrinsic(intrinsic_name, lhs, rhs)?
            }
            BuiltinMethod::SaturatingMul => self.emit_saturating_mul(lhs, rhs, unsigned)?,
            BuiltinMethod::StringLen | BuiltinMethod::StringClone | BuiltinMethod::StructClone => {
                unreachable!("string/struct intrinsics are handled by codegen_builtin_method")
            }
        };

        Ok(value.into())
    }

    /// Emit a saturating add/sub via the overloaded `llvm.{s,u}{add,sub}.sat` intrinsic,
    /// which returns the already-clamped result directly.
    fn emit_saturating_sat_intrinsic(
        &self,
        intrinsic_name: &str,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        let int_ty = lhs.get_type();
        let intrinsic = Intrinsic::find(intrinsic_name).ok_or_else(|| {
            CodegenError::InternalError(format!("missing LLVM intrinsic {intrinsic_name}"))
        })?;
        let decl = intrinsic
            .get_declaration(&self.module, &[int_ty.into()])
            .ok_or_else(|| {
                CodegenError::InternalError(format!(
                    "could not declare LLVM intrinsic {intrinsic_name}"
                ))
            })?;
        Ok(self
            .builder
            .build_call(decl, &[lhs.into(), rhs.into()], "sat")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| {
                CodegenError::InternalError("saturating intrinsic returned void".to_string())
            })?
            .into_int_value())
    }

    /// Emit a saturating multiply. LLVM has no plain saturating-mul intrinsic, so this
    /// uses `{s,u}mul.with.overflow` and selects the saturation bound on overflow: unsigned
    /// clamps to MAX; signed clamps to MIN when the true product is negative (operand signs
    /// differ) and to MAX otherwise.
    fn emit_saturating_mul(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
        unsigned: bool,
    ) -> CodegenResult<IntValue<'ctx>> {
        let int_ty = lhs.get_type();
        let intrinsic_name = if unsigned {
            "llvm.umul.with.overflow"
        } else {
            "llvm.smul.with.overflow"
        };
        let intrinsic = Intrinsic::find(intrinsic_name).ok_or_else(|| {
            CodegenError::InternalError(format!("missing LLVM intrinsic {intrinsic_name}"))
        })?;
        let decl = intrinsic
            .get_declaration(&self.module, &[int_ty.into()])
            .ok_or_else(|| {
                CodegenError::InternalError(format!(
                    "could not declare LLVM intrinsic {intrinsic_name}"
                ))
            })?;

        let agg = self
            .builder
            .build_call(decl, &[lhs.into(), rhs.into()], "smul")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| {
                CodegenError::InternalError("mul-overflow intrinsic returned void".to_string())
            })?
            .into_struct_value();
        let result = self
            .builder
            .build_extract_value(agg, 0, "smul.res")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let overflowed = self
            .builder
            .build_extract_value(agg, 1, "smul.ovf")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();

        let width = int_ty.get_bit_width();
        let umax = int_ty.const_all_ones();
        let bound = if unsigned {
            umax
        } else {
            // Signed bounds for width N: MAX = 2^(N-1) - 1, MIN = -2^(N-1) (bit pattern 1<<(N-1)).
            let smax = ((1u128 << (width - 1)) - 1) as u64;
            let smin = 1u64 << (width - 1);
            let smax_val = int_ty.const_int(smax, false);
            let smin_val = int_ty.const_int(smin, false);
            let zero = int_ty.const_zero();
            let signs = self
                .builder
                .build_xor(lhs, rhs, "smul.signs")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            let negative = self
                .builder
                .build_int_compare(IntPredicate::SLT, signs, zero, "smul.neg")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.builder
                .build_select(negative, smin_val, smax_val, "smul.bound")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_int_value()
        };

        Ok(self
            .builder
            .build_select(overflowed, bound, result, "sat.mul")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value())
    }
}
