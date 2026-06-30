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
use neuro_hir::{HirItem, HirProgram, HirSelfParam};
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
///   `file:line:col` in panic-family runtime diagnostics (§1.2)
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
    let items = &program.items;

    // Collect struct definitions first so struct field/parameter types resolve below.
    let mut struct_defs: HashMap<String, Vec<(String, Type)>> = HashMap::new();
    for item in items {
        if let HirItem::Struct(def) = item {
            let mut fields = Vec::new();
            for field in &def.fields {
                fields.push((field.name.clone(), Type::from_hir(&field.ty)));
            }
            struct_defs.insert(def.name.clone(), fields);
        }
    }

    // Collect each enum's payload word count `W` (§3.5): the widest variant's field
    // count, so every value of the enum maps to one `{ i32, [W x i64] }` aggregate.
    let mut enum_words: HashMap<String, u32> = HashMap::new();
    for item in items {
        if let HirItem::Enum(def) = item {
            let words = def
                .variants
                .iter()
                .map(|v| v.fields.len())
                .max()
                .unwrap_or(0) as u32;
            enum_words.insert(def.name.clone(), words);
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
                    // Consuming `self` was rejected by semantic analysis and must not
                    // reach codegen; `&self`, `&mut self`, and associated functions
                    // all need a registered signature.
                    if matches!(method.self_param, Some(HirSelfParam::Owned)) {
                        continue;
                    }

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

            HirItem::Struct(_) | HirItem::Const(_) | HirItem::Enum(_) => {}
        }
    }

    // Collect the structs implementing `Drop` (§2.1) so codegen can insert their
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

    // Initialize LLVM context
    let context = LLVMContext::create();
    let mut codegen_ctx = CodegenContext::new(&context, "neuro_module");
    codegen_ctx.set_struct_defs(struct_defs);
    codegen_ctx.set_enum_words(enum_words);
    codegen_ctx.set_drop_types(drop_types);

    // Supply source so panic-family builtins can render `file:line:col` in their
    // runtime diagnostics (§1.2).
    codegen_ctx.set_source(source_location::SourceFile::new(
        source_path.to_string(),
        source.to_string(),
    ));

    // Debug builds (-O0) trap on integer overflow; release builds wrap (§1.2).
    codegen_ctx.set_overflow_checks(optimization == OptimizationLevelSetting::O0);

    // Emit module-level constants as LLVM global constants before any function.
    // This ensures all globals are defined before function bodies reference them.
    for item in items {
        if let HirItem::Const(def) = item {
            codegen_ctx.codegen_global_const(def)?;
        }
    }

    // Generate code for each function and impl method
    for item in items {
        match item {
            HirItem::Function(func_def) => {
                codegen_ctx.codegen_function(func_def, &func_types)?;
            }
            HirItem::Impl(impl_def) => {
                codegen_ctx.codegen_impl(impl_def, &func_types)?;
            }
            HirItem::Const(_) | HirItem::Struct(_) | HirItem::Enum(_) => {}
        }
    }

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

    // Generate object code
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
}
