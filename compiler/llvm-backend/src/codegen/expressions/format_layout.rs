// Runtime helpers that shape already-rendered text: field padding, debug quoting,
// UTF-8 encoding of a `char`, and the two fix-ups that bring C's float output in
// line with the language's specifier table.

use inkwell::values::{FunctionValue, IntValue};
use inkwell::{AddressSpace, IntPredicate};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};

/// Align codes shared with the caller in `interp.rs`.
pub(crate) const ALIGN_LEFT: u64 = 0;
pub(crate) const ALIGN_RIGHT: u64 = 1;
pub(crate) const ALIGN_CENTER: u64 = 2;

fn llvm_err(e: inkwell::builder::BuilderError) -> CodegenError {
    CodegenError::LlvmError(e.to_string())
}

impl<'ctx> CodegenContext<'ctx> {
    /// `{ptr, len} __neuro_pad(i8* s, i64 len, i64 width, i32 align, i8 fill)`.
    ///
    /// Returns the input untouched when it already fills the field. Zero filling is
    /// sign-aware: `-42` padded to width 6 is `-00042`, never `00-042`, because the
    /// sign belongs at the front of the number, not inside its digits.
    pub(crate) fn get_or_define_pad(&self) -> CodegenResult<FunctionValue<'ctx>> {
        const NAME: &str = "__neuro_pad";
        if let Some(f) = self.module.get_function(NAME) {
            return Ok(f);
        }
        let i64_type = self.context.i64_type();
        let i32_type = self.context.i32_type();
        let i8_type = self.context.i8_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let (func, saved) = self.begin_helper(
            NAME,
            &[
                ptr_type.into(),
                i64_type.into(),
                i64_type.into(),
                i32_type.into(),
                i8_type.into(),
            ],
        );

        let source = self.param(func, 0)?.into_pointer_value();
        let len = self.param(func, 1)?.into_int_value();
        let width = self.param(func, 2)?.into_int_value();
        let align = self.param(func, 3)?.into_int_value();
        let fill = self.param(func, 4)?.into_int_value();

        let unchanged = self.context.append_basic_block(func, "unchanged");
        let pad = self.context.append_basic_block(func, "pad");
        let maybe_sign = self.context.append_basic_block(func, "maybe.sign");
        let sign_first = self.context.append_basic_block(func, "sign.first");
        let plain = self.context.append_basic_block(func, "plain");
        let done = self.context.append_basic_block(func, "done");

        let fits = self
            .builder
            .build_int_compare(IntPredicate::UGE, len, width, "pad.fits")
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(fits, unchanged, pad)
            .map_err(llvm_err)?;

        self.builder.position_at_end(unchanged);
        let as_is = self.build_string_value(source, len)?;
        self.builder.build_return(Some(&as_is)).map_err(llvm_err)?;

        self.builder.position_at_end(pad);
        let buf = self.build_malloc(width, "pad.buf")?;
        let memset = self.get_or_declare_memset();
        let fill_word = self
            .builder
            .build_int_z_extend(fill, i32_type, "pad.fill")
            .map_err(llvm_err)?;
        self.builder
            .build_call(memset, &[buf.into(), fill_word.into(), width.into()], "")
            .map_err(llvm_err)?;

        let slack = self
            .builder
            .build_int_sub(width, len, "pad.slack")
            .map_err(llvm_err)?;
        let half = self
            .builder
            .build_int_unsigned_div(slack, i64_type.const_int(2, false), "pad.half")
            .map_err(llvm_err)?;
        let is_left = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                align,
                i32_type.const_int(ALIGN_LEFT, false),
                "pad.left",
            )
            .map_err(llvm_err)?;
        let is_center = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                align,
                i32_type.const_int(ALIGN_CENTER, false),
                "pad.center",
            )
            .map_err(llvm_err)?;
        let centered_or_right = self
            .builder
            .build_select(is_center, half, slack, "pad.off.nonleft")
            .map_err(llvm_err)?
            .into_int_value();
        let offset = self
            .builder
            .build_select(is_left, i64_type.const_zero(), centered_or_right, "pad.off")
            .map_err(llvm_err)?
            .into_int_value();

        let zero_fill = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                fill,
                i8_type.const_int(u64::from(b'0'), false),
                "pad.zero",
            )
            .map_err(llvm_err)?;
        let non_empty = self
            .builder
            .build_int_compare(
                IntPredicate::UGT,
                len,
                i64_type.const_zero(),
                "pad.nonempty",
            )
            .map_err(llvm_err)?;
        let check_sign = self
            .builder
            .build_and(zero_fill, non_empty, "pad.check.sign")
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(check_sign, maybe_sign, plain)
            .map_err(llvm_err)?;

        self.builder.position_at_end(maybe_sign);
        let first = self.load_byte(source, i64_type.const_zero(), "pad.first")?;
        let is_plus = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                first,
                i8_type.const_int(u64::from(b'+'), false),
                "pad.is.plus",
            )
            .map_err(llvm_err)?;
        let is_minus = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                first,
                i8_type.const_int(u64::from(b'-'), false),
                "pad.is.minus",
            )
            .map_err(llvm_err)?;
        let is_sign = self
            .builder
            .build_or(is_plus, is_minus, "pad.is.sign")
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(is_sign, sign_first, plain)
            .map_err(llvm_err)?;

        self.builder.position_at_end(sign_first);
        self.store_byte(buf, i64_type.const_zero(), first)?;
        let body_offset = self
            .builder
            .build_int_add(offset, i64_type.const_int(1, false), "pad.body.off")
            .map_err(llvm_err)?;
        let body_dst = self.byte_offset(buf, body_offset, "pad.body.dst")?;
        let body_src = self.byte_offset(source, i64_type.const_int(1, false), "pad.body.src")?;
        let body_len = self
            .builder
            .build_int_sub(len, i64_type.const_int(1, false), "pad.body.len")
            .map_err(llvm_err)?;
        self.build_memcpy_call(body_dst, body_src, body_len)?;
        self.builder
            .build_unconditional_branch(done)
            .map_err(llvm_err)?;

        self.builder.position_at_end(plain);
        let dst = self.byte_offset(buf, offset, "pad.dst")?;
        self.build_memcpy_call(dst, source, len)?;
        self.builder
            .build_unconditional_branch(done)
            .map_err(llvm_err)?;

        self.builder.position_at_end(done);
        let padded = self.build_string_value(buf, width)?;
        self.builder.build_return(Some(&padded)).map_err(llvm_err)?;

        self.end_helper(saved);
        Ok(func)
    }

    /// `{ptr, len} __neuro_quote(i8* s, i64 len, i8 quote)` — wrap text in the
    /// given delimiter for `:?` rendering of `string` and `char`.
    pub(crate) fn get_or_define_quote(&self) -> CodegenResult<FunctionValue<'ctx>> {
        const NAME: &str = "__neuro_quote";
        if let Some(f) = self.module.get_function(NAME) {
            return Ok(f);
        }
        let i64_type = self.context.i64_type();
        let i8_type = self.context.i8_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let (func, saved) =
            self.begin_helper(NAME, &[ptr_type.into(), i64_type.into(), i8_type.into()]);

        let source = self.param(func, 0)?.into_pointer_value();
        let len = self.param(func, 1)?.into_int_value();
        let quote = self.param(func, 2)?.into_int_value();

        let total = self
            .builder
            .build_int_add(len, i64_type.const_int(2, false), "quote.len")
            .map_err(llvm_err)?;
        let buf = self.build_malloc(total, "quote.buf")?;
        self.store_byte(buf, i64_type.const_zero(), quote)?;
        let body = self.byte_offset(buf, i64_type.const_int(1, false), "quote.body")?;
        self.build_memcpy_call(body, source, len)?;
        let tail = self
            .builder
            .build_int_add(len, i64_type.const_int(1, false), "quote.tail")
            .map_err(llvm_err)?;
        self.store_byte(buf, tail, quote)?;

        let quoted = self.build_string_value(buf, total)?;
        self.builder.build_return(Some(&quoted)).map_err(llvm_err)?;
        self.end_helper(saved);
        Ok(func)
    }

    /// `{ptr, len} __neuro_utf8(i32 codepoint)` — encode one Unicode scalar value.
    /// A `char` is a code point, but a `string` is UTF-8 bytes, so interpolating a
    /// `char` has to encode rather than copy.
    pub(crate) fn get_or_define_utf8(&self) -> CodegenResult<FunctionValue<'ctx>> {
        const NAME: &str = "__neuro_utf8";
        if let Some(f) = self.module.get_function(NAME) {
            return Ok(f);
        }
        let i64_type = self.context.i64_type();
        let (func, saved) = self.begin_helper(NAME, &[self.context.i32_type().into()]);

        let code = self.param(func, 0)?.into_int_value();
        let wide = self
            .builder
            .build_int_z_extend(code, i64_type, "utf8.cp")
            .map_err(llvm_err)?;
        let buf = self.build_malloc(i64_type.const_int(4, false), "utf8.buf")?;
        let count = self
            .builder
            .build_alloca(i64_type, "utf8.count")
            .map_err(llvm_err)?;

        let one_byte = self.context.append_basic_block(func, "utf8.one");
        let check_two = self.context.append_basic_block(func, "utf8.check.two");
        let two_bytes = self.context.append_basic_block(func, "utf8.two");
        let check_three = self.context.append_basic_block(func, "utf8.check.three");
        let three_bytes = self.context.append_basic_block(func, "utf8.three");
        let four_bytes = self.context.append_basic_block(func, "utf8.four");
        let done = self.context.append_basic_block(func, "utf8.done");

        let below_ascii = self
            .builder
            .build_int_compare(
                IntPredicate::ULT,
                wide,
                i64_type.const_int(0x80, false),
                "utf8.ascii",
            )
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(below_ascii, one_byte, check_two)
            .map_err(llvm_err)?;

        // Each arm writes the lead byte with its length marker, then the
        // continuation bytes six bits at a time, high bits first.
        self.builder.position_at_end(one_byte);
        self.store_utf8_byte(buf, 0, &wide, 0, 0x00)?;
        self.builder
            .build_store(count, i64_type.const_int(1, false))
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(done)
            .map_err(llvm_err)?;

        self.builder.position_at_end(check_two);
        let below_two = self
            .builder
            .build_int_compare(
                IntPredicate::ULT,
                wide,
                i64_type.const_int(0x800, false),
                "utf8.two.range",
            )
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(below_two, two_bytes, check_three)
            .map_err(llvm_err)?;

        self.builder.position_at_end(two_bytes);
        self.store_utf8_byte(buf, 0, &wide, 6, 0xC0)?;
        self.store_utf8_byte(buf, 1, &wide, 0, 0x80)?;
        self.builder
            .build_store(count, i64_type.const_int(2, false))
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(done)
            .map_err(llvm_err)?;

        self.builder.position_at_end(check_three);
        let below_three = self
            .builder
            .build_int_compare(
                IntPredicate::ULT,
                wide,
                i64_type.const_int(0x10000, false),
                "utf8.three.range",
            )
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(below_three, three_bytes, four_bytes)
            .map_err(llvm_err)?;

        self.builder.position_at_end(three_bytes);
        self.store_utf8_byte(buf, 0, &wide, 12, 0xE0)?;
        self.store_utf8_byte(buf, 1, &wide, 6, 0x80)?;
        self.store_utf8_byte(buf, 2, &wide, 0, 0x80)?;
        self.builder
            .build_store(count, i64_type.const_int(3, false))
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(done)
            .map_err(llvm_err)?;

        self.builder.position_at_end(four_bytes);
        self.store_utf8_byte(buf, 0, &wide, 18, 0xF0)?;
        self.store_utf8_byte(buf, 1, &wide, 12, 0x80)?;
        self.store_utf8_byte(buf, 2, &wide, 6, 0x80)?;
        self.store_utf8_byte(buf, 3, &wide, 0, 0x80)?;
        self.builder
            .build_store(count, i64_type.const_int(4, false))
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(done)
            .map_err(llvm_err)?;

        self.builder.position_at_end(done);
        let len = self
            .builder
            .build_load(i64_type, count, "utf8.len")
            .map_err(llvm_err)?
            .into_int_value();
        let encoded = self.build_string_value(buf, len)?;
        self.builder
            .build_return(Some(&encoded))
            .map_err(llvm_err)?;

        self.end_helper(saved);
        Ok(func)
    }

    /// Write `marker | ((code >> shift) & 0x3F)` — or the whole low byte when
    /// `marker` is 0 — into `buf[index]`.
    fn store_utf8_byte(
        &self,
        buf: inkwell::values::PointerValue<'ctx>,
        index: u64,
        code: &IntValue<'ctx>,
        shift: u64,
        marker: u64,
    ) -> CodegenResult<()> {
        let i64_type = self.context.i64_type();
        let i8_type = self.context.i8_type();
        let shifted = self
            .builder
            .build_right_shift(*code, i64_type.const_int(shift, false), false, "utf8.shift")
            .map_err(llvm_err)?;
        let mask = if marker == 0 { 0x7F } else { 0x3F };
        let bits = self
            .builder
            .build_and(shifted, i64_type.const_int(mask, false), "utf8.bits")
            .map_err(llvm_err)?;
        let tagged = self
            .builder
            .build_or(bits, i64_type.const_int(marker, false), "utf8.tagged")
            .map_err(llvm_err)?;
        let byte = self
            .builder
            .build_int_truncate(tagged, i8_type, "utf8.byte")
            .map_err(llvm_err)?;
        self.store_byte(buf, i64_type.const_int(index, false), byte)
    }
}
