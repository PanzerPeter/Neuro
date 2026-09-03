// Codegen for expressions: Builtin method calls and integer intrinsics.

use inkwell::intrinsics::Intrinsic;
use inkwell::values::*;
use inkwell::{FloatPredicate, IntPredicate};
use neuro_hir::{HirExpr, HirExprKind};

use crate::codegen::context::{BuiltinMethod, CodegenContext};
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

impl<'ctx> CodegenContext<'ctx> {
    /// Lower a compiler-known intrinsic method call on a builtin receiver. `recv_ty` is the
    /// receiver's resolved HIR type (the call dispatcher maps it from `object.ty`), used for
    /// the integer intrinsics' signedness and the string/array receiver shape; `result_ty`
    /// is the call's resolved result type, which supplies the `Option<T>` instance the
    /// `checked_*` intrinsics build.
    pub(crate) fn codegen_builtin_method(
        &mut self,
        kind: BuiltinMethod,
        recv_ty: &Type,
        result_ty: &Type,
        receiver: &HirExpr,
        args: &[HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        match kind {
            // `string.len()` reads field 1 of the fat pointer `{ ptr, i64 }` — the
            // stored byte length. O(1), no scan; the value is already the u64 length.
            // A `&string` receiver is auto-dereferenced first.
            BuiltinMethod::StringLen => {
                let struct_val = self.string_receiver_struct(receiver)?;
                self.builder
                    .build_extract_value(struct_val, 1, "str.len")
                    .map_err(|e| {
                        CodegenError::LlvmError(format!("failed to extract string length: {}", e))
                    })
            }
            // `string.clone()` — explicit deep copy of an owned string.
            // String literals live in immutable `.rodata` and no heap-backed string type
            // exists yet (Phase 1.7), so duplicating the `{ ptr, len }` fat-pointer value is
            // observationally a deep copy: the pointee bytes are immutable and shared safely.
            // When runtime heap strings land this must duplicate the underlying buffer.
            // A `&string` receiver is auto-dereferenced first.
            BuiltinMethod::StringClone => Ok(self.string_receiver_struct(receiver)?.into()),
            // `string.slice(a..b)` — a borrowed `&string` view into the receiver's
            // UTF-8 bytes, with runtime bounds and codepoint-boundary checks.
            BuiltinMethod::StringSlice => self.codegen_string_slice(receiver, args),
            // `seq.slice(a..b)` — a borrowed `&[T]` view over an array, `Vec`, or slice.
            BuiltinMethod::SequenceSlice => self.codegen_sequence_slice(receiver, args),
            // `slice.len()` — the length word of the fat pointer.
            BuiltinMethod::SliceLen => self.codegen_slice_len(receiver),
            // `string.char_slice(a..b)` — the same borrowed view, located by counting
            // code points instead of bytes.
            BuiltinMethod::StringCharSlice => self.codegen_char_slice(receiver, args),
            // `string.__char_at(offset)` — the decode step behind the prelude's codepoint
            // iterator, and the only byte-indexed read of a string anywhere.
            BuiltinMethod::StringCharAt => self.codegen_char_at(receiver, args),
            // `array.len()` — the static length `N` of `[T; N]`, read from the
            // receiver type recorded by the type pass. A compile-time constant `u64`;
            // the receiver is not evaluated (length is independent of its value).
            BuiltinMethod::ArrayLen => {
                let size = match recv_ty.referent() {
                    Type::Array { size, .. } => *size,
                    other => {
                        return Err(CodegenError::InternalError(format!(
                            "array len receiver is not an array: {:?}",
                            other
                        )))
                    }
                };
                Ok(self.context.i64_type().const_int(size as u64, false).into())
            }
            // `struct.clone()` — structs are stack-allocated aggregates with no heap
            // backing yet, so loading the receiver's value is a faithful deep copy. When a
            // struct gains a heap-owning field this must recurse into that field's clone.
            // A `&Struct` receiver is auto-dereferenced first.
            BuiltinMethod::StructClone => {
                let recv_val = self.codegen_expr(receiver)?;
                match recv_val {
                    BasicValueEnum::PointerValue(ptr) => {
                        let name = match recv_ty.referent() {
                            Type::Struct(n) => n,
                            other => {
                                return Err(CodegenError::InternalError(format!(
                                    "struct clone receiver is not a struct: {:?}",
                                    other
                                )))
                            }
                        };
                        let struct_ty = self.get_struct_llvm_type(name)?;
                        self.builder
                            .build_load(struct_ty, ptr, "deref.struct")
                            .map_err(|e| CodegenError::LlvmError(e.to_string()))
                    }
                    other => Ok(other),
                }
            }
            // `float.is_nan()` — the receiver is the whole computation; no arguments.
            BuiltinMethod::IsNan => self.codegen_is_nan(receiver),
            BuiltinMethod::CheckedAdd | BuiltinMethod::CheckedSub | BuiltinMethod::CheckedMul => {
                self.codegen_checked_int_intrinsic(kind, recv_ty, result_ty, receiver, args)
            }
            BuiltinMethod::WrappingAdd
            | BuiltinMethod::WrappingSub
            | BuiltinMethod::WrappingMul
            | BuiltinMethod::SaturatingAdd
            | BuiltinMethod::SaturatingSub
            | BuiltinMethod::SaturatingMul
            | BuiltinMethod::Shr => self.codegen_int_intrinsic(kind, recv_ty, receiver, args),
        }
    }

    /// Lower `string.slice(a..b)` / `string.slice(a..=b)` to a borrowed `&string`.
    ///
    /// Computes a `(base + start, end - start)` fat pointer into the receiver's UTF-8
    /// data — zero copy, since strings are immutable. Both bounds and the two endpoint
    /// UTF-8 codepoint boundaries are validated at runtime in every build; a violation
    /// routes through the panic runtime (abort, no unwinding). The result `&string` is the
    /// computed fat pointer itself, so it is returned by value like any other aggregate.
    fn codegen_string_slice(
        &mut self,
        receiver: &HirExpr,
        args: &[HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let (start_expr, end_expr, inclusive, offset) = match args.first() {
            Some(HirExpr {
                kind:
                    HirExprKind::Range {
                        start,
                        end,
                        inclusive,
                    },
                span,
                ..
            }) => (start.as_ref(), end.as_ref(), *inclusive, span.start),
            _ => {
                return Err(CodegenError::InternalError(
                    "string.slice reached codegen without a range argument".into(),
                ))
            }
        };

        let fat = self.string_receiver_struct(receiver)?;
        let base_ptr = self
            .builder
            .build_extract_value(fat, 0, "slice.base.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let len = self
            .builder
            .build_extract_value(fat, 1, "slice.len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();

        let i64_ty = self.context.i64_type();
        let start = self.slice_index_to_i64(start_expr)?;
        let raw_end = self.slice_index_to_i64(end_expr)?;
        // `a..=b` covers byte `b`, so the exclusive upper bound is `b + 1`.
        let end = if inclusive {
            self.builder
                .build_int_add(raw_end, i64_ty.const_int(1, false), "slice.incl.end")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
        } else {
            raw_end
        };

        // Bounds: 0 <= start <= end <= len. Signed comparisons so a negative bound is caught.
        let zero = i64_ty.const_zero();
        let start_nonneg = self
            .builder
            .build_int_compare(IntPredicate::SGE, start, zero, "slice.start.nonneg")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let start_le_end = self
            .builder
            .build_int_compare(IntPredicate::SLE, start, end, "slice.start.le.end")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let end_le_len = self
            .builder
            .build_int_compare(IntPredicate::SLE, end, len, "slice.end.le.len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let lower_ok = self
            .builder
            .build_and(start_nonneg, start_le_end, "slice.lower.ok")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let in_bounds = self
            .builder
            .build_and(lower_ok, end_le_len, "slice.in.bounds")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.codegen_guard_or_panic(in_bounds, "string slice out of bounds", offset)?;

        // Both endpoints must land on UTF-8 code-point boundaries.
        let start_aligned = self.slice_boundary_ok(base_ptr, start, len)?;
        let end_aligned = self.slice_boundary_ok(base_ptr, end, len)?;
        let aligned = self
            .builder
            .build_and(start_aligned, end_aligned, "slice.aligned")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.codegen_guard_or_panic(aligned, "string slice splits a UTF-8 code point", offset)?;

        self.string_fat_slice(base_ptr, start, end)
    }

    /// Build the `&string` fat pointer for the byte range `[start, end)` of the string
    /// based at `base_ptr`. Shared by `.slice` and `.char_slice`, which differ only in how
    /// they arrive at the two byte offsets.
    ///
    /// The caller MUST have already guarded `0 <= start <= end <= len`; this function
    /// emits no checks of its own.
    pub(super) fn string_fat_slice(
        &mut self,
        base_ptr: PointerValue<'ctx>,
        start: IntValue<'ctx>,
        end: IntValue<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        // SAFETY: the caller's bounds guard proved `0 <= start <= len`, so offsetting the
        // base pointer by `start` stays within the string's allocation.
        let new_ptr = unsafe {
            self.builder
                .build_in_bounds_gep(self.context.i8_type(), base_ptr, &[start], "slice.ptr")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
        };
        let new_len = self
            .builder
            .build_int_sub(end, start, "slice.newlen")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let fat_ty = self.type_mapper.map_type(&Type::String)?.into_struct_type();
        let with_ptr = self
            .builder
            .build_insert_value(fat_ty.get_undef(), new_ptr, 0, "slice.res.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_struct_value();
        let fat_val = self
            .builder
            .build_insert_value(with_ptr, new_len, 1, "slice.res")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_struct_value();

        // `&string` is the `{ ptr, i64 }` fat pointer by value, so the computed slice is
        // the result — no stack slot, and nothing whose address could outlive this frame.
        Ok(fat_val.into())
    }

    /// Lower a slice bound expression to an `i64` index, sign-extending or truncating a
    /// differently sized integer (a bare literal defaults to `i32` in the backend).
    pub(super) fn slice_index_to_i64(&mut self, expr: &HirExpr) -> CodegenResult<IntValue<'ctx>> {
        let value = self.codegen_expr(expr)?.into_int_value();
        let i64_ty = self.context.i64_type();
        let width = value.get_type().get_bit_width();
        if width < 64 {
            self.builder
                .build_int_s_extend(value, i64_ty, "slice.idx")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))
        } else if width > 64 {
            self.builder
                .build_int_truncate(value, i64_ty, "slice.idx")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))
        } else {
            Ok(value)
        }
    }

    /// Build the `i1` "byte `offset` is a valid UTF-8 code-point boundary" predicate: an
    /// offset is valid when it is 0, equals `len`, or the byte there is not a continuation
    /// byte (`0b10xxxxxx`). The load index is clamped to 0 at the edges, so the read never
    /// touches `base[len]`.
    fn slice_boundary_ok(
        &mut self,
        base: PointerValue<'ctx>,
        offset: IntValue<'ctx>,
        len: IntValue<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        let i64_ty = self.context.i64_type();
        let i8_ty = self.context.i8_type();
        let zero = i64_ty.const_zero();

        let at_start = self
            .builder
            .build_int_compare(IntPredicate::EQ, offset, zero, "slice.b.start")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let at_end = self
            .builder
            .build_int_compare(IntPredicate::EQ, offset, len, "slice.b.end")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let at_edge = self
            .builder
            .build_or(at_start, at_end, "slice.b.edge")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let safe_idx = self
            .builder
            .build_select(at_edge, zero, offset, "slice.b.idx")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();

        // SAFETY: at an edge the index is clamped to 0; otherwise `0 < offset < len`, so
        // `base[safe_idx]` is an interior byte within the allocation.
        let byte_ptr = unsafe {
            self.builder
                .build_in_bounds_gep(i8_ty, base, &[safe_idx], "slice.b.ptr")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
        };
        let byte = self
            .builder
            .build_load(i8_ty, byte_ptr, "slice.byte")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let masked = self
            .builder
            .build_and(byte, i8_ty.const_int(0xC0, false), "slice.b.mask")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let is_cont = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                masked,
                i8_ty.const_int(0x80, false),
                "slice.b.cont",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let not_cont = self
            .builder
            .build_not(is_cont, "slice.b.notcont")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_or(at_edge, not_cont, "slice.b.ok")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// Lower a string receiver to its `{ ptr, len }` fat-pointer value.
    ///
    /// An owned `string` and an immutable `&string` are both already the struct. A `&mut
    /// string` receiver is the referent's address, so it is loaded through.
    pub(super) fn string_receiver_struct(
        &mut self,
        receiver: &HirExpr,
    ) -> CodegenResult<StructValue<'ctx>> {
        let recv_val = self.codegen_expr(receiver)?;
        match recv_val {
            BasicValueEnum::StructValue(sv) => Ok(sv),
            BasicValueEnum::PointerValue(ptr) => {
                let string_ty = self.type_mapper.map_type(&Type::String)?;
                Ok(self
                    .builder
                    .build_load(string_ty, ptr, "deref.str")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                    .into_struct_value())
            }
            other => Err(CodegenError::InternalError(format!(
                "string receiver did not lower to a fat pointer or borrow: {:?}",
                other
            ))),
        }
    }

    /// Lower `float.is_nan()` to an unordered self-comparison (`fcmp uno x, x`).
    ///
    /// NaN is the only value that compares unordered with itself, which is exactly why the
    /// test cannot be written in source: `x != x` uses the ordered predicate and is false
    /// for NaN like every other NaN comparison.
    fn codegen_is_nan(&mut self, receiver: &HirExpr) -> CodegenResult<BasicValueEnum<'ctx>> {
        let recv_val = self.codegen_expr(receiver)?;
        let BasicValueEnum::FloatValue(value) = recv_val else {
            return Err(CodegenError::InternalError(format!(
                "is_nan receiver did not lower to a float: {:?}",
                recv_val
            )));
        };
        Ok(self
            .builder
            .build_float_compare(FloatPredicate::UNO, value, value, "is.nan")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into())
    }

    /// Lower an integer intrinsic (`wrapping_*`, `saturating_*`, `.shr`) on a builtin
    /// integer receiver. Signedness is read from the receiver's recorded type, selecting
    /// the arithmetic (`ashr`) vs. logical (`lshr`) shift and the `s`/`u` saturating path.
    fn codegen_int_intrinsic(
        &mut self,
        kind: BuiltinMethod,
        recv_ty: &Type,
        receiver: &HirExpr,
        args: &[HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let unsigned = recv_ty.is_unsigned_int();
        let (lhs, rhs) = self.int_intrinsic_operands(recv_ty, receiver, args)?;

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
            BuiltinMethod::StringLen
            | BuiltinMethod::StringClone
            | BuiltinMethod::StringSlice
            | BuiltinMethod::StringCharSlice
            | BuiltinMethod::StringCharAt
            | BuiltinMethod::StructClone
            | BuiltinMethod::ArrayLen
            | BuiltinMethod::SequenceSlice
            | BuiltinMethod::SliceLen
            | BuiltinMethod::IsNan
            | BuiltinMethod::CheckedAdd
            | BuiltinMethod::CheckedSub
            | BuiltinMethod::CheckedMul => {
                unreachable!("handled by codegen_builtin_method's other arms")
            }
        };

        Ok(value.into())
    }

    /// Lower `checked_add` / `checked_sub` / `checked_mul` on an integer receiver to
    /// `Option::Some(result)`, or `Option::None` when the operation overflowed.
    ///
    /// The `llvm.{s,u}{add,sub,mul}.with.overflow` intrinsics report both halves in one
    /// aggregate, so the overflow bit selects the variant with no branch. `result_ty` is
    /// the monomorphized `Option<T>` the frontend resolved; its variant tags and payload
    /// layout are read from it rather than assumed.
    fn codegen_checked_int_intrinsic(
        &mut self,
        kind: BuiltinMethod,
        recv_ty: &Type,
        result_ty: &Type,
        receiver: &HirExpr,
        args: &[HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let unsigned = recv_ty.is_unsigned_int();
        let (lhs, rhs) = self.int_intrinsic_operands(recv_ty, receiver, args)?;

        let intrinsic_name = match (kind, unsigned) {
            (BuiltinMethod::CheckedAdd, false) => "llvm.sadd.with.overflow",
            (BuiltinMethod::CheckedAdd, true) => "llvm.uadd.with.overflow",
            (BuiltinMethod::CheckedSub, false) => "llvm.ssub.with.overflow",
            (BuiltinMethod::CheckedSub, true) => "llvm.usub.with.overflow",
            (BuiltinMethod::CheckedMul, false) => "llvm.smul.with.overflow",
            (BuiltinMethod::CheckedMul, true) => "llvm.umul.with.overflow",
            _ => {
                return Err(CodegenError::InternalError(
                    "non-checked intrinsic routed to the checked path".into(),
                ))
            }
        };

        let (value, overflowed) = self.emit_with_overflow(intrinsic_name, lhs, rhs)?;
        let ok = self
            .builder
            .build_not(overflowed, "chk.ok")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.build_option_value(result_ty, ok, value.into(), recv_ty)
    }

    /// Evaluate an integer intrinsic's receiver and single argument, both coerced to the
    /// receiver's exact integer type. The argument literal can arrive widened (the
    /// backend's literal default is `i32`), and semantic analysis has already proven the
    /// two types compatible.
    fn int_intrinsic_operands(
        &mut self,
        recv_ty: &Type,
        receiver: &HirExpr,
        args: &[HirExpr],
    ) -> CodegenResult<(IntValue<'ctx>, IntValue<'ctx>)> {
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
        Ok((lhs, rhs))
    }

    /// Call an overloaded `llvm.*.with.overflow` intrinsic, returning its
    /// `(wrapped result, overflow bit)` pair.
    fn emit_with_overflow(
        &self,
        intrinsic_name: &str,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> CodegenResult<(IntValue<'ctx>, IntValue<'ctx>)> {
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
            .build_call(decl, &[lhs.into(), rhs.into()], "ovf")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError(format!("{intrinsic_name} returned void")))?
            .into_struct_value();
        let result = self
            .builder
            .build_extract_value(agg, 0, "ovf.res")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let overflowed = self
            .builder
            .build_extract_value(agg, 1, "ovf.bit")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        Ok((result, overflowed))
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
        let (result, overflowed) = self.emit_with_overflow(intrinsic_name, lhs, rhs)?;

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
