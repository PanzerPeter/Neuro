// Feature slice for LLVM IR generation and optimization.
// Public API: the `compile()` entry point.

mod codegen;
mod errors;
mod softfloat;
mod type_mapping;
mod types;

pub use errors::{CodegenError, CodegenResult};

use inkwell::context::Context as LLVMContext;
use inkwell::OptimizationLevel as LlvmOptimizationLevel;
use neuro_hir::{HirItem, HirProgram};
use std::collections::HashMap;
use types::Type;

use codegen::CodegenContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationLevelSetting {
    O0,
    O1,
    O2,
    O3,
}

impl OptimizationLevelSetting {
    pub fn from_u8(level: u8) -> CodegenResult<Self> {
        match level {
            0 => Ok(Self::O0),
            1 => Ok(Self::O1),
            2 => Ok(Self::O2),
            3 => Ok(Self::O3),
            other => Err(CodegenError::InvalidOptimizationLevel(other)),
        }
    }

    fn to_llvm(self) -> LlvmOptimizationLevel {
        match self {
            Self::O0 => LlvmOptimizationLevel::None,
            Self::O1 => LlvmOptimizationLevel::Less,
            Self::O2 => LlvmOptimizationLevel::Default,
            Self::O3 => LlvmOptimizationLevel::Aggressive,
        }
    }
}

/// Compile a typed HIR program to linkable LLVM object code.
///
/// The backend's entry point. It consumes the HIR produced by `hir-lowering`
/// and emits LLVM IR, then object code; every HIR node carries its resolved
/// type, so the backend reads types directly rather than re-deriving them.
///
/// # Arguments
///
/// * `optimization` - Optimization level (also selects overflow trapping at -O0)
/// * `source` / `source_path` - Original module text and path, used only to render
///   `file:line:col` in panic-family runtime diagnostics
///
/// # Examples
///
/// ```
/// use syntax_parsing::parse;
/// use hir_lowering::lower_program;
/// use llvm_backend::{compile, OptimizationLevelSetting};
///
/// let source = "func add(a: i32, b: i32) -> i32 { return a + b }";
/// let ast = parse(source).unwrap();
/// let hir = lower_program(&ast).unwrap();
/// let object_code =
///     compile(&hir, OptimizationLevelSetting::O2, source, "example.nr").unwrap();
/// // Write object_code to file or link to executable
/// ```
pub fn compile(
    program: &HirProgram,
    optimization: OptimizationLevelSetting,
    source: &str,
    source_path: &str,
) -> CodegenResult<Vec<u8>> {
    let context = LLVMContext::create();
    let codegen_ctx = build_module(&context, program, optimization, source, source_path)?;
    emit_object_code(&codegen_ctx, optimization)
}

/// Generate and verify the LLVM module for `program`.
///
/// Split from `compile` so the emitted IR can be inspected directly; object emission
/// erases the structure the codegen tests assert on (cold thunks, branch weights).
fn build_module<'ctx>(
    context: &'ctx LLVMContext,
    program: &HirProgram,
    optimization: OptimizationLevelSetting,
    source: &str,
    source_path: &str,
) -> CodegenResult<CodegenContext<'ctx>> {
    let items = &program.items;

    // Collect struct definitions first so struct field/parameter types resolve below.
    let mut struct_defs: HashMap<String, Vec<(String, Type)>> = HashMap::new();
    let mut struct_written_names: HashMap<String, String> = HashMap::new();
    for item in items {
        if let HirItem::Struct(def) = item {
            let mut fields = Vec::new();
            for field in &def.fields {
                fields.push((field.name.clone(), Type::from_hir(&field.ty)));
            }
            struct_defs.insert(def.name.clone(), fields);
            struct_written_names.insert(def.name.clone(), def.written_name.clone());
        }
    }

    // Collect each enum's payload word count `W`: the widest variant's field
    // count, so every value of the enum maps to one `{ i32, [W x i64] }` aggregate.
    let mut enum_words: HashMap<String, u32> = HashMap::new();
    // Variant names in declaration (discriminant) order, so a compiler-generated
    // construction — the `Option<T>` a collection reader returns — can look a tag up
    // by name instead of assuming the prelude's declaration order.
    let mut enum_variants: HashMap<String, Vec<String>> = HashMap::new();
    for item in items {
        if let HirItem::Enum(def) = item {
            let words = def
                .variants
                .iter()
                .map(|v| v.fields.len())
                .max()
                .unwrap_or(0) as u32;
            enum_words.insert(def.name.clone(), words);
            enum_variants.insert(
                def.name.clone(),
                def.variants.iter().map(|v| v.name.clone()).collect(),
            );
        }
    }

    // Extract function signatures from the HIR (caller validated semantics already).
    let mut func_types = HashMap::new();
    for item in items {
        match item {
            HirItem::Function(func_def) => {
                let param_types = func_def
                    .params
                    .iter()
                    .map(|p| Type::from_hir(&p.ty))
                    .collect();
                func_types.insert(
                    func_def.name.clone(),
                    Type::Function {
                        params: param_types,
                        ret: Box::new(Type::from_hir(&func_def.return_type)),
                    },
                );
            }

            HirItem::Impl(impl_def) => {
                let struct_name = &impl_def.type_name;
                for method in &impl_def.methods {
                    // An owned `self` reaches codegen only on a `Copy` receiver
                    // (operator-trait methods); it needs a registered signature
                    // like `&self`. Non-`Copy` owned `self` was rejected by the checker.
                    let mangled = format!("{}__{}", struct_name, method.name);
                    let mut param_types: Vec<Type> = Vec::new();

                    // Implicit `self` parameter for instance methods.
                    if method.self_param.is_some() {
                        param_types.push(Type::Struct(struct_name.clone()));
                    }

                    for param in &method.params {
                        param_types.push(Type::from_hir(&param.ty));
                    }

                    func_types.insert(
                        mangled,
                        Type::Function {
                            params: param_types,
                            ret: Box::new(Type::from_hir(&method.return_type)),
                        },
                    );
                }
            }

            HirItem::Struct(_)
            | HirItem::Const(_)
            | HirItem::Enum(_)
            | HirItem::Trait(_)
            | HirItem::Closure(_) => {}
        }
    }

    // Collect each declared trait's method order. The position of a method in
    // this list is its vtable slot, so every implementor lays out its table identically
    // and a virtual call can index a fixed offset.
    let mut trait_methods: HashMap<String, Vec<String>> = HashMap::new();
    for item in items {
        if let HirItem::Trait(def) = item {
            trait_methods.insert(def.name.clone(), def.methods.clone());
        }
    }

    // Collect the structs implementing `Drop` so codegen can insert their
    // scope-exit destructor calls. Semantic analysis has already validated the
    // `impl Drop for T { func drop(&mut self) }` shape and the no-Copy rule.
    let mut drop_types: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in items {
        if let HirItem::Impl(impl_def) = item {
            if impl_def.trait_name.as_deref() == Some("Drop") {
                drop_types.insert(impl_def.type_name.clone());
            }
        }
    }

    let mut codegen_ctx = CodegenContext::new(context, "neuro_module");
    codegen_ctx.set_struct_defs(struct_defs);
    codegen_ctx.set_struct_written_names(struct_written_names);
    codegen_ctx.set_enum_words(enum_words);
    codegen_ctx.set_enum_variants(enum_variants);
    codegen_ctx.set_drop_types(drop_types);
    codegen_ctx.set_trait_methods(trait_methods);

    // Supply source so panic-family builtins can render `file:line:col` in their
    // runtime diagnostics.
    codegen_ctx.set_source(source_location::SourceFile::new(
        source_path.to_string(),
        source.to_string(),
    ));

    // Debug builds (-O0) trap on integer overflow; release builds wrap.
    codegen_ctx.set_overflow_checks(optimization == OptimizationLevelSetting::O0);
    // A tensor is copied as one LLVM value, which only `-O1`'s SROA turns into a memcpy.
    codegen_ctx.set_tensor_limit(optimization == OptimizationLevelSetting::O0);

    // Emit module-level constants as LLVM global constants before any function.
    // This ensures all globals are defined before function bodies reference them.
    for item in items {
        if let HirItem::Const(def) = item {
            codegen_ctx.codegen_global_const(def)?;
        }
    }

    // Pre-declare every function/method signature before generating any body, so a
    // call resolves regardless of definition order. Monomorphized generic instances
    // may be called by — or call — items appearing before them, so lazy
    // per-item declaration is not sufficient.
    for item in items {
        match item {
            HirItem::Function(func_def) => {
                codegen_ctx.declare_function(func_def, &func_types)?;
            }
            HirItem::Impl(impl_def) => {
                codegen_ctx.declare_impl(impl_def, &func_types)?;
            }
            HirItem::Closure(closure) => {
                codegen_ctx.declare_closure(closure)?;
            }
            HirItem::Const(_) | HirItem::Struct(_) | HirItem::Enum(_) | HirItem::Trait(_) => {}
        }
    }

    // Emit each `(trait, type)` method table once every method signature is declared but
    // before any body is generated, so a trait object built anywhere in the module finds
    // its vtable already present regardless of item order.
    codegen_ctx.emit_vtables(items)?;

    // Generate code for each function and impl method
    for item in items {
        match item {
            HirItem::Function(func_def) => {
                codegen_ctx.codegen_function(func_def, &func_types)?;
            }
            HirItem::Impl(impl_def) => {
                codegen_ctx.codegen_impl(impl_def, &func_types)?;
            }
            HirItem::Closure(closure) => {
                codegen_ctx.codegen_closure(closure)?;
            }
            HirItem::Const(_) | HirItem::Struct(_) | HirItem::Enum(_) | HirItem::Trait(_) => {}
        }
    }

    // Drain buffered standard output on every path out of the process. Runs here because
    // only a finished module knows whether it prints at all, and because the exit paths
    // it edits — `main`'s returns, `abort`, `llvm.trap` — are all emitted by now.
    codegen_ctx.finalize_stdout_buffer()?;

    // Link self-contained soft-float conversion builtins when the module uses
    // f16/bf16, so the emitted object resolves the half-precision libcalls
    // itself instead of depending on a platform runtime (libgcc/compiler-rt),
    // which is absent under the Windows linkers. See `softfloat`.
    if softfloat::module_uses_half_precision(&codegen_ctx.module) {
        softfloat::link_builtins(codegen_ctx.context, &codegen_ctx.module)
            .map_err(CodegenError::LlvmError)?;
    }

    // Verify the module
    if let Err(err) = codegen_ctx.module.verify() {
        return Err(CodegenError::LlvmError(format!(
            "module verification failed: {}",
            err
        )));
    }

    Ok(codegen_ctx)
}

impl OptimizationLevelSetting {
    /// The name of the LLVM middle-end pass pipeline to run before instruction
    /// selection, or `None` at -O0 where the IR is handed to the backend as emitted.
    ///
    /// `TargetMachine`'s own optimization level only tunes instruction selection and
    /// register allocation — it runs no IR passes at all. Without this pipeline every
    /// local stays in the `alloca` codegen gave it (no mem2reg/SROA), no call is
    /// inlined, and nothing is hoisted out of a loop, so -O1..-O3 emit essentially
    /// the same code as -O0.
    fn pass_pipeline(self) -> Option<&'static str> {
        match self {
            // -O0 also selects checked arithmetic; leaving the IR untouched keeps
            // every overflow check and bounds guard exactly where codegen put it.
            Self::O0 => None,
            Self::O1 => Some("default<O1>"),
            Self::O2 => Some("default<O2>"),
            Self::O3 => Some("default<O3>"),
        }
    }
}

/// Build the target machine for the host, at `optimization`'s backend level.
///
/// Split out of `emit_object_code` so `optimize_module` can be driven directly by the
/// tests that assert on the IR the pipeline produces.
fn host_target_machine(
    optimization: OptimizationLevelSetting,
) -> CodegenResult<(
    inkwell::targets::TargetMachine,
    inkwell::targets::TargetTriple,
)> {
    let target_triple = inkwell::targets::TargetMachine::get_default_triple();
    inkwell::targets::Target::initialize_native(&inkwell::targets::InitializationConfig::default())
        .map_err(|e| CodegenError::InitializationFailed(e.to_string()))?;

    let target = inkwell::targets::Target::from_triple(&target_triple)
        .map_err(|e| CodegenError::InitializationFailed(format!("failed to get target: {}", e)))?;

    let target_machine = target
        .create_target_machine(
            &target_triple,
            "generic",
            "",
            optimization.to_llvm(),
            // PIC relocation model is required so the emitted object can be linked into
            // a PIE executable (the default on modern Linux distributions). RelocMode::Default
            // maps to Static on some targets, which emits R_X86_64_32 relocations that ld
            // rejects with -pie.
            inkwell::targets::RelocMode::PIC,
            inkwell::targets::CodeModel::Default,
        )
        .ok_or_else(|| {
            CodegenError::InitializationFailed("failed to create target machine".to_string())
        })?;

    Ok((target_machine, target_triple))
}

/// Stamp `codegen_ctx`'s module with its target and run the IR pass pipeline over it.
fn optimize_module(
    codegen_ctx: &CodegenContext<'_>,
    target_machine: &inkwell::targets::TargetMachine,
    target_triple: &inkwell::targets::TargetTriple,
    optimization: OptimizationLevelSetting,
) -> CodegenResult<()> {
    // Stamp the module with the target it is being compiled for. Without a data layout
    // the optimizer falls back to defaults and cannot reason about the size, alignment,
    // or pointer width of the types it is transforming, which degrades SROA, GVN, and
    // the vectorizer — and would be outright wrong for any target whose layout differs
    // from the default guess.
    codegen_ctx
        .module
        .set_data_layout(&target_machine.get_target_data().get_data_layout());
    codegen_ctx.module.set_triple(target_triple);

    // Run the IR pass pipeline before instruction selection. `create_target_machine`'s
    // optimization level governs only the backend (ISel, scheduling, regalloc); the
    // middle-end passes that promote allocas to SSA values, inline, and hoist
    // loop-invariant work have to be requested separately.
    if let Some(pipeline) = optimization.pass_pipeline() {
        codegen_ctx
            .module
            .run_passes(
                pipeline,
                target_machine,
                inkwell::passes::PassBuilderOptions::create(),
            )
            .map_err(|e| {
                CodegenError::LlvmError(format!(
                    "optimization pipeline `{}` failed: {}",
                    pipeline, e
                ))
            })?;
    }

    Ok(())
}

/// Emit linkable object code for an already-verified module.
fn emit_object_code(
    codegen_ctx: &CodegenContext<'_>,
    optimization: OptimizationLevelSetting,
) -> CodegenResult<Vec<u8>> {
    let (target_machine, target_triple) = host_target_machine(optimization)?;
    optimize_module(codegen_ctx, &target_machine, &target_triple, optimization)?;

    let object_code = target_machine
        .write_to_memory_buffer(&codegen_ctx.module, inkwell::targets::FileType::Object)
        .map_err(|e| CodegenError::LlvmError(format!("failed to generate object code: {}", e)))?;

    Ok(object_code.as_slice().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use type_mapping::TypeMapper;

    /// Parse and lower `source` to typed HIR for the backend smoke tests. Mirrors the
    /// `parse → lower → compile` pipeline `neurc` runs (lowering assumes well-typedness).
    fn lower(source: &str) -> neuro_hir::HirProgram {
        let ast = syntax_parsing::parse(source).expect("parsing failed");
        hir_lowering::lower_program(&ast).expect("HIR lowering failed")
    }

    /// Compile `source` to LLVM IR text, for the tests that assert on module structure
    /// rather than on the opaque object code `compile` returns.
    fn module_ir(source: &str, optimization: OptimizationLevelSetting) -> String {
        let hir = lower(source);
        let context = LLVMContext::create();
        let codegen_ctx = build_module(&context, &hir, optimization, source, "outlining.nr")
            .expect("module generation failed");
        codegen_ctx.module.print_to_string().to_string()
    }

    /// `source` lowered, compiled, and run through the optimization pipeline for
    /// `optimization` — the IR instruction selection actually receives, rather than the
    /// unoptimized IR `module_ir` returns.
    fn optimized_ir(source: &str, optimization: OptimizationLevelSetting) -> String {
        let hir = lower(source);
        let context = LLVMContext::create();
        let codegen_ctx = build_module(&context, &hir, optimization, source, "optimized.nr")
            .expect("module generation failed");
        let (machine, triple) =
            host_target_machine(optimization).expect("host target machine unavailable");
        optimize_module(&codegen_ctx, &machine, &triple, optimization)
            .expect("optimization pipeline failed");
        codegen_ctx.module.print_to_string().to_string()
    }

    /// The body of the function named `name` in `ir`, excluding every other definition.
    fn function_body<'a>(ir: &'a str, name: &str) -> &'a str {
        let header = format!("@{}(", name);
        let start = ir
            .find(&header)
            .unwrap_or_else(|| panic!("no definition of @{} in the module", name));
        let rest = &ir[start..];
        match rest.find("\n}\n") {
            Some(end) => &rest[..end],
            None => rest,
        }
    }

    #[test]
    fn panic_diagnostics_are_outlined_out_of_the_hot_function() {
        let source = r#"
            func main() -> i32 {
                val arr: [i32; 3] = [1, 2, 3]
                assert(arr.len() == 3)
                return 0
            }
        "#;

        let ir = module_ir(source, OptimizationLevelSetting::O0);
        let main_body = function_body(&ir, "main");

        assert!(
            !main_body.contains("@abort") && !main_body.contains("@write"),
            "the diagnostic machinery must not remain inline in @main:\n{}",
            main_body
        );
        assert!(
            main_body.contains("call void @neuro.cold.panic.0()"),
            "the failure block must call the outlined thunk:\n{}",
            main_body
        );
        assert!(
            ir.contains("define private void @neuro.cold.panic.0()"),
            "the outlined thunk must be a module-private definition:\n{}",
            ir
        );
    }

    #[test]
    fn outlined_thunks_are_cold_noreturn_and_pinned() {
        let source = r#"
            func main() -> i32 {
                panic("stop")
            }
        "#;

        let ir = module_ir(source, OptimizationLevelSetting::O0);

        // `noinline` is the attribute that actually holds the outlining in place; without
        // it the inliner folds a single-call-site function back into its caller.
        for attribute in ["cold", "noreturn", "noinline", "minsize"] {
            assert!(
                ir.contains(attribute),
                "outlined thunks must carry `{}`:\n{}",
                attribute,
                ir
            );
        }
        assert!(
            ir.contains("declare void @abort() #"),
            "abort must carry an attribute group (cold, noreturn):\n{}",
            ir
        );
    }

    #[test]
    fn a_runtime_panic_message_is_passed_to_the_thunk() {
        // The message is a runtime `string`, so only the constant fragments are baked
        // into the thunk; the fat pointer travels as two arguments.
        let source = r#"
            func main() -> i32 {
                panic("stop")
            }
        "#;

        let ir = module_ir(source, OptimizationLevelSetting::O0);

        assert!(
            ir.contains("define private void @neuro.cold.panic.0(ptr %0, i64 %1)"),
            "the message thunk must take the fat pointer's (ptr, len) pair:\n{}",
            ir
        );
    }

    #[test]
    fn identically_worded_failures_share_one_thunk() {
        // Monomorphization copies a generic body once per type argument, so both copies
        // render the same diagnostic text from the same span.
        let source = r#"
            func checked<T>(value: T) -> T {
                assert(true)
                value
            }

            func main() -> i32 {
                val a = checked(1)
                val b = checked(2.5)
                return a
            }
        "#;

        let ir = module_ir(source, OptimizationLevelSetting::O0);
        let thunks = ir.matches("define private void @neuro.cold.panic.").count();

        assert_eq!(thunks, 1, "the two instances must share one thunk:\n{}", ir);
    }

    #[test]
    fn guard_and_overflow_branches_are_weighted() {
        // At -O0 arithmetic panics on overflow, so this program carries two runtime
        // guards: a bounds check and an overflow check. Both report through the panic
        // machinery, so both have the one guard shape and weight the same edge cold.
        let source = r#"
            func main() -> i32 {
                val arr: [i32; 3] = [1, 2, 3]
                mut i: i32 = 0
                val total = arr[i] + 1
                return total
            }
        "#;

        let ir = module_ir(source, OptimizationLevelSetting::O0);

        assert!(
            ir.contains(r#"!{!"branch_weights", i32 2000, i32 1}"#),
            "a guard's failure edge must be the unlikely one:\n{}",
            ir
        );
        assert!(
            ir.contains("panic: integer overflow at"),
            "an overflow must report a located diagnostic, not a bare trap:\n{}",
            ir
        );
        assert!(
            !ir.contains("@llvm.trap"),
            "nothing may still abort through a silent trap:\n{}",
            ir
        );
    }

    /// Calls to `free` in the body of the function named `name`.
    fn free_calls(ir: &str, name: &str) -> usize {
        function_body(ir, name).matches("call void @free(").count()
    }

    #[test]
    fn an_interpolated_temporary_is_freed() {
        // `println` copies the bytes out to fd 1 and keeps none of them, so both buffers
        // the argument cost — the rendered hole and the joined result — are dead on
        // return. A loop around this is what made the leak unbounded.
        let source = r#"
            func main() -> i32 {
                mut i: i32 = 0
                while i < 3 {
                    println("line {i}")
                    i += 1
                }
                return 0
            }
        "#;

        let ir = module_ir(source, OptimizationLevelSetting::O0);

        assert!(
            free_calls(&ir, "main") >= 2,
            "the rendered hole and the joined string must both be released:\n{}",
            function_body(&ir, "main")
        );
    }

    #[test]
    fn a_borrowed_argument_is_never_freed() {
        // A literal lives in `.rodata`. Handing that pointer to `free` would abort, so
        // the ownership test has to answer `false` for everything it cannot prove.
        let source = r#"
            func main() -> i32 {
                println("line")
                return 0
            }
        "#;

        let ir = module_ir(source, OptimizationLevelSetting::O0);

        assert_eq!(
            free_calls(&ir, "main"),
            0,
            "a `.rodata` literal must not be freed:\n{}",
            function_body(&ir, "main")
        );
    }

    #[test]
    fn a_heap_initialized_string_binding_is_freed_at_scope_exit() {
        // The binding outlives the statement that built it, so its buffer is released by
        // the scope-exit machinery rather than at the point of use.
        let source = r#"
            func main() -> i32 {
                val greeting = "a" + "b"
                println(greeting)
                return 0
            }
        "#;

        let ir = module_ir(source, OptimizationLevelSetting::O0);
        let body = function_body(&ir, "main");

        assert!(
            free_calls(&ir, "main") >= 1,
            "a binding initialized by concatenation owns its buffer:\n{}",
            body
        );
        assert!(
            body.contains("drop.flag"),
            "the release must be flag-guarded, so a moved value is not freed twice:\n{}",
            body
        );
    }

    #[test]
    fn a_borrowed_string_binding_is_not_freed() {
        // Same shape, but the initializer is a literal: nothing was allocated, so nothing
        // may be released.
        let source = r#"
            func main() -> i32 {
                val greeting = "ab"
                println(greeting)
                return 0
            }
        "#;

        let ir = module_ir(source, OptimizationLevelSetting::O0);

        assert_eq!(
            free_calls(&ir, "main"),
            0,
            "a binding holding a literal owns nothing:\n{}",
            function_body(&ir, "main")
        );
    }

    #[test]
    fn a_pass_through_transform_frees_only_what_it_replaced() {
        // `__neuro_pad` returns its input untouched when the text already fills the
        // field, so the result can be the same buffer that was handed in. The release
        // is therefore guarded by a pointer comparison rather than emitted outright.
        let source = r#"
            func main() -> i32 {
                val n = 7
                println("{n:>8}")
                return 0
            }
        "#;

        let ir = module_ir(source, OptimizationLevelSetting::O0);
        let body = function_body(&ir, "main");

        assert!(
            body.contains("interp.same"),
            "a padded hole must compare the two pointers before freeing either:\n{}",
            body
        );
    }

    #[test]
    fn optimization_levels_run_an_ir_pipeline() {
        // Codegen gives every local an `alloca`. Only an IR pass — not the
        // `TargetMachine`'s optimization level, which runs none — promotes those to SSA
        // values, so a surviving `alloca` in this loop means no pipeline ran.
        let source = r#"
            func total(n: i32) -> i32 {
                mut sum: i32 = 0
                mut i: i32 = 0
                while i < n {
                    sum += i
                    i += 1
                }
                return sum
            }

            func main() -> i32 {
                return total(10)
            }
        "#;

        for level in [
            OptimizationLevelSetting::O1,
            OptimizationLevelSetting::O2,
            OptimizationLevelSetting::O3,
        ] {
            let ir = optimized_ir(source, level);
            let body = function_body(&ir, "total");
            assert!(
                !body.contains("alloca"),
                "{:?} left a stack slot unpromoted, so no IR pipeline ran:\n{}",
                level,
                body
            );
        }

        // -O0 deliberately runs no pipeline: its trapping arithmetic and bounds guards
        // must stay exactly where codegen emitted them.
        let unoptimized = optimized_ir(source, OptimizationLevelSetting::O0);
        assert!(
            function_body(&unoptimized, "total").contains("alloca"),
            "-O0 must hand the IR to instruction selection untouched:\n{}",
            unoptimized
        );
    }

    #[test]
    fn the_optimized_module_carries_its_target() {
        // Without a data layout the optimizer cannot reason about the size, alignment,
        // or pointer width of what it transforms.
        let ir = optimized_ir(
            "func main() -> i32 { return 0 }",
            OptimizationLevelSetting::O2,
        );

        assert!(
            ir.contains("target datalayout") && ir.contains("target triple"),
            "the module must name the target it was compiled for:\n{}",
            ir
        );
    }

    #[test]
    fn test_type_mapper_primitives() {
        let context = LLVMContext::create();
        let mapper = TypeMapper::new(&context);

        assert!(mapper.map_type(&Type::I32).is_ok());
        assert!(mapper.map_type(&Type::I64).is_ok());
        assert!(mapper.map_type(&Type::F32).is_ok());
        assert!(mapper.map_type(&Type::F64).is_ok());
        assert!(mapper.map_type(&Type::Bool).is_ok());
        assert!(mapper.map_type(&Type::Void).is_err());
    }

    #[test]
    fn test_type_predicates() {
        assert!(TypeMapper::is_float_type(&Type::F32));
        assert!(TypeMapper::is_float_type(&Type::F64));
        assert!(!TypeMapper::is_float_type(&Type::I32));

        // Test unsigned integer predicate
        assert!(TypeMapper::is_unsigned_int(&Type::U32));
        assert!(!TypeMapper::is_unsigned_int(&Type::I32));
    }

    #[test]
    fn test_compile_simple_function() {
        let source = r#"
            func add(a: i32, b: i32) -> i32 {
                return a + b
            }
        "#;

        let hir = lower(source);
        let result = compile(&hir, OptimizationLevelSetting::O0, source, "test.nr");

        assert!(result.is_ok(), "compilation failed: {:?}", result.err());
        let object_code = result.unwrap();
        assert!(!object_code.is_empty(), "object code should not be empty");
    }

    #[test]
    fn test_compile_milestone_program() {
        let source = r#"
            func add(a: i32, b: i32) -> i32 {
                return a + b
            }

            func main() -> i32 {
                val result = add(5, 3)
                return result
            }
        "#;

        let hir = lower(source);
        let result = compile(&hir, OptimizationLevelSetting::O2, source, "test.nr");

        assert!(result.is_ok(), "compilation failed: {:?}", result.err());
        let object_code = result.unwrap();
        assert!(!object_code.is_empty(), "object code should not be empty");
    }

    #[test]
    fn test_overflow_checks_emit_valid_ir_at_o0() {
        // -O0 routes integer +/-/* through the with-overflow intrinsics and a
        // trap block; module verification must accept the resulting IR.
        let source = r#"
            func main() -> i32 {
                mut x: i32 = 2147483647
                val y: i32 = 1
                val z: i32 = x + y
                return z
            }
        "#;

        let hir = lower(source);
        let result = compile(&hir, OptimizationLevelSetting::O0, source, "test.nr");

        assert!(result.is_ok(), "compilation failed: {:?}", result.err());
        assert!(
            !result.unwrap().is_empty(),
            "object code should not be empty"
        );
    }

    #[test]
    fn test_overflow_wraps_emit_valid_ir_at_o2() {
        // -O2 emits plain wrapping arithmetic (no intrinsic, no trap block).
        let source = r#"
            func main() -> i32 {
                mut x: u8 = 200u8
                val y: u8 = 100u8
                val z: u8 = x + y
                return z as i32
            }
        "#;

        let hir = lower(source);
        let result = compile(&hir, OptimizationLevelSetting::O2, source, "test.nr");

        assert!(result.is_ok(), "compilation failed: {:?}", result.err());
        assert!(
            !result.unwrap().is_empty(),
            "object code should not be empty"
        );
    }

    #[test]
    fn standard_output_is_buffered_rather_than_written_per_call() {
        let source = r#"
            func main() -> i32 {
                println("one")
                return 0
            }
        "#;

        let ir = module_ir(source, OptimizationLevelSetting::O0);
        let main_body = function_body(&ir, "main");

        assert!(
            !main_body.contains("@write("),
            "a print must reach the buffer, not the syscall:\n{}",
            main_body
        );
        assert_eq!(
            main_body.matches("call void @neuro.print.emit(").count(),
            2,
            "println emits its text and its newline into the same buffer:\n{}",
            main_body
        );
        assert!(
            ir.contains("@neuro.print.buffer = private global"),
            "the buffer must be a module-private reservation:\n{}",
            ir
        );
    }

    #[test]
    fn a_module_that_never_prints_reserves_no_output_buffer() {
        // The drain is inserted after every body is generated precisely so that a
        // program with no print keeps its exit paths — and its .bss — untouched.
        let source = r#"
            func main() -> i32 {
                val n: i32 = 41
                return n + 1
            }
        "#;

        let ir = module_ir(source, OptimizationLevelSetting::O0);

        assert!(
            !ir.contains("neuro.print"),
            "the standard-output runtime must not be emitted at all:\n{}",
            ir
        );
    }

    #[test]
    fn every_exit_path_drains_the_output_buffer() {
        // `main` returning and `abort` are the only two ways this language stops
        // running, and `abort` runs no exit hook. Both a panicking `assert` and an
        // overflowing `+` reach the second.
        let source = r#"
            func main() -> i32 {
                mut n: i32 = 2147483647
                println("working")
                assert(n > 0)
                n = n + 1
                return 0
            }
        "#;

        let ir = module_ir(source, OptimizationLevelSetting::O0);

        assert!(
            function_body(&ir, "main").contains("call void @neuro.print.flush()"),
            "main must drain before it returns:\n{}",
            function_body(&ir, "main")
        );
        let drained = ir
            .split("call void @abort()")
            .next()
            .map(|head| head.trim_end().ends_with("call void @neuro.print.flush()"))
            .unwrap_or(false);
        assert!(
            drained,
            "the panic runtime must drain the buffer first:\n{}",
            ir
        );
    }

    #[test]
    fn test_optimization_level_parsing() {
        assert_eq!(
            OptimizationLevelSetting::from_u8(0).unwrap(),
            OptimizationLevelSetting::O0
        );
        assert_eq!(
            OptimizationLevelSetting::from_u8(1).unwrap(),
            OptimizationLevelSetting::O1
        );
        assert_eq!(
            OptimizationLevelSetting::from_u8(2).unwrap(),
            OptimizationLevelSetting::O2
        );
        assert_eq!(
            OptimizationLevelSetting::from_u8(3).unwrap(),
            OptimizationLevelSetting::O3
        );
        assert!(OptimizationLevelSetting::from_u8(4).is_err());
    }

    /// A statically shaped tensor has its whole shape in its type, so the buffer is a
    /// flat row-major array and a literal reaches the binding as one constant.
    #[test]
    fn a_tensor_literal_lowers_to_a_flat_row_major_buffer() {
        let source = r#"
            func main() -> i32 {
                val m: Tensor<f32, [2, 3]> = [
                    [1.0, 2.0, 3.0],
                    [4.0, 5.0, 6.0]
                ]
                return 0
            }
        "#;

        let ir = optimized_ir(source, OptimizationLevelSetting::O0);
        let body = function_body(&ir, "main");
        assert!(
            body.contains("[6 x float]"),
            "a [2, 3] tensor is a 6-element buffer:\n{body}"
        );
        assert!(
            body.contains("float 3.000000e+00") && body.contains("float 6.000000e+00"),
            "the literal's values must reach the buffer:\n{body}"
        );
    }

    /// `zeros()` is a constant fill, so nothing per-element survives to run time.
    #[test]
    fn a_zeros_tensor_lowers_to_a_zero_initializer() {
        let source = r#"
            func main() -> i32 {
                val z = Tensor::<f32, [4, 4]>::zeros()
                return 0
            }
        "#;

        let ir = module_ir(source, OptimizationLevelSetting::O0);
        let body = function_body(&ir, "main");
        assert!(
            body.contains("[16 x float] zeroinitializer"),
            "zeros() is a zero-initialized 16-element buffer:\n{body}"
        );
    }

    /// The diagonal is what distinguishes `identity()` from `ones()`, and it is folded
    /// at compile time rather than written by a loop.
    #[test]
    fn an_identity_tensor_carries_ones_only_on_its_diagonal() {
        let source = r#"
            func main() -> i32 {
                val e = Tensor::<i32, [3, 3]>::identity()
                return 0
            }
        "#;

        let ir = module_ir(source, OptimizationLevelSetting::O0);
        let body = function_body(&ir, "main");
        assert!(
            body.contains(
                "[9 x i32] [i32 1, i32 0, i32 0, i32 0, i32 1, i32 0, i32 0, i32 0, i32 1]"
            ),
            "identity() puts ones on the diagonal of a row-major buffer:\n{body}"
        );
    }

    /// `random_normal` is the one construction with a runtime cost: a counted loop over
    /// the buffer, drawing through the module's own generator.
    #[test]
    fn random_normal_fills_the_buffer_through_the_module_generator() {
        let source = r#"
            func main() -> i32 {
                val r = Tensor::<f32, [8, 4]>::random_normal(0.0f32, 0.02f32)
                return 0
            }
        "#;

        let ir = module_ir(source, OptimizationLevelSetting::O0);
        assert!(
            ir.contains("@__neuro_rng_state = private global i64"),
            "the generator keeps its state in a private module global:\n{ir}"
        );
        assert!(
            ir.contains("define internal double @__neuro_rng_normal_f64()"),
            "the normal draw is emitted once per module:\n{ir}"
        );
        let body = function_body(&ir, "main");
        assert!(
            body.contains("call double @__neuro_rng_normal_f64()")
                && body.contains("tensor.rand.head"),
            "the fill is a counted loop over the buffer:\n{body}"
        );
    }
}
