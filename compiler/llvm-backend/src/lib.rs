// Neuro Programming Language - LLVM Backend
// Feature slice for LLVM IR generation and optimization
//
// This slice follows Vertical Slice Architecture (VSA) principles:
// - Self-contained code generation functionality
// - Minimal dependencies (only infrastructure and LLVM)
// - Clear module boundaries with pub(crate) for internals
// - Public API limited to compile() entry point

mod codegen;
mod errors;
mod type_mapping;
mod types;

// Public exports
pub use errors::{CodegenError, CodegenResult};

use ast_types::{Item, SelfParam};
use inkwell::context::Context as LLVMContext;
use inkwell::OptimizationLevel as LlvmOptimizationLevel;
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

/// Compile Neuro AST to LLVM object code.
///
/// This is the main entry point for the LLVM backend. It takes a type-checked
/// AST and generates LLVM IR, then compiles it to object code.
///
/// # Phase 1 Support
///
/// Currently supports:
/// - Function definitions with parameters and return types
/// - Primitive types: `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`, `f32`, `f64`, `bool`
/// - Binary operators (arithmetic, comparison, logical)
/// - Unary operators (negation, logical not)
/// - Variable declarations
/// - Function calls
/// - If/else statements
/// - While loops and range-for loops with `break` and `continue`
/// - Return statements
///
/// # Arguments
///
/// * `items` - Type-checked AST items (functions)
///
/// # Returns
///
/// * `Ok(Vec<u8>)` - LLVM object code (can be linked to executable)
/// * `Err(CodegenError)` - Code generation failed
///
/// # Examples
///
/// ```
/// use syntax_parsing::parse;
/// use llvm_backend::{compile, OptimizationLevelSetting};
///
/// fn main() {
///     let source = r#"
///         func add(a: i32, b: i32) -> i32 {
///             return a + b
///         }
///     "#;
///
///     let ast = parse(source).unwrap();
///     let object_code =
///         compile(&ast, OptimizationLevelSetting::O2, source, "example.nr").unwrap();
///     // Write object_code to file or link to executable
/// }
/// ```
pub fn compile(
    items: &[Item],
    optimization: OptimizationLevelSetting,
    source: &str,
    source_path: &str,
) -> CodegenResult<Vec<u8>> {
    // Collect struct definitions first so resolve_syntax_type can recognise struct names
    // when processing function parameter and return types below.
    let mut struct_defs: HashMap<String, Vec<(String, Type)>> = HashMap::new();
    for item in items {
        if let Item::Struct(def) = item {
            let mut fields = Vec::new();
            for field in &def.fields {
                fields.push((field.name.name.clone(), resolve_syntax_type(&field.ty)?));
            }
            struct_defs.insert(def.name.name.clone(), fields);
        }
    }

    // Extract function types from AST (caller is responsible for semantic validation)
    let mut func_types = HashMap::new();
    for item in items {
        match item {
            Item::Function(func_def) => {
                let mut param_types = Vec::new();
                for param in &func_def.params {
                    let ty = resolve_syntax_type(&param.ty)?;
                    param_types.push(ty);
                }

                let return_type = if let Some(ret_ty) = &func_def.return_type {
                    resolve_syntax_type(ret_ty)?
                } else {
                    Type::Void
                };

                func_types.insert(
                    func_def.name.name.clone(),
                    Type::Function {
                        params: param_types,
                        ret: Box::new(return_type),
                    },
                );
            }

            Item::Impl(impl_def) => {
                let struct_name = &impl_def.type_name.name;
                for method in &impl_def.methods {
                    // Only &self and associated functions are supported; others were
                    // rejected by semantic analysis and must not reach codegen.
                    if matches!(
                        method.self_param,
                        Some(SelfParam::RefMut) | Some(SelfParam::Owned)
                    ) {
                        continue;
                    }

                    let mangled = format!("{}__{}", struct_name, method.name.name);
                    let mut param_types: Vec<Type> = Vec::new();

                    // Implicit `self` parameter for instance methods.
                    if method.self_param.is_some() {
                        param_types.push(Type::Struct(struct_name.clone()));
                    }

                    for param in &method.params {
                        param_types.push(resolve_syntax_type(&param.ty)?);
                    }

                    let return_type = if let Some(ret_ty) = &method.return_type {
                        resolve_syntax_type(ret_ty)?
                    } else {
                        Type::Void
                    };

                    func_types.insert(
                        mangled,
                        Type::Function {
                            params: param_types,
                            ret: Box::new(return_type),
                        },
                    );
                }
            }

            Item::Struct(_) | Item::Const(_) => {}
        }
    }

    // Initialize LLVM context
    let context = LLVMContext::create();
    let mut codegen_ctx = CodegenContext::new(&context, "neuro_module");
    codegen_ctx.set_struct_defs(struct_defs);

    // Supply source so panic-family builtins can render `file:line:col` in their
    // runtime diagnostics (§1.2).
    codegen_ctx.set_source(source_location::SourceFile::new(
        source_path.to_string(),
        source.to_string(),
    ));

    // Debug builds (-O0) trap on integer overflow; release builds wrap (§1.2).
    codegen_ctx.set_overflow_checks(optimization == OptimizationLevelSetting::O0);

    // Store type information for expressions (including const types for identifier resolution)
    codegen_ctx.store_expr_types(items, &func_types)?;

    // Emit module-level constants as LLVM global constants before any function.
    // This ensures all globals are defined before function bodies reference them.
    for item in items {
        if let Item::Const(def) = item {
            codegen_ctx.codegen_global_const(def)?;
        }
    }

    // Generate code for each function and impl method
    for item in items {
        match item {
            Item::Function(func_def) => {
                codegen_ctx.codegen_function(func_def, &func_types)?;
            }
            Item::Impl(impl_def) => {
                codegen_ctx.codegen_impl(impl_def, &func_types)?;
            }
            Item::Const(_) | Item::Struct(_) => {}
        }
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

/// Helper function to resolve AST types to backend codegen types
fn resolve_syntax_type(ty: &ast_types::Type) -> CodegenResult<Type> {
    match ty {
        ast_types::Type::Named(ident) => match ident.name.as_str() {
            // Signed integers
            "i8" => Ok(Type::I8),
            "i16" => Ok(Type::I16),
            "i32" => Ok(Type::I32),
            "i64" => Ok(Type::I64),
            // Unsigned integers
            "u8" => Ok(Type::U8),
            "u16" => Ok(Type::U16),
            "u32" => Ok(Type::U32),
            "u64" => Ok(Type::U64),
            // Floating point
            "f32" => Ok(Type::F32),
            "f64" => Ok(Type::F64),
            // Other types
            "bool" => Ok(Type::Bool),
            "char" => Ok(Type::Char),
            "string" => Ok(Type::String),
            "void" => Ok(Type::Void),
            // Struct types are recognised here as named types; their field layout is
            // resolved later by CodegenContext when it has access to struct_defs.
            name => Ok(Type::Struct(name.to_string())),
        },
        // Immutable borrow `&T` (§2.4): an opaque pointer to the referent's storage.
        ast_types::Type::Reference { inner, .. } => {
            Ok(Type::Reference(Box::new(resolve_syntax_type(inner)?)))
        }
        ast_types::Type::Tensor { .. } => Err(CodegenError::UnsupportedType(
            "tensor types not supported in Phase 1".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use type_mapping::TypeMapper;

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

        let items = syntax_parsing::parse(source).expect("parsing failed");
        let result = compile(&items, OptimizationLevelSetting::O0, source, "test.nr");

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

        let items = syntax_parsing::parse(source).expect("parsing failed");
        let result = compile(&items, OptimizationLevelSetting::O2, source, "test.nr");

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

        let items = syntax_parsing::parse(source).expect("parsing failed");
        let result = compile(&items, OptimizationLevelSetting::O0, source, "test.nr");

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

        let items = syntax_parsing::parse(source).expect("parsing failed");
        let result = compile(&items, OptimizationLevelSetting::O2, source, "test.nr");

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
