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
    /// `string.slice(a..b)` → a borrowed `&string` sub-slice; panics on an out-of-bounds
    /// or mid-codepoint boundary (§2.7).
    StringSlice,
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
    /// `array.len()` → the compile-time element count `N` of `[T; N]`, as `u64` (§3.1).
    ArrayLen,
}

/// Resolve a compiler-known intrinsic on a builtin receiver, returning the method
/// tag and its result type. Mirrors the resolver in `semantic-analysis`; the duplication
/// keeps the backend independent of the type-checker slice.
pub(crate) fn resolve_builtin_method(recv: &Type, method: &str) -> Option<(BuiltinMethod, Type)> {
    // Auto-deref an immutable borrow `&string` so `r.len()` / `r.clone()` resolve through
    // the reference (§2.4). The integer intrinsics below intentionally require a value
    // receiver — reading a scalar through a reference needs the deref operator (later phase).
    // The second element is the call's *result* type. The receiver type (possibly
    // `&string`) is recorded separately by the type pass, letting codegen decide whether
    // to load through the reference.
    match (recv.referent(), method) {
        (Type::String, "len") => Some((BuiltinMethod::StringLen, Type::U64)),
        (Type::String, "clone") => Some((BuiltinMethod::StringClone, Type::String)),
        // The slice's result is a borrowed `&string` view (§2.7); lowered to an opaque
        // pointer to the computed fat pointer.
        (Type::String, "slice") => Some((
            BuiltinMethod::StringSlice,
            Type::Reference(Box::new(Type::String)),
        )),
        // `array.len()` (§3.1) → the static element count as `u64`. Auto-derefs a
        // borrow of an array (`&[T; N]`) like the string builtins above.
        (Type::Array { .. }, "len") => Some((BuiltinMethod::ArrayLen, Type::U64)),
        // Integer intrinsics require a value receiver (matched on `recv`, not the referent):
        // reading a scalar through `&T` needs the deref operator. They return the receiver's
        // own integer type (§1.2, §1.4).
        (_, m) if recv.is_integer() => {
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
    /// Loop label (`outer:`, §3.7) when present, so a labeled `break`/`continue`
    /// can target this loop rather than the innermost one.
    pub(crate) label: Option<String>,
    /// The basic block where a `continue` statement should jump.
    pub(crate) continue_bb: BasicBlock<'ctx>,
    /// The basic block where a `break` statement should jump.
    pub(crate) break_bb: BasicBlock<'ctx>,
    /// Result slot for a value-producing `loop` (§3.7). A value-carrying `break v`
    /// stores `v` here before branching to `break_bb`; the loop expression loads
    /// it at exit. `None` for `while`/`for` (unit) and unit `loop`s.
    pub(crate) break_slot: Option<PointerValue<'ctx>>,
    /// Index of this loop's body drop scope in `drop_scopes`. A `break`/`continue`
    /// leaving the loop runs the destructors of every scope from the innermost open
    /// one down to and including this one, before branching (§2.1).
    pub(crate) drop_scope_depth: usize,
}

/// A live owned binding (local or by-value parameter) of a `Drop` type whose
/// destructor must run at scope exit (§2.1).
///
/// `flag_ptr` is an `i1` slot initialized to `true` at the binding site and set
/// `false` when the value is moved out, so the scope-exit drop is elided for a
/// moved value (§2.2) — the runtime drop-flag mechanism that keeps conditional
/// moves sound.
pub(crate) struct DropEntry<'ctx> {
    /// Source binding name, used to clear the flag when the value is moved.
    pub(crate) name: String,
    /// Address of the binding's storage; passed as the `&mut self` receiver to `drop`.
    pub(crate) storage_ptr: PointerValue<'ctx>,
    /// The `i1` drop-flag slot.
    pub(crate) flag_ptr: PointerValue<'ctx>,
    /// Struct name, used to resolve the `{struct}__drop` destructor function.
    pub(crate) struct_name: String,
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

    /// Resolved Neuro types of the in-scope local bindings, parameters, and `self`
    /// (name → type), populated as each binding is lowered. The HIR carries every
    /// expression's type inline, so this only serves the place-statement codegen
    /// (`object.field = …` and `target[i] = …`) that must recover the *binding's*
    /// nominal type — a struct or array name LLVM types do not preserve.
    pub(crate) type_env: HashMap<String, Type>,

    /// Active loop targets for break/continue statements.
    pub(crate) loop_targets: Vec<LoopTargets<'ctx>>,

    /// Struct field definitions (name → ordered [(field_name, field_type)]).
    /// Populated before code generation begins; used by GEP and insertvalue.
    pub(crate) struct_defs: HashMap<String, Vec<(String, Type)>>,

    /// Evaluated constant values (both module-level and function-level).
    /// `codegen_identifier` checks this before `variables` to allow locals to shadow consts.
    pub(crate) const_values: HashMap<String, BasicValueEnum<'ctx>>,

    /// When true (debug builds, `-O0`), integer `+`/`-`/`*` are emitted with
    /// overflow detection that traps at runtime. When false (release builds),
    /// the plain wrapping instruction is emitted. See §1.2.
    pub(crate) overflow_checks: bool,

    /// Source text wrapper for the module being compiled, used to render `file:line:col`
    /// in panic-family diagnostics (§1.2). `None` when the caller did not supply source
    /// (e.g. the library doctest); panic diagnostics then omit the location suffix.
    pub(crate) source: Option<SourceFile>,

    /// Names of structs implementing the `Drop` lang-item (`impl Drop for T`, §2.1).
    /// A binding of such a type gets a scope-exit destructor call. Empty for programs
    /// with no Drop types, in which case all drop machinery below stays inert.
    pub(crate) drop_types: std::collections::HashSet<String>,

    /// Stack of lexical drop scopes, innermost last. Each scope lists the owned
    /// `Drop`-typed bindings declared in it, in declaration order; on normal scope
    /// exit they are dropped in reverse (LIFO). Empty unless `drop_types` is non-empty.
    pub(crate) drop_scopes: Vec<Vec<DropEntry<'ctx>>>,
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
            type_env: HashMap::new(),
            loop_targets: Vec::new(),
            struct_defs: HashMap::new(),
            const_values: HashMap::new(),
            overflow_checks: false,
            source: None,
            drop_types: std::collections::HashSet::new(),
            drop_scopes: Vec::new(),
        }
    }

    /// Record the set of structs implementing `Drop` (§2.1) before code generation.
    pub(crate) fn set_drop_types(&mut self, drop_types: std::collections::HashSet<String>) {
        self.drop_types = drop_types;
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

    /// Get the external libc `malloc` declaration, inserting it on first use.
    /// `malloc(size: i64) -> ptr`. Backs the heap buffer for runtime string
    /// concatenation (§2.7); `size_t` is 64-bit on every supported target.
    pub(crate) fn get_or_declare_malloc(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("malloc") {
            return f;
        }
        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = ptr_type.fn_type(&[self.context.i64_type().into()], false);
        self.module
            .add_function("malloc", fn_type, Some(inkwell::module::Linkage::External))
    }

    /// Get the external libc `memcpy` declaration, inserting it on first use.
    /// `memcpy(dst: ptr, src: ptr, n: i64) -> dst`. Copies each operand's bytes
    /// into the freshly allocated buffer during string concatenation (§2.7).
    pub(crate) fn get_or_declare_memcpy(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("memcpy") {
            return f;
        }
        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = ptr_type.fn_type(
            &[
                ptr_type.into(),
                ptr_type.into(),
                self.context.i64_type().into(),
            ],
            false,
        );
        self.module
            .add_function("memcpy", fn_type, Some(inkwell::module::Linkage::External))
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
    fn string_slice_resolves_to_string_reference() {
        let resolved = resolve_builtin_method(&Type::String, "slice");
        assert!(matches!(
            resolved,
            Some((BuiltinMethod::StringSlice, Type::Reference(inner))) if matches!(*inner, Type::String)
        ));
    }

    #[test]
    fn slice_resolves_through_a_string_borrow() {
        // A `&string` receiver auto-derefs (§2.4), so `.slice` resolves on it too.
        let recv = Type::Reference(Box::new(Type::String));
        assert!(matches!(
            resolve_builtin_method(&recv, "slice"),
            Some((BuiltinMethod::StringSlice, _))
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
