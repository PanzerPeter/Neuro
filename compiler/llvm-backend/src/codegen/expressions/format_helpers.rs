// Runtime helper functions backing interpolation's format mini-language.
//
// Each helper is emitted once per module with internal linkage and returns the
// `{ ptr, i64 }` string fat pointer. They exist as real functions rather than
// inline IR because a program with many holes would otherwise duplicate the same
// scan-and-copy loops at every site.

use inkwell::module::Linkage;
use inkwell::values::{BasicValueEnum, FunctionValue, IntValue, PointerValue};
use inkwell::{AddressSpace, IntPredicate};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};

/// Digits are written as ASCII, so a bit value is offset by `'0'`.
const ASCII_ZERO: u64 = b'0' as u64;

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

    /// `{ptr, len} __neuro_fmt_int(i64 value, i8* fmt)` — render `value` through
    /// `snprintf` with a caller-supplied conversion (`%lld`, `%llx`, …).
    pub(crate) fn get_or_define_fmt_int(&self) -> CodegenResult<FunctionValue<'ctx>> {
        const NAME: &str = "__neuro_fmt_int";
        if let Some(f) = self.module.get_function(NAME) {
            return Ok(f);
        }
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let (func, saved) = self.begin_helper(NAME, &[i64_type.into(), ptr_type.into()]);

        let value = self.param(func, 0)?;
        let fmt = self.param(func, 1)?.into_pointer_value();
        let result = self.build_snprintf_alloc(fmt, value)?;

        self.builder
            .build_return(Some(&result))
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.end_helper(saved);
        Ok(func)
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
        let result = self.build_snprintf_alloc(fmt, value)?;

        self.builder
            .build_return(Some(&result))
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.end_helper(saved);
        Ok(func)
    }

    /// Probe for the rendered length, allocate exactly that (plus the terminator
    /// `snprintf` insists on writing), then render for real.
    fn build_snprintf_alloc(
        &self,
        fmt: PointerValue<'ctx>,
        value: BasicValueEnum<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let snprintf = self.get_or_declare_snprintf();
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let llvm_err = |e: inkwell::builder::BuilderError| CodegenError::LlvmError(e.to_string());

        let probe = self
            .builder
            .build_call(
                snprintf,
                &[
                    ptr_type.const_null().into(),
                    i64_type.const_zero().into(),
                    fmt.into(),
                    value.into(),
                ],
                "fmt.probe",
            )
            .map_err(llvm_err)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("snprintf returned void".to_string()))?
            .into_int_value();

        let widened = self
            .builder
            .build_int_s_extend(probe, i64_type, "fmt.len.wide")
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

        let capacity = self
            .builder
            .build_int_add(len, i64_type.const_int(1, false), "fmt.cap")
            .map_err(llvm_err)?;
        let buf = self.build_malloc(capacity, "fmt.buf")?;
        self.builder
            .build_call(
                snprintf,
                &[buf.into(), capacity.into(), fmt.into(), value.into()],
                "",
            )
            .map_err(llvm_err)?;

        self.build_string_value(buf, len)
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
