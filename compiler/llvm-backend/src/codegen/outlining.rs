// Outlining of error paths into cold functions.
//
// A panic-family error path is several instructions of diagnostic machinery — one
// `write(2, …)` per message fragment, an `abort()`, and the `.rodata` globals they
// reference — that runs at most once in a program's lifetime. Emitted inline it still
// occupies cache lines in the middle of the hot function, between the guard branch and
// the code that follows it, and it does so at every bounds check, every assertion, and
// every string-slice boundary check.
//
// This pass emits that machinery once, in a module-private function marked `cold`, and
// leaves a single call behind at the failure site. The hot path keeps a compare, a
// branch, and a call; everything else moves out of line. Thunks are deduplicated by
// their rendered diagnostic text, so identically-worded failures share one body.
//
// The branch itself is annotated too: `!prof` weights tell block placement which edge is
// the improbable one, so the failure edge is laid out away from the fall-through path.

use inkwell::attributes::AttributeLoc;
use inkwell::module::Linkage;
use inkwell::values::{BasicValueEnum, FunctionValue, InstructionValue};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};

/// Relative weight of a guard's success edge. The absolute values are arbitrary; only
/// the ratio reaches block placement, and it must be lopsided enough that the failure
/// edge is never chosen as the fall-through.
const HOT_EDGE_WEIGHT: u64 = 2000;

/// Relative weight of a guard's failure edge — the panic path, which a correct program
/// never takes.
const COLD_EDGE_WEIGHT: u64 = 1;

/// LLVM metadata kind whose node carries branch probabilities.
const BRANCH_WEIGHT_KIND: &str = "prof";

/// Leading tag of a `!prof` branch-weight node, per the LLVM metadata format.
const BRANCH_WEIGHT_TAG: &str = "branch_weights";

impl<'ctx> CodegenContext<'ctx> {
    /// Terminate the current block with a call to the outlined thunk that prints
    /// `text` to stderr and aborts.
    ///
    /// The caller must have positioned the builder at the failure block; on return that
    /// block is terminated and no further instructions may be added to it.
    pub(crate) fn emit_outlined_panic(&mut self, text: &str) -> CodegenResult<()> {
        let thunk = self.cold_panic_thunk(text)?;
        self.emit_cold_call(thunk, &[])
    }

    /// Terminate the current block with a call to the outlined thunk that prints
    /// `panic: `, the runtime `string` fat pointer `message`, then `suffix`, and aborts.
    ///
    /// Only the constant fragments are baked into the thunk; the message travels as the
    /// `(ptr, len)` pair the fat pointer already holds, so one thunk serves every
    /// `panic(msg)` sharing a source location.
    pub(crate) fn emit_outlined_panic_with_message(
        &mut self,
        message: BasicValueEnum<'ctx>,
        suffix: &str,
    ) -> CodegenResult<()> {
        let fat = message.into_struct_value();
        let ptr = self
            .builder
            .build_extract_value(fat, 0, "panic.msg.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let len = self
            .builder
            .build_extract_value(fat, 1, "panic.msg.len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let thunk = self.cold_message_panic_thunk(suffix)?;
        self.emit_cold_call(thunk, &[ptr.into(), len.into()])
    }

    /// Mark a runtime guard's two edges as hot and cold.
    ///
    /// Every guard in the language has the same shape — branch to the continuation when
    /// the condition holds, to the failure path when it does not — so the false edge is
    /// always the cold one.
    pub(crate) fn mark_cold_branch(&self, branch: InstructionValue<'ctx>) -> CodegenResult<()> {
        let weights = self.context.metadata_node(&[
            self.context.metadata_string(BRANCH_WEIGHT_TAG).into(),
            self.context
                .i32_type()
                .const_int(HOT_EDGE_WEIGHT, false)
                .into(),
            self.context
                .i32_type()
                .const_int(COLD_EDGE_WEIGHT, false)
                .into(),
        ]);
        branch
            .set_metadata(weights, self.context.get_kind_id(BRANCH_WEIGHT_KIND))
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// Get or create the parameterless thunk printing `text`.
    fn cold_panic_thunk(&mut self, text: &str) -> CodegenResult<FunctionValue<'ctx>> {
        let key = (false, text.to_string());
        if let Some(existing) = self.cold_thunks.get(&key) {
            return Ok(*existing);
        }

        let fn_type = self.context.void_type().fn_type(&[], false);
        let thunk = self.declare_cold_thunk(fn_type);
        self.build_thunk_body(thunk, |ctx| {
            ctx.emit_write_cstr(text)?;
            ctx.emit_abort_unreachable()
        })?;

        self.cold_thunks.insert(key, thunk);
        Ok(thunk)
    }

    /// Get or create the `(ptr, i64)` thunk printing `panic: <message><suffix>`.
    fn cold_message_panic_thunk(&mut self, suffix: &str) -> CodegenResult<FunctionValue<'ctx>> {
        let key = (true, suffix.to_string());
        if let Some(existing) = self.cold_thunks.get(&key) {
            return Ok(*existing);
        }

        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = self
            .context
            .void_type()
            .fn_type(&[ptr_type.into(), self.context.i64_type().into()], false);
        let thunk = self.declare_cold_thunk(fn_type);
        self.build_thunk_body(thunk, |ctx| {
            let ptr = thunk.get_nth_param(0).ok_or_else(|| {
                CodegenError::InternalError("cold panic thunk lost its pointer parameter".into())
            })?;
            let len = thunk
                .get_nth_param(1)
                .ok_or_else(|| {
                    CodegenError::InternalError("cold panic thunk lost its length parameter".into())
                })?
                .into_int_value();
            ctx.emit_write_cstr("panic: ")?;
            ctx.emit_write(ptr, len)?;
            ctx.emit_write_cstr(suffix)?;
            ctx.emit_abort_unreachable()
        })?;

        self.cold_thunks.insert(key, thunk);
        Ok(thunk)
    }

    /// Declare the next cold thunk. Private linkage keeps it out of the symbol table;
    /// `noinline` is what actually holds the outlining in place, since without it the
    /// inliner would happily fold a single-call-site function straight back in.
    fn declare_cold_thunk(
        &self,
        fn_type: inkwell::types::FunctionType<'ctx>,
    ) -> FunctionValue<'ctx> {
        let name = format!("neuro.cold.panic.{}", self.cold_thunks.len());
        let thunk = self
            .module
            .add_function(&name, fn_type, Some(Linkage::Private));
        for attribute in ["cold", "noreturn", "noinline", "minsize"] {
            thunk.add_attribute(
                AttributeLoc::Function,
                self.context.create_enum_attribute(
                    inkwell::attributes::Attribute::get_named_enum_kind_id(attribute),
                    0,
                ),
            );
        }
        thunk
    }

    /// Fill `thunk`'s body, restoring the builder to the block the caller was emitting
    /// into. Thunks are created lazily, in the middle of generating a hot function.
    fn build_thunk_body(
        &mut self,
        thunk: FunctionValue<'ctx>,
        emit: impl FnOnce(&mut Self) -> CodegenResult<()>,
    ) -> CodegenResult<()> {
        let resume_at = self.builder.get_insert_block();
        let entry = self.context.append_basic_block(thunk, "entry");
        self.builder.position_at_end(entry);

        let result = emit(self);

        if let Some(block) = resume_at {
            self.builder.position_at_end(block);
        }
        result
    }

    /// Call an outlined thunk and terminate the block. The call site repeats the
    /// callee's `cold` / `noreturn` attributes so the information survives inlining of
    /// the *enclosing* function into one of its callers.
    fn emit_cold_call(
        &self,
        thunk: FunctionValue<'ctx>,
        args: &[inkwell::values::BasicMetadataValueEnum<'ctx>],
    ) -> CodegenResult<()> {
        let call = self
            .builder
            .build_call(thunk, args, "")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        for attribute in ["cold", "noreturn"] {
            call.add_attribute(
                AttributeLoc::Function,
                self.context.create_enum_attribute(
                    inkwell::attributes::Attribute::get_named_enum_kind_id(attribute),
                    0,
                ),
            );
        }
        self.builder
            .build_unreachable()
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }
}
