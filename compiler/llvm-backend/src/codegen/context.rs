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

use crate::errors::{CodegenError, CodegenResult};
use crate::type_mapping::TypeMapper;
use crate::types::{CollectionKind, Type};

/// A compiler-known intrinsic method on a builtin (non-struct) receiver type.
/// Recorded by the type-collection pass so `codegen_expr` can lower the call
/// without a struct mangled-name lookup.
#[derive(Clone, Copy)]
pub(crate) enum BuiltinMethod {
    /// `string.len()` → field-1 byte length of the string fat pointer.
    StringLen,
    /// `string.clone()` → a copy of the string fat pointer value.
    StringClone,
    /// `string.slice(a..b)` → a borrowed `&string` sub-slice; panics on an out-of-bounds
    /// or mid-codepoint boundary.
    StringSlice,
    /// `string.char_slice(a..b)` → a borrowed `&string` sub-slice whose range counts
    /// codepoints rather than bytes; panics on an out-of-bounds range. No boundary check
    /// is needed — a codepoint index cannot name a position inside a code point.
    StringCharSlice,
    /// `string.__char_at(offset)` → the Unicode scalar whose UTF-8 encoding begins at
    /// that byte. Private to the prelude, which writes its codepoint iterator against it.
    StringCharAt,
    /// `struct.clone()` → a copy of the struct aggregate value, for `@derive(Clone)` types.
    StructClone,
    /// `int.wrapping_add(rhs)` → two's-complement wrapping add.
    WrappingAdd,
    /// `int.wrapping_sub(rhs)` → two's-complement wrapping subtract.
    WrappingSub,
    /// `int.wrapping_mul(rhs)` → two's-complement wrapping multiply.
    WrappingMul,
    /// `int.saturating_add(rhs)` → clamp to type MIN/MAX on overflow.
    SaturatingAdd,
    /// `int.saturating_sub(rhs)` → clamp to type MIN/MAX on overflow.
    SaturatingSub,
    /// `int.saturating_mul(rhs)` → clamp to type MIN/MAX on overflow.
    SaturatingMul,
    /// `int.shr(n)` → right shift: arithmetic for signed, logical for unsigned.
    Shr,
    /// `int.checked_add(rhs)` → `Option::Some(sum)`, or `Option::None` on overflow.
    CheckedAdd,
    /// `int.checked_sub(rhs)` → `Option::Some(difference)`, or `Option::None` on overflow.
    CheckedSub,
    /// `int.checked_mul(rhs)` → `Option::Some(product)`, or `Option::None` on overflow.
    CheckedMul,
    /// `array.len()` → the compile-time element count `N` of `[T; N]`, as `u64`.
    ArrayLen,
    /// `seq.slice(a..b)` on an array, `Vec`, or slice → a borrowed `&[T]` view over the
    /// receiver's buffer; panics on an out-of-bounds range.
    SequenceSlice,
    /// `slice.len()` → the length word of the `(ptr, len)` fat pointer, as `u64`.
    SliceLen,
    /// `float.is_nan()` → `fcmp uno`: an unordered self-comparison, true only for NaN.
    IsNan,
}

/// Resolve a compiler-known intrinsic on a builtin receiver. Mirrors the resolver in
/// `semantic-analysis`; the duplication keeps the backend independent of the
/// type-checker slice.
///
/// Only the method tag is resolved here — the call's result type comes from the HIR
/// node, because `checked_*` yields a monomorphized `Option<T>` instance whose mangled
/// name only the frontend can produce.
pub(crate) fn resolve_builtin_method(recv: &Type, method: &str) -> Option<BuiltinMethod> {
    // Auto-deref an immutable borrow `&string` so `r.len()` / `r.clone()` resolve through
    // the reference. The integer intrinsics below intentionally require a value
    // receiver — reading a scalar through a reference needs the deref operator (later phase).
    // The receiver type (possibly `&string`) is carried by the HIR receiver node, letting
    // codegen decide whether to load through the reference.
    match (recv.referent(), method) {
        (Type::String, "len") => Some(BuiltinMethod::StringLen),
        (Type::String, "clone") => Some(BuiltinMethod::StringClone),
        (Type::String, "slice") => Some(BuiltinMethod::StringSlice),
        (Type::String, "char_slice") => Some(BuiltinMethod::StringCharSlice),
        (Type::String, "__char_at") => Some(BuiltinMethod::StringCharAt),
        // `array.len()` → the static element count as `u64`. Auto-derefs a
        // borrow of an array (`&[T; N]`) like the string builtins above.
        (Type::Array { .. }, "len") => Some(BuiltinMethod::ArrayLen),
        // `.slice(range)` borrows a run out of any contiguous container. A `Vec`
        // receiver resolves here rather than through the collection surface: the result
        // is a view into the buffer, not an operation on the header.
        (
            Type::Array { .. }
            | Type::Slice(_)
            | Type::Collection {
                kind: CollectionKind::Vec,
                ..
            },
            "slice",
        ) => Some(BuiltinMethod::SequenceSlice),
        (Type::Slice(_), "len") => Some(BuiltinMethod::SliceLen),
        // `.is_nan()` needs a value receiver too, and is spelled out over `F32`/`F64`
        // rather than `Type::is_float`: that backend predicate also admits `f16`/`bf16`,
        // whose scalar contract is storage and casts only.
        (_, "is_nan") if matches!(recv, Type::F32 | Type::F64) => Some(BuiltinMethod::IsNan),
        // Integer intrinsics require a value receiver (matched on `recv`, not the referent):
        // reading a scalar through `&T` needs the deref operator.
        (_, m) if recv.is_integer() => match m {
            "wrapping_add" => Some(BuiltinMethod::WrappingAdd),
            "wrapping_sub" => Some(BuiltinMethod::WrappingSub),
            "wrapping_mul" => Some(BuiltinMethod::WrappingMul),
            "saturating_add" => Some(BuiltinMethod::SaturatingAdd),
            "saturating_sub" => Some(BuiltinMethod::SaturatingSub),
            "saturating_mul" => Some(BuiltinMethod::SaturatingMul),
            "shr" => Some(BuiltinMethod::Shr),
            "checked_add" => Some(BuiltinMethod::CheckedAdd),
            "checked_sub" => Some(BuiltinMethod::CheckedSub),
            "checked_mul" => Some(BuiltinMethod::CheckedMul),
            _ => None,
        },
        _ => None,
    }
}

/// Tracks basic blocks for loop control flow (`continue` and `break`).
pub(crate) struct LoopTargets<'ctx> {
    /// Loop label (`outer:`) when present, so a labeled `break`/`continue`
    /// can target this loop rather than the innermost one.
    pub(crate) label: Option<String>,
    /// The basic block where a `continue` statement should jump.
    pub(crate) continue_bb: BasicBlock<'ctx>,
    /// The basic block where a `break` statement should jump.
    pub(crate) break_bb: BasicBlock<'ctx>,
    /// Result slot for a value-producing `loop`. A value-carrying `break v`
    /// stores `v` here before branching to `break_bb`; the loop expression loads
    /// it at exit. `None` for `while`/`for` (unit) and unit `loop`s.
    pub(crate) break_slot: Option<PointerValue<'ctx>>,
    /// Index of this loop's body drop scope in `drop_scopes`. A `break`/`continue`
    /// leaving the loop runs the destructors of every scope from the innermost open
    /// one down to and including this one, before branching.
    pub(crate) drop_scope_depth: usize,
}

/// A live owned binding (local or by-value parameter) of a `Drop` type whose
/// destructor must run at scope exit.
///
/// `flag_ptr` is an `i1` slot initialized to `true` at the binding site and set
/// `false` when the value is moved out, so the scope-exit drop is elided for a
/// moved value — the runtime drop-flag mechanism that keeps conditional
/// moves sound.
pub(crate) struct DropEntry<'ctx> {
    /// Source binding name, used to clear the flag when the value is moved.
    pub(crate) name: String,
    /// Address of the binding's storage; passed as the `&mut self` receiver to `drop`.
    pub(crate) storage_ptr: PointerValue<'ctx>,
    /// The `i1` drop-flag slot.
    pub(crate) flag_ptr: PointerValue<'ctx>,
    /// What running the destructor means for this binding.
    pub(crate) target: DropTarget,
}

/// How a scope-exit destructor is emitted for an owned binding.
#[derive(Clone)]
pub(crate) enum DropTarget {
    /// A user `impl Drop for T`: call `{T}__drop(&mut self)`.
    UserDrop(String),
    /// A standard collection: release the heap buffer its header points at.
    Collection,
    /// A `string` binding initialized from a producer that always allocates
    /// (interpolation or `+`): release the buffer its fat pointer points at.
    /// Only a binding whose initializer was proven to allocate carries this, so
    /// a `.rodata` literal is never handed to `free`.
    HeapString,
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
    /// Struct key → the name the programmer wrote for it. The two differ only for a
    /// monomorphized generic instance, whose key is mangled; the derived debug
    /// rendering shows the written name.
    pub(crate) struct_written_names: HashMap<String, String>,

    /// Evaluated constant values (both module-level and function-level).
    /// `codegen_identifier` checks this before `variables` to allow locals to shadow consts.
    pub(crate) const_values: HashMap<String, BasicValueEnum<'ctx>>,

    /// When true (debug builds, `-O0`), integer `+`/`-`/`*` are emitted with
    /// overflow detection that traps at runtime. When false (release builds),
    /// the plain wrapping instruction is emitted. See.
    pub(crate) overflow_checks: bool,

    /// Source text wrapper for the module being compiled, used to render `file:line:col`
    /// in panic-family diagnostics. `None` when the caller did not supply source
    /// (e.g. the library doctest); panic diagnostics then omit the location suffix.
    pub(crate) source: Option<SourceFile>,

    /// Names of structs implementing the `Drop` lang-item (`impl Drop for T`).
    /// A binding of such a type gets a scope-exit destructor call. Empty for programs
    /// with no Drop types, in which case all drop machinery below stays inert.
    pub(crate) drop_types: std::collections::HashSet<String>,

    /// Stack of lexical drop scopes, innermost last. Each scope lists the owned
    /// `Drop`-typed bindings declared in it, in declaration order; on normal scope
    /// exit they are dropped in reverse (LIFO). Empty unless `drop_types` is non-empty.
    pub(crate) drop_scopes: Vec<Vec<DropEntry<'ctx>>>,

    /// Declared traits → their method names in declaration order. The index of a
    /// name in this list is its vtable slot, shared by every implementor of the trait.
    /// Empty for programs that declare no traits.
    pub(crate) trait_methods: HashMap<String, Vec<String>>,

    /// Emitted vtables, keyed by `(trait name, concrete type name)`. Each is a
    /// private global array of pointers to that type's thunks, in trait method order.
    pub(crate) vtables: HashMap<(String, String), inkwell::values::GlobalValue<'ctx>>,

    /// Enum name → variant names in declaration order, which is discriminant order.
    /// Surface constructions carry their tag in the HIR; this table serves the
    /// constructions codegen synthesizes itself, which know a variant only by name.
    pub(crate) enum_variants: HashMap<String, Vec<String>>,

    /// Outlined cold panic thunks, keyed by whether the thunk takes a runtime message
    /// and by the constant diagnostic text baked into it. Failure sites worded
    /// identically share one body instead of emitting the diagnostic machinery twice.
    pub(crate) cold_thunks: HashMap<(bool, String), FunctionValue<'ctx>>,

    /// Every `abort` and `llvm.trap` call emitted, in emission order. Neither runs an
    /// exit hook, so buffered standard output has to be drained immediately in front of
    /// them; `finalize_stdout_buffer` does that once the module is known to print at all.
    /// Empty for a program with no panic path and no `-O0` overflow check.
    pub(crate) process_exit_points: Vec<inkwell::values::InstructionValue<'ctx>>,
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
            struct_written_names: HashMap::new(),
            const_values: HashMap::new(),
            overflow_checks: false,
            source: None,
            trait_methods: HashMap::new(),
            vtables: HashMap::new(),
            drop_types: std::collections::HashSet::new(),
            drop_scopes: Vec::new(),
            enum_variants: HashMap::new(),
            cold_thunks: HashMap::new(),
            process_exit_points: Vec::new(),
        }
    }

    /// Record the call just emitted as a point where the process stops running, so the
    /// standard-output buffer is drained in front of it.
    ///
    /// Takes the block's last instruction rather than the call's own value: inkwell's
    /// `CallSiteValue` does not convert to an `InstructionValue`, and a call is the last
    /// thing in its block at the moment it is built.
    pub(crate) fn record_process_exit(&mut self) {
        let last = self
            .builder
            .get_insert_block()
            .and_then(|block| block.get_last_instruction());
        if let Some(instruction) = last {
            self.process_exit_points.push(instruction);
        }
    }

    /// Allocate a fixed-size stack slot in the current function's **entry block**.
    ///
    /// Every local binding and every result/scratch slot must go through this. An
    /// `alloca` emitted at the current builder position is executed once per pass
    /// through that position, so a slot allocated inside a loop body grows the stack by
    /// one slot per iteration until the process runs out of it — a segfault on a
    /// perfectly ordinary counted loop. LLVM's `mem2reg` cannot rescue it either: the
    /// pass only promotes allocas that are already in the entry block, so the leak
    /// survives every optimization level.
    ///
    /// The initializing store stays where the caller emits it. Reusing one slot across
    /// iterations is sound because every one of these slots is written before it is
    /// read, and a fresh frame per call keeps recursion correct.
    pub(crate) fn entry_alloca(
        &self,
        ty: impl inkwell::types::BasicType<'ctx>,
        name: &str,
    ) -> CodegenResult<inkwell::values::PointerValue<'ctx>> {
        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("stack slot requested outside a function".to_string())
        })?;
        let entry = function.get_first_basic_block().ok_or_else(|| {
            CodegenError::InternalError("function has no entry block".to_string())
        })?;

        let restore = self.builder.get_insert_block();
        match entry.get_first_instruction() {
            Some(first) => self.builder.position_before(&first),
            None => self.builder.position_at_end(entry),
        }
        let slot = self
            .builder
            .build_alloca(ty, name)
            .map_err(|e| CodegenError::LlvmError(e.to_string()));
        if let Some(block) = restore {
            self.builder.position_at_end(block);
        }
        slot
    }

    /// Record each enum's variant order before code generation, so a synthesized
    /// construction can resolve a variant name to its discriminant.
    pub(crate) fn set_enum_variants(&mut self, enum_variants: HashMap<String, Vec<String>>) {
        self.enum_variants = enum_variants;
    }

    /// The discriminant of `variant` in `enum_name`.
    pub(crate) fn enum_variant_tag(&self, enum_name: &str, variant: &str) -> CodegenResult<u32> {
        self.enum_variants
            .get(enum_name)
            .and_then(|variants| variants.iter().position(|v| v == variant))
            .map(|index| index as u32)
            .ok_or_else(|| {
                CodegenError::InternalError(format!(
                    "enum '{}' has no variant '{}'",
                    enum_name, variant
                ))
            })
    }

    /// Get the external libc `memset` declaration, inserting it on first use.
    /// `memset(dst, byte: i32, n: i64) -> dst`. Resets a collection's slots on `clear`.
    pub(crate) fn get_or_declare_memset(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("memset") {
            return f;
        }
        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = ptr_type.fn_type(
            &[
                ptr_type.into(),
                self.context.i32_type().into(),
                self.context.i64_type().into(),
            ],
            false,
        );
        self.module
            .add_function("memset", fn_type, Some(inkwell::module::Linkage::External))
    }

    /// Record the set of structs implementing `Drop` before code generation.
    /// Record each declared trait's method order, fixing the vtable slot layout.
    pub(crate) fn set_trait_methods(&mut self, trait_methods: HashMap<String, Vec<String>>) {
        self.trait_methods = trait_methods;
    }

    pub(crate) fn set_drop_types(&mut self, drop_types: std::collections::HashSet<String>) {
        self.drop_types = drop_types;
    }

    /// Record each enum's payload word count so enum types map to the
    /// `{ i32, [W x i64] }` tagged union before code generation begins.
    pub(crate) fn set_enum_words(&mut self, enum_words: std::collections::HashMap<String, u32>) {
        self.type_mapper.set_enum_words(enum_words);
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
    /// concatenation; `size_t` is 64-bit on every supported target.
    pub(crate) fn get_or_declare_malloc(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("malloc") {
            return f;
        }
        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = ptr_type.fn_type(&[self.context.i64_type().into()], false);
        self.module
            .add_function("malloc", fn_type, Some(inkwell::module::Linkage::External))
    }

    /// Get the external libc `free` declaration, inserting it on first use.
    /// `free(ptr)`. Releases a collection's heap buffer when its owner leaves scope;
    /// a null pointer is a defined no-op, so an untouched empty collection needs no guard.
    pub(crate) fn get_or_declare_free(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("free") {
            return f;
        }
        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = self.context.void_type().fn_type(&[ptr_type.into()], false);
        self.module
            .add_function("free", fn_type, Some(inkwell::module::Linkage::External))
    }

    /// Get the external libc `realloc` declaration, inserting it on first use.
    /// `realloc(ptr, size: i64) -> ptr`. Grows a collection's buffer, preserving its
    /// contents; a null `ptr` degenerates to `malloc`, which is how the first
    /// insertion into an empty collection allocates.
    pub(crate) fn get_or_declare_realloc(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("realloc") {
            return f;
        }
        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = ptr_type.fn_type(&[ptr_type.into(), self.context.i64_type().into()], false);
        self.module
            .add_function("realloc", fn_type, Some(inkwell::module::Linkage::External))
    }

    /// Get the external libc `memmove` declaration, inserting it on first use.
    /// `memmove(dst, src, n: i64) -> dst`. Shifts the ordered map's slot array on
    /// insertion and removal, where source and destination overlap.
    pub(crate) fn get_or_declare_memmove(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("memmove") {
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
            .add_function("memmove", fn_type, Some(inkwell::module::Linkage::External))
    }

    /// Get the external libc `memcpy` declaration, inserting it on first use.
    /// `memcpy(dst: ptr, src: ptr, n: i64) -> dst`. Copies each operand's bytes
    /// into the freshly allocated buffer during string concatenation.
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
    /// which is exactly the panic contract (no landing pads, `Drop`/`defer` skipped).
    pub(crate) fn get_or_declare_abort(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("abort") {
            return f;
        }
        let fn_type = self.context.void_type().fn_type(&[], false);
        let func =
            self.module
                .add_function("abort", fn_type, Some(inkwell::module::Linkage::External));
        // `cold` alongside `noreturn`: a block whose terminator is `unreachable` is only
        // treated as unlikely-executed by LLVM's placement heuristics when the call
        // preceding it is itself marked cold — `noreturn` on its own does not imply it.
        for attribute in ["noreturn", "cold"] {
            func.add_attribute(
                inkwell::attributes::AttributeLoc::Function,
                self.context
                    .create_enum_attribute(Attribute::get_named_enum_kind_id(attribute), 0),
            );
        }
        func
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_builtin_method, BuiltinMethod};
    use crate::types::{CollectionKind, Type};

    #[test]
    fn string_intrinsics_resolve() {
        assert!(matches!(
            resolve_builtin_method(&Type::String, "len"),
            Some(BuiltinMethod::StringLen)
        ));
        assert!(matches!(
            resolve_builtin_method(&Type::String, "clone"),
            Some(BuiltinMethod::StringClone)
        ));
        assert!(matches!(
            resolve_builtin_method(&Type::String, "slice"),
            Some(BuiltinMethod::StringSlice)
        ));
        assert!(matches!(
            resolve_builtin_method(&Type::String, "char_slice"),
            Some(BuiltinMethod::StringCharSlice)
        ));
    }

    /// Every contiguous container answers `.slice` with the same intrinsic, and a `Vec`
    /// receiver reaches it here rather than through the collection method surface.
    #[test]
    fn sequence_slice_resolves_on_every_contiguous_receiver() {
        let array = Type::Array {
            element: Box::new(Type::I32),
            size: 4,
        };
        let vector = Type::Collection {
            kind: CollectionKind::Vec,
            args: vec![Type::I32],
        };
        let slice = Type::Reference {
            inner: Box::new(Type::Slice(Box::new(Type::I32))),
            mutable: false,
        };
        for recv in [array, vector, slice.clone()] {
            assert!(matches!(
                resolve_builtin_method(&recv, "slice"),
                Some(BuiltinMethod::SequenceSlice)
            ));
        }
        assert!(matches!(
            resolve_builtin_method(&slice, "len"),
            Some(BuiltinMethod::SliceLen)
        ));
    }

    #[test]
    fn slice_resolves_through_a_string_borrow() {
        // A `&string` receiver auto-derefs, so both slice spellings resolve on it too.
        let recv = Type::Reference {
            inner: Box::new(Type::String),
            mutable: false,
        };
        assert!(matches!(
            resolve_builtin_method(&recv, "slice"),
            Some(BuiltinMethod::StringSlice)
        ));
        assert!(matches!(
            resolve_builtin_method(&recv, "char_slice"),
            Some(BuiltinMethod::StringCharSlice)
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
    fn integer_intrinsics_resolve_on_any_integer_receiver() {
        assert!(matches!(
            resolve_builtin_method(&Type::U8, "wrapping_add"),
            Some(BuiltinMethod::WrappingAdd)
        ));
        assert!(matches!(
            resolve_builtin_method(&Type::I64, "saturating_mul"),
            Some(BuiltinMethod::SaturatingMul)
        ));
        assert!(matches!(
            resolve_builtin_method(&Type::I32, "shr"),
            Some(BuiltinMethod::Shr)
        ));
    }

    #[test]
    fn checked_intrinsics_resolve_on_any_integer_receiver() {
        assert!(matches!(
            resolve_builtin_method(&Type::U8, "checked_add"),
            Some(BuiltinMethod::CheckedAdd)
        ));
        assert!(matches!(
            resolve_builtin_method(&Type::I32, "checked_sub"),
            Some(BuiltinMethod::CheckedSub)
        ));
        assert!(matches!(
            resolve_builtin_method(&Type::I64, "checked_mul"),
            Some(BuiltinMethod::CheckedMul)
        ));
    }

    #[test]
    fn is_nan_resolves_on_full_precision_floats_only() {
        assert!(matches!(
            resolve_builtin_method(&Type::F32, "is_nan"),
            Some(BuiltinMethod::IsNan)
        ));
        assert!(matches!(
            resolve_builtin_method(&Type::F64, "is_nan"),
            Some(BuiltinMethod::IsNan)
        ));
        assert!(resolve_builtin_method(&Type::F16, "is_nan").is_none());
        assert!(resolve_builtin_method(&Type::BF16, "is_nan").is_none());
        assert!(resolve_builtin_method(&Type::I32, "is_nan").is_none());
        // A value receiver is required, as for the integer intrinsics.
        let borrowed = Type::Reference {
            inner: Box::new(Type::F64),
            mutable: false,
        };
        assert!(resolve_builtin_method(&borrowed, "is_nan").is_none());
    }

    #[test]
    fn integer_intrinsics_reject_non_integer_receiver() {
        assert!(resolve_builtin_method(&Type::String, "wrapping_add").is_none());
        assert!(resolve_builtin_method(&Type::F64, "saturating_sub").is_none());
        assert!(resolve_builtin_method(&Type::I32, "wrapping_div").is_none());
        assert!(resolve_builtin_method(&Type::F64, "checked_add").is_none());
        assert!(resolve_builtin_method(&Type::I32, "checked_div").is_none());
    }
}
