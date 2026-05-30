// NEURO Programming Language - LLVM Backend
// Code generation context and LLVM IR generation

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context as LLVMContext;
use inkwell::module::Module;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};
use std::collections::HashMap;

use crate::type_mapping::TypeMapper;
use crate::types::Type;

/// Tracks basic blocks for loop control flow (`continue` and `break`).
pub(crate) struct LoopTargets<'ctx> {
    /// The basic block where a `continue` statement should jump.
    pub(crate) continue_bb: BasicBlock<'ctx>,
    /// The basic block where a `break` statement should jump.
    pub(crate) break_bb: BasicBlock<'ctx>,
}

/// Central state container for LLVM IR code generation.
pub(crate) struct CodegenContext<'ctx> {
    /// LLVM's thread-local execution context.
    pub(crate) context: &'ctx LLVMContext,
    /// The top-level LLVM module being generated.
    pub(crate) module: Module<'ctx>,
    /// Builder used to emit LLVM IR instructions.
    pub(crate) builder: Builder<'ctx>,
    /// Maps high-level AST types to low-level LLVM types.
    pub(crate) type_mapper: TypeMapper<'ctx>,

    /// Local variables in the current function (name -> pointer to stack allocation)
    pub(crate) variables: HashMap<String, PointerValue<'ctx>>,

    /// Types of local variables (needed for opaque pointers)
    pub(crate) variable_types: HashMap<String, BasicTypeEnum<'ctx>>,

    /// Function declarations (name -> LLVM function)
    pub(crate) functions: HashMap<String, FunctionValue<'ctx>>,

    /// Current function being compiled (for return type checking)
    pub(crate) current_function: Option<FunctionValue<'ctx>>,

    /// Type information for expressions (needed for operator codegen)
    pub(crate) expr_types: HashMap<usize, Type>, // Maps expression span.start -> Type

    /// Variable type information during type collection (name -> Type)
    pub(crate) type_env: HashMap<String, Type>,

    /// Active loop targets for break/continue statements.
    pub(crate) loop_targets: Vec<LoopTargets<'ctx>>,

    /// Struct field definitions (name → ordered [(field_name, field_type)]).
    /// Populated before code generation begins; used by GEP and insertvalue.
    pub(crate) struct_defs: HashMap<String, Vec<(String, Type)>>,

    /// Maps FieldAccess span.start → struct name of the object.
    /// Needed because FieldAccess and its first sub-expression (the object Identifier)
    /// share the same span.start, causing expr_types collisions.
    pub(crate) fa_struct_names: HashMap<usize, String>,

    /// Evaluated constant values (both module-level and function-level).
    /// `codegen_identifier` checks this before `variables` to allow locals to shadow consts.
    pub(crate) const_values: HashMap<String, BasicValueEnum<'ctx>>,

    /// Types of module-level constants, pre-populated so `visit_function_for_types`
    /// can seed `type_env` after each clear.
    pub(crate) global_const_types: HashMap<String, Type>,

    /// When true (debug builds, `-O0`), integer `+`/`-`/`*` are emitted with
    /// overflow detection that traps at runtime. When false (release builds),
    /// the plain wrapping instruction is emitted. See §1.2.
    pub(crate) overflow_checks: bool,
}

impl<'ctx> CodegenContext<'ctx> {
    pub(crate) fn new(context: &'ctx LLVMContext, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();
        let type_mapper = TypeMapper::new(context);

        Self {
            context,
            module,
            builder,
            type_mapper,
            variables: HashMap::new(),
            variable_types: HashMap::new(),
            functions: HashMap::new(),
            current_function: None,
            expr_types: HashMap::new(),
            type_env: HashMap::new(),
            loop_targets: Vec::new(),
            struct_defs: HashMap::new(),
            fa_struct_names: HashMap::new(),
            const_values: HashMap::new(),
            global_const_types: HashMap::new(),
            overflow_checks: false,
        }
    }

    /// Enable or disable debug-build integer overflow trapping.
    /// Enabled for `-O0` (debug), disabled for `-O1..-O3` (release).
    pub(crate) fn set_overflow_checks(&mut self, enabled: bool) {
        self.overflow_checks = enabled;
    }

    /// Get the external `memcmp` declaration, inserting it on first use.
    /// memcmp(s1: ptr, s2: ptr, n: i64) -> i32 — libc, always available on Linux/macOS.
    pub(crate) fn get_or_declare_memcmp(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("memcmp") {
            return f;
        }
        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = self.context.i32_type().fn_type(
            &[
                ptr_type.into(),
                ptr_type.into(),
                self.context.i64_type().into(),
            ],
            false,
        );
        self.module
            .add_function("memcmp", fn_type, Some(inkwell::module::Linkage::External))
    }
}
