// Codegen for the standard-output builtins: `print` and `println`.
//
// Both take the `{ ptr, len }` string fat pointer their argument already is and hand it
// to the POSIX `write` syscall on fd 1 — the same primitive the panic runtime uses for
// its stderr diagnostics, with no buffering layer in between. Formatting is not this
// module's concern: string interpolation has already rendered every hole into the fat
// pointer by the time the call is reached, so a `print` is one argument wide.
//
// `write` is permitted to consume fewer bytes than it was offered, which a pipe with a
// full buffer routinely does, so the bytes go through one module-private helper holding
// the retry loop rather than a bare call per site. Output is the language's primary
// result channel; silently truncating it would be worse than the loop it costs.

use inkwell::module::Linkage;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue};
use inkwell::IntPredicate;
use neuro_hir::HirExpr;

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};

/// The stdout file descriptor, per POSIX; `print` / `println` output is written here.
const STDOUT_FD: u64 = 1;

/// The module-private helper carrying the short-write retry loop.
const WRITE_ALL_FN: &str = "neuro.print.write_all";

/// The `.rodata` global holding the single byte `println` appends.
const NEWLINE_GLOBAL: &str = "neuro.print.newline";

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

        let write_all = self.get_or_build_write_all()?;
        let text_args: [BasicMetadataValueEnum; 2] = [ptr.into(), len.into()];
        self.builder
            .build_call(write_all, &text_args, "")
            .map_err(llvm_err)?;

        if name == "println" {
            let newline = self.get_or_create_newline()?;
            let newline_args: [BasicMetadataValueEnum; 2] = [
                newline.into(),
                self.context.i64_type().const_int(1, false).into(),
            ];
            self.builder
                .build_call(write_all, &newline_args, "")
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
