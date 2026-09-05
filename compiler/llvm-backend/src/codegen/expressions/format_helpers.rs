// Runtime helper functions backing interpolation's format mini-language.
//
// Each helper is emitted once per module with internal linkage and returns the
// `{ ptr, i64 }` string fat pointer. They exist as real functions rather than
// inline IR because a program with many holes would otherwise duplicate the same
// scan-and-copy loops at every site.

use inkwell::module::Linkage;
use inkwell::values::{BasicValueEnum, FunctionValue, IntValue, PointerValue};
use inkwell::{AddressSpace, IntPredicate};

use shared_types::MAX_FORMAT_PRECISION;

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};

/// Digits are written as ASCII, so a bit value is offset by `'0'`.
const ASCII_ZERO: u64 = b'0' as u64;

/// Scratch bytes the integer renderer reserves. The widest rendering it can produce is
/// a 64-bit magnitude in octal — 22 digits — plus a sign, and octal is the sparsest
/// radix it handles, so 24 leaves the loop unable to run off the front of the buffer.
const MAX_INT_TEXT_BYTES: u64 = 24;

/// Scratch bytes the float renderer renders into before copying the text out. The
/// widest conversion the format mini-language admits is `%.Nf` on an `f64` of full
/// magnitude: a sign, 309 integer digits, the point, and `N` decimals, where `N` is
/// capped at [`MAX_FORMAT_PRECISION`] by semantic analysis. The rest is slack — a
/// wrong guess here costs a second render, not correctness.
const SCRATCH_TEXT_BYTES: u64 = 512 + MAX_FORMAT_PRECISION as u64;

impl<'ctx> CodegenContext<'ctx> {
    /// The `{ ptr, i64 }` layout every string value and format helper uses.
    pub(crate) fn string_fat_ptr_type(&self) -> inkwell::types::StructType<'ctx> {
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        self.context
            .struct_type(&[ptr_type.into(), self.context.i64_type().into()], false)
    }

    /// Assemble a `{ ptr, len }` string value from its two fields.
    pub(crate) fn build_string_value(
        &self,
        ptr: PointerValue<'ctx>,
        len: IntValue<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let with_ptr = self
            .builder
            .build_insert_value(self.string_fat_ptr_type().get_undef(), ptr, 0, "str.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_struct_value();
        let full = self
            .builder
            .build_insert_value(with_ptr, len, 1, "str")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(full.into_struct_value().into())
    }

    /// `snprintf(buf, size, fmt, ...) -> i32`. The `(NULL, 0)` probe form gives the
    /// exact rendered length, so every helper allocates precisely once.
    fn get_or_declare_snprintf(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("snprintf") {
            return f;
        }
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let fn_type = self.context.i32_type().fn_type(
            &[
                ptr_type.into(),
                self.context.i64_type().into(),
                ptr_type.into(),
            ],
            true,
        );
        self.module
            .add_function("snprintf", fn_type, Some(Linkage::External))
    }

    /// Start a helper definition: declare it, remember where the caller was
    /// building, and position the builder in the new entry block.
    pub(crate) fn begin_helper(
        &self,
        name: &str,
        params: &[inkwell::types::BasicMetadataTypeEnum<'ctx>],
    ) -> (
        FunctionValue<'ctx>,
        Option<inkwell::basic_block::BasicBlock<'ctx>>,
    ) {
        let fn_type = self.string_fat_ptr_type().fn_type(params, false);
        let func = self
            .module
            .add_function(name, fn_type, Some(Linkage::Internal));
        let saved = self.builder.get_insert_block();
        let entry = self.context.append_basic_block(func, "entry");
        self.builder.position_at_end(entry);
        (func, saved)
    }

    pub(crate) fn end_helper(&self, saved: Option<inkwell::basic_block::BasicBlock<'ctx>>) {
        if let Some(block) = saved {
            self.builder.position_at_end(block);
        }
    }

    pub(crate) fn param(
        &self,
        func: FunctionValue<'ctx>,
        index: u32,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        func.get_nth_param(index).ok_or_else(|| {
            CodegenError::InternalError(format!("format helper is missing parameter {}", index))
        })
    }

    pub(crate) fn build_malloc(
        &self,
        size: IntValue<'ctx>,
        name: &str,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let malloc = self.get_or_declare_malloc();
        self.builder
            .build_call(malloc, &[size.into()], name)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("malloc returned void".to_string()))
            .map(|value| value.into_pointer_value())
    }

    pub(crate) fn build_memcpy_call(
        &self,
        dst: PointerValue<'ctx>,
        src: PointerValue<'ctx>,
        len: IntValue<'ctx>,
    ) -> CodegenResult<()> {
        let memcpy = self.get_or_declare_memcpy();
        self.builder
            .build_call(memcpy, &[dst.into(), src.into(), len.into()], "")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// `buf + offset` as an `i8*`.
    pub(crate) fn byte_offset(
        &self,
        buf: PointerValue<'ctx>,
        offset: IntValue<'ctx>,
        name: &str,
    ) -> CodegenResult<PointerValue<'ctx>> {
        // SAFETY: every call site offsets within a buffer it has just sized to at
        // least `offset` bytes, so the resulting pointer stays inside the allocation.
        unsafe {
            self.builder
                .build_in_bounds_gep(self.context.i8_type(), buf, &[offset], name)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))
        }
    }

    pub(crate) fn store_byte(
        &self,
        buf: PointerValue<'ctx>,
        offset: IntValue<'ctx>,
        byte: IntValue<'ctx>,
    ) -> CodegenResult<()> {
        let slot = self.byte_offset(buf, offset, "byte.slot")?;
        self.builder
            .build_store(slot, byte)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    pub(crate) fn load_byte(
        &self,
        buf: PointerValue<'ctx>,
        offset: IntValue<'ctx>,
        name: &str,
    ) -> CodegenResult<IntValue<'ctx>> {
        let slot = self.byte_offset(buf, offset, "load.slot")?;
        self.builder
            .build_load(self.context.i8_type(), slot, name)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
            .map(|value| value.into_int_value())
    }

    /// Read `buf[index]`, or `0` when `valid` is false. The index is clamped to 0
    /// rather than branched around: every caller guarantees at least one byte, so
    /// the clamped load stays inside the allocation and its result is discarded.
    pub(crate) fn load_byte_guarded(
        &self,
        buf: PointerValue<'ctx>,
        index: IntValue<'ctx>,
        valid: IntValue<'ctx>,
        name: &str,
    ) -> CodegenResult<IntValue<'ctx>> {
        let i64_type = self.context.i64_type();
        let i8_type = self.context.i8_type();
        let llvm_err = |e: inkwell::builder::BuilderError| CodegenError::LlvmError(e.to_string());
        let safe_index = self
            .builder
            .build_select(valid, index, i64_type.const_zero(), "guard.index")
            .map_err(llvm_err)?
            .into_int_value();
        let loaded = self.load_byte(buf, safe_index, name)?;
        self.builder
            .build_select(valid, loaded, i8_type.const_zero(), "guard.byte")
            .map_err(llvm_err)
            .map(|value| value.into_int_value())
    }

    /// `{ptr, len} __neuro_fmt_int_<radix>(i64 magnitude, i8 sign)` — render an
    /// unsigned magnitude in `radix`, prefixed by the ASCII byte `sign` unless that
    /// byte is zero.
    ///
    /// This used to go through `snprintf`, twice: once against `(NULL, 0)` to learn the
    /// rendered length and once more to render it. That is two traversals of a printf
    /// format string, two variadic calls, and a locale consultation to turn an integer
    /// into at most twenty digits — and it dominated the cost of every interpolated
    /// hole, which is the most common thing a Neuro program does with a number. The
    /// digits are produced directly here instead: one pass, no probe, and the divisor
    /// is a compile-time constant per radix, so instruction selection turns it into a
    /// multiply-and-shift rather than a hardware division.
    ///
    /// The caller supplies the magnitude and the sign byte separately because the
    /// checker has already narrowed what can reach here: `+` is rejected on unsigned
    /// values and on the radix conversions, so a sign is only ever possible on signed
    /// decimal, and the negation that produces the magnitude belongs at the one call
    /// site that can need it. `0 - i64::MIN` wraps to the bit pattern of `2^63`, which
    /// read as unsigned is exactly the magnitude wanted.
    pub(crate) fn get_or_define_fmt_int(
        &self,
        radix: u64,
        upper: bool,
    ) -> CodegenResult<FunctionValue<'ctx>> {
        let name = format!("__neuro_fmt_int_{}{}", radix, if upper { "u" } else { "" });
        if let Some(f) = self.module.get_function(&name) {
            return Ok(f);
        }
        let llvm_err = |e: inkwell::builder::BuilderError| CodegenError::LlvmError(e.to_string());
        let i64_type = self.context.i64_type();
        let i8_type = self.context.i8_type();
        let scratch_ty = i8_type.array_type(MAX_INT_TEXT_BYTES as u32);
        let (func, saved) = self.begin_helper(&name, &[i64_type.into(), i8_type.into()]);

        let magnitude = self.param(func, 0)?.into_int_value();
        let sign = self.param(func, 1)?.into_int_value();
        let table = self
            .builder
            .build_global_string_ptr(
                if upper {
                    "0123456789ABCDEF"
                } else {
                    "0123456789abcdef"
                },
                "fmt.digits",
            )
            .map_err(llvm_err)?
            .as_pointer_value();

        // Digits come out least significant first, so they are written backwards into a
        // fixed scratch buffer and the finished text is the buffer's tail.
        let scratch = self
            .builder
            .build_alloca(scratch_ty, "fmt.scratch")
            .map_err(llvm_err)?;
        let cursor = self
            .builder
            .build_alloca(i64_type, "fmt.cursor")
            .map_err(llvm_err)?;
        let rest = self
            .builder
            .build_alloca(i64_type, "fmt.rest")
            .map_err(llvm_err)?;
        self.builder
            .build_store(cursor, i64_type.const_int(MAX_INT_TEXT_BYTES, false))
            .map_err(llvm_err)?;
        self.builder
            .build_store(rest, magnitude)
            .map_err(llvm_err)?;

        let digit_bb = self.context.append_basic_block(func, "fmt.digit");
        let sign_bb = self.context.append_basic_block(func, "fmt.sign");
        let prefix_bb = self.context.append_basic_block(func, "fmt.prefix");
        let done_bb = self.context.append_basic_block(func, "fmt.done");

        // A do-while, not a while: zero renders as the single digit `0`.
        self.builder
            .build_unconditional_branch(digit_bb)
            .map_err(llvm_err)?;

        self.builder.position_at_end(digit_bb);
        let value = self
            .builder
            .build_load(i64_type, rest, "fmt.rest.load")
            .map_err(llvm_err)?
            .into_int_value();
        let radix_c = i64_type.const_int(radix, false);
        let digit = self
            .builder
            .build_int_unsigned_rem(value, radix_c, "fmt.digit.value")
            .map_err(llvm_err)?;
        let quotient = self
            .builder
            .build_int_unsigned_div(value, radix_c, "fmt.rest.next")
            .map_err(llvm_err)?;
        // SAFETY: `digit` is below `radix`, and every radix reaching here is at most 16,
        // so the index stays inside the sixteen-byte digit table.
        let digit_ptr = unsafe {
            self.builder
                .build_in_bounds_gep(i8_type, table, &[digit], "fmt.digit.ptr")
                .map_err(llvm_err)?
        };
        let character = self
            .builder
            .build_load(i8_type, digit_ptr, "fmt.digit.char")
            .map_err(llvm_err)?
            .into_int_value();
        self.write_scratch_byte(scratch_ty, scratch, cursor, character)?;
        self.builder.build_store(rest, quotient).map_err(llvm_err)?;
        let more = self
            .builder
            .build_int_compare(
                IntPredicate::NE,
                quotient,
                i64_type.const_zero(),
                "fmt.more",
            )
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(more, digit_bb, sign_bb)
            .map_err(llvm_err)?;

        self.builder.position_at_end(sign_bb);
        let signed = self
            .builder
            .build_int_compare(IntPredicate::NE, sign, i8_type.const_zero(), "fmt.has.sign")
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(signed, prefix_bb, done_bb)
            .map_err(llvm_err)?;

        self.builder.position_at_end(prefix_bb);
        self.write_scratch_byte(scratch_ty, scratch, cursor, sign)?;
        self.builder
            .build_unconditional_branch(done_bb)
            .map_err(llvm_err)?;

        self.builder.position_at_end(done_bb);
        let start = self
            .builder
            .build_load(i64_type, cursor, "fmt.start")
            .map_err(llvm_err)?
            .into_int_value();
        let len = self
            .builder
            .build_int_sub(
                i64_type.const_int(MAX_INT_TEXT_BYTES, false),
                start,
                "fmt.len",
            )
            .map_err(llvm_err)?;
        let buf = self.build_malloc(len, "fmt.buf")?;
        // SAFETY: `start` is the index of the first byte written, so it is within the
        // scratch buffer and `len` bytes follow it up to the buffer's end.
        let text = unsafe {
            self.builder
                .build_in_bounds_gep(
                    scratch_ty,
                    scratch,
                    &[i64_type.const_zero(), start],
                    "fmt.text",
                )
                .map_err(llvm_err)?
        };
        self.build_memcpy_call(buf, text, len)?;
        let result = self.build_string_value(buf, len)?;

        self.builder.build_return(Some(&result)).map_err(llvm_err)?;
        self.end_helper(saved);
        Ok(func)
    }

    /// Step `cursor` back one byte and write `byte` there.
    fn write_scratch_byte(
        &self,
        scratch_ty: inkwell::types::ArrayType<'ctx>,
        scratch: PointerValue<'ctx>,
        cursor: PointerValue<'ctx>,
        byte: IntValue<'ctx>,
    ) -> CodegenResult<()> {
        let llvm_err = |e: inkwell::builder::BuilderError| CodegenError::LlvmError(e.to_string());
        let i64_type = self.context.i64_type();
        let index = self
            .builder
            .build_load(i64_type, cursor, "fmt.cursor.load")
            .map_err(llvm_err)?
            .into_int_value();
        let next = self
            .builder
            .build_int_sub(index, i64_type.const_int(1, false), "fmt.cursor.next")
            .map_err(llvm_err)?;
        // SAFETY: the buffer is sized to the widest rendering any radix can produce, so
        // the cursor cannot walk off its front before the digit loop ends.
        let slot = unsafe {
            self.builder
                .build_in_bounds_gep(
                    scratch_ty,
                    scratch,
                    &[i64_type.const_zero(), next],
                    "fmt.slot",
                )
                .map_err(llvm_err)?
        };
        self.builder.build_store(slot, byte).map_err(llvm_err)?;
        self.builder.build_store(cursor, next).map_err(llvm_err)?;
        Ok(())
    }

    /// `{ptr, len} __neuro_fmt_flt(double value, i8* fmt)` — the float twin of
    /// [`Self::get_or_define_fmt_int`].
    pub(crate) fn get_or_define_fmt_float(&self) -> CodegenResult<FunctionValue<'ctx>> {
        const NAME: &str = "__neuro_fmt_flt";
        if let Some(f) = self.module.get_function(NAME) {
            return Ok(f);
        }
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let (func, saved) =
            self.begin_helper(NAME, &[self.context.f64_type().into(), ptr_type.into()]);

        let value = self.param(func, 0)?;
        let fmt = self.param(func, 1)?.into_pointer_value();
        let result = self.build_snprintf_alloc(func, fmt, value)?;

        self.builder
            .build_return(Some(&result))
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.end_helper(saved);
        Ok(func)
    }

    /// Render `value` through `snprintf` into a fresh, exactly-sized buffer.
    ///
    /// `snprintf` returns the length it *would* have written whatever it was given, so
    /// one call into a scratch buffer large enough for every conversion the format
    /// mini-language admits yields both the text and its length. The obvious
    /// alternative — a `(NULL, 0)` probe call for the length, then a second call to
    /// render — costs two traversals of the format string and two `double`-to-decimal
    /// conversions for one result, and float rendering is hot enough in a language that
    /// prints numbers for a living to be worth the stack.
    ///
    /// The oversize branch exists so correctness never rests on
    /// [`SCRATCH_TEXT_BYTES`] being a large enough guess: when the render did not fit,
    /// it falls back to allocating what `snprintf` asked for and rendering again.
    ///
    /// `helper` is the function being defined, not the one that called for it: this runs
    /// inside a helper body the builder was moved into, so `current_function` still
    /// names the caller and the blocks below would be appended to the wrong function.
    fn build_snprintf_alloc(
        &self,
        helper: FunctionValue<'ctx>,
        fmt: PointerValue<'ctx>,
        value: BasicValueEnum<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let snprintf = self.get_or_declare_snprintf();
        let i64_type = self.context.i64_type();
        let i8_type = self.context.i8_type();
        let llvm_err = |e: inkwell::builder::BuilderError| CodegenError::LlvmError(e.to_string());

        let scratch_ty = i8_type.array_type(SCRATCH_TEXT_BYTES as u32);
        let scratch = self
            .builder
            .build_alloca(scratch_ty, "fmt.scratch")
            .map_err(llvm_err)?;
        let capacity = i64_type.const_int(SCRATCH_TEXT_BYTES, false);
        let written = self
            .builder
            .build_call(
                snprintf,
                &[scratch.into(), capacity.into(), fmt.into(), value.into()],
                "fmt.render",
            )
            .map_err(llvm_err)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("snprintf returned void".to_string()))?
            .into_int_value();

        let widened = self
            .builder
            .build_int_s_extend(written, i64_type, "fmt.len.wide")
            .map_err(llvm_err)?;
        // A negative return means the C library refused the conversion; clamp so the
        // allocation below can never be handed a wrapped-around size.
        let negative = self
            .builder
            .build_int_compare(
                IntPredicate::SLT,
                widened,
                i64_type.const_zero(),
                "fmt.len.neg",
            )
            .map_err(llvm_err)?;
        let len = self
            .builder
            .build_select(negative, i64_type.const_zero(), widened, "fmt.len")
            .map_err(llvm_err)?
            .into_int_value();

        let fits = self
            .builder
            .build_int_compare(IntPredicate::ULT, len, capacity, "fmt.fits")
            .map_err(llvm_err)?;
        let copy_bb = self.context.append_basic_block(helper, "fmt.copy");
        let again_bb = self.context.append_basic_block(helper, "fmt.again");
        let done_bb = self.context.append_basic_block(helper, "fmt.joined");
        let branch = self
            .builder
            .build_conditional_branch(fits, copy_bb, again_bb)
            .map_err(llvm_err)?;
        self.mark_cold_branch(branch)?;

        self.builder.position_at_end(copy_bb);
        let copied = self.build_malloc(len, "fmt.buf")?;
        self.build_memcpy_call(copied, scratch, len)?;
        self.builder
            .build_unconditional_branch(done_bb)
            .map_err(llvm_err)?;

        self.builder.position_at_end(again_bb);
        let needed = self
            .builder
            .build_int_add(len, i64_type.const_int(1, false), "fmt.cap.big")
            .map_err(llvm_err)?;
        let rendered = self.build_malloc(needed, "fmt.buf.big")?;
        self.builder
            .build_call(
                snprintf,
                &[rendered.into(), needed.into(), fmt.into(), value.into()],
                "",
            )
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(done_bb)
            .map_err(llvm_err)?;

        self.builder.position_at_end(done_bb);
        let buf = self
            .builder
            .build_phi(self.context.ptr_type(AddressSpace::default()), "fmt.text")
            .map_err(llvm_err)?;
        buf.add_incoming(&[(&copied, copy_bb), (&rendered, again_bb)]);

        self.build_string_value(buf.as_basic_value().into_pointer_value(), len)
    }

    /// `{ptr, len} __neuro_fmt_bin(i64 masked)` — base-2 digits of an already
    /// width-masked value, most significant first, with no leading zeros.
    /// `printf` has no binary conversion, so this is written out by hand.
    pub(crate) fn get_or_define_fmt_binary(&self) -> CodegenResult<FunctionValue<'ctx>> {
        const NAME: &str = "__neuro_fmt_bin";
        if let Some(f) = self.module.get_function(NAME) {
            return Ok(f);
        }
        let i64_type = self.context.i64_type();
        let i8_type = self.context.i8_type();
        let llvm_err = |e: inkwell::builder::BuilderError| CodegenError::LlvmError(e.to_string());
        let (func, saved) = self.begin_helper(NAME, &[i64_type.into()]);

        let value = self.param(func, 0)?.into_int_value();
        let digits = self
            .builder
            .build_alloca(i64_type, "bin.digits")
            .map_err(llvm_err)?;
        let cursor = self
            .builder
            .build_alloca(i64_type, "bin.cursor")
            .map_err(llvm_err)?;
        self.builder
            .build_store(digits, i64_type.const_int(1, false))
            .map_err(llvm_err)?;
        let shifted = self
            .builder
            .build_right_shift(value, i64_type.const_int(1, false), false, "bin.rest")
            .map_err(llvm_err)?;
        self.builder
            .build_store(cursor, shifted)
            .map_err(llvm_err)?;

        let count_head = self.context.append_basic_block(func, "count.head");
        let count_body = self.context.append_basic_block(func, "count.body");
        let write_setup = self.context.append_basic_block(func, "write.setup");
        let write_head = self.context.append_basic_block(func, "write.head");
        let write_body = self.context.append_basic_block(func, "write.body");
        let done = self.context.append_basic_block(func, "done");

        self.builder
            .build_unconditional_branch(count_head)
            .map_err(llvm_err)?;

        self.builder.position_at_end(count_head);
        let rest = self
            .builder
            .build_load(i64_type, cursor, "bin.rest.load")
            .map_err(llvm_err)?
            .into_int_value();
        let more = self
            .builder
            .build_int_compare(IntPredicate::NE, rest, i64_type.const_zero(), "bin.more")
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(more, count_body, write_setup)
            .map_err(llvm_err)?;

        self.builder.position_at_end(count_body);
        let count = self
            .builder
            .build_load(i64_type, digits, "bin.count")
            .map_err(llvm_err)?
            .into_int_value();
        let bumped = self
            .builder
            .build_int_add(count, i64_type.const_int(1, false), "bin.count.next")
            .map_err(llvm_err)?;
        self.builder.build_store(digits, bumped).map_err(llvm_err)?;
        let narrowed = self
            .builder
            .build_right_shift(rest, i64_type.const_int(1, false), false, "bin.rest.next")
            .map_err(llvm_err)?;
        self.builder
            .build_store(cursor, narrowed)
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(count_head)
            .map_err(llvm_err)?;

        self.builder.position_at_end(write_setup);
        let total = self
            .builder
            .build_load(i64_type, digits, "bin.total")
            .map_err(llvm_err)?
            .into_int_value();
        let buf = self.build_malloc(total, "bin.buf")?;
        let index = self
            .builder
            .build_alloca(i64_type, "bin.index")
            .map_err(llvm_err)?;
        self.builder
            .build_store(index, i64_type.const_zero())
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(write_head)
            .map_err(llvm_err)?;

        self.builder.position_at_end(write_head);
        let i = self
            .builder
            .build_load(i64_type, index, "bin.i")
            .map_err(llvm_err)?
            .into_int_value();
        let in_range = self
            .builder
            .build_int_compare(IntPredicate::ULT, i, total, "bin.in.range")
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(in_range, write_body, done)
            .map_err(llvm_err)?;

        self.builder.position_at_end(write_body);
        let from_end = self
            .builder
            .build_int_sub(total, i64_type.const_int(1, false), "bin.last")
            .map_err(llvm_err)?;
        let shift = self
            .builder
            .build_int_sub(from_end, i, "bin.shift")
            .map_err(llvm_err)?;
        let bit = self
            .builder
            .build_right_shift(value, shift, false, "bin.bit.wide")
            .map_err(llvm_err)?;
        let masked = self
            .builder
            .build_and(bit, i64_type.const_int(1, false), "bin.bit")
            .map_err(llvm_err)?;
        let digit = self
            .builder
            .build_int_add(masked, i64_type.const_int(ASCII_ZERO, false), "bin.ascii")
            .map_err(llvm_err)?;
        let byte = self
            .builder
            .build_int_truncate(digit, i8_type, "bin.byte")
            .map_err(llvm_err)?;
        self.store_byte(buf, i, byte)?;
        let next = self
            .builder
            .build_int_add(i, i64_type.const_int(1, false), "bin.i.next")
            .map_err(llvm_err)?;
        self.builder.build_store(index, next).map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(write_head)
            .map_err(llvm_err)?;

        self.builder.position_at_end(done);
        let result = self.build_string_value(buf, total)?;
        self.builder.build_return(Some(&result)).map_err(llvm_err)?;

        self.end_helper(saved);
        Ok(func)
    }
}
