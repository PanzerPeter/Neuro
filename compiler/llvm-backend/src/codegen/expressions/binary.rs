// Codegen for expressions: Binary operators: arithmetic, comparison, logical, string equality.

use ast_types::BinaryOp;
use inkwell::intrinsics::Intrinsic;
use inkwell::values::*;
use inkwell::{FloatPredicate, IntPredicate};
use neuro_hir::HirExpr;

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::type_mapping::TypeMapper;
use crate::types::Type;

impl<'ctx> CodegenContext<'ctx> {
    /// Compare two string fat-pointers for byte-level equality.
    ///
    /// Uses the length field for an O(1) short-circuit before falling back to
    /// `memcmp`. When lengths differ the length passed to `memcmp` is set to 0
    /// (memcmp with n=0 returns 0), so `len_eq` being false drives the final AND
    /// to false without reading out-of-bounds memory.
    pub(crate) fn codegen_string_eq(
        &self,
        lhs: BasicValueEnum<'ctx>,
        rhs: BasicValueEnum<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        let lhs_struct = lhs.into_struct_value();
        let rhs_struct = rhs.into_struct_value();

        let ptr1 = self
            .builder
            .build_extract_value(lhs_struct, 0, "s1.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let len1 = self
            .builder
            .build_extract_value(lhs_struct, 1, "s1.len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();

        let ptr2 = self
            .builder
            .build_extract_value(rhs_struct, 0, "s2.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let len2 = self
            .builder
            .build_extract_value(rhs_struct, 1, "s2.len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();

        let len_eq = self
            .builder
            .build_int_compare(IntPredicate::EQ, len1, len2, "len_eq")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let zero_len = self.context.i64_type().const_int(0, false);
        let cmp_len = self
            .builder
            .build_select(len_eq, len1, zero_len, "cmp_len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let memcmp_fn = self.get_or_declare_memcmp();
        let call = self
            .builder
            .build_call(
                memcmp_fn,
                &[ptr1.into(), ptr2.into(), cmp_len.into()],
                "memcmp_res",
            )
            .map_err(|e| CodegenError::LlvmError(format!("failed to call memcmp: {}", e)))?;

        let memcmp_val = call
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("memcmp returned void".to_string()))?
            .into_int_value();

        let content_eq = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                memcmp_val,
                self.context.i32_type().const_int(0, false),
                "content_eq",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder
            .build_and(len_eq, content_eq, "str_eq")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// Concatenate two string fat-pointers into a new owned `string` (§2.7).
    ///
    /// Allocates `len1 + len2` bytes via `malloc` and copies both operands' bytes
    /// in with `memcpy`, returning a fresh `{ ptr, len }`. The result is a new,
    /// immutable, heap-backed string; the operands are read, not consumed. The
    /// buffer has no null terminator (consistent with the fat-pointer `len`
    /// contract, §1.5) and is not yet freed — runtime heap strings leak until
    /// `Drop` lands (Phase 1.7); see the alpha memory warning in the README.
    pub(crate) fn codegen_string_concat(
        &self,
        lhs: BasicValueEnum<'ctx>,
        rhs: BasicValueEnum<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let lhs_struct = lhs.into_struct_value();
        let rhs_struct = rhs.into_struct_value();

        let ptr1 = self
            .builder
            .build_extract_value(lhs_struct, 0, "cat.s1.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let len1 = self
            .builder
            .build_extract_value(lhs_struct, 1, "cat.s1.len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let ptr2 = self
            .builder
            .build_extract_value(rhs_struct, 0, "cat.s2.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let len2 = self
            .builder
            .build_extract_value(rhs_struct, 1, "cat.s2.len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();

        let total_len = self
            .builder
            .build_int_add(len1, len2, "cat.len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let malloc_fn = self.get_or_declare_malloc();
        let buf = self
            .builder
            .build_call(malloc_fn, &[total_len.into()], "cat.buf")
            .map_err(|e| CodegenError::LlvmError(format!("failed to call malloc: {}", e)))?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("malloc returned void".to_string()))?
            .into_pointer_value();

        let memcpy_fn = self.get_or_declare_memcpy();
        self.builder
            .build_call(memcpy_fn, &[buf.into(), ptr1.into(), len1.into()], "")
            .map_err(|e| CodegenError::LlvmError(format!("failed to call memcpy: {}", e)))?;

        // SAFETY: `buf` was just allocated with `len1 + len2` bytes, so offsetting
        // by `len1` stays within the allocation; the second copy writes the
        // remaining `len2` bytes starting at that offset.
        let dst2 = unsafe {
            self.builder
                .build_in_bounds_gep(self.context.i8_type(), buf, &[len1], "cat.dst2")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
        };
        self.builder
            .build_call(memcpy_fn, &[dst2.into(), ptr2.into(), len2.into()], "")
            .map_err(|e| CodegenError::LlvmError(format!("failed to call memcpy: {}", e)))?;

        let fat_ptr_type = self.type_mapper.map_type(&Type::String)?.into_struct_type();
        let with_ptr = self
            .builder
            .build_insert_value(fat_ptr_type.get_undef(), buf, 0, "cat.res.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_struct_value();
        let fat_ptr = self
            .builder
            .build_insert_value(with_ptr, total_len, 1, "cat.res")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        Ok(fat_ptr.into_struct_value().into())
    }

    /// Normalize a string operand to its `{ ptr, len }` fat-pointer struct value,
    /// auto-dereferencing a `&string` slice (§2.7) — a borrow lowers to a pointer
    /// to the fat pointer, so it is loaded; an owned `string` is already the struct.
    fn load_string_fatptr(
        &self,
        value: BasicValueEnum<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        match value {
            BasicValueEnum::PointerValue(ptr) => {
                let string_ty = self.type_mapper.map_type(&Type::String)?;
                self.builder
                    .build_load(string_ty, ptr, "deref.str")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))
            }
            other => Ok(other),
        }
    }

    /// Emit a short-circuiting logical `&&` or `||` (§1.4).
    ///
    /// Operands are guaranteed `bool` (i1) by semantic analysis. The LHS is
    /// evaluated first; the RHS is only evaluated on the branch where the result
    /// is not yet decided:
    ///   - `&&`: `if lhs { rhs } else { false }`
    ///   - `||`: `if lhs { true } else { rhs }`
    ///
    /// The two incoming values are merged with a phi node in the merge block.
    fn codegen_short_circuit(
        &mut self,
        left: &HirExpr,
        op: BinaryOp,
        right: &HirExpr,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let parent_fn = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("logical operator outside a function".into())
        })?;

        let bool_ty = self.context.bool_type();

        // Evaluate the LHS in the current block.
        let lhs = self.codegen_expr(left)?.into_int_value();
        // The phi's predecessor for the short-circuit edge is the block we end up
        // in after evaluating the LHS (it may itself have appended blocks).
        let entry_bb = self.builder.get_insert_block().ok_or_else(|| {
            CodegenError::InternalError("no insertion block for logical operator".into())
        })?;

        let rhs_bb = self.context.append_basic_block(parent_fn, "logic.rhs");
        let merge_bb = self.context.append_basic_block(parent_fn, "logic.merge");

        // `&&` evaluates the RHS when the LHS is true; `||` when it is false.
        match op {
            BinaryOp::And => self
                .builder
                .build_conditional_branch(lhs, rhs_bb, merge_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
            _ => self
                .builder
                .build_conditional_branch(lhs, merge_bb, rhs_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
        };

        // RHS block: evaluate the RHS, then branch to merge. Capture the block we
        // actually end up in — RHS codegen may append further blocks (e.g. a nested
        // if-expression) — so the phi uses the current block, not `rhs_bb`.
        self.builder.position_at_end(rhs_bb);
        let rhs = self.codegen_expr(right)?.into_int_value();
        let rhs_end_bb = self.builder.get_insert_block().ok_or_else(|| {
            CodegenError::InternalError("no insertion block after logical RHS".into())
        })?;
        let rhs_terminated = self.current_block_terminated();
        if !rhs_terminated {
            self.builder
                .build_unconditional_branch(merge_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        }

        // The short-circuit constant: `&&` yields false, `||` yields true.
        let short_circuit_val = match op {
            BinaryOp::And => bool_ty.const_int(0, false),
            _ => bool_ty.const_int(1, false),
        };

        self.builder.position_at_end(merge_bb);
        let phi = self
            .builder
            .build_phi(bool_ty, "logic.result")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        // Skip the RHS incoming edge if that block terminated (e.g. RHS diverged).
        if rhs_terminated {
            phi.add_incoming(&[(&short_circuit_val, entry_bb)]);
        } else {
            phi.add_incoming(&[(&short_circuit_val, entry_bb), (&rhs, rhs_end_bb)]);
        }

        Ok(phi.as_basic_value())
    }

    /// Emit integer `+`, `-`, or `*`.
    ///
    /// In debug builds (`-O0`, `overflow_checks` enabled) the operation uses the
    /// LLVM `{s,u}{add,sub,mul}.with.overflow` intrinsic and aborts via `llvm.trap`
    /// when the result overflows, matching the §1.2 rule that debug arithmetic
    /// panics on overflow. In release builds the plain wrapping instruction is
    /// emitted, giving two's-complement wraparound.
    fn codegen_int_arith(
        &mut self,
        op: BinaryOp,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
        unsigned: bool,
        name: &str,
    ) -> CodegenResult<IntValue<'ctx>> {
        if !self.overflow_checks {
            return self.emit_wrapping_int_arith(op, lhs, rhs, name);
        }

        let intrinsic_name = match (op, unsigned) {
            (BinaryOp::Add, false) => "llvm.sadd.with.overflow",
            (BinaryOp::Add, true) => "llvm.uadd.with.overflow",
            (BinaryOp::Subtract, false) => "llvm.ssub.with.overflow",
            (BinaryOp::Subtract, true) => "llvm.usub.with.overflow",
            (BinaryOp::Multiply, false) => "llvm.smul.with.overflow",
            (BinaryOp::Multiply, true) => "llvm.umul.with.overflow",
            _ => return self.emit_wrapping_int_arith(op, lhs, rhs, name),
        };

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

        let agg = self
            .builder
            .build_call(decl, &[lhs.into(), rhs.into()], name)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| {
                CodegenError::InternalError("overflow intrinsic returned void".to_string())
            })?
            .into_struct_value();

        let result = self
            .builder
            .build_extract_value(agg, 0, "arith.res")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let overflowed = self
            .builder
            .build_extract_value(agg, 1, "arith.ovf")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();

        let func = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("arithmetic outside a function".into()))?;
        let trap_bb = self.context.append_basic_block(func, "arith.overflow");
        let cont_bb = self.context.append_basic_block(func, "arith.cont");

        self.builder
            .build_conditional_branch(overflowed, trap_bb, cont_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(trap_bb);
        self.emit_trap()?;
        self.builder
            .build_unreachable()
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(cont_bb);
        Ok(result)
    }

    /// Emit the plain wrapping integer instruction (release-build path).
    fn emit_wrapping_int_arith(
        &self,
        op: BinaryOp,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
        name: &str,
    ) -> CodegenResult<IntValue<'ctx>> {
        let value = match op {
            BinaryOp::Add => self.builder.build_int_add(lhs, rhs, name),
            BinaryOp::Subtract => self.builder.build_int_sub(lhs, rhs, name),
            BinaryOp::Multiply => self.builder.build_int_mul(lhs, rhs, name),
            _ => {
                return Err(CodegenError::InternalError(
                    "emit_wrapping_int_arith called with a non-arithmetic operator".to_string(),
                ))
            }
        };
        value.map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// Emit a call to `llvm.trap`, which terminates the process on execution.
    fn emit_trap(&self) -> CodegenResult<()> {
        let trap = Intrinsic::find("llvm.trap")
            .ok_or_else(|| CodegenError::InternalError("missing llvm.trap intrinsic".into()))?;
        let decl = trap
            .get_declaration(&self.module, &[])
            .ok_or_else(|| CodegenError::InternalError("could not declare llvm.trap".into()))?;
        self.builder
            .build_call(decl, &[], "")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Generate code for a binary expression
    pub(crate) fn codegen_binary(
        &mut self,
        left: &HirExpr,
        op: BinaryOp,
        right: &HirExpr,
        left_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        // `&&` and `||` short-circuit (§1.4): the RHS must only be evaluated when
        // the LHS does not already decide the result. This requires branching, so
        // it is handled before the eager operand evaluation below.
        if matches!(op, BinaryOp::And | BinaryOp::Or) {
            return self.codegen_short_circuit(left, op, right);
        }

        let lhs = self.codegen_expr(left)?;
        let rhs = self.codegen_expr(right)?;

        // String equality (§2.7) compares UTF-8 bytes. Either operand may be an
        // owned `string` (a `{ ptr, len }` struct value) or a `&string` slice (a
        // pointer to one); each is normalized to the fat-pointer struct before the
        // byte compare. Handled before the numeric coercion below, which assumes a
        // scalar left type and would mis-coerce a borrowed (pointer) operand.
        if matches!(op, BinaryOp::Equal | BinaryOp::NotEqual)
            && matches!(left_ty.referent(), Type::String)
        {
            let lhs_str = self.load_string_fatptr(lhs)?;
            let rhs_str = self.load_string_fatptr(rhs)?;
            let eq = self.codegen_string_eq(lhs_str, rhs_str)?;
            return match op {
                BinaryOp::Equal => Ok(eq.into()),
                _ => Ok(self
                    .builder
                    .build_not(eq, "str_ne")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                    .into()),
            };
        }

        // String concatenation (§2.7): `string + string` allocates a fresh heap
        // buffer and copies both operands' bytes in. Either operand may be owned or
        // a `&string` slice, so each is normalized to its fat pointer first. Handled
        // before the numeric coercion below, which assumes scalar operands.
        if matches!(op, BinaryOp::Add) && matches!(left_ty.referent(), Type::String) {
            let lhs_str = self.load_string_fatptr(lhs)?;
            let rhs_str = self.load_string_fatptr(rhs)?;
            return self.codegen_string_concat(lhs_str, rhs_str);
        }

        // Coerce both operands to the left-operand semantic type.  Literals always
        // emit at their default width (i32 / f64); when the expression context is
        // wider (e.g. `i64_var - 3000000000`) both sides must match before any
        // arithmetic or comparison instruction is emitted.  The coercion is a no-op
        // when the LLVM types already agree.
        let target_llvm = self.type_mapper.map_type(left_ty)?;
        let lhs = self.coerce_if_needed(lhs, target_llvm, left_ty)?;
        let rhs = self.coerce_if_needed(rhs, target_llvm, left_ty)?;

        match op {
            // Arithmetic operators
            BinaryOp::Add => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_add(lhs.into_float_value(), rhs.into_float_value(), "addtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    let unsigned = TypeMapper::is_unsigned_int(left_ty);
                    Ok(self
                        .codegen_int_arith(
                            BinaryOp::Add,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            unsigned,
                            "addtmp",
                        )?
                        .into())
                }
            }
            BinaryOp::Subtract => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_sub(lhs.into_float_value(), rhs.into_float_value(), "subtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    let unsigned = TypeMapper::is_unsigned_int(left_ty);
                    Ok(self
                        .codegen_int_arith(
                            BinaryOp::Subtract,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            unsigned,
                            "subtmp",
                        )?
                        .into())
                }
            }
            BinaryOp::Multiply => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_mul(lhs.into_float_value(), rhs.into_float_value(), "multmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    let unsigned = TypeMapper::is_unsigned_int(left_ty);
                    Ok(self
                        .codegen_int_arith(
                            BinaryOp::Multiply,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            unsigned,
                            "multmp",
                        )?
                        .into())
                }
            }
            BinaryOp::Divide => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_div(lhs.into_float_value(), rhs.into_float_value(), "divtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_unsigned_int(left_ty) {
                    // Unsigned integer division
                    Ok(self
                        .builder
                        .build_int_unsigned_div(
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "divtmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    // Signed integer division
                    Ok(self
                        .builder
                        .build_int_signed_div(lhs.into_int_value(), rhs.into_int_value(), "divtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::Modulo => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_rem(lhs.into_float_value(), rhs.into_float_value(), "modtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_unsigned_int(left_ty) {
                    // Unsigned integer modulo
                    Ok(self
                        .builder
                        .build_int_unsigned_rem(
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "modtmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    // Signed integer modulo
                    Ok(self
                        .builder
                        .build_int_signed_rem(lhs.into_int_value(), rhs.into_int_value(), "modtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }

            // Comparison operators (string equality handled above the coercion)
            BinaryOp::Equal => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_compare(
                            FloatPredicate::OEQ,
                            lhs.into_float_value(),
                            rhs.into_float_value(),
                            "eqtmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::EQ,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "eqtmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::NotEqual => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_compare(
                            FloatPredicate::ONE,
                            lhs.into_float_value(),
                            rhs.into_float_value(),
                            "netmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::NE,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "netmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::Less => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_compare(
                            FloatPredicate::OLT,
                            lhs.into_float_value(),
                            rhs.into_float_value(),
                            "lttmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_unsigned_int(left_ty) {
                    // Unsigned less than comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::ULT,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "lttmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    // Signed less than comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::SLT,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "lttmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::Greater => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_compare(
                            FloatPredicate::OGT,
                            lhs.into_float_value(),
                            rhs.into_float_value(),
                            "gttmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_unsigned_int(left_ty) {
                    // Unsigned greater than comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::UGT,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "gttmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    // Signed greater than comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::SGT,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "gttmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::LessEqual => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_compare(
                            FloatPredicate::OLE,
                            lhs.into_float_value(),
                            rhs.into_float_value(),
                            "letmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_unsigned_int(left_ty) {
                    // Unsigned less than or equal comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::ULE,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "letmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    // Signed less than or equal comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::SLE,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "letmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::GreaterEqual => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_compare(
                            FloatPredicate::OGE,
                            lhs.into_float_value(),
                            rhs.into_float_value(),
                            "getmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_unsigned_int(left_ty) {
                    // Unsigned greater than or equal comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::UGE,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "getmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    // Signed greater than or equal comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::SGE,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "getmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }

            // Logical `&&`/`||` short-circuit and are handled at the top of this
            // function before operands are evaluated; they never reach this match.
            BinaryOp::And | BinaryOp::Or => Err(CodegenError::InternalError(
                "logical operator reached eager binary match; should short-circuit".into(),
            )),

            // Bitwise operators
            BinaryOp::BitAnd => Ok(self
                .builder
                .build_and(lhs.into_int_value(), rhs.into_int_value(), "bandtmp")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into()),
            BinaryOp::BitOr => Ok(self
                .builder
                .build_or(lhs.into_int_value(), rhs.into_int_value(), "bortmp")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into()),
            BinaryOp::BitXor => Ok(self
                .builder
                .build_xor(lhs.into_int_value(), rhs.into_int_value(), "xortmp")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into()),
            BinaryOp::Shl => Ok(self
                .builder
                .build_left_shift(lhs.into_int_value(), rhs.into_int_value(), "shltmp")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into()),
            // Gated upstream by the type checker (OperatorNotYetSupported); reaching codegen
            // means semantic analysis was skipped — surface that as an ICE rather than panic.
            BinaryOp::NullCoalesce => Err(CodegenError::InternalError(
                "operator '??' reached codegen; semantic analysis must reject it (Phase 2 feature)"
                    .into(),
            )),
        }
    }
}
