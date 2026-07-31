// Map key operations: equality, total order, and hashing.
//
// The compiler supplies all three for the builtin key types. A struct key routes them to
// its own `PartialEq` / `Comparable` / `Hashable` impl methods, which semantic analysis
// has already required — that is how `BTreeMap<OrderedF32, V>` gets a total order over
// values that `f32` itself cannot provide.

use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use inkwell::IntPredicate;

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// The string hash helper: `__neuro_hash_string(ptr, len) -> i64`.
const STRING_HASH_HELPER: &str = "__neuro_hash_string";

/// FNV-1a's 64-bit offset basis and prime. FNV is chosen over a wider mixer because it
/// needs no buffering and one multiply per byte, which suits the short keys that
/// dominate; the map's probe sequence tolerates its weak avalanche.
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Mixing constants for integer keys (the finalizer of the SplitMix64 generator).
/// Consecutive integer keys otherwise land in consecutive slots, which turns a linear
/// probe into a long cluster on the very access pattern that is most common.
const MIX_MULTIPLIER_1: u64 = 0xff51_afd7_ed55_8ccd;
const MIX_MULTIPLIER_2: u64 = 0xc4ce_b9fe_1a85_ec53;
const MIX_SHIFT: u64 = 33;

impl<'ctx> CodegenContext<'ctx> {
    /// Whether two keys are equal.
    pub(super) fn emit_key_eq(
        &mut self,
        key_ty: &Type,
        lhs: BasicValueEnum<'ctx>,
        rhs: BasicValueEnum<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        if key_ty.is_int_like() {
            return self
                .builder
                .build_int_compare(
                    IntPredicate::EQ,
                    lhs.into_int_value(),
                    rhs.into_int_value(),
                    "key.eq",
                )
                .map_err(|e| CodegenError::LlvmError(e.to_string()));
        }
        if matches!(key_ty, Type::String) {
            return self.codegen_string_eq(lhs, rhs);
        }
        self.call_key_method(key_ty, "eq", &[lhs, rhs], "key.eq")
    }

    /// Whether `lhs` orders strictly before `rhs`. Only the ordered map needs this, and
    /// only for key types semantic analysis has verified carry a total order.
    pub(super) fn emit_key_lt(
        &mut self,
        key_ty: &Type,
        lhs: BasicValueEnum<'ctx>,
        rhs: BasicValueEnum<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        if key_ty.is_int_like() {
            let predicate = if key_ty.is_unsigned_like() {
                IntPredicate::ULT
            } else {
                IntPredicate::SLT
            };
            return self
                .builder
                .build_int_compare(
                    predicate,
                    lhs.into_int_value(),
                    rhs.into_int_value(),
                    "key.lt",
                )
                .map_err(|e| CodegenError::LlvmError(e.to_string()));
        }
        if matches!(key_ty, Type::String) {
            return self.emit_string_lt(lhs, rhs);
        }
        self.call_key_method(key_ty, "lt", &[lhs, rhs], "key.lt")
    }

    /// A 64-bit hash of a key.
    pub(super) fn emit_key_hash(
        &mut self,
        key_ty: &Type,
        key: BasicValueEnum<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        if key_ty.is_int_like() {
            let widened = self
                .builder
                .build_int_z_extend(key.into_int_value(), self.context.i64_type(), "key.wide")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            return self.emit_integer_mix(widened);
        }
        if matches!(key_ty, Type::String) {
            let bytes = self
                .builder
                .build_extract_value(key.into_struct_value(), 0, "key.str.ptr")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_pointer_value();
            let len = self
                .builder
                .build_extract_value(key.into_struct_value(), 1, "key.str.len")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_int_value();
            let helper = self.build_string_hash_helper()?;
            return Ok(self
                .builder
                .build_call(helper, &[bytes.into(), len.into()], "key.hash")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| CodegenError::InternalError("string hash returned void".into()))?
                .into_int_value());
        }
        self.call_key_method(key_ty, "hash", &[key], "key.hash")
    }

    /// Scramble an integer key so that adjacent values do not land in adjacent slots.
    fn emit_integer_mix(&mut self, value: IntValue<'ctx>) -> CodegenResult<IntValue<'ctx>> {
        let i64_ty = self.context.i64_type();
        let shift = i64_ty.const_int(MIX_SHIFT, false);
        let mut acc = value;
        for multiplier in [MIX_MULTIPLIER_1, MIX_MULTIPLIER_2] {
            let shifted = self
                .builder
                .build_right_shift(acc, shift, false, "mix.shr")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            let xored = self
                .builder
                .build_xor(acc, shifted, "mix.xor")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            acc = self
                .builder
                .build_int_mul(xored, i64_ty.const_int(multiplier, false), "mix.mul")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        }
        let shifted = self
            .builder
            .build_right_shift(acc, shift, false, "mix.shr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_xor(acc, shifted, "mix.final")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// Lexicographic `<` over two UTF-8 strings: compare the shared prefix, and fall
    /// back to the lengths when it matches.
    fn emit_string_lt(
        &mut self,
        lhs: BasicValueEnum<'ctx>,
        rhs: BasicValueEnum<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        let lhs_struct = lhs.into_struct_value();
        let rhs_struct = rhs.into_struct_value();
        let lhs_ptr = self
            .builder
            .build_extract_value(lhs_struct, 0, "lt.l.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let lhs_len = self
            .builder
            .build_extract_value(lhs_struct, 1, "lt.l.len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let rhs_ptr = self
            .builder
            .build_extract_value(rhs_struct, 0, "lt.r.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let rhs_len = self
            .builder
            .build_extract_value(rhs_struct, 1, "lt.r.len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();

        let shorter = self
            .builder
            .build_int_compare(IntPredicate::ULT, lhs_len, rhs_len, "lt.shorter")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let common = self
            .builder
            .build_select(shorter, lhs_len, rhs_len, "lt.common")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();

        let memcmp = self.get_or_declare_memcmp();
        let order = self
            .builder
            .build_call(
                memcmp,
                &[lhs_ptr.into(), rhs_ptr.into(), common.into()],
                "lt.cmp",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("memcmp returned void".into()))?
            .into_int_value();

        let i32_zero = self.context.i32_type().const_zero();
        let prefix_equal = self
            .builder
            .build_int_compare(IntPredicate::EQ, order, i32_zero, "lt.prefix.eq")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let prefix_less = self
            .builder
            .build_int_compare(IntPredicate::SLT, order, i32_zero, "lt.prefix.lt")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_select(prefix_equal, shorter, prefix_less, "lt.result")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
            .map(|v| v.into_int_value())
    }

    /// Call a struct key's trait method (`eq` / `lt` / `hash`), adapting each argument
    /// to the receiving parameter: an impl written with reference parameters takes
    /// addresses, one written by value takes the aggregate itself.
    fn call_key_method(
        &mut self,
        key_ty: &Type,
        method: &str,
        args: &[BasicValueEnum<'ctx>],
        name: &str,
    ) -> CodegenResult<IntValue<'ctx>> {
        let Type::Struct(struct_name) = key_ty else {
            return Err(CodegenError::UnsupportedType(format!(
                "{:?} cannot be used as a map key",
                key_ty
            )));
        };
        let mangled = format!("{}__{}", struct_name, method);
        let callee = *self
            .functions
            .get(&mangled)
            .ok_or_else(|| CodegenError::UndefinedFunction(mangled.clone()))?;

        let param_types = callee.get_type().get_param_types();
        let mut call_args = Vec::with_capacity(args.len());
        for (index, value) in args.iter().enumerate() {
            let wants_pointer = matches!(
                param_types.get(index),
                Some(inkwell::types::BasicMetadataTypeEnum::PointerType(_))
            );
            if wants_pointer && !value.is_pointer_value() {
                call_args.push(self.spill_to_stack(*value)?.into());
            } else {
                call_args.push((*value).into());
            }
        }

        let result = self
            .builder
            .build_call(callee, &call_args, name)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| {
                CodegenError::InternalError(format!("'{}' returned no value", mangled))
            })?;
        Ok(result.into_int_value())
    }

    /// Materialize a value into a stack slot and yield its address, so it can be passed
    /// where a `&self` / `&Rhs` parameter is expected.
    fn spill_to_stack(&mut self, value: BasicValueEnum<'ctx>) -> CodegenResult<PointerValue<'ctx>> {
        let slot = self.entry_alloca(value.get_type(), "key.spill")?;
        self.builder
            .build_store(slot, value)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(slot)
    }

    /// Emit (once per module) the FNV-1a string hash.
    fn build_string_hash_helper(&mut self) -> CodegenResult<inkwell::values::FunctionValue<'ctx>> {
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        let i64_ty = self.context.i64_type();
        let fn_type = i64_ty.fn_type(&[ptr_ty.into(), i64_ty.into()], false);

        self.get_or_build_helper(STRING_HASH_HELPER, fn_type, |ctx, func| {
            let entry = ctx.context.append_basic_block(func, "entry");
            let cond_bb = ctx.context.append_basic_block(func, "cond");
            let body_bb = ctx.context.append_basic_block(func, "body");
            let exit_bb = ctx.context.append_basic_block(func, "exit");
            ctx.builder.position_at_end(entry);

            let bytes = func
                .get_nth_param(0)
                .ok_or_else(|| CodegenError::InternalError("hash helper arity".into()))?
                .into_pointer_value();
            let len = func
                .get_nth_param(1)
                .ok_or_else(|| CodegenError::InternalError("hash helper arity".into()))?
                .into_int_value();

            let hash_slot = ctx.entry_alloca(i64_ty, "hash")?;
            let index_slot = ctx.entry_alloca(i64_ty, "i")?;
            ctx.builder
                .build_store(hash_slot, i64_ty.const_int(FNV_OFFSET_BASIS, false))
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.builder
                .build_store(index_slot, i64_ty.const_zero())
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.builder
                .build_unconditional_branch(cond_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            ctx.builder.position_at_end(cond_bb);
            let index = ctx
                .builder
                .build_load(i64_ty, index_slot, "i.val")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_int_value();
            let more = ctx
                .builder
                .build_int_compare(IntPredicate::ULT, index, len, "more")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.builder
                .build_conditional_branch(more, body_bb, exit_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            ctx.builder.position_at_end(body_bb);
            // SAFETY: the loop condition above proved `index < len`, so the byte is
            // inside the string's UTF-8 buffer.
            let byte_ptr = unsafe {
                ctx.builder
                    .build_in_bounds_gep(ctx.context.i8_type(), bytes, &[index], "byte.ptr")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            };
            let byte = ctx
                .builder
                .build_load(ctx.context.i8_type(), byte_ptr, "byte")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_int_value();
            let byte64 = ctx
                .builder
                .build_int_z_extend(byte, i64_ty, "byte64")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            let hash = ctx
                .builder
                .build_load(i64_ty, hash_slot, "hash.val")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_int_value();
            let xored = ctx
                .builder
                .build_xor(hash, byte64, "hash.xor")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            let multiplied = ctx
                .builder
                .build_int_mul(xored, i64_ty.const_int(FNV_PRIME, false), "hash.mul")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.builder
                .build_store(hash_slot, multiplied)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            let next = ctx
                .builder
                .build_int_add(index, i64_ty.const_int(1, false), "i.next")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.builder
                .build_store(index_slot, next)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.builder
                .build_unconditional_branch(cond_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            ctx.builder.position_at_end(exit_bb);
            let final_hash = ctx
                .builder
                .build_load(i64_ty, hash_slot, "hash.out")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.builder
                .build_return(Some(&final_hash))
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            Ok(())
        })
    }
}
