// The two fix-ups that reconcile C's float output with the language's specifier table.

use inkwell::values::FunctionValue;
use inkwell::{AddressSpace, IntPredicate};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};

impl<'ctx> CodegenContext<'ctx> {
    /// `{ptr, len} __neuro_point(i8* s, i64 len)`, which appends `.0` when `%g` produced
    /// a bare integer.
    ///
    /// Default float rendering goes through `%.16g`, which drops the fraction of a
    /// whole number and prints `2` for `2.0`. Restoring the point keeps a float
    /// visibly a float, matching what `{x}` shows for every other value of the type.
    /// Text already carrying `.`, an exponent, or `inf`/`nan` is returned untouched.
    pub(crate) fn get_or_define_ensure_point(&self) -> CodegenResult<FunctionValue<'ctx>> {
        const NAME: &str = "__neuro_point";
        if let Some(f) = self.module.get_function(NAME) {
            return Ok(f);
        }
        let i64_type = self.context.i64_type();
        let i8_type = self.context.i8_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let (func, saved) = self.begin_helper(NAME, &[ptr_type.into(), i64_type.into()]);

        let source = self.param(func, 0)?.into_pointer_value();
        let len = self.param(func, 1)?.into_int_value();

        let cursor = self.builder.build_alloca(i64_type, "pt.cursor")?;
        self.builder.build_store(cursor, i64_type.const_zero())?;

        let scan_head = self.context.append_basic_block(func, "pt.scan");
        let scan_body = self.context.append_basic_block(func, "pt.body");
        let scan_next = self.context.append_basic_block(func, "pt.next");
        let unchanged = self.context.append_basic_block(func, "pt.unchanged");
        let append = self.context.append_basic_block(func, "pt.append");

        self.builder.build_unconditional_branch(scan_head)?;

        self.builder.position_at_end(scan_head);
        let i = self
            .builder
            .build_load(i64_type, cursor, "pt.i")?
            .into_int_value();
        let in_range = self
            .builder
            .build_int_compare(IntPredicate::ULT, i, len, "pt.in.range")?;
        self.builder
            .build_conditional_branch(in_range, scan_body, append)?;

        self.builder.position_at_end(scan_body);
        let byte = self.load_byte(source, i, "pt.byte")?;
        // `.` and `e` mean a fraction or exponent is already present; `n`/`i` catch
        // `nan` and `inf`, which must not grow a fraction.
        let mut already = None;
        for marker in *b".eni" {
            let hit = self.builder.build_int_compare(
                IntPredicate::EQ,
                byte,
                i8_type.const_int(u64::from(marker), false),
                "pt.hit",
            )?;
            already = Some(match already {
                None => hit,
                Some(prev) => self.builder.build_or(prev, hit, "pt.any")?,
            });
        }
        let already = already
            .ok_or_else(|| CodegenError::InternalError("empty float marker set".to_string()))?;
        self.builder
            .build_conditional_branch(already, unchanged, scan_next)?;

        self.builder.position_at_end(scan_next);
        let next = self
            .builder
            .build_int_add(i, i64_type.const_int(1, false), "pt.i.next")?;
        self.builder.build_store(cursor, next)?;
        self.builder.build_unconditional_branch(scan_head)?;

        self.builder.position_at_end(unchanged);
        let as_is = self.build_string_value(source, len)?;
        self.builder.build_return(Some(&as_is))?;

        self.builder.position_at_end(append);
        let total = self
            .builder
            .build_int_add(len, i64_type.const_int(2, false), "pt.total")?;
        let buf = self.build_malloc(total, "pt.buf")?;
        self.build_memcpy_call(buf, source, len)?;
        self.store_byte(buf, len, i8_type.const_int(u64::from(b'.'), false))?;
        let last = self
            .builder
            .build_int_add(len, i64_type.const_int(1, false), "pt.last")?;
        self.store_byte(buf, last, i8_type.const_int(u64::from(b'0'), false))?;
        let extended = self.build_string_value(buf, total)?;
        self.builder.build_return(Some(&extended))?;

        self.end_helper(saved);
        Ok(func)
    }

    /// `{ptr, len} __neuro_exp(i8* s, i64 len, i1 trim)`, which turns C's scientific
    /// output into the form the language specifies.
    ///
    /// `%e` writes `3.141590e+00`; the table specifies `3.14159e0`. Two fix-ups get
    /// there: the exponent loses its `+` and its leading zeros (keeping one digit),
    /// and, when `trim` is set (which it is for a hole that named no
    /// precision), the mantissa loses the trailing zeros the fixed conversion padded it with.
    /// An explicit `{x:.2e}` clears `trim`, because there the zeros were asked for.
    /// Output with no `e` (`inf`, `nan`) is returned untouched.
    pub(crate) fn get_or_define_normalize_exponent(&self) -> CodegenResult<FunctionValue<'ctx>> {
        const NAME: &str = "__neuro_exp";
        if let Some(f) = self.module.get_function(NAME) {
            return Ok(f);
        }
        let i64_type = self.context.i64_type();
        let i8_type = self.context.i8_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let (func, saved) = self.begin_helper(
            NAME,
            &[
                ptr_type.into(),
                i64_type.into(),
                self.context.bool_type().into(),
            ],
        );

        let source = self.param(func, 0)?.into_pointer_value();
        let len = self.param(func, 1)?.into_int_value();
        let trim = self.param(func, 2)?.into_int_value();

        let cursor = self.builder.build_alloca(i64_type, "exp.cursor")?;
        let mantissa = self.builder.build_alloca(i64_type, "exp.mantissa")?;
        let digit_cursor = self.builder.build_alloca(i64_type, "exp.digit.cursor")?;
        let saw_point = self
            .builder
            .build_alloca(self.context.bool_type(), "exp.saw.point")?;
        self.builder.build_store(cursor, i64_type.const_zero())?;
        self.builder
            .build_store(saw_point, self.context.bool_type().const_zero())?;

        let find_head = self.context.append_basic_block(func, "exp.find");
        let find_body = self.context.append_basic_block(func, "exp.find.body");
        let find_next = self.context.append_basic_block(func, "exp.find.next");
        let unchanged = self.context.append_basic_block(func, "exp.unchanged");
        let read_sign = self.context.append_basic_block(func, "exp.sign");
        let trim_head = self.context.append_basic_block(func, "exp.trim");
        let trim_check = self.context.append_basic_block(func, "exp.trim.check");
        let trim_next = self.context.append_basic_block(func, "exp.trim.next");
        let trim_end = self.context.append_basic_block(func, "exp.trim.end");
        let trim_skip = self.context.append_basic_block(func, "exp.trim.skip");
        let skip_head = self.context.append_basic_block(func, "exp.skip");
        let skip_check = self.context.append_basic_block(func, "exp.skip.check");
        let skip_next = self.context.append_basic_block(func, "exp.skip.next");
        let rebuild = self.context.append_basic_block(func, "exp.rebuild");
        let put_sign = self.context.append_basic_block(func, "exp.put.sign");
        let after_sign = self.context.append_basic_block(func, "exp.after.sign");

        self.builder.build_unconditional_branch(find_head)?;

        self.builder.position_at_end(find_head);
        let i = self
            .builder
            .build_load(i64_type, cursor, "exp.i")?
            .into_int_value();
        let in_range = self
            .builder
            .build_int_compare(IntPredicate::ULT, i, len, "exp.in.range")?;
        self.builder
            .build_conditional_branch(in_range, find_body, unchanged)?;

        self.builder.position_at_end(find_body);
        let byte = self.load_byte(source, i, "exp.byte")?;
        // Whether the mantissa has a decimal point decides if its trailing zeros
        // are padding (`3.141590`) or the value's own digits (`100`).
        let is_point_byte = self.builder.build_int_compare(
            IntPredicate::EQ,
            byte,
            i8_type.const_int(u64::from(b'.'), false),
            "exp.byte.point",
        )?;
        let seen = self
            .builder
            .build_load(self.context.bool_type(), saw_point, "exp.seen")?
            .into_int_value();
        let seen_now = self.builder.build_or(seen, is_point_byte, "exp.seen.now")?;
        self.builder.build_store(saw_point, seen_now)?;
        let is_e = self.builder.build_int_compare(
            IntPredicate::EQ,
            byte,
            i8_type.const_int(u64::from(b'e'), false),
            "exp.is.e",
        )?;
        self.builder
            .build_conditional_branch(is_e, read_sign, find_next)?;

        self.builder.position_at_end(find_next);
        let next = self
            .builder
            .build_int_add(i, i64_type.const_int(1, false), "exp.i.next")?;
        self.builder.build_store(cursor, next)?;
        self.builder.build_unconditional_branch(find_head)?;

        self.builder.position_at_end(unchanged);
        let as_is = self.build_string_value(source, len)?;
        self.builder.build_return(Some(&as_is))?;

        self.builder.position_at_end(read_sign);
        let marker = self
            .builder
            .build_load(i64_type, cursor, "exp.marker")?
            .into_int_value();
        let head = self
            .builder
            .build_int_add(marker, i64_type.const_int(1, false), "exp.head")?;
        let has_more =
            self.builder
                .build_int_compare(IntPredicate::ULT, head, len, "exp.has.more")?;
        let sign_byte = self.load_byte_guarded(source, head, has_more, "exp.sign.byte")?;
        let negative = self.builder.build_int_compare(
            IntPredicate::EQ,
            sign_byte,
            i8_type.const_int(u64::from(b'-'), false),
            "exp.negative",
        )?;
        let positive = self.builder.build_int_compare(
            IntPredicate::EQ,
            sign_byte,
            i8_type.const_int(u64::from(b'+'), false),
            "exp.positive",
        )?;
        let signed = self.builder.build_or(negative, positive, "exp.signed")?;
        let after_marker =
            self.builder
                .build_int_add(head, i64_type.const_int(1, false), "exp.after.marker")?;
        let digits_start = self
            .builder
            .build_select(signed, after_marker, head, "exp.digits")?
            .into_int_value();
        self.builder.build_store(digit_cursor, digits_start)?;
        self.builder.build_store(mantissa, marker)?;
        self.builder
            .build_conditional_branch(trim, trim_head, trim_skip)?;

        self.builder.position_at_end(trim_head);
        let m = self
            .builder
            .build_load(i64_type, mantissa, "exp.m")?
            .into_int_value();
        let has_digits = self.builder.build_int_compare(
            IntPredicate::UGT,
            m,
            i64_type.const_zero(),
            "exp.m.any",
        )?;
        self.builder
            .build_conditional_branch(has_digits, trim_check, trim_end)?;

        self.builder.position_at_end(trim_check);
        let before = self
            .builder
            .build_int_sub(m, i64_type.const_int(1, false), "exp.m.prev")?;
        let trailing = self.load_byte(source, before, "exp.m.byte")?;
        let is_pad_zero = self.builder.build_int_compare(
            IntPredicate::EQ,
            trailing,
            i8_type.const_int(u64::from(b'0'), false),
            "exp.m.zero",
        )?;
        self.builder
            .build_conditional_branch(is_pad_zero, trim_next, trim_end)?;

        self.builder.position_at_end(trim_next);
        self.builder.build_store(mantissa, before)?;
        self.builder.build_unconditional_branch(trim_head)?;

        // Zeros are only padding when the mantissa has a decimal point. Without one
        // the digits are the value itself, so the mantissa is restored untouched.
        self.builder.position_at_end(trim_end);
        let trimmed = self
            .builder
            .build_load(i64_type, mantissa, "exp.trimmed")?
            .into_int_value();
        let trimmed_any = self.builder.build_int_compare(
            IntPredicate::UGT,
            trimmed,
            i64_type.const_zero(),
            "exp.trimmed.any",
        )?;
        let point_index =
            self.builder
                .build_int_sub(trimmed, i64_type.const_int(1, false), "exp.point.index")?;
        let point_byte =
            self.load_byte_guarded(source, point_index, trimmed_any, "exp.point.byte")?;
        // A mantissa left ending in `.` (every fraction digit was a zero) drops it.
        let ends_with_point = self.builder.build_int_compare(
            IntPredicate::EQ,
            point_byte,
            i8_type.const_int(u64::from(b'.'), false),
            "exp.ends.point",
        )?;
        let without_point = self
            .builder
            .build_select(ends_with_point, point_index, trimmed, "exp.no.point")?
            .into_int_value();
        let has_point = self
            .builder
            .build_load(self.context.bool_type(), saw_point, "exp.has.point")?
            .into_int_value();
        let kept = self
            .builder
            .build_select(has_point, without_point, marker, "exp.kept")?
            .into_int_value();
        self.builder.build_store(mantissa, kept)?;
        self.builder.build_unconditional_branch(skip_head)?;

        self.builder.position_at_end(trim_skip);
        self.builder.build_store(mantissa, marker)?;
        self.builder.build_unconditional_branch(skip_head)?;

        // Trim leading exponent zeros, but never the last digit: `e+00` is `e0`.
        self.builder.position_at_end(skip_head);
        let k = self
            .builder
            .build_load(i64_type, digit_cursor, "exp.k")?
            .into_int_value();
        let limit = self
            .builder
            .build_int_sub(len, i64_type.const_int(1, false), "exp.limit")?;
        let can_trim =
            self.builder
                .build_int_compare(IntPredicate::ULT, k, limit, "exp.can.trim")?;
        self.builder
            .build_conditional_branch(can_trim, skip_check, rebuild)?;

        self.builder.position_at_end(skip_check);
        let digit = self.load_byte(source, k, "exp.digit")?;
        let is_zero = self.builder.build_int_compare(
            IntPredicate::EQ,
            digit,
            i8_type.const_int(u64::from(b'0'), false),
            "exp.is.zero",
        )?;
        self.builder
            .build_conditional_branch(is_zero, skip_next, rebuild)?;

        self.builder.position_at_end(skip_next);
        let advanced = self
            .builder
            .build_int_add(k, i64_type.const_int(1, false), "exp.k.next")?;
        self.builder.build_store(digit_cursor, advanced)?;
        self.builder.build_unconditional_branch(skip_head)?;

        self.builder.position_at_end(rebuild);
        let mantissa_len = self
            .builder
            .build_load(i64_type, mantissa, "exp.mantissa.len")?
            .into_int_value();
        let tail_start = self
            .builder
            .build_load(i64_type, digit_cursor, "exp.tail.start")?
            .into_int_value();
        let tail_len = self
            .builder
            .build_int_sub(len, tail_start, "exp.tail.len")?;
        let sign_len = self
            .builder
            .build_select(
                negative,
                i64_type.const_int(1, false),
                i64_type.const_zero(),
                "exp.sign.len",
            )?
            .into_int_value();
        let after_e = self.builder.build_int_add(
            mantissa_len,
            i64_type.const_int(1, false),
            "exp.after.e",
        )?;
        let body_start = self
            .builder
            .build_int_add(after_e, sign_len, "exp.body.start")?;
        let total = self
            .builder
            .build_int_add(body_start, tail_len, "exp.total")?;
        let buf = self.build_malloc(total, "exp.buf")?;
        self.build_memcpy_call(buf, source, mantissa_len)?;
        self.store_byte(buf, mantissa_len, i8_type.const_int(u64::from(b'e'), false))?;
        self.builder
            .build_conditional_branch(negative, put_sign, after_sign)?;

        self.builder.position_at_end(put_sign);
        self.store_byte(buf, after_e, i8_type.const_int(u64::from(b'-'), false))?;
        self.builder.build_unconditional_branch(after_sign)?;

        self.builder.position_at_end(after_sign);
        let dst = self.byte_offset(buf, body_start, "exp.dst")?;
        let src = self.byte_offset(source, tail_start, "exp.src")?;
        self.build_memcpy_call(dst, src, tail_len)?;
        let normalized = self.build_string_value(buf, total)?;
        self.builder.build_return(Some(&normalized))?;

        self.end_helper(saved);
        Ok(func)
    }
}
