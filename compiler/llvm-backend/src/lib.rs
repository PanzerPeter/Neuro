// NEURO Programming Language - LLVM Backend
// Feature slice for LLVM IR generation and optimization
//
// This slice follows Vertical Slice Architecture (VSA) principles:
// - Self-contained code generation functionality
// - Minimal dependencies (only infrastructure, syntax-parsing, and semantic-analysis)
// - Clear module boundaries with pub(crate) for internals
// - Public API limited to compile() entry point

mod codegen;
mod errors;
mod type_mapping;

// Public exports
pub use errors::{CodegenError, CodegenResult};

use inkwell::context::Context as LLVMContext;
use inkwell::OptimizationLevel;
use semantic_analysis::Type;
use std::collections::HashMap;
use syntax_parsing::Item;

use codegen::CodegenContext;

/// Compile NEURO AST to LLVM object code.
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
/// ```ignore
/// use syntax_parsing::parse;
/// use semantic_analysis::type_check;
/// use llvm_backend::compile;
///
/// let source = r#"
///     func add(a: i32, b: i32) -> i32 {
///         return a + b
///     }
/// "#;
///
/// let ast = parse(source)?;
/// type_check(&ast)?;
/// let object_code = compile(&ast)?;
/// // Write object_code to file or link to executable
/// ```
pub fn compile(items: &[Item]) -> CodegenResult<Vec<u8>> {
    // First, run type checking to get function signatures
    semantic_analysis::type_check(items).map_err(|errors| {
        CodegenError::InternalError(format!("type checking failed with {} errors", errors.len()))
    })?;

    // Extract function types (we need this for codegen)
    let mut func_types = HashMap::new();
    for item in items {
        let Item::Function(func_def) = item;
        // Re-create function type from definition
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

        let func_type = Type::Function {
            params: param_types,
            ret: Box::new(return_type),
        };

        func_types.insert(func_def.name.name.clone(), func_type);
    }

    // Initialize LLVM context
    let context = LLVMContext::create();
    let mut codegen_ctx = CodegenContext::new(&context, "neuro_module");

    // Store type information for expressions
    codegen_ctx.store_expr_types(items, &func_types)?;

    // Generate code for each function
    for item in items {
        let Item::Function(func_def) = item;
        codegen_ctx.codegen_function(func_def, &func_types)?;
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
            OptimizationLevel::None,
            inkwell::targets::RelocMode::Default,
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

/// Helper function to resolve syntax-parsing types to semantic types
fn resolve_syntax_type(ty: &syntax_parsing::Type) -> CodegenResult<Type> {
    match ty {
        syntax_parsing::Type::Named(ident) => match ident.name.as_str() {
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
            "void" => Ok(Type::Void),
            name => Err(CodegenError::UnsupportedType(format!(
                "unknown type: {}",
                name
            ))),
        },
        syntax_parsing::Type::Tensor { .. } => Err(CodegenError::UnsupportedType(
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

        // Test integer type predicates
        assert!(Type::I8.is_integer());
        assert!(Type::I16.is_integer());
        assert!(Type::I32.is_integer());
        assert!(Type::I64.is_integer());
        assert!(Type::U8.is_integer());
        assert!(Type::U16.is_integer());
        assert!(Type::U32.is_integer());
        assert!(Type::U64.is_integer());
        assert!(!Type::F32.is_integer());
        assert!(!Type::Bool.is_integer());

        // Test signed vs unsigned predicates
        assert!(Type::I32.is_signed_int());
        assert!(!Type::U32.is_signed_int());
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
        let result = compile(&items);

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
        let result = compile(&items);

        assert!(result.is_ok(), "compilation failed: {:?}", result.err());
        let object_code = result.unwrap();
        assert!(!object_code.is_empty(), "object code should not be empty");
    }
}
