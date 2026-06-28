use crate::errors::MlirError;

use melior::{
    dialect::{func, llvm, DialectRegistry},
    ir::{
        attribute::{StringAttribute, TypeAttribute},
        operation::OperationLike,
        r#type::{FunctionType, IntegerType},
        BlockLike, Identifier, Location, Module, Operation, Region, Type,
    },
    utility::register_all_dialects,
    Context,
};
use neuro_hir::{HirItem, HirProgram, HirSelfParam, HirType};

/// Bit widths for the fixed-size integer scalars, keyed off the HIR type.
const I8_BITS: u32 = 8;
const I16_BITS: u32 = 16;
const I32_BITS: u32 = 32;
const I64_BITS: u32 = 64;
/// `bool` lowers to MLIR's signless `i1`; `char` to a 32-bit Unicode scalar.
const BOOL_BITS: u32 = 1;
const CHAR_BITS: u32 = 32;

/// Lower a typed HIR program to a trivial MLIR module and return its textual form.
///
/// This is the Phase 1.8 scaffold of the MLIR path: it walks the typed HIR and
/// emits one `func.func` *declaration* (external, empty body) per free function
/// and per `impl` method, mapping each HIR type to its MLIR counterpart. Function
/// bodies are intentionally not lowered yet — that is the Phase 3+ tensor / linalg
/// work; this stage proves the HIR → `melior` → verified MLIR pipeline end-to-end.
///
/// # Errors
///
/// Returns [`MlirError::UnsupportedType`] if a HIR type with no MLIR scaffold
/// mapping appears in value position, or [`MlirError::ModuleVerificationFailed`]
/// if the constructed module fails MLIR's own verifier.
pub fn lower_program(program: &HirProgram) -> Result<String, MlirError> {
    let registry = DialectRegistry::new();
    register_all_dialects(&registry);

    let context = Context::new();
    context.append_dialect_registry(&registry);
    context.load_all_available_dialects();

    let location = Location::unknown(&context);
    let module = Module::new(location);

    for item in &program.items {
        match item {
            HirItem::Function(function) => {
                let params: Vec<HirType> = function.params.iter().map(|p| p.ty.clone()).collect();
                let op = declare_function(
                    &context,
                    location,
                    &function.name,
                    &params,
                    &function.return_type,
                )?;
                module.body().append_operation(op);
            }
            HirItem::Impl(impl_block) => {
                for method in &impl_block.methods {
                    let mut params: Vec<HirType> = Vec::new();
                    // The receiver lowers to an opaque pointer to the struct; the
                    // scaffold does not yet distinguish &self / &mut self / self.
                    if method.self_param.is_some() {
                        params.push(receiver_type(&impl_block.type_name, &method.self_param));
                    }
                    params.extend(method.params.iter().map(|p| p.ty.clone()));
                    let name = format!("{}_{}", impl_block.type_name, method.name);
                    let op =
                        declare_function(&context, location, &name, &params, &method.return_type)?;
                    module.body().append_operation(op);
                }
            }
            // Structs and constants carry no callable surface; the scaffold module
            // is a set of function declarations only.
            HirItem::Struct(_) | HirItem::Const(_) => {}
        }
    }

    if !module.as_operation().verify() {
        return Err(MlirError::ModuleVerificationFailed);
    }

    Ok(module.as_operation().to_string())
}

/// The HIR type of a method receiver: a borrow of the owning struct for `&self` /
/// `&mut self`, or the owned struct for a consuming `self`. All three lower to an
/// opaque pointer in the scaffold, but keeping the distinction here documents intent.
fn receiver_type(type_name: &str, self_param: &Option<HirSelfParam>) -> HirType {
    match self_param {
        Some(HirSelfParam::Ref) => HirType::Reference {
            inner: Box::new(HirType::Struct(type_name.to_string())),
            mutable: false,
        },
        Some(HirSelfParam::RefMut) => HirType::Reference {
            inner: Box::new(HirType::Struct(type_name.to_string())),
            mutable: true,
        },
        _ => HirType::Struct(type_name.to_string()),
    }
}

/// Build an external `func.func` declaration (empty region) for the given signature.
fn declare_function<'c>(
    context: &'c Context,
    location: Location<'c>,
    name: &str,
    param_types: &[HirType],
    return_type: &HirType,
) -> Result<Operation<'c>, MlirError> {
    let inputs = param_types
        .iter()
        .map(|ty| map_type(context, ty))
        .collect::<Result<Vec<_>, _>>()?;

    let results = match return_type {
        HirType::Void => Vec::new(),
        other => vec![map_type(context, other)?],
    };

    let fn_type = FunctionType::new(context, &inputs, &results);

    // An empty region makes this an external declaration; private visibility keeps
    // it unexported, matching its declaration-only role in the scaffold module.
    let visibility = (
        Identifier::new(context, "sym_visibility"),
        StringAttribute::new(context, "private").into(),
    );

    Ok(func::func(
        context,
        StringAttribute::new(context, name),
        TypeAttribute::new(fn_type.into()),
        Region::new(),
        &[visibility],
        location,
    ))
}

/// Map a resolved HIR type to its MLIR scaffold type.
///
/// Scalars map to their natural MLIR types; every aggregate or reference type maps
/// to an opaque LLVM pointer until real tensor / struct lowering lands (Phase 3+).
fn map_type<'c>(context: &'c Context, ty: &HirType) -> Result<Type<'c>, MlirError> {
    let mapped = match ty {
        HirType::I8 | HirType::U8 => IntegerType::new(context, I8_BITS).into(),
        HirType::I16 | HirType::U16 => IntegerType::new(context, I16_BITS).into(),
        HirType::I32 | HirType::U32 => IntegerType::new(context, I32_BITS).into(),
        HirType::I64 | HirType::U64 => IntegerType::new(context, I64_BITS).into(),
        HirType::Bool => IntegerType::new(context, BOOL_BITS).into(),
        HirType::Char => IntegerType::new(context, CHAR_BITS).into(),
        HirType::F16 => Type::float16(context),
        HirType::BF16 => Type::bfloat16(context),
        HirType::F32 => Type::float32(context),
        HirType::F64 => Type::float64(context),
        HirType::String
        | HirType::Struct(_)
        | HirType::Reference { .. }
        | HirType::Array { .. }
        | HirType::Tuple(_)
        // Address space 0; all LLVM pointers are opaque (`!llvm.ptr`) since LLVM 19.
        | HirType::Function { .. } => llvm::r#type::pointer(context, 0),
        HirType::Void => {
            return Err(MlirError::UnsupportedType(
                "void cannot appear in value position".to_string(),
            ))
        }
    };
    Ok(mapped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use neuro_hir::{HirFunction, HirImpl, HirMethod, HirParam};
    use shared_types::Span;

    fn span() -> Span {
        Span::new(0, 1)
    }

    fn param(name: &str, ty: HirType) -> HirParam {
        HirParam {
            name: name.to_string(),
            ty,
            span: span(),
        }
    }

    #[test]
    fn lowers_free_function_to_func_declaration() {
        // func add(a: i32, b: i32) -> i32
        let program = HirProgram {
            items: vec![HirItem::Function(HirFunction {
                name: "add".to_string(),
                params: vec![param("a", HirType::I32), param("b", HirType::I32)],
                return_type: HirType::I32,
                body: vec![],
                span: span(),
            })],
        };

        let ir = lower_program(&program).expect("scaffold should produce a verifiable module");
        assert!(ir.contains("func.func"), "expected a func.func op:\n{ir}");
        assert!(ir.contains("@add"), "expected the function symbol:\n{ir}");
        assert!(
            ir.contains("(i32, i32) -> i32"),
            "expected the mapped signature:\n{ir}"
        );
    }

    #[test]
    fn lowers_method_with_receiver_and_void_return() {
        // impl Point { func reset(&mut self) }
        let program = HirProgram {
            items: vec![HirItem::Impl(HirImpl {
                type_name: "Point".to_string(),
                trait_name: None,
                methods: vec![HirMethod {
                    name: "reset".to_string(),
                    self_param: Some(HirSelfParam::RefMut),
                    params: vec![],
                    return_type: HirType::Void,
                    body: vec![],
                    span: span(),
                }],
                span: span(),
            })],
        };

        let ir = lower_program(&program).expect("scaffold should produce a verifiable module");
        assert!(
            ir.contains("@Point_reset"),
            "expected the mangled method symbol:\n{ir}"
        );
        // The &mut self receiver maps to an opaque pointer; a unit return has no result.
        assert!(
            ir.contains("!llvm.ptr"),
            "expected the receiver pointer type:\n{ir}"
        );
    }

    #[test]
    fn maps_scalar_types_to_mlir_equivalents() {
        let program = HirProgram {
            items: vec![HirItem::Function(HirFunction {
                name: "scalars".to_string(),
                params: vec![
                    param("a", HirType::Bool),
                    param("b", HirType::Char),
                    param("c", HirType::F64),
                ],
                return_type: HirType::F32,
                body: vec![],
                span: span(),
            })],
        };

        let ir = lower_program(&program).expect("scaffold should produce a verifiable module");
        assert!(
            ir.contains("(i1, i32, f64) -> f32"),
            "expected scalar type mapping:\n{ir}"
        );
    }
}
