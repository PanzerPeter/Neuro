// Codegen for the prelude-private `string.__char_at(offset)` — the Unicode scalar whose
// UTF-8 encoding begins at a byte offset.
//
// This is the one step `Chars::next` cannot take in source: the language exposes no
// byte-indexed read of a string, deliberately, because a byte index is meaningless for
// every purpose but decoding the encoding itself. The semantic pass refuses the method to
// every module but the prelude's, so the only caller is the iterator the prelude declares.
//
// Decoding is a scan over the code point's own bytes rather than a four-way branch on the
// lead byte: a continuation byte is recognisable on its own (`0b10xxxxxx`), so the loop
// consumes exactly as many as the scalar has and needs no width computed up front. The
// text is well-formed UTF-8 by construction — literals are validated at parse time and
// every borrowed view lands on a code point boundary — so the loop's own bound is the
// string length rather than a validity check.

use inkwell::module::Linkage;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue};
use inkwell::IntPredicate;
use neuro_hir::HirExpr;

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};

/// The module-private helper that decodes one code point.
const CHAR_AT_FN: &str = "neuro.string.char_at";

/// The UTF-8 continuation-byte pattern `0b10xxxxxx`.
const UTF8_CONTINUATION_MASK: u64 = 0xC0;
const UTF8_CONTINUATION_BITS: u64 = 0x80;
/// The payload a continuation byte carries, and the bits it contributes.
const UTF8_CONTINUATION_PAYLOAD: u64 = 0x3F;
const UTF8_CONTINUATION_SHIFT: u64 = 6;
/// Lead-byte boundaries and the payload each keeps: one-byte scalars below `0x80`, then
/// the two-, three-, and four-byte leads with 5, 4, and 3 payload bits.
const UTF8_ASCII_LIMIT: u64 = 0x80;
const UTF8_THREE_BYTE_LEAD: u64 = 0xE0;
const UTF8_FOUR_BYTE_LEAD: u64 = 0xF0;
const UTF8_TWO_BYTE_PAYLOAD: u64 = 0x1F;
const UTF8_THREE_BYTE_PAYLOAD: u64 = 0x0F;
const UTF8_FOUR_BYTE_PAYLOAD: u64 = 0x07;

fn llvm_err(e: inkwell::builder::BuilderError) -> CodegenError {
    CodegenError::LlvmError(e.to_string())
}

impl<'ctx> CodegenContext<'ctx> {
    /// Lower `string.__char_at(offset)` to the code point starting at that byte.
    pub(super) fn codegen_char_at(
        &mut self,
        receiver: &HirExpr,
        args: &[HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let offset_expr = args.first().ok_or_else(|| {
            CodegenError::InternalError("string.__char_at reached codegen without an offset".into())
        })?;

        let fat = self.string_receiver_struct(receiver)?;
        let base_ptr = self
            .builder
            .build_extract_value(fat, 0, "cat.base.ptr")
            .map_err(llvm_err)?
            .into_pointer_value();
        let len = self
            .builder
            .build_extract_value(fat, 1, "cat.len")
            .map_err(llvm_err)?
            .into_int_value();
        let offset = self.codegen_expr(offset_expr)?.into_int_value();

        let char_at = self.get_or_build_char_at()?;
        let call_args: [BasicMetadataValueEnum; 3] = [base_ptr.into(), len.into(), offset.into()];
        self.builder
            .build_call(char_at, &call_args, "cat.scalar")
            .map_err(llvm_err)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("char_at produced no result".into()))
    }

    /// Fetch the module's `char_at` helper, emitting it on first use.
    fn get_or_build_char_at(&mut self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(CHAR_AT_FN) {
            return Ok(existing);
        }

        let i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = i32_type.fn_type(&[ptr_type.into(), i64_type.into(), i64_type.into()], false);
        let function = self
            .module
            .add_function(CHAR_AT_FN, fn_type, Some(Linkage::Private));

        // The body is emitted into its own function, so the builder has to be put back
        // wherever the caller was mid-expression.
        let resume_at = self.builder.get_insert_block();
        let built = self.build_char_at_body(function);
        if let Some(block) = resume_at {
            self.builder.position_at_end(block);
        }
        built?;

        Ok(function)
    }

    /// Emit `char_at`'s body: take the lead byte's payload, then fold in every
    /// continuation byte that follows it.
    fn build_char_at_body(&self, function: FunctionValue<'ctx>) -> CodegenResult<()> {
        let i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();

        let base = function
            .get_nth_param(0)
            .ok_or_else(|| {
                CodegenError::InternalError("char_at lost its pointer parameter".into())
            })?
            .into_pointer_value();
        let len = function
            .get_nth_param(1)
            .ok_or_else(|| CodegenError::InternalError("char_at lost its length parameter".into()))?
            .into_int_value();
        let offset = function
            .get_nth_param(2)
            .ok_or_else(|| CodegenError::InternalError("char_at lost its offset parameter".into()))?
            .into_int_value();

        let entry = self.context.append_basic_block(function, "entry");
        let lead = self.context.append_basic_block(function, "cat.lead");
        let head = self.context.append_basic_block(function, "cat.head");
        let peek = self.context.append_basic_block(function, "cat.peek");
        let fold = self.context.append_basic_block(function, "cat.fold");
        let done = self.context.append_basic_block(function, "cat.done");

        self.builder.position_at_end(entry);
        let scalar_slot = self
            .builder
            .build_alloca(i32_type, "cat.scalar")
            .map_err(llvm_err)?;
        let cursor_slot = self
            .builder
            .build_alloca(i64_type, "cat.cursor")
            .map_err(llvm_err)?;
        self.builder
            .build_store(scalar_slot, i32_type.const_zero())
            .map_err(llvm_err)?;
        // An offset at or past the end has no code point standing on it. `Chars::next`
        // stops before that, so this answers the empty scalar rather than panicking.
        let in_bounds = self
            .builder
            .build_int_compare(IntPredicate::ULT, offset, len, "cat.in.bounds")
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(in_bounds, lead, done)
            .map_err(llvm_err)?;

        self.builder.position_at_end(lead);
        let lead_byte = self.load_byte_as_i32(base, offset, "cat.lead.byte")?;
        let ascii = self
            .builder
            .build_int_compare(
                IntPredicate::ULT,
                lead_byte,
                i32_type.const_int(UTF8_ASCII_LIMIT, false),
                "cat.ascii",
            )
            .map_err(llvm_err)?;
        let two_byte = self
            .builder
            .build_int_compare(
                IntPredicate::ULT,
                lead_byte,
                i32_type.const_int(UTF8_THREE_BYTE_LEAD, false),
                "cat.two.byte",
            )
            .map_err(llvm_err)?;
        let three_byte = self
            .builder
            .build_int_compare(
                IntPredicate::ULT,
                lead_byte,
                i32_type.const_int(UTF8_FOUR_BYTE_LEAD, false),
                "cat.three.byte",
            )
            .map_err(llvm_err)?;
        let payload_two = self
            .builder
            .build_and(
                lead_byte,
                i32_type.const_int(UTF8_TWO_BYTE_PAYLOAD, false),
                "cat.payload2",
            )
            .map_err(llvm_err)?;
        let payload_three = self
            .builder
            .build_and(
                lead_byte,
                i32_type.const_int(UTF8_THREE_BYTE_PAYLOAD, false),
                "cat.payload3",
            )
            .map_err(llvm_err)?;
        let payload_four = self
            .builder
            .build_and(
                lead_byte,
                i32_type.const_int(UTF8_FOUR_BYTE_PAYLOAD, false),
                "cat.payload4",
            )
            .map_err(llvm_err)?;
        let wide = self
            .builder
            .build_select(three_byte, payload_three, payload_four, "cat.wide")
            .map_err(llvm_err)?
            .into_int_value();
        let multi = self
            .builder
            .build_select(two_byte, payload_two, wide, "cat.multi")
            .map_err(llvm_err)?
            .into_int_value();
        let start = self
            .builder
            .build_select(ascii, lead_byte, multi, "cat.start")
            .map_err(llvm_err)?
            .into_int_value();
        self.builder
            .build_store(scalar_slot, start)
            .map_err(llvm_err)?;
        let next = self
            .builder
            .build_int_add(offset, i64_type.const_int(1, false), "cat.next")
            .map_err(llvm_err)?;
        self.builder
            .build_store(cursor_slot, next)
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(head)
            .map_err(llvm_err)?;

        // The two halves of "there is another continuation byte" need separate blocks:
        // the byte may only be read once the cursor is known to be inside the string.
        self.builder.position_at_end(head);
        let cursor = self
            .builder
            .build_load(i64_type, cursor_slot, "cat.c")
            .map_err(llvm_err)?
            .into_int_value();
        let readable = self
            .builder
            .build_int_compare(IntPredicate::ULT, cursor, len, "cat.readable")
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(readable, peek, done)
            .map_err(llvm_err)?;

        self.builder.position_at_end(peek);
        let byte = self.load_byte_as_i32(base, cursor, "cat.byte")?;
        let masked = self
            .builder
            .build_and(
                byte,
                i32_type.const_int(UTF8_CONTINUATION_MASK, false),
                "cat.cont.mask",
            )
            .map_err(llvm_err)?;
        let is_continuation = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                masked,
                i32_type.const_int(UTF8_CONTINUATION_BITS, false),
                "cat.is.cont",
            )
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(is_continuation, fold, done)
            .map_err(llvm_err)?;

        self.builder.position_at_end(fold);
        let acc = self
            .builder
            .build_load(i32_type, scalar_slot, "cat.acc")
            .map_err(llvm_err)?
            .into_int_value();
        let shifted = self
            .builder
            .build_left_shift(
                acc,
                i32_type.const_int(UTF8_CONTINUATION_SHIFT, false),
                "cat.shift",
            )
            .map_err(llvm_err)?;
        let carried = self
            .builder
            .build_and(
                byte,
                i32_type.const_int(UTF8_CONTINUATION_PAYLOAD, false),
                "cat.carried",
            )
            .map_err(llvm_err)?;
        let folded = self
            .builder
            .build_or(shifted, carried, "cat.folded")
            .map_err(llvm_err)?;
        self.builder
            .build_store(scalar_slot, folded)
            .map_err(llvm_err)?;
        let advanced = self
            .builder
            .build_int_add(cursor, i64_type.const_int(1, false), "cat.advanced")
            .map_err(llvm_err)?;
        self.builder
            .build_store(cursor_slot, advanced)
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(head)
            .map_err(llvm_err)?;

        self.builder.position_at_end(done);
        let result = self
            .builder
            .build_load(i32_type, scalar_slot, "cat.result")
            .map_err(llvm_err)?;
        self.builder.build_return(Some(&result)).map_err(llvm_err)?;

        Ok(())
    }

    /// Load one byte of the string and widen it, so masks and shifts run at the scalar's
    /// own width instead of overflowing a byte.
    fn load_byte_as_i32(
        &self,
        base: inkwell::values::PointerValue<'ctx>,
        index: inkwell::values::IntValue<'ctx>,
        name: &str,
    ) -> CodegenResult<inkwell::values::IntValue<'ctx>> {
        let i8_type = self.context.i8_type();
        let i32_type = self.context.i32_type();
        // SAFETY: the index is proven below `len` by the caller's bounds test, so the
        // address stays inside the string's own UTF-8 allocation.
        let ptr = unsafe {
            self.builder
                .build_in_bounds_gep(i8_type, base, &[index], "cat.gep")
                .map_err(llvm_err)?
        };
        let byte = self
            .builder
            .build_load(i8_type, ptr, "cat.raw")
            .map_err(llvm_err)?
            .into_int_value();
        self.builder
            .build_int_z_extend(byte, i32_type, name)
            .map_err(llvm_err)
    }
}
