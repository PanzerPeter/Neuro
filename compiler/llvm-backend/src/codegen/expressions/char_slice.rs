// Codegen for `string.char_slice(range)` — a borrowed sub-slice located by code point
// rather than by byte.
//
// `.slice(a..b)` is the cheap operation: `a` and `b` are already byte offsets, so the
// whole method is a `gep` and a subtraction, and its only runtime cost is proving the two
// endpoints do not split a multi-byte code point. That contract is wrong for text whose
// positions came from counting characters rather than from a previous byte offset — the
// tokenizer and NLP workloads §2.7 points here — where "the first three characters" of
// `"héllo"` is four bytes, not three.
//
// A code point index therefore has to be resolved into a byte offset, and UTF-8 offers no
// way to do that but to walk the bytes: the encoding is variable-width, so the nth code
// point's position depends on the width of all n before it. Hence the linear scan §2.7
// specifies, and hence the two boundary rules `.slice` enforces do not appear here at all
// — a scan can only ever stop on a lead byte, so a code point index cannot name a position
// inside a code point in the first place. The only way this method can fail is a range
// that runs off the end of the string or one whose bounds are reversed.
//
// The scan lives in one module-private helper rather than inline at each call site: it is
// a loop, and `.char_slice` needs two of them (one per endpoint), so inlining would put
// four basic blocks into the caller for every use.

use inkwell::module::Linkage;
use inkwell::values::{
    BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PointerValue,
};
use inkwell::IntPredicate;
use neuro_hir::{HirExpr, HirExprKind};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};

/// The module-private helper that walks UTF-8 bytes to the byte offset of a code point
/// index, reporting [`CHAR_OFFSET_NOT_FOUND`] when the index is past the end.
const CHAR_OFFSET_FN: &str = "neuro.string.char_offset";

/// [`CHAR_OFFSET_FN`]'s answer for a code point index the string does not reach. Out of
/// band as a byte offset, which is never negative, so no separate flag is returned.
const CHAR_OFFSET_NOT_FOUND: i64 = -1;

/// The UTF-8 continuation-byte pattern `0b10xxxxxx`: a byte matching it is the interior of
/// a multi-byte code point, and every byte that does not match it starts a new one.
const UTF8_CONTINUATION_MASK: u64 = 0xC0;
const UTF8_CONTINUATION_BITS: u64 = 0x80;

fn llvm_err(e: inkwell::builder::BuilderError) -> CodegenError {
    CodegenError::LlvmError(e.to_string())
}

impl<'ctx> CodegenContext<'ctx> {
    /// Lower `string.char_slice(a..b)` / `string.char_slice(a..=b)` to a borrowed
    /// `&string` whose range counts code points.
    ///
    /// Each endpoint is resolved to a byte offset by a scan, and the resulting byte range
    /// builds the same zero-copy fat pointer `.slice` produces. A code point index the
    /// string does not reach, or a reversed range, panics (abort, no unwinding) in every
    /// build, matching `.slice`'s bounds contract.
    pub(super) fn codegen_char_slice(
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
                    "string.char_slice reached codegen without a range argument".into(),
                ))
            }
        };

        let fat = self.string_receiver_struct(receiver)?;
        let base_ptr = self
            .builder
            .build_extract_value(fat, 0, "cslice.base.ptr")
            .map_err(llvm_err)?
            .into_pointer_value();
        let len = self
            .builder
            .build_extract_value(fat, 1, "cslice.len")
            .map_err(llvm_err)?
            .into_int_value();

        let i64_ty = self.context.i64_type();
        let start_cp = self.slice_index_to_i64(start_expr)?;
        let raw_end_cp = self.slice_index_to_i64(end_expr)?;
        // `a..=b` covers code point `b`, so the exclusive upper bound is `b + 1`.
        let end_cp = if inclusive {
            self.builder
                .build_int_add(raw_end_cp, i64_ty.const_int(1, false), "cslice.incl.end")
                .map_err(llvm_err)?
        } else {
            raw_end_cp
        };

        let char_offset = self.get_or_build_char_offset()?;
        let start_byte = self.call_char_offset(char_offset, base_ptr, len, start_cp)?;
        let end_byte = self.call_char_offset(char_offset, base_ptr, len, end_cp)?;

        // Three failures collapse into two comparisons. A negative code point index and
        // one past the last character both come back as CHAR_OFFSET_NOT_FOUND, so testing
        // the start offset for non-negativity catches both on that endpoint; and because
        // byte offsets rise with code point indices, `start <= end` on the resolved
        // offsets rejects a reversed range and a not-found end together.
        let not_found = i64_ty.const_int(CHAR_OFFSET_NOT_FOUND as u64, true);
        let start_found = self
            .builder
            .build_int_compare(IntPredicate::SGT, start_byte, not_found, "cslice.start.ok")
            .map_err(llvm_err)?;
        let ordered = self
            .builder
            .build_int_compare(IntPredicate::SLE, start_byte, end_byte, "cslice.ordered")
            .map_err(llvm_err)?;
        let in_bounds = self
            .builder
            .build_and(start_found, ordered, "cslice.in.bounds")
            .map_err(llvm_err)?;
        self.codegen_guard_or_panic(in_bounds, "string char slice out of bounds", offset)?;

        self.string_fat_slice(base_ptr, start_byte, end_byte)
    }

    /// Emit one `char_offset(ptr, len, index)` call.
    fn call_char_offset(
        &mut self,
        char_offset: FunctionValue<'ctx>,
        base_ptr: PointerValue<'ctx>,
        len: IntValue<'ctx>,
        index: IntValue<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        let call_args: [BasicMetadataValueEnum; 3] = [base_ptr.into(), len.into(), index.into()];
        Ok(self
            .builder
            .build_call(char_offset, &call_args, "cslice.off")
            .map_err(llvm_err)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("char_offset produced no result".into()))?
            .into_int_value())
    }

    /// Fetch the module's `char_offset` helper, emitting it on first use.
    fn get_or_build_char_offset(&mut self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(CHAR_OFFSET_FN) {
            return Ok(existing);
        }

        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = i64_type.fn_type(&[ptr_type.into(), i64_type.into(), i64_type.into()], false);
        let function = self
            .module
            .add_function(CHAR_OFFSET_FN, fn_type, Some(Linkage::Private));

        // The body is emitted into its own function, so the builder has to be put back
        // wherever the caller was mid-expression.
        let resume_at = self.builder.get_insert_block();
        let built = self.build_char_offset_body(function);
        if let Some(block) = resume_at {
            self.builder.position_at_end(block);
        }
        built?;

        Ok(function)
    }

    /// Emit `char_offset`'s body: walk code points from the start of the string until
    /// `index` of them have been passed, and return the byte offset standing there.
    ///
    /// Counting up to `index` rather than to `index - 1` is what makes the end of the
    /// string a legal answer: for a string of `n` characters the loop leaves the cursor at
    /// `len` with `n` seen, so `char_offset(s, n)` is `len` — the exclusive upper bound of
    /// a range covering the whole string — while `n + 1` runs out of bytes and reports
    /// [`CHAR_OFFSET_NOT_FOUND`].
    fn build_char_offset_body(&self, function: FunctionValue<'ctx>) -> CodegenResult<()> {
        let i64_type = self.context.i64_type();
        let i8_type = self.context.i8_type();

        let base = function
            .get_nth_param(0)
            .ok_or_else(|| {
                CodegenError::InternalError("char_offset lost its pointer parameter".into())
            })?
            .into_pointer_value();
        let len = function
            .get_nth_param(1)
            .ok_or_else(|| {
                CodegenError::InternalError("char_offset lost its length parameter".into())
            })?
            .into_int_value();
        let target = function
            .get_nth_param(2)
            .ok_or_else(|| {
                CodegenError::InternalError("char_offset lost its index parameter".into())
            })?
            .into_int_value();

        let entry = self.context.append_basic_block(function, "entry");
        let head = self.context.append_basic_block(function, "cp.head");
        let exhausted = self.context.append_basic_block(function, "cp.exhausted");
        let step = self.context.append_basic_block(function, "cp.step");
        let scan_head = self.context.append_basic_block(function, "cp.scan.head");
        let scan_body = self.context.append_basic_block(function, "cp.scan.body");
        let advance = self.context.append_basic_block(function, "cp.advance");
        let found = self.context.append_basic_block(function, "cp.found");
        let missing = self.context.append_basic_block(function, "cp.missing");

        self.builder.position_at_end(entry);
        let byte_slot = self
            .builder
            .build_alloca(i64_type, "cp.byte")
            .map_err(llvm_err)?;
        let seen_slot = self
            .builder
            .build_alloca(i64_type, "cp.seen")
            .map_err(llvm_err)?;
        self.builder
            .build_store(byte_slot, i64_type.const_zero())
            .map_err(llvm_err)?;
        self.builder
            .build_store(seen_slot, i64_type.const_zero())
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(head)
            .map_err(llvm_err)?;

        // Test for arrival before testing for bytes: the answer for the last position is
        // `len` itself, which is reached with no bytes left to read.
        self.builder.position_at_end(head);
        let byte = self
            .builder
            .build_load(i64_type, byte_slot, "cp.b")
            .map_err(llvm_err)?
            .into_int_value();
        let seen = self
            .builder
            .build_load(i64_type, seen_slot, "cp.s")
            .map_err(llvm_err)?
            .into_int_value();
        let arrived = self
            .builder
            .build_int_compare(IntPredicate::EQ, seen, target, "cp.arrived")
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(arrived, found, exhausted)
            .map_err(llvm_err)?;

        // Not there yet, and no byte left to consume: the index is past the end. This is
        // also where a negative `target` lands, since `seen` counts up from zero and can
        // never equal it.
        self.builder.position_at_end(exhausted);
        let has_bytes = self
            .builder
            .build_int_compare(IntPredicate::SLT, byte, len, "cp.has.bytes")
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(has_bytes, step, missing)
            .map_err(llvm_err)?;

        // Step over this code point: its lead byte, then every continuation byte after it.
        self.builder.position_at_end(step);
        let after_lead = self
            .builder
            .build_int_add(byte, i64_type.const_int(1, false), "cp.after.lead")
            .map_err(llvm_err)?;
        self.builder
            .build_store(byte_slot, after_lead)
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(scan_head)
            .map_err(llvm_err)?;

        self.builder.position_at_end(scan_head);
        let cursor = self
            .builder
            .build_load(i64_type, byte_slot, "cp.cursor")
            .map_err(llvm_err)?
            .into_int_value();
        let in_range = self
            .builder
            .build_int_compare(IntPredicate::SLT, cursor, len, "cp.cursor.in.range")
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(in_range, scan_body, advance)
            .map_err(llvm_err)?;

        self.builder.position_at_end(scan_body);
        // SAFETY: this block is reached only under `cursor < len`, and `cursor` counts up
        // from zero, so the read is inside the string's allocation.
        let byte_ptr = unsafe {
            self.builder
                .build_in_bounds_gep(i8_type, base, &[cursor], "cp.byte.ptr")
                .map_err(llvm_err)?
        };
        let raw = self
            .builder
            .build_load(i8_type, byte_ptr, "cp.raw")
            .map_err(llvm_err)?
            .into_int_value();
        let masked = self
            .builder
            .build_and(
                raw,
                i8_type.const_int(UTF8_CONTINUATION_MASK, false),
                "cp.masked",
            )
            .map_err(llvm_err)?;
        let is_continuation = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                masked,
                i8_type.const_int(UTF8_CONTINUATION_BITS, false),
                "cp.is.cont",
            )
            .map_err(llvm_err)?;
        let next_cursor = self
            .builder
            .build_int_add(cursor, i64_type.const_int(1, false), "cp.next.cursor")
            .map_err(llvm_err)?;
        let kept = self
            .builder
            .build_select(is_continuation, next_cursor, cursor, "cp.kept")
            .map_err(llvm_err)?
            .into_int_value();
        self.builder
            .build_store(byte_slot, kept)
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(is_continuation, scan_head, advance)
            .map_err(llvm_err)?;

        self.builder.position_at_end(advance);
        let next_seen = self
            .builder
            .build_int_add(seen, i64_type.const_int(1, false), "cp.next.seen")
            .map_err(llvm_err)?;
        self.builder
            .build_store(seen_slot, next_seen)
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(head)
            .map_err(llvm_err)?;

        self.builder.position_at_end(found);
        self.builder.build_return(Some(&byte)).map_err(llvm_err)?;

        self.builder.position_at_end(missing);
        let not_found = i64_type.const_int(CHAR_OFFSET_NOT_FOUND as u64, true);
        self.builder
            .build_return(Some(&not_found))
            .map_err(llvm_err)?;

        Ok(())
    }
}
