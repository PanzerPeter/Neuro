// `String`: the growable UTF-8 text buffer.
//
// The buffer is a plain byte run of `cap` bytes holding `len` live ones — a `Vec<u8>`
// under a text surface — so `len()` and `clear()` are the shared collection operations
// and only appending and the copy back out to an immutable `string` are specific here.
//
// Growth reserves a whole run of bytes at once rather than one element at a time, which
// is why it does not reuse the `Vec` reserve helper: appending an n-byte string must
// reach `len + n` capacity in a single step, not by doubling n times.

use inkwell::values::{BasicValueEnum, FunctionValue, IntValue, PointerValue};
use inkwell::IntPredicate;
use neuro_hir::HirExpr;

use super::{initial_capacity, FIELD_CAP, FIELD_LEN};
use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// The shared byte-capacity helper: `__neuro_string_reserve(header, extra_bytes)`.
const RESERVE_HELPER: &str = "__neuro_string_reserve";

impl<'ctx> CodegenContext<'ctx> {
    /// `s.push_str(text)` — reserve room for the argument's bytes, copy them in after the
    /// live ones, and advance the length.
    ///
    /// The argument is read, never stored, so its own binding keeps ownership of it and
    /// no move is recorded — the same contract `+` gives its operands.
    pub(crate) fn codegen_string_push_str(
        &mut self,
        header: PointerValue<'ctx>,
        args: &[HirExpr],
    ) -> CodegenResult<()> {
        let text_expr = args.first().ok_or_else(|| {
            CodegenError::InternalError("String::push_str reached codegen without text".into())
        })?;
        let text = self.codegen_expr(text_expr)?;
        let (src, extra) = self.split_string_fatptr(text)?;

        let reserve = self.build_string_reserve_helper()?;
        self.builder
            .build_call(reserve, &[header.into(), extra.into()], "")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let buffer = self.load_header_buffer(header)?;
        let len = self.load_header_field(header, FIELD_LEN, "str.len")?;
        // SAFETY: the reserve call above guarantees the buffer holds at least
        // `len + extra` bytes, so `buffer + len` starts a run of `extra` writable bytes
        // inside the allocation.
        let dst = unsafe {
            self.builder
                .build_in_bounds_gep(self.context.i8_type(), buffer, &[len], "str.dst")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
        };
        let memcpy = self.get_or_declare_memcpy();
        self.builder
            .build_call(memcpy, &[dst.into(), src.into(), extra.into()], "")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let grown = self
            .builder
            .build_int_add(len, extra, "str.len.new")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.store_header_field(header, FIELD_LEN, grown)?;
        Ok(())
    }

    /// `s.to_string()` — copy the accumulated bytes into a fresh owned `string`.
    ///
    /// A borrowed view into the builder's buffer would be free, but a later `push_str`
    /// may reallocate and leave it dangling, and the borrow checker does not yet track a
    /// builder's outstanding views. One copy at the end of a build is the sound answer.
    pub(crate) fn codegen_string_to_owned(
        &mut self,
        header: PointerValue<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let i64_ty = self.context.i64_type();
        let len = self.load_header_field(header, FIELD_LEN, "str.len")?;
        let buffer = self.load_header_buffer(header)?;

        // `malloc(0)` may hand back null, which would make an empty result
        // indistinguishable from a failed allocation; one spare byte keeps every
        // `string` this produces a real pointer.
        let one = i64_ty.const_int(1, false);
        let empty = self
            .builder
            .build_int_compare(IntPredicate::EQ, len, i64_ty.const_zero(), "str.empty")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let alloc_size = self
            .builder
            .build_select(empty, one, len, "str.alloc")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let copy = self.build_malloc(alloc_size, "str.copy")?;

        let memcpy = self.get_or_declare_memcpy();
        self.builder
            .build_call(memcpy, &[copy.into(), buffer.into(), len.into()], "")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let fat_ptr_type = self.type_mapper.map_type(&Type::String)?.into_struct_type();
        let with_ptr = self
            .builder
            .build_insert_value(fat_ptr_type.get_undef(), copy, 0, "str.res.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_struct_value();
        Ok(self
            .builder
            .build_insert_value(with_ptr, len, 1, "str.res")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_struct_value()
            .into())
    }

    /// Split a `string` / `&string` operand into its data pointer and byte length.
    ///
    /// Both are the `{ ptr, i64 }` fat pointer by value; only a `&mut string`, which is
    /// the referent's address, needs a load, and semantic analysis does not admit one
    /// here — the load arm exists so a future widening cannot miscompile silently.
    fn split_string_fatptr(
        &mut self,
        value: BasicValueEnum<'ctx>,
    ) -> CodegenResult<(PointerValue<'ctx>, IntValue<'ctx>)> {
        let fat_ptr = match value {
            BasicValueEnum::PointerValue(ptr) => {
                let string_ty = self.type_mapper.map_type(&Type::String)?;
                self.builder
                    .build_load(string_ty, ptr, "str.arg")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            }
            other => other,
        };
        let fat_ptr = fat_ptr.into_struct_value();
        let data = self
            .builder
            .build_extract_value(fat_ptr, 0, "str.arg.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let len = self
            .builder
            .build_extract_value(fat_ptr, 1, "str.arg.len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        Ok((data, len))
    }

    /// Emit (once per module) `__neuro_string_reserve(header, extra)`: ensure the buffer
    /// holds `len + extra` bytes, reallocating to the larger of a doubled capacity and
    /// the exact requirement when it does not.
    ///
    /// Taking the max of the two keeps a single large append from becoming a chain of
    /// doublings while leaving repeated small appends amortized O(1).
    fn build_string_reserve_helper(&mut self) -> CodegenResult<FunctionValue<'ctx>> {
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        let i64_ty = self.context.i64_type();
        let fn_type = self
            .context
            .void_type()
            .fn_type(&[ptr_ty.into(), i64_ty.into()], false);

        self.get_or_build_helper(RESERVE_HELPER, fn_type, |ctx, func| {
            let entry = ctx.context.append_basic_block(func, "entry");
            let grow_bb = ctx.context.append_basic_block(func, "grow");
            let done_bb = ctx.context.append_basic_block(func, "done");
            ctx.builder.position_at_end(entry);

            let header = func
                .get_nth_param(0)
                .ok_or_else(|| CodegenError::InternalError("reserve helper arity".into()))?
                .into_pointer_value();
            let extra = func
                .get_nth_param(1)
                .ok_or_else(|| CodegenError::InternalError("reserve helper arity".into()))?
                .into_int_value();

            let len = ctx.load_header_field(header, FIELD_LEN, "len")?;
            let cap = ctx.load_header_field(header, FIELD_CAP, "cap")?;
            let needed = ctx
                .builder
                .build_int_add(len, extra, "needed")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            let fits = ctx
                .builder
                .build_int_compare(IntPredicate::ULE, needed, cap, "fits")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.builder
                .build_conditional_branch(fits, done_bb, grow_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            ctx.builder.position_at_end(grow_bb);
            let doubled = ctx
                .builder
                .build_int_mul(cap, i64_ty.const_int(2, false), "double")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            let doubled_short = ctx
                .builder
                .build_int_compare(IntPredicate::ULT, doubled, needed, "double.short")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            let candidate = ctx
                .builder
                .build_select(doubled_short, needed, doubled, "cap.candidate")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_int_value();
            let min_cap = i64_ty.const_int(initial_capacity(), false);
            let use_min = ctx
                .builder
                .build_int_compare(IntPredicate::ULT, candidate, min_cap, "too.small")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            let new_cap = ctx
                .builder
                .build_select(use_min, min_cap, candidate, "new.cap")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_int_value();

            let old = ctx.load_header_buffer(header)?;
            let realloc = ctx.get_or_declare_realloc();
            let grown = ctx
                .builder
                .build_call(realloc, &[old.into(), new_cap.into()], "grown")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| CodegenError::InternalError("realloc returned void".into()))?
                .into_pointer_value();
            ctx.store_header_buffer(header, grown)?;
            ctx.store_header_field(header, FIELD_CAP, new_cap)?;
            ctx.builder
                .build_unconditional_branch(done_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            ctx.builder.position_at_end(done_bb);
            ctx.builder
                .build_return(None)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            Ok(())
        })
    }
}
