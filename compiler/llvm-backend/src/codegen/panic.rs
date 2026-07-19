// Codegen for the panic-family builtins: `panic`, `assert`, `unreachable`.
//
// The panic contract is *abort, no unwinding*: a panic prints a diagnostic
// (message + source location) to stderr and terminates via `abort()` (SIGABRT).
// No landing pads are emitted, so the happy path stays zero-cost and `Drop` / `defer`
// fire only on normal scope exit. Diagnostics are written with the POSIX `write`
// syscall to stderr (fd 2) rather than buffered stdio, so the message reaches the
// terminal before the process dies.

use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum};
use neuro_hir::HirExpr;

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};

/// The stderr file descriptor, per POSIX; panic diagnostics are written here.
const STDERR_FD: u64 = 2;

impl<'ctx> CodegenContext<'ctx> {
    /// True when `name` is a compiler-known panic-family builtin.
    /// Mirrors the resolver in `semantic-analysis`; the duplication keeps the backend
    /// independent of the type-checker slice.
    pub(crate) fn is_panic_builtin(name: &str) -> bool {
        matches!(name, "panic" | "assert" | "unreachable")
    }

    /// Lower a call to a panic-family builtin. The returned value is a placeholder
    /// (`i32 0`) the caller discards: these builtins diverge, so the basic block is
    /// already terminated by `unreachable` for `panic` / `unreachable`. `assert` only
    /// aborts on a false condition and otherwise leaves the builder at a live block.
    pub(crate) fn codegen_panic_builtin(
        &mut self,
        name: &str,
        args: &[HirExpr],
        span: shared_types::Span,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let location = self.panic_location_suffix(span.start);
        match name {
            "panic" => {
                let message = args.first().ok_or_else(|| {
                    CodegenError::InternalError("panic() reached codegen without a message".into())
                })?;
                let msg_val = self.codegen_expr(message)?;
                self.emit_write_cstr("panic: ")?;
                self.emit_write_string_value(msg_val)?;
                self.emit_write_cstr(&format!("{}\n", location))?;
                self.emit_abort_unreachable()?;
            }
            "unreachable" => {
                self.emit_write_cstr(&format!(
                    "internal error: entered unreachable code{}\n",
                    location
                ))?;
                self.emit_abort_unreachable()?;
            }
            "assert" => {
                let condition = args.first().ok_or_else(|| {
                    CodegenError::InternalError(
                        "assert() reached codegen without a condition".into(),
                    )
                })?;
                self.codegen_assert(condition, &location)?;
            }
            other => {
                return Err(CodegenError::InternalError(format!(
                    "unknown panic builtin '{}' reached codegen",
                    other
                )))
            }
        }

        // Placeholder value; the unit result is unused. A constant emits no instruction,
        // so it is safe to produce even after the block has been terminated.
        Ok(self.context.i32_type().const_int(0, false).into())
    }

    /// Emit a runtime guard: continue when `ok` (an `i1`) is true, otherwise print
    /// `panic: <message>` with the source location for `offset` and abort. The
    /// builder is left positioned at the continuation block. Shared by the bounds and
    /// UTF-8 codepoint-boundary checks of `string.slice`.
    pub(crate) fn codegen_guard_or_panic(
        &mut self,
        ok: inkwell::values::IntValue<'ctx>,
        message: &str,
        offset: usize,
    ) -> CodegenResult<()> {
        let parent_fn = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("runtime guard outside a function during codegen".into())
        })?;
        let fail_bb = self.context.append_basic_block(parent_fn, "guard.fail");
        let cont_bb = self.context.append_basic_block(parent_fn, "guard.cont");

        self.builder
            .build_conditional_branch(ok, cont_bb, fail_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(fail_bb);
        let location = self.panic_location_suffix(offset);
        self.emit_write_cstr(&format!("panic: {}{}\n", message, location))?;
        self.emit_abort_unreachable()?;

        self.builder.position_at_end(cont_bb);
        Ok(())
    }

    /// Lower `assert(cond)`: continue on a true condition, abort on a false one.
    fn codegen_assert(&mut self, condition: &HirExpr, location: &str) -> CodegenResult<()> {
        let cond_val = self.codegen_expr(condition)?.into_int_value();

        let parent_fn = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("assert outside function during codegen".into())
        })?;
        let fail_bb = self.context.append_basic_block(parent_fn, "assert.fail");
        let cont_bb = self.context.append_basic_block(parent_fn, "assert.cont");

        self.builder
            .build_conditional_branch(cond_val, cont_bb, fail_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(fail_bb);
        self.emit_write_cstr(&format!("assertion failed{}\n", location))?;
        self.emit_abort_unreachable()?;

        self.builder.position_at_end(cont_bb);
        Ok(())
    }

    /// Render the ` at file:line:col` suffix for a panic diagnostic from a byte offset.
    /// Empty when no source was supplied (e.g. the library doctest path).
    fn panic_location_suffix(&self, offset: usize) -> String {
        match &self.source {
            Some(src) => {
                let pos = src.position_at(offset);
                format!(" at {}:{}:{}", src.path, pos.line, pos.column)
            }
            None => String::new(),
        }
    }

    /// Emit `write(2, <global ".rodata" bytes>, len)` for a compile-time-known string.
    fn emit_write_cstr(&self, text: &str) -> CodegenResult<()> {
        let global = self
            .builder
            .build_global_string_ptr(text, "panic.str")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let len = self.context.i64_type().const_int(text.len() as u64, false);
        self.emit_write(global.as_pointer_value().into(), len)
    }

    /// Emit `write(2, ptr, len)` for a runtime `string` fat pointer `{ ptr, i64 }`.
    fn emit_write_string_value(&self, value: BasicValueEnum<'ctx>) -> CodegenResult<()> {
        let fat = value.into_struct_value();
        let ptr = self
            .builder
            .build_extract_value(fat, 0, "panic.msg.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let len = self
            .builder
            .build_extract_value(fat, 1, "panic.msg.len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        self.emit_write(ptr, len)
    }

    /// Emit a single `write(STDERR_FD, ptr, len)` call, discarding the return value.
    fn emit_write(
        &self,
        ptr: BasicValueEnum<'ctx>,
        len: inkwell::values::IntValue<'ctx>,
    ) -> CodegenResult<()> {
        let write_fn = self.get_or_declare_write();
        let fd = self.context.i32_type().const_int(STDERR_FD, false);
        let args: [BasicMetadataValueEnum; 3] = [fd.into(), ptr.into(), len.into()];
        self.builder
            .build_call(write_fn, &args, "panic.write")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Emit `abort()` followed by an `unreachable` terminator, ending the basic block.
    fn emit_abort_unreachable(&self) -> CodegenResult<()> {
        let abort_fn = self.get_or_declare_abort();
        self.builder
            .build_call(abort_fn, &[], "panic.abort")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unreachable()
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }
}
