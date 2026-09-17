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
    // construction (the `Option<T>` a collection reader returns) can look a tag up
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
    // scope-exit destructor calls, and the structs implementing `PoolAware` so a
    // `pool` body registers them with the arena instead. Semantic analysis has already
    // validated both shapes.
    let mut drop_types: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut pool_aware_types: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in items {
        if let HirItem::Impl(impl_def) = item {
            match impl_def.trait_name.as_deref() {
                Some("Drop") => {
                    drop_types.insert(impl_def.type_name.clone());
                }
                Some("PoolAware") => {
                    pool_aware_types.insert(impl_def.type_name.clone());
                }
                _ => {}
            }
        }
    }

    let mut codegen_ctx = CodegenContext::new(context, "neuro_module");
    codegen_ctx.set_struct_defs(struct_defs);
    codegen_ctx.set_struct_written_names(struct_written_names);
    codegen_ctx.set_enum_words(enum_words);
    codegen_ctx.set_enum_variants(enum_variants);
    codegen_ctx.set_drop_types(drop_types);
    codegen_ctx.set_pool_aware_types(pool_aware_types);
    codegen_ctx.set_trait_methods(trait_methods);

    // Supply source so panic-family builtins can render `file:line:col` in their
    // runtime diagnostics.
    codegen_ctx.set_source(source_location::SourceFile::new(
        source_path.to_string(),
        source.to_string(),
    ));

    // Debug builds (-O0) trap on integer overflow; release builds wrap.
    codegen_ctx.set_overflow_checks(optimization == OptimizationLevelSetting::O0);

    // Emit module-level constants as LLVM global constants before any function.
    // This ensures all globals are defined before function bodies reference them.
    for item in items {
        if let HirItem::Const(def) = item {
            codegen_ctx.codegen_global_const(def)?;
        }
    }

    // Pre-declare every function/method signature before generating any body, so a
    // call resolves regardless of definition order. Monomorphized generic instances
    // may be called by (or call) items appearing before them, so lazy
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
    // it edits (`main`'s returns, `abort`, `llvm.trap`) are all emitted by now.
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
    /// register allocation: it runs no IR passes at all. Without this pipeline every
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
    // the vectorizer, and would be outright wrong for any target whose layout differs
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
mod tests;
