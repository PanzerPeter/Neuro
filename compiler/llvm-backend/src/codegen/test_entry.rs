//! The entry point of a `neurc test` executable.
//!
//! The harness runs the one executable once per test, naming the test by its index in
//! `argv[1]`, so a panicking test aborts only its own process. The entry is emitted as the C
//! `main` and the program's own `main`, if it has one, keeps its body under another symbol,
//! so a test may still call it.

use inkwell::AddressSpace;
use inkwell::module::Linkage;

use super::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};

/// The C runtime's entry symbol.
const ENTRY: &str = "main";

/// Where the program's own `main` goes. The dot keeps it out of the names a Neuro
/// declaration can have.
const PROGRAM_MAIN: &str = "neuro.program_main";

/// The exit status for an index that names no test, which only a caller other than the
/// harness can pass.
const NO_SUCH_TEST: u64 = 2;

impl<'ctx> CodegenContext<'ctx> {
    /// Emit `main(argc, argv)` calling `tests[atoi(argv[1])]` and returning 0. Runs before
    /// the standard-output drain is placed, which treats this function's returns as the
    /// process's.
    pub(crate) fn define_test_entry(&mut self, tests: &[String]) -> CodegenResult<()> {
        if let Some(program_main) = self.module.get_function(ENTRY) {
            // Calls already emitted hold the function, not its name, so they follow it.
            program_main.as_global_value().set_name(PROGRAM_MAIN);
        }

        let i32_type = self.context.i32_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let entry = self.module.add_function(
            ENTRY,
            i32_type.fn_type(&[i32_type.into(), ptr_type.into()], false),
            Some(Linkage::External),
        );
        let atoi = self.extern_fn("atoi", i32_type.fn_type(&[ptr_type.into()], false));

        let start = self.context.append_basic_block(entry, "start");
        let select = self.context.append_basic_block(entry, "select");
        let unknown = self.context.append_basic_block(entry, "unknown");
        let missing = |what: &str| CodegenError::InternalError(format!("test entry: {what}"));

        self.builder.position_at_end(start);
        let argc = entry
            .get_nth_param(0)
            .ok_or_else(|| missing("no argc"))?
            .into_int_value();
        let argv = entry
            .get_nth_param(1)
            .ok_or_else(|| missing("no argv"))?
            .into_pointer_value();
        let named = self.builder.build_int_compare(
            inkwell::IntPredicate::SGT,
            argc,
            i32_type.const_int(1, false),
            "named",
        )?;
        self.builder
            .build_conditional_branch(named, select, unknown)?;

        self.builder.position_at_end(select);
        // SAFETY: the C runtime gives `argv` `argc` entries, and this block is reached only
        // with `argc > 1`, so entry 1 is inside the array.
        let slot = unsafe {
            self.builder.build_in_bounds_gep(
                ptr_type,
                argv,
                &[i32_type.const_int(1, false)],
                "slot",
            )?
        };
        let text = self.builder.build_load(ptr_type, slot, "text")?;
        let index = self
            .builder
            .build_call(atoi, &[text.into()], "index")?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| missing("atoi returned nothing"))?
            .into_int_value();

        let mut cases = Vec::with_capacity(tests.len());
        for (position, name) in tests.iter().enumerate() {
            let test = self
                .module
                .get_function(name)
                .ok_or_else(|| CodegenError::UndefinedFunction(name.clone()))?;
            let block = self.context.append_basic_block(entry, "test");
            self.builder.position_at_end(block);
            self.builder.build_call(test, &[], "")?;
            self.builder
                .build_return(Some(&i32_type.const_int(0, false)))?;
            cases.push((i32_type.const_int(position as u64, false), block));
        }
        self.builder.position_at_end(select);
        self.builder.build_switch(index, unknown, &cases)?;

        self.builder.position_at_end(unknown);
        self.builder
            .build_return(Some(&i32_type.const_int(NO_SUCH_TEST, false)))?;
        Ok(())
    }
}
