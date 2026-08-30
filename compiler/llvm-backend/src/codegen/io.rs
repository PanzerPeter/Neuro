// Codegen for the standard-output builtins: `print` and `println`.
//
// Both take the `{ ptr, len }` string fat pointer their argument already is. Formatting
// is not this module's concern: string interpolation has already rendered every hole into
// the fat pointer by the time the call is reached, so a `print` is one argument wide.
//
// The bytes do not reach the POSIX `write` syscall one call at a time. A `write` is a
// round trip into the kernel whether it carries four bytes or four thousand, and an
// unbuffered `println` costs two of them — one for the text, one for the newline — so a
// printing loop spent effectively all of its time in syscall entry rather than in the
// program. The bytes are copied into a module-private page-sized buffer instead and
// drained when it fills, which is what every other language's standard output does.
//
// Buffering is only correct if the buffer is guaranteed to reach fd 1 before the process
// stops running, and this language has exactly three ways to stop: `main` returns, the
// panic runtime aborts, or `-O0` arithmetic traps on overflow. `finalize_stdout_buffer`
// walks all three and inserts the drain, which is why no exit path needs to remember to.
// Flushing ahead of a panic is what keeps its stderr diagnostic behind the output that
// logically precedes it, rather than jumbled in front of it.
//
// A terminal is buffered by line rather than by page: a program printing progress must
// show it as it happens, not a page at a time. `println` therefore ends with a call that
// drains only when fd 1 is a terminal. It still beats the unbuffered version there, since
// the text and its newline now leave in one `write` instead of two.
//
// The newline is the one byte `\n`. On Windows fd 1 is a CRT text-mode descriptor, so
// that byte leaves the process as `\r\n` — the translation a C `printf` gets on the same
// platform. The builtins follow that convention rather than forcing the descriptor into
// binary mode, so tests and golden files compare text with line endings normalized.
//
// `write` is permitted to consume fewer bytes than it was offered, which a pipe with a
// full buffer routinely does, so every drain goes through one module-private helper
// holding the retry loop rather than a bare call per site. Output is the language's
// primary result channel; silently truncating it would be worse than the loop it costs.

use inkwell::module::Linkage;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, GlobalValue};
use inkwell::IntPredicate;
use neuro_hir::HirExpr;

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};

/// The stdout file descriptor, per POSIX; `print` / `println` output is written here.
const STDOUT_FD: u64 = 1;

/// Bytes held before the buffer is drained to fd 1. One page: large enough that a
/// printing loop issues one `write` per thousands of lines, small enough that the
/// `.bss` reservation is irrelevant next to the syscalls it removes.
const PRINT_BUFFER_BYTES: u64 = 4096;

/// The module-private helper carrying the short-write retry loop.
const WRITE_ALL_FN: &str = "neuro.print.write_all";

/// The module-private helper that copies bytes into the buffer, draining first when
/// they do not fit and bypassing it entirely when they never could.
const EMIT_FN: &str = "neuro.print.emit";

/// The module-private helper `println` ends with: drains the buffer, but only when fd 1
/// is a terminal.
const LINE_END_FN: &str = "neuro.print.line_end";

/// The module-private helper that writes the buffered bytes out and empties the buffer.
const FLUSH_FN: &str = "neuro.print.flush";

/// The `.bss` global holding bytes written but not yet drained.
const BUFFER_GLOBAL: &str = "neuro.print.buffer";

/// The `.bss` global counting the bytes currently held in [`BUFFER_GLOBAL`].
const USED_GLOBAL: &str = "neuro.print.used";

/// The `.bss` global caching whether fd 1 is a terminal, resolved on first use.
const MODE_GLOBAL: &str = "neuro.print.mode";

/// [`MODE_GLOBAL`] before the first `isatty` probe. A never-printing program leaves it
/// here, so it never asks the OS anything.
const MODE_UNRESOLVED: u64 = 0;

/// [`MODE_GLOBAL`] for a pipe, file, or anything else that is not a terminal: hold bytes
/// until the buffer fills.
const MODE_BLOCK: u64 = 1;

/// [`MODE_GLOBAL`] for a terminal: drain at the end of every `println`, so a program
/// printing its progress is watchable while it runs.
const MODE_LINE: u64 = 2;

/// The `.rodata` global holding the single byte `println` appends.
const NEWLINE_GLOBAL: &str = "neuro.print.newline";

/// The entry function whose every `ret` must drain the buffer. `main` is emitted under
/// its own name — there is no wrapper around it — so this is the C entry point itself.
const ENTRY_FN: &str = "main";

/// libc's terminal test. MSVC's CRT exposes the POSIX name only under its underscored
/// spelling; `neurc` compiles for the host, so the choice is made here rather than
/// carried through the module.
const ISATTY_FN: &str = if cfg!(windows) { "_isatty" } else { "isatty" };

fn llvm_err(e: inkwell::builder::BuilderError) -> CodegenError {
    CodegenError::LlvmError(e.to_string())
}

impl<'ctx> CodegenContext<'ctx> {
    /// True when `name` is a compiler-known standard-output builtin.
    /// Mirrors the resolver in `semantic-analysis`; the duplication keeps the backend
    /// independent of the type-checker slice.
    pub(crate) fn is_io_builtin(name: &str) -> bool {
        matches!(name, "print" | "println")
    }

    /// Lower a call to `print` / `println`. Both return unit, so nothing is produced
    /// for the caller to bind; the builder is left at the same live block.
    pub(crate) fn codegen_io_builtin(&mut self, name: &str, args: &[HirExpr]) -> CodegenResult<()> {
        let text = args.first().ok_or_else(|| {
            CodegenError::InternalError(format!("{}() reached codegen without its text", name))
        })?;
        let value = self.codegen_expr(text)?;
        let (ptr, len) = self.split_printable(value, name)?;

        // An argument the caller built here — `println("n = {n}")`, `print(a + b)` — is a
        // temporary nothing else can reach. `emit` has consumed the bytes by the time it
        // returns — copied into the buffer, or handed to `write` on the bypass path — and
        // retains none of them, so the buffer is dead the moment the last one leaves, and
        // the allocation and the free sit in the same block with nothing between them
        // that could escape it. A borrowed argument (a literal, a variable, a slice)
        // answers `false` here and is left alone.
        let owns_argument = Self::produces_owned_string(text);

        let emit = self.get_or_build_emit()?;
        let text_args: [BasicMetadataValueEnum; 2] = [ptr.into(), len.into()];
        self.builder
            .build_call(emit, &text_args, "")
            .map_err(llvm_err)?;

        if name == "println" {
            let newline = self.get_or_create_newline()?;
            let newline_args: [BasicMetadataValueEnum; 2] = [
                newline.into(),
                self.context.i64_type().const_int(1, false).into(),
            ];
            self.builder
                .build_call(emit, &newline_args, "")
                .map_err(llvm_err)?;

            // The line terminator is where a line boundary is, and the compiler knows it
            // here — so a terminal is served without the runtime ever scanning bytes for
            // a newline. `print` writes no terminator and so ends no line, matching what
            // C's line-buffered stdio does with a `printf` that has no `\n`.
            let line_end = self.get_or_build_line_end()?;
            self.builder
                .build_call(line_end, &[], "")
                .map_err(llvm_err)?;
        }

        if owns_argument {
            let free_fn = self.get_or_declare_free();
            self.builder
                .build_call(free_fn, &[ptr.into()], "")
                .map_err(llvm_err)?;
        }

        Ok(())
    }

    /// Split a `string` / `&string` value into its `(ptr, len)` pair.
    ///
    /// Reports a non-aggregate operand as an internal error rather than asking the value
    /// for a struct variant it does not have: the type checker rejects `&mut string`
    /// (a pointer to the fat pointer) here, and a regression in that rule must surface as
    /// a diagnostic, not as an aborted compiler.
    fn split_printable(
        &self,
        value: BasicValueEnum<'ctx>,
        name: &str,
    ) -> CodegenResult<(
        inkwell::values::PointerValue<'ctx>,
        inkwell::values::IntValue<'ctx>,
    )> {
        if !value.is_struct_value() {
            return Err(CodegenError::InternalError(format!(
                "{}() reached codegen with a non-string argument",
                name
            )));
        }
        let fat = value.into_struct_value();
        let ptr = self
            .builder
            .build_extract_value(fat, 0, "print.ptr")
            .map_err(llvm_err)?
            .into_pointer_value();
        let len = self
            .builder
            .build_extract_value(fat, 1, "print.len")
            .map_err(llvm_err)?
            .into_int_value();
        Ok((ptr, len))
    }

    /// Get the shared newline global, emitting it on first use.
    fn get_or_create_newline(&self) -> CodegenResult<inkwell::values::PointerValue<'ctx>> {
        if let Some(existing) = self.module.get_global(NEWLINE_GLOBAL) {
            return Ok(existing.as_pointer_value());
        }
        let global = self
            .builder
            .build_global_string_ptr("\n", NEWLINE_GLOBAL)
            .map_err(llvm_err)?;
        Ok(global.as_pointer_value())
    }

    /// Get the byte buffer, reserving it on first use.
    fn get_or_create_buffer(&self) -> GlobalValue<'ctx> {
        if let Some(existing) = self.module.get_global(BUFFER_GLOBAL) {
            return existing;
        }
        let buffer_type = self.context.i8_type().array_type(PRINT_BUFFER_BYTES as u32);
        let global = self.module.add_global(buffer_type, None, BUFFER_GLOBAL);
        global.set_linkage(Linkage::Private);
        global.set_initializer(&buffer_type.const_zero());
        global
    }

    /// Get one of the buffer's integer state globals, reserving it on first use.
    fn get_or_create_state(
        &self,
        name: &str,
        int_type: inkwell::types::IntType<'ctx>,
    ) -> GlobalValue<'ctx> {
        if let Some(existing) = self.module.get_global(name) {
            return existing;
        }
        let global = self.module.add_global(int_type, None, name);
        global.set_linkage(Linkage::Private);
        global.set_initializer(&int_type.const_zero());
        global
    }

    /// Get the external libc `isatty` declaration, inserting it on first use.
    fn get_or_declare_isatty(&self) -> FunctionValue<'ctx> {
        if let Some(existing) = self.module.get_function(ISATTY_FN) {
            return existing;
        }
        let i32_type = self.context.i32_type();
        let fn_type = i32_type.fn_type(&[i32_type.into()], false);
        self.module
            .add_function(ISATTY_FN, fn_type, Some(Linkage::External))
    }

    /// Get the `emit(ptr, len)` helper, emitting its body on first use.
    ///
    /// Three outcomes, in the order a hot printing loop meets them: the bytes fit and are
    /// copied; they do not fit, so the buffer is drained first; they are larger than the
    /// buffer will ever be, so they go straight to `write` after that drain rather than
    /// being chopped into page-sized pieces. The bypass is what keeps a single enormous
    /// string — a rendered tensor, a whole report — at one syscall.
    fn get_or_build_emit(&mut self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(EMIT_FN) {
            return Ok(existing);
        }

        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = self
            .context
            .void_type()
            .fn_type(&[ptr_type.into(), i64_type.into()], false);
        let function = self
            .module
            .add_function(EMIT_FN, fn_type, Some(Linkage::Private));

        let write_all = self.get_or_build_write_all()?;
        let flush = self.get_or_build_flush()?;
        let buffer = self.get_or_create_buffer().as_pointer_value();
        let used_slot = self
            .get_or_create_state(USED_GLOBAL, self.context.i64_type())
            .as_pointer_value();

        let resume_at = self.builder.get_insert_block();
        let built = self.build_emit_body(function, write_all, flush, buffer, used_slot);
        if let Some(block) = resume_at {
            self.builder.position_at_end(block);
        }
        built?;

        Ok(function)
    }

    fn build_emit_body(
        &self,
        function: FunctionValue<'ctx>,
        write_all: FunctionValue<'ctx>,
        flush: FunctionValue<'ctx>,
        buffer: inkwell::values::PointerValue<'ctx>,
        used_slot: inkwell::values::PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        let i64_type = self.context.i64_type();
        let capacity = i64_type.const_int(PRINT_BUFFER_BYTES, false);
        let text = function
            .get_nth_param(0)
            .ok_or_else(|| CodegenError::InternalError("emit lost its buffer parameter".into()))?
            .into_pointer_value();
        let len = function
            .get_nth_param(1)
            .ok_or_else(|| CodegenError::InternalError("emit lost its length parameter".into()))?
            .into_int_value();

        let entry = self.context.append_basic_block(function, "entry");
        let spill = self.context.append_basic_block(function, "emit.spill");
        let bypass = self.context.append_basic_block(function, "emit.bypass");
        let copy = self.context.append_basic_block(function, "emit.copy");
        let done = self.context.append_basic_block(function, "emit.done");

        self.builder.position_at_end(entry);
        let used = self
            .builder
            .build_load(i64_type, used_slot, "emit.used")
            .map_err(llvm_err)?
            .into_int_value();
        let free = self
            .builder
            .build_int_sub(capacity, used, "emit.free")
            .map_err(llvm_err)?;
        let fits = self
            .builder
            .build_int_compare(IntPredicate::ULE, len, free, "emit.fits")
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(fits, copy, spill)
            .map_err(llvm_err)?;

        self.builder.position_at_end(spill);
        self.builder.build_call(flush, &[], "").map_err(llvm_err)?;
        let oversize = self
            .builder
            .build_int_compare(IntPredicate::UGT, len, capacity, "emit.oversize")
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(oversize, bypass, copy)
            .map_err(llvm_err)?;

        self.builder.position_at_end(bypass);
        let bypass_args: [BasicMetadataValueEnum; 2] = [text.into(), len.into()];
        self.builder
            .build_call(write_all, &bypass_args, "")
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(done)
            .map_err(llvm_err)?;

        // Reached from both the fits-already block and the just-drained one, so the
        // offset is re-read rather than carried in: the drain reset it to zero.
        self.builder.position_at_end(copy);
        let offset = self
            .builder
            .build_load(i64_type, used_slot, "emit.offset")
            .map_err(llvm_err)?
            .into_int_value();
        let cursor = self.byte_offset(buffer, offset, "emit.cursor")?;
        self.build_memcpy_call(cursor, text, len)?;
        let filled = self
            .builder
            .build_int_add(offset, len, "emit.filled")
            .map_err(llvm_err)?;
        self.builder
            .build_store(used_slot, filled)
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(done)
            .map_err(llvm_err)?;

        self.builder.position_at_end(done);
        self.builder.build_return(None).map_err(llvm_err)?;
        Ok(())
    }

    /// Get the `flush()` helper, emitting its body on first use.
    ///
    /// Cheap enough to call unconditionally from every process-exit path: a program that
    /// printed nothing loads a zero and returns.
    pub(crate) fn get_or_build_flush(&mut self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(FLUSH_FN) {
            return Ok(existing);
        }

        let fn_type = self.context.void_type().fn_type(&[], false);
        let function = self
            .module
            .add_function(FLUSH_FN, fn_type, Some(Linkage::Private));

        let write_all = self.get_or_build_write_all()?;
        let buffer = self.get_or_create_buffer().as_pointer_value();
        let used_slot = self
            .get_or_create_state(USED_GLOBAL, self.context.i64_type())
            .as_pointer_value();

        let resume_at = self.builder.get_insert_block();
        let built = self.build_flush_body(function, write_all, buffer, used_slot);
        if let Some(block) = resume_at {
            self.builder.position_at_end(block);
        }
        built?;

        Ok(function)
    }

    fn build_flush_body(
        &self,
        function: FunctionValue<'ctx>,
        write_all: FunctionValue<'ctx>,
        buffer: inkwell::values::PointerValue<'ctx>,
        used_slot: inkwell::values::PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        let i64_type = self.context.i64_type();

        let entry = self.context.append_basic_block(function, "entry");
        let drain = self.context.append_basic_block(function, "flush.drain");
        let done = self.context.append_basic_block(function, "flush.done");

        self.builder.position_at_end(entry);
        let used = self
            .builder
            .build_load(i64_type, used_slot, "flush.used")
            .map_err(llvm_err)?
            .into_int_value();
        let pending = self
            .builder
            .build_int_compare(
                IntPredicate::UGT,
                used,
                i64_type.const_zero(),
                "flush.pending",
            )
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(pending, drain, done)
            .map_err(llvm_err)?;

        self.builder.position_at_end(drain);
        let drain_args: [BasicMetadataValueEnum; 2] = [buffer.into(), used.into()];
        self.builder
            .build_call(write_all, &drain_args, "")
            .map_err(llvm_err)?;
        self.builder
            .build_store(used_slot, i64_type.const_zero())
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(done)
            .map_err(llvm_err)?;

        self.builder.position_at_end(done);
        self.builder.build_return(None).map_err(llvm_err)?;
        Ok(())
    }

    /// Get the `line_end()` helper, emitting its body on first use.
    ///
    /// The `isatty` probe is cached because the answer cannot change for a running
    /// process, and asking once per printed line would reintroduce exactly the syscall
    /// per `println` the buffer exists to remove.
    fn get_or_build_line_end(&mut self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(LINE_END_FN) {
            return Ok(existing);
        }

        let fn_type = self.context.void_type().fn_type(&[], false);
        let function = self
            .module
            .add_function(LINE_END_FN, fn_type, Some(Linkage::Private));

        let flush = self.get_or_build_flush()?;
        let isatty = self.get_or_declare_isatty();
        let mode_slot = self
            .get_or_create_state(MODE_GLOBAL, self.context.i8_type())
            .as_pointer_value();

        let resume_at = self.builder.get_insert_block();
        let built = self.build_line_end_body(function, flush, isatty, mode_slot);
        if let Some(block) = resume_at {
            self.builder.position_at_end(block);
        }
        built?;

        Ok(function)
    }

    fn build_line_end_body(
        &self,
        function: FunctionValue<'ctx>,
        flush: FunctionValue<'ctx>,
        isatty: FunctionValue<'ctx>,
        mode_slot: inkwell::values::PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        let i8_type = self.context.i8_type();
        let i32_type = self.context.i32_type();

        let entry = self.context.append_basic_block(function, "entry");
        let probe = self.context.append_basic_block(function, "line.probe");
        let decide = self.context.append_basic_block(function, "line.decide");
        let drain = self.context.append_basic_block(function, "line.drain");
        let done = self.context.append_basic_block(function, "line.done");

        self.builder.position_at_end(entry);
        let mode = self
            .builder
            .build_load(i8_type, mode_slot, "line.mode")
            .map_err(llvm_err)?
            .into_int_value();
        let unresolved = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                mode,
                i8_type.const_int(MODE_UNRESOLVED, false),
                "line.unresolved",
            )
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(unresolved, probe, decide)
            .map_err(llvm_err)?;

        self.builder.position_at_end(probe);
        let fd = i32_type.const_int(STDOUT_FD, false);
        let answer = self
            .builder
            .build_call(isatty, &[fd.into()], "line.isatty")
            .map_err(llvm_err)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("isatty() produced no result".into()))?
            .into_int_value();
        let terminal = self
            .builder
            .build_int_compare(
                IntPredicate::NE,
                answer,
                i32_type.const_zero(),
                "line.terminal",
            )
            .map_err(llvm_err)?;
        let resolved = self
            .builder
            .build_select(
                terminal,
                i8_type.const_int(MODE_LINE, false),
                i8_type.const_int(MODE_BLOCK, false),
                "line.resolved",
            )
            .map_err(llvm_err)?;
        self.builder
            .build_store(mode_slot, resolved)
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(decide)
            .map_err(llvm_err)?;

        self.builder.position_at_end(decide);
        let settled = self
            .builder
            .build_load(i8_type, mode_slot, "line.settled")
            .map_err(llvm_err)?
            .into_int_value();
        let by_line = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                settled,
                i8_type.const_int(MODE_LINE, false),
                "line.by_line",
            )
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(by_line, drain, done)
            .map_err(llvm_err)?;

        self.builder.position_at_end(drain);
        self.builder.build_call(flush, &[], "").map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(done)
            .map_err(llvm_err)?;

        self.builder.position_at_end(done);
        self.builder.build_return(None).map_err(llvm_err)?;
        Ok(())
    }

    /// Drain the buffer on every path out of the process.
    ///
    /// Run once, after every body is generated, because only then is it known whether the
    /// module prints at all — a program that never does keeps its exit paths untouched
    /// and reserves no buffer. The three paths are `main`'s returns, the panic runtime's
    /// `abort`, and the `-O0` overflow `llvm.trap`; the latter two are recorded as they
    /// are emitted, since `abort` and `trap` run no exit hook a buffer could register.
    pub(crate) fn finalize_stdout_buffer(&mut self) -> CodegenResult<()> {
        if self.module.get_global(BUFFER_GLOBAL).is_none() {
            return Ok(());
        }

        let flush = self.get_or_build_flush()?;
        let resume_at = self.builder.get_insert_block();

        let exits = std::mem::take(&mut self.process_exit_points);
        for instruction in &exits {
            self.builder.position_before(instruction);
            self.builder.build_call(flush, &[], "").map_err(llvm_err)?;
        }

        if let Some(entry_fn) = self.module.get_function(ENTRY_FN) {
            for block in entry_fn.get_basic_blocks() {
                let Some(terminator) = block.get_terminator() else {
                    continue;
                };
                if terminator.get_opcode() != inkwell::values::InstructionOpcode::Return {
                    continue;
                }
                self.builder.position_before(&terminator);
                self.builder.build_call(flush, &[], "").map_err(llvm_err)?;
            }
        }

        if let Some(block) = resume_at {
            self.builder.position_at_end(block);
        }
        Ok(())
    }

    /// Get the `write_all(ptr, len)` helper, emitting its body on first use.
    ///
    /// Built lazily in the middle of a hot function, so the builder is put back where the
    /// caller left it before returning — on the failing path too.
    fn get_or_build_write_all(&mut self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(WRITE_ALL_FN) {
            return Ok(existing);
        }

        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = self
            .context
            .void_type()
            .fn_type(&[ptr_type.into(), self.context.i64_type().into()], false);
        let function = self
            .module
            .add_function(WRITE_ALL_FN, fn_type, Some(Linkage::Private));

        let resume_at = self.builder.get_insert_block();
        let built = self.build_write_all_body(function);
        if let Some(block) = resume_at {
            self.builder.position_at_end(block);
        }
        built?;

        Ok(function)
    }

    /// Emit `write_all`'s body: call `write` until the whole buffer is consumed, giving
    /// up when the syscall reports an error or makes no progress.
    fn build_write_all_body(&self, function: FunctionValue<'ctx>) -> CodegenResult<()> {
        let i64_type = self.context.i64_type();
        let buf = function
            .get_nth_param(0)
            .ok_or_else(|| {
                CodegenError::InternalError("write_all lost its buffer parameter".into())
            })?
            .into_pointer_value();
        let len = function
            .get_nth_param(1)
            .ok_or_else(|| {
                CodegenError::InternalError("write_all lost its length parameter".into())
            })?
            .into_int_value();

        let entry = self.context.append_basic_block(function, "entry");
        let head = self.context.append_basic_block(function, "write.head");
        let body = self.context.append_basic_block(function, "write.body");
        let advance = self.context.append_basic_block(function, "write.advance");
        let done = self.context.append_basic_block(function, "write.done");

        self.builder.position_at_end(entry);
        let offset_slot = self
            .builder
            .build_alloca(i64_type, "write.offset")
            .map_err(llvm_err)?;
        self.builder
            .build_store(offset_slot, i64_type.const_zero())
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(head)
            .map_err(llvm_err)?;

        self.builder.position_at_end(head);
        let offset = self
            .builder
            .build_load(i64_type, offset_slot, "write.off")
            .map_err(llvm_err)?
            .into_int_value();
        let remaining = self
            .builder
            .build_int_sub(len, offset, "write.remaining")
            .map_err(llvm_err)?;
        let more = self
            .builder
            .build_int_compare(
                IntPredicate::SGT,
                remaining,
                i64_type.const_zero(),
                "write.more",
            )
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(more, body, done)
            .map_err(llvm_err)?;

        self.builder.position_at_end(body);
        // SAFETY: `offset` is the count of bytes already written and the loop is entered
        // only while it is below `len`, so the cursor stays inside the caller's buffer.
        let cursor = unsafe {
            self.builder
                .build_in_bounds_gep(self.context.i8_type(), buf, &[offset], "write.cursor")
                .map_err(llvm_err)?
        };
        let write_fn = self.get_or_declare_write();
        let fd = self.context.i32_type().const_int(STDOUT_FD, false);
        let call_args: [BasicMetadataValueEnum; 3] = [fd.into(), cursor.into(), remaining.into()];
        let written = self
            .builder
            .build_call(write_fn, &call_args, "write.n")
            .map_err(llvm_err)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("write() produced no result".into()))?
            .into_int_value();
        // A negative return is the error report and a zero one means the descriptor took
        // nothing; retrying either would spin forever, so both end the loop.
        let progressed = self
            .builder
            .build_int_compare(
                IntPredicate::SGT,
                written,
                i64_type.const_zero(),
                "write.progressed",
            )
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(progressed, advance, done)
            .map_err(llvm_err)?;

        self.builder.position_at_end(advance);
        let next = self
            .builder
            .build_int_add(offset, written, "write.next")
            .map_err(llvm_err)?;
        self.builder
            .build_store(offset_slot, next)
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(head)
            .map_err(llvm_err)?;

        self.builder.position_at_end(done);
        self.builder.build_return(None).map_err(llvm_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::CodegenContext;

    #[test]
    fn the_output_builtins_are_recognized_by_name() {
        assert!(CodegenContext::is_io_builtin("print"));
        assert!(CodegenContext::is_io_builtin("println"));
    }

    #[test]
    fn other_names_are_not_output_builtins() {
        for name in ["panic", "assert", "unreachable", "printf", "print_line", ""] {
            assert!(!CodegenContext::is_io_builtin(name), "{name}");
        }
    }
}
