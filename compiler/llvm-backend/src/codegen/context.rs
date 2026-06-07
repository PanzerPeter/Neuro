// Neuro Programming Language - LLVM Backend
// Code generation context and LLVM IR generation

use inkwell::attributes::Attribute;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context as LLVMContext;
use inkwell::module::Module;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};
use source_location::SourceFile;
use std::collections::HashMap;

use crate::type_mapping::TypeMapper;
use crate::types::Type;

/// A compiler-known intrinsic method on a builtin (non-struct) receiver type.
/// Recorded by the type-collection pass so `codegen_expr` can lower the call
/// without a struct mangled-name lookup.
#[derive(Clone, Copy)]
pub(crate) enum BuiltinMethod {
    /// `string.len()` → field-1 byte length of the string fat pointer (§2.7).
    StringLen,
    /// `string.clone()` → a copy of the string fat pointer value (§2.7).
    StringClone,
    /// `struct.clone()` → a copy of the struct aggregate value, for `@derive(Clone)` types (§2.3).
    StructClone,
    /// `int.wrapping_add(rhs)` → two's-complement wrapping add (§1.2).
    WrappingAdd,
    /// `int.wrapping_sub(rhs)` → two's-complement wrapping subtract (§1.2).
    WrappingSub,
    /// `int.wrapping_mul(rhs)` → two's-complement wrapping multiply (§1.2).
    WrappingMul,
    /// `int.saturating_add(rhs)` → clamp to type MIN/MAX on overflow (§1.2).
    SaturatingAdd,
    /// `int.saturating_sub(rhs)` → clamp to type MIN/MAX on overflow (§1.2).
    SaturatingSub,
    /// `int.saturating_mul(rhs)` → clamp to type MIN/MAX on overflow (§1.2).
    SaturatingMul,
    /// `int.shr(n)` → right shift: arithmetic for signed, logical for unsigned (§1.4).
    Shr,
}

/// Resolve a compiler-known intrinsic on a builtin receiver, returning the method
/// tag and its result type. Mirrors the resolver in `semantic-analysis`; the duplication
/// keeps the backend independent of the type-checker slice.
pub(crate) fn resolve_builtin_method(recv: &Type, method: &str) -> Option<(BuiltinMethod, Type)> {
    match (recv, method) {
        (Type::String, "len") => Some((BuiltinMethod::StringLen, Type::U64)),
        (Type::String, "clone") => Some((BuiltinMethod::StringClone, Type::String)),
        // Integer intrinsics return the receiver's own integer type (§1.2, §1.4).
        (t, m) if t.is_integer() => {
            let kind = match m {
                "wrapping_add" => BuiltinMethod::WrappingAdd,
                "wrapping_sub" => BuiltinMethod::WrappingSub,
                "wrapping_mul" => BuiltinMethod::WrappingMul,
                "saturating_add" => BuiltinMethod::SaturatingAdd,
                "saturating_sub" => BuiltinMethod::SaturatingSub,
                "saturating_mul" => BuiltinMethod::SaturatingMul,
                "shr" => BuiltinMethod::Shr,
                _ => return None,
            };
            Some((kind, recv.clone()))
        }
        _ => None,
    }
}

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

    /// Maps a binary expression's full span `(start, end)` → its left-operand type, used by
    /// `codegen_binary` to pick the comparison/arithmetic instruction width and signedness.
    /// Keyed by the full span rather than `span.start + 1`: a binary node and its leftmost
    /// descendant share the same `span.start`, so the parent's left-type slot would clobber
    /// the child's. `(start, end)` is unique per node (the child's `end` is always smaller).
    pub(crate) binary_left_types: HashMap<(usize, usize), Type>,

    /// Maps a builtin method-call's `Call` full span `(start, end)` → the resolved intrinsic
    /// plus the receiver's type, so `codegen_expr` lowers it directly instead of looking up a
    /// struct method. The receiver type is stored here rather than read back from `expr_types`
    /// because an enclosing cast (`x.shr(n) as T`) shares the receiver's `span.start` and
    /// would overwrite that entry, losing the receiver's signedness. Keyed by the full span
    /// rather than `span.start` because a chained builtin call (`s.clone().len()`) nests two
    /// `Call` nodes that share the same `span.start`; the full `(start, end)` is unique per node.
    pub(crate) builtin_methods: HashMap<(usize, usize), (BuiltinMethod, Type)>,

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

    /// Source text wrapper for the module being compiled, used to render `file:line:col`
    /// in panic-family diagnostics (§1.2). `None` when the caller did not supply source
    /// (e.g. the library doctest); panic diagnostics then omit the location suffix.
    pub(crate) source: Option<SourceFile>,
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
            binary_left_types: HashMap::new(),
            builtin_methods: HashMap::new(),
            const_values: HashMap::new(),
            global_const_types: HashMap::new(),
            overflow_checks: false,
            source: None,
        }
    }

    /// Enable or disable debug-build integer overflow trapping.
    /// Enabled for `-O0` (debug), disabled for `-O1..-O3` (release).
    pub(crate) fn set_overflow_checks(&mut self, enabled: bool) {
        self.overflow_checks = enabled;
    }

    /// Provide the module source so panic-family diagnostics can render `file:line:col`.
    pub(crate) fn set_source(&mut self, source: SourceFile) {
        self.source = Some(source);
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

    /// Get the external POSIX `write` declaration, inserting it on first use.
    /// `write(fd: i32, buf: ptr, count: i64) -> i64`. Used by the panic runtime to emit
    /// the diagnostic to stderr (fd 2); the return value is discarded. POSIX-standard on
    /// Linux/macOS and exposed by the MSVC CRT compatibility layer on Windows.
    pub(crate) fn get_or_declare_write(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("write") {
            return f;
        }
        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = self.context.i64_type().fn_type(
            &[
                self.context.i32_type().into(),
                ptr_type.into(),
                self.context.i64_type().into(),
            ],
            false,
        );
        self.module
            .add_function("write", fn_type, Some(inkwell::module::Linkage::External))
    }

    /// Get the external libc `abort` declaration, inserting it on first use.
    /// `abort() -> void`. Terminates the process via SIGABRT without unwinding the stack,
    /// which is exactly the §1.2 panic contract (no landing pads, `Drop`/`defer` skipped).
    pub(crate) fn get_or_declare_abort(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("abort") {
            return f;
        }
        let fn_type = self.context.void_type().fn_type(&[], false);
        let func =
            self.module
                .add_function("abort", fn_type, Some(inkwell::module::Linkage::External));
        func.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            self.context
                .create_enum_attribute(Attribute::get_named_enum_kind_id("noreturn"), 0),
        );
        func
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_builtin_method, BuiltinMethod};
    use crate::types::Type;

    #[test]
    fn string_len_resolves_to_u64() {
        let resolved = resolve_builtin_method(&Type::String, "len");
        assert!(matches!(
            resolved,
            Some((BuiltinMethod::StringLen, Type::U64))
        ));
    }

    #[test]
    fn string_clone_resolves_to_string() {
        let resolved = resolve_builtin_method(&Type::String, "clone");
        assert!(matches!(
            resolved,
            Some((BuiltinMethod::StringClone, Type::String))
        ));
    }

    #[test]
    fn unknown_builtin_method_is_unresolved() {
        assert!(resolve_builtin_method(&Type::String, "capacity").is_none());
        assert!(resolve_builtin_method(&Type::I32, "len").is_none());
        // `.clone()` is a string-only builtin; integers take the assignment (Copy) path.
        assert!(resolve_builtin_method(&Type::I32, "clone").is_none());
    }

    #[test]
    fn integer_intrinsics_resolve_to_receiver_type() {
        assert!(matches!(
            resolve_builtin_method(&Type::U8, "wrapping_add"),
            Some((BuiltinMethod::WrappingAdd, Type::U8))
        ));
        assert!(matches!(
            resolve_builtin_method(&Type::I64, "saturating_mul"),
            Some((BuiltinMethod::SaturatingMul, Type::I64))
        ));
        assert!(matches!(
            resolve_builtin_method(&Type::I32, "shr"),
            Some((BuiltinMethod::Shr, Type::I32))
        ));
    }

    #[test]
    fn integer_intrinsics_reject_non_integer_receiver() {
        assert!(resolve_builtin_method(&Type::String, "wrapping_add").is_none());
        assert!(resolve_builtin_method(&Type::F64, "saturating_sub").is_none());
        assert!(resolve_builtin_method(&Type::I32, "wrapping_div").is_none());
    }
}
