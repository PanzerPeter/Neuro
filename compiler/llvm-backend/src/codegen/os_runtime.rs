//! Calls into the operating-system runtime, `os_runtime.c`, behind `std::fs`, `std::env`,
//! `std::io::read_line` and `std::time`.
//!
//! The standard library's bodies reach the runtime through intrinsics only a library module
//! may name (`__read_text(path)`). Each lowers to one call: a `&string` or `&[u8]` argument
//! is passed as its pointer and byte count, and a result holding text is read back from the
//! buffer the runtime allocated, which the program then owns.
//!
//! A program links the runtime only when one of its own functions reaches it. Every library
//! body is emitted, so the library's functions nothing calls are deleted first; a program that
//! does no I/O keeps its `main` and links nothing.

use inkwell::AddressSpace;
use inkwell::context::Context;
use inkwell::module::{Linkage, Module};
use inkwell::types::BasicMetadataTypeEnum;
use inkwell::values::{
    BasicMetadataValueEnum, BasicValue, BasicValueEnum, FunctionValue, IntValue, PointerValue,
};
use neuro_hir::HirExpr;

use super::CodegenContext;
use super::external_bodies::{internalize, link_ir};
use crate::errors::{CodegenError, CodegenResult};
use crate::types::{CollectionKind, Type};

const OS_RUNTIME_IR: &str = include_str!("os_runtime.ll");

/// What every standard-library function's name holds except the prelude's, whose names stay
/// bare: the prefix of a library function, and part of a prelude method instantiated over a
/// library type (`Result$g_..std::fs::..__is_ok`). Only these are pruned: a prelude function
/// can be named by a vtable the program never builds a value for, and is small.
const LIBRARY_PREFIX: &str = "std::";

/// The intrinsics, by the name a library body calls them with, and the runtime entry each
/// one calls. The two writes share an entry: a `string` and a `&[u8]` reach it alike.
const INTRINSICS: [(&str, &str); 10] = [
    ("__read_text", "__neuro_read_text"),
    ("__read_bytes", "__neuro_read_bytes"),
    ("__write_text", "__neuro_write_file"),
    ("__write_bytes", "__neuro_write_file"),
    ("__remove_file", "__neuro_remove_file"),
    ("__arg_count", "__neuro_arg_count"),
    ("__arg", "__neuro_arg"),
    ("__env_var", "__neuro_env_var"),
    ("__read_line", "__neuro_read_line"),
    ("__monotonic_nanos", "__neuro_monotonic_nanos"),
];

/// What the runtime defines, made internal once linked.
const RUNTIME_ENTRY_POINTS: [&str; 9] = [
    "__neuro_read_text",
    "__neuro_read_bytes",
    "__neuro_write_file",
    "__neuro_remove_file",
    "__neuro_arg_count",
    "__neuro_arg",
    "__neuro_env_var",
    "__neuro_read_line",
    "__neuro_monotonic_nanos",
];

/// The entries that read the command line, which only the entry point can store.
const ARGUMENT_ENTRY_POINTS: [&str; 2] = ["__neuro_arg_count", "__neuro_arg"];

/// The command line as the runtime reads it.
const ARGC_GLOBAL: &str = "__neuro_argc";
const ARGV_GLOBAL: &str = "__neuro_argv";

/// The runtime's hooks for what each system spells its own way. See `os_runtime.c`.
const ERRNO_HOOK: &str = "__neuro_errno_location";
const CLOCK_HOOK: &str = "__neuro_clock_gettime";
const CLOCK_ID_HOOK: &str = "__neuro_clock_id";
const COUNTER_HOOK: &str = "__neuro_query_counter";
const FREQUENCY_HOOK: &str = "__neuro_query_frequency";

/// Where `errno` lives: glibc and musl, Darwin, and the Windows CRT name it differently.
const ERRNO_FN: &str = if cfg!(target_os = "windows") {
    "_errno"
} else if cfg!(target_os = "macos") {
    "__error"
} else {
    "__errno_location"
};

/// `CLOCK_MONOTONIC`, which Linux and Darwin number differently. Windows has no
/// `clock_gettime` and counts with the performance counter instead.
const MONOTONIC_CLOCK_ID: u64 = if cfg!(target_os = "macos") { 6 } else { 1 };

/// The C entry symbol, and where the program's own `main` goes when the entry stores the
/// command line first. The dot keeps it out of the names a Neuro declaration can have.
const ENTRY: &str = "main";
const PROGRAM_MAIN: &str = "neuro.program_main";

/// The status a runtime entry returns on success. See `os_runtime.c`.
const STATUS_OK: u64 = 0;

/// The shape of what an intrinsic hands back, read off its declared result type.
enum Answer {
    /// A count or a clock reading, returned by the entry as it is.
    Word,
    /// A `string` the runtime wrote, its status dropped (`__arg`).
    Text,
    /// `(string, i32)`: the text, or the reason on failure, and the status.
    TextAndStatus,
    /// `(Vec<u8>, string, i32)`: the bytes on success, the reason on failure, and the status.
    BytesReasonAndStatus,
}

impl<'ctx> CodegenContext<'ctx> {
    /// Whether `name` is one of the library's runtime intrinsics. Mirrors the resolver in
    /// `semantic-analysis`; the duplication keeps the backend independent of it.
    pub(crate) fn is_os_intrinsic(name: &str) -> bool {
        INTRINSICS.iter().any(|(intrinsic, _)| *intrinsic == name)
    }

    /// Lower a call to the intrinsic `name`, whose declared result type is `result`.
    pub(crate) fn codegen_os_intrinsic(
        &mut self,
        name: &str,
        args: &[HirExpr],
        result: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let symbol = INTRINSICS
            .iter()
            .find(|(intrinsic, _)| *intrinsic == name)
            .map(|(_, symbol)| *symbol)
            .ok_or_else(|| CodegenError::InternalError(format!("no runtime intrinsic `{name}`")))?;
        let answer = answer_shape(result)?;
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let i64_type = self.context.i64_type();
        let i32_type = self.context.i32_type();

        let mut types: Vec<BasicMetadataTypeEnum<'ctx>> = Vec::new();
        let mut values: Vec<BasicMetadataValueEnum<'ctx>> = Vec::new();
        for arg in args {
            let value = self.codegen_expr(arg)?;
            match value {
                // A `&string` or a `&[u8]`: its pointer and its byte count.
                BasicValueEnum::StructValue(fat) => {
                    let ptr = self.builder.build_extract_value(fat, 0, "os.arg.ptr")?;
                    let len = if matches!(Type::from_hir(&arg.ty).referent(), Type::String) {
                        self.string_len(fat)?
                    } else {
                        self.builder
                            .build_extract_value(fat, 1, "os.arg.len")?
                            .into_int_value()
                    };
                    types.extend::<[BasicMetadataTypeEnum; 2]>([ptr_type.into(), i64_type.into()]);
                    values.extend::<[BasicMetadataValueEnum; 2]>([ptr.into(), len.into()]);
                }
                BasicValueEnum::IntValue(word) => {
                    let word = self.builder.build_int_z_extend_or_bit_cast(
                        word,
                        i64_type,
                        "os.arg.word",
                    )?;
                    types.push(i64_type.into());
                    values.push(word.into());
                }
                other => {
                    return Err(CodegenError::InternalError(format!(
                        "`{name}` was handed {other:?}"
                    )));
                }
            }
        }

        if let Answer::Word = answer {
            let entry = self.extern_fn(symbol, i64_type.fn_type(&types, false));
            return self.call_word(entry, &values, "os.word");
        }

        let out = self.entry_alloca(ptr_type, "os.out")?;
        let out_len = self.entry_alloca(i64_type, "os.out.len")?;
        self.builder.build_store(out, ptr_type.const_null())?;
        self.builder.build_store(out_len, i64_type.const_zero())?;
        types.extend::<[BasicMetadataTypeEnum; 2]>([ptr_type.into(), ptr_type.into()]);
        values.extend::<[BasicMetadataValueEnum; 2]>([out.into(), out_len.into()]);
        if name == "__read_line" {
            // A prompt written with `print` has no newline to drain it, and must be on the
            // screen before the program waits for the answer.
            let sync = self.stdin_sync_fn();
            self.builder.build_call(sync, &[], "")?;
        }
        let entry = self.extern_fn(symbol, i32_type.fn_type(&types, false));
        let status = self
            .call_word(entry, &values, "os.status")?
            .into_int_value();
        let buffer = self
            .builder
            .build_load(ptr_type, out, "os.buf")?
            .into_pointer_value();
        let length = self
            .builder
            .build_load(i64_type, out_len, "os.len")?
            .into_int_value();

        match answer {
            Answer::Text => self.build_owned_string_value(buffer, length),
            Answer::TextAndStatus => {
                let text = self.build_owned_string_value(buffer, length)?;
                self.build_tuple(result, &[text, status.into()])
            }
            Answer::BytesReasonAndStatus => {
                self.split_bytes_and_reason(result, buffer, length, status)
            }
            Answer::Word => Err(CodegenError::InternalError(
                "a word answer reached the text path".into(),
            )),
        }
    }

    /// `(bytes, "", 0)` on success and `(empty, reason, status)` on failure: the runtime's
    /// one buffer is whichever the status says it is.
    fn split_bytes_and_reason(
        &mut self,
        result: &Type,
        buffer: PointerValue<'ctx>,
        length: IntValue<'ctx>,
        status: IntValue<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let null = self.context.ptr_type(AddressSpace::default()).const_null();
        let zero = self.context.i64_type().const_zero();
        let succeeded = self.builder.build_int_compare(
            inkwell::IntPredicate::EQ,
            status,
            status.get_type().const_int(STATUS_OK, false),
            "os.ok",
        )?;
        let pick = |ctx: &Self, ok: BasicValueEnum<'ctx>, failed: BasicValueEnum<'ctx>| {
            ctx.builder.build_select(succeeded, ok, failed, "os.pick")
        };
        let bytes_ptr = pick(self, buffer.into(), null.into())?.into_pointer_value();
        let bytes_len = pick(self, length.into(), zero.into())?.into_int_value();
        let reason_ptr = pick(self, null.into(), buffer.into())?.into_pointer_value();
        let reason_len = pick(self, zero.into(), length.into())?.into_int_value();

        // The runtime's buffer comes from `malloc`, which is what a `Vec` grows with
        // `realloc` and releases with `free`, so it becomes the vector's own.
        let header_ty = self.type_mapper.collection_header_type();
        let mut header = header_ty.const_zero();
        for (field, value) in [
            (0, BasicValueEnum::from(bytes_ptr)),
            (1, bytes_len.into()),
            (2, bytes_len.into()),
        ] {
            header = self
                .builder
                .build_insert_value(header, value, field, "os.bytes")?
                .into_struct_value();
        }
        let reason = self.build_owned_string_value(reason_ptr, reason_len)?;
        self.build_tuple(result, &[header.into(), reason, status.into()])
    }

    fn build_tuple(
        &self,
        ty: &Type,
        fields: &[BasicValueEnum<'ctx>],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let tuple_ty = self.get_any_llvm_type(ty)?.into_struct_type();
        let mut tuple = tuple_ty.get_undef();
        for (index, field) in fields.iter().enumerate() {
            tuple = self
                .builder
                .build_insert_value(tuple, *field, index as u32, "os.result")?
                .into_struct_value();
        }
        Ok(tuple.into())
    }

    fn call_word(
        &self,
        entry: FunctionValue<'ctx>,
        values: &[BasicMetadataValueEnum<'ctx>],
        name: &str,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        self.builder
            .build_call(entry, values, name)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError(format!("{name} returned nothing")))
    }

    /// Delete the standard library's functions nothing calls, until none is left, and the
    /// runtime entries only they called, so the runtime is linked and the command line stored
    /// only for a program that uses them.
    pub(crate) fn prune_unused_library_functions(&mut self) {
        loop {
            let unused: Vec<_> = self
                .module
                .get_functions()
                .filter(|function| {
                    function
                        .get_name()
                        .to_str()
                        .is_ok_and(|name| name.contains(LIBRARY_PREFIX))
                })
                .filter(|function| !is_used(*function))
                .collect();
            if unused.is_empty() {
                break;
            }
            for function in unused {
                // The builder may still sit in the last body emitted. Deleting that body frees
                // its blocks, and a later `get_insert_block` would hand back a dangling one.
                if self
                    .builder
                    .get_insert_block()
                    .and_then(|block| block.get_parent())
                    == Some(function)
                {
                    self.builder.clear_insertion_position();
                }
                if let Ok(name) = function.get_name().to_str() {
                    self.functions.remove(name);
                }
                // SAFETY: the function has no use left in the module, and its handle was just
                // dropped from `functions`, the only place a later pass could find it by.
                unsafe { function.delete() };
            }
        }
        // What the deleted bodies called is left declared and unused.
        for symbol in RUNTIME_ENTRY_POINTS {
            if let Some(entry) = self.module.get_function(symbol)
                && !is_used(entry)
            {
                // SAFETY: the declaration has no use, and the backend keeps no handle to it.
                unsafe { entry.delete() };
            }
        }
    }

    /// Whether a function the program keeps calls the runtime entry `symbol`.
    fn calls(&self, symbol: &str) -> bool {
        self.module.get_function(symbol).is_some_and(is_used)
    }

    /// Whether the module calls the runtime, so it needs linking in.
    pub(crate) fn requires_os_runtime(&self) -> bool {
        RUNTIME_ENTRY_POINTS.iter().any(|symbol| self.calls(symbol))
    }

    /// Store the command line where the runtime reads it, before the program runs, when the
    /// program reads it. A test build's entry already takes `argc` and `argv`, and stores
    /// them first; otherwise `main` moves aside for an entry that stores them and calls it.
    pub(crate) fn store_program_arguments(&mut self) -> CodegenResult<()> {
        if !ARGUMENT_ENTRY_POINTS
            .iter()
            .any(|symbol| self.calls(symbol))
        {
            return Ok(());
        }
        let Some(main) = self.module.get_function(ENTRY) else {
            return Ok(());
        };
        let i32_type = self.context.i32_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let resume_at = self.builder.get_insert_block();

        let entry = if main.count_params() == 2 {
            let start = main
                .get_first_basic_block()
                .ok_or_else(|| CodegenError::InternalError("the entry has no body".into()))?;
            match start.get_first_instruction() {
                Some(first) => self.builder.position_before(&first),
                None => self.builder.position_at_end(start),
            }
            main
        } else {
            main.as_global_value().set_name(PROGRAM_MAIN);
            let entry = self.module.add_function(
                ENTRY,
                i32_type.fn_type(&[i32_type.into(), ptr_type.into()], false),
                Some(Linkage::External),
            );
            let start = self.context.append_basic_block(entry, "start");
            self.builder.position_at_end(start);
            entry
        };

        let (argc, argv) = self.command_line(entry)?;
        let argc_global = self.runtime_global(ARGC_GLOBAL, i32_type.into());
        let argv_global = self.runtime_global(ARGV_GLOBAL, ptr_type.into());
        self.builder.build_store(argc_global, argc)?;
        self.builder.build_store(argv_global, argv)?;

        if entry != main {
            let status = self.call_word(main, &[], "status")?;
            self.builder.build_return(Some(&status))?;
        }
        if let Some(block) = resume_at {
            self.builder.position_at_end(block);
        }
        Ok(())
    }

    /// `argc` and `argv` as the entry point can read them. On Windows they come from the
    /// CRT, which keeps them whether or not its startup code called `main` with them: a link
    /// that names `main` as the image entry hands it no arguments at all.
    fn command_line(
        &self,
        entry: FunctionValue<'ctx>,
    ) -> CodegenResult<(BasicValueEnum<'ctx>, BasicValueEnum<'ctx>)> {
        let i32_type = self.context.i32_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        if cfg!(target_os = "windows") {
            let argc_fn = self.extern_fn("__p___argc", ptr_type.fn_type(&[], false));
            let argv_fn = self.extern_fn("__p___argv", ptr_type.fn_type(&[], false));
            let argc_at = self
                .call_word(argc_fn, &[], "argc.at")?
                .into_pointer_value();
            let argv_at = self
                .call_word(argv_fn, &[], "argv.at")?
                .into_pointer_value();
            let argc = self.builder.build_load(i32_type, argc_at, "argc")?;
            let argv = self.builder.build_load(ptr_type, argv_at, "argv")?;
            return Ok((argc, argv));
        }
        let missing = || CodegenError::InternalError("the entry has no command line".into());
        Ok((
            entry.get_nth_param(0).ok_or_else(missing)?,
            entry.get_nth_param(1).ok_or_else(missing)?,
        ))
    }

    /// A declaration of the runtime global `name`, which linking the runtime defines.
    fn runtime_global(
        &self,
        name: &str,
        ty: inkwell::types::BasicTypeEnum<'ctx>,
    ) -> PointerValue<'ctx> {
        self.module
            .get_global(name)
            .unwrap_or_else(|| self.module.add_global(ty, None, name))
            .as_pointer_value()
    }
}

/// Whether anything in the module refers to `function`: a call, a vtable, a stored pointer.
pub(crate) fn is_used(function: FunctionValue<'_>) -> bool {
    function
        .as_global_value()
        .as_pointer_value()
        .get_first_use()
        .is_some()
}

fn answer_shape(result: &Type) -> CodegenResult<Answer> {
    let shape = match result {
        Type::String => Answer::Text,
        Type::Tuple(fields) => match fields.as_slice() {
            [Type::String, _] => Answer::TextAndStatus,
            [
                Type::Collection {
                    kind: CollectionKind::Vec,
                    ..
                },
                Type::String,
                _,
            ] => Answer::BytesReasonAndStatus,
            _ => return Err(unexpected(result)),
        },
        other if other.is_integer() => Answer::Word,
        _ => return Err(unexpected(result)),
    };
    Ok(shape)
}

fn unexpected(result: &Type) -> CodegenError {
    CodegenError::InternalError(format!("no runtime intrinsic answers {result:?}"))
}

/// Link the runtime into `module`, make it internal, and point its hooks at what this
/// system calls `errno` and its monotonic clock.
pub(crate) fn link_os_runtime<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> CodegenResult<()> {
    link_ir(context, module, OS_RUNTIME_IR, "the OS runtime")?;
    internalize(module, &RUNTIME_ENTRY_POINTS);

    let i32_type = context.i32_type();
    let ptr_type = context.ptr_type(AddressSpace::default());
    let declare = |name: &str, ty: inkwell::types::FunctionType<'ctx>| {
        module
            .get_function(name)
            .unwrap_or_else(|| module.add_function(name, ty, Some(Linkage::External)))
            .as_global_value()
            .as_pointer_value()
    };
    let mut hooks = vec![(ERRNO_HOOK, declare(ERRNO_FN, ptr_type.fn_type(&[], false)))];
    if cfg!(target_os = "windows") {
        let query = i32_type.fn_type(&[ptr_type.into()], false);
        hooks.push((COUNTER_HOOK, declare("QueryPerformanceCounter", query)));
        hooks.push((FREQUENCY_HOOK, declare("QueryPerformanceFrequency", query)));
    } else {
        let gettime = i32_type.fn_type(&[i32_type.into(), ptr_type.into()], false);
        hooks.push((CLOCK_HOOK, declare("clock_gettime", gettime)));
    }
    for (hook, target) in hooks {
        let global = runtime_hook(module, hook)?;
        global.set_initializer(&target);
    }
    runtime_hook(module, CLOCK_ID_HOOK)?
        .set_initializer(&i32_type.const_int(MONOTONIC_CLOCK_ID, false));

    for name in [
        ERRNO_HOOK,
        CLOCK_HOOK,
        CLOCK_ID_HOOK,
        COUNTER_HOOK,
        FREQUENCY_HOOK,
        ARGC_GLOBAL,
        ARGV_GLOBAL,
    ] {
        if let Some(global) = module.get_global(name) {
            global.set_linkage(Linkage::Internal);
        }
    }
    Ok(())
}

fn runtime_hook<'ctx>(
    module: &Module<'ctx>,
    name: &str,
) -> CodegenResult<inkwell::values::GlobalValue<'ctx>> {
    module
        .get_global(name)
        .ok_or_else(|| CodegenError::InternalError(format!("the OS runtime defines no `{name}`")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_intrinsic_calls_an_entry_the_runtime_defines() {
        for (intrinsic, symbol) in INTRINSICS {
            assert!(
                RUNTIME_ENTRY_POINTS.contains(&symbol),
                "`{intrinsic}` calls `{symbol}`, which is not an entry point"
            );
            assert!(
                OS_RUNTIME_IR.contains(&format!("@{symbol}(")),
                "os_runtime.ll defines no `{symbol}`"
            );
        }
    }

    #[test]
    fn every_hook_is_a_global_of_the_runtime() {
        for hook in [
            ERRNO_HOOK,
            CLOCK_HOOK,
            CLOCK_ID_HOOK,
            COUNTER_HOOK,
            FREQUENCY_HOOK,
            ARGC_GLOBAL,
            ARGV_GLOBAL,
        ] {
            assert!(
                OS_RUNTIME_IR.contains(&format!("@{hook} = ")),
                "os_runtime.ll defines no global `{hook}`"
            );
        }
    }
}
