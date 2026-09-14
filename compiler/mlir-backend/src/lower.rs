use crate::{context::new_context, errors::MlirError, tensor_arithmetic};

use melior::{
    dialect::{func, llvm},
    ir::{
        attribute::{StringAttribute, TypeAttribute},
        operation::OperationLike,
        r#type::{FunctionType, IntegerType, RankedTensorType},
        BlockLike, Identifier, Location, Module, Operation, Region, Type, TypeLike,
    },
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

/// Lower a typed HIR program to an MLIR module and return its textual form.
///
/// Walks the typed HIR and emits one `func.func` per free function and per `impl`
/// method, mapping each HIR type to its MLIR counterpart. A function whose body is
/// element-wise tensor arithmetic becomes a *definition* built from the `linalg`
/// and `tensor` dialects; every other function stays an external *declaration*
/// (empty region), because scalar codegen belongs to the LLVM backend alone.
///
/// # Errors
///
/// Returns [`MlirError::UnsupportedType`] if a HIR type with no MLIR mapping
/// appears in value position, [`MlirError::AttributeSyntax`] if a generated
/// `linalg` attribute is rejected, or [`MlirError::ModuleVerificationFailed`] if
/// the constructed module fails MLIR's own verifier.
pub fn lower_program(program: &HirProgram) -> Result<String, MlirError> {
    let context = new_context();
    let module = build_module(&context, program)?;

    Ok(module.as_operation().to_string())
}

/// Build the verified module in a caller-owned context.
///
/// Split out of [`lower_program`] so the translating path can keep working on the
/// live `Module` instead of re-parsing its printed form.
pub(crate) fn build_module<'c>(
    context: &'c Context,
    program: &HirProgram,
) -> Result<Module<'c>, MlirError> {
    let location = Location::unknown(context);
    let module = Module::new(location);

    for item in &program.items {
        match item {
            HirItem::Function(function) => {
                let params: Vec<HirType> = function.params.iter().map(|p| p.ty.clone()).collect();
                // A tensor-arithmetic body is the one kind this path defines rather
                // than declares; everything else stays external, which is what keeps
                // scalar codegen from existing twice.
                let op = match tensor_arithmetic::build_body(context, location, function)? {
                    Some(region) => define_function(
                        context,
                        location,
                        &function.name,
                        &params,
                        &function.return_type,
                        region,
                    )?,
                    None => declare_function(
                        context,
                        location,
                        &function.name,
                        &params,
                        &function.return_type,
                    )?,
                };
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
                        declare_function(context, location, &name, &params, &method.return_type)?;
                    module.body().append_operation(op);
                }
            }
            HirItem::Closure(closure) => {
                // A lifted closure is an ordinary function whose implicit first
                // parameter is the captured-environment pointer, matching the
                // LLVM backend's calling convention for `__closure_N`.
                let mut params: Vec<HirType> = vec![environment_type(&closure.name)];
                params.extend(closure.params.iter().map(|p| p.ty.clone()));
                let op = declare_function(
                    context,
                    location,
                    &closure.name,
                    &params,
                    &closure.return_type,
                )?;
                module.body().append_operation(op);
            }
            // Structs, enums, constants, and traits carry no callable surface; a
            // trait item is only a vtable slot order, and its methods reach
            // the module through the implementors' `impl` blocks.
            HirItem::Struct(_) | HirItem::Enum(_) | HirItem::Const(_) | HirItem::Trait(_) => {}
        }
    }

    if !module.as_operation().verify() {
        return Err(MlirError::ModuleVerificationFailed);
    }

    Ok(module)
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

/// The HIR type of a closure's implicit environment parameter: a borrow of the
/// per-closure capture record. Like the method receiver it lowers to an opaque
/// pointer in the scaffold; naming the struct keeps the intent readable.
fn environment_type(closure_name: &str) -> HirType {
    HirType::Reference {
        inner: Box::new(HirType::Struct(format!("{closure_name}_env"))),
        mutable: false,
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
    let fn_type = signature(context, param_types, return_type)?;

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

/// Build a `func.func` definition carrying an already-lowered body region.
///
/// The visibility attribute the declaration path sets is deliberately absent: a
/// definition is the module's exported surface, and marking it private would hide
/// the only function this path actually emits code for.
fn define_function<'c>(
    context: &'c Context,
    location: Location<'c>,
    name: &str,
    param_types: &[HirType],
    return_type: &HirType,
    body: Region<'c>,
) -> Result<Operation<'c>, MlirError> {
    let fn_type = signature(context, param_types, return_type)?;

    Ok(func::func(
        context,
        StringAttribute::new(context, name),
        TypeAttribute::new(fn_type.into()),
        body,
        &[],
        location,
    ))
}

/// Map a HIR signature to its MLIR function type. `void` is the empty result list
/// in return position, which is the one place it is not an unsupported type.
fn signature<'c>(
    context: &'c Context,
    param_types: &[HirType],
    return_type: &HirType,
) -> Result<FunctionType<'c>, MlirError> {
    let inputs = param_types
        .iter()
        .map(|ty| map_type(context, ty))
        .collect::<Result<Vec<_>, _>>()?;

    let results = match return_type {
        HirType::Void => Vec::new(),
        other => vec![map_type(context, other)?],
    };

    Ok(FunctionType::new(context, &inputs, &results))
}

/// MLIR's sentinel extent for a dynamically shaped `?` axis.
///
/// Read from the C API rather than written out as `i64::MIN` so the value tracks
/// the toolchain instead of this file.
fn dynamic_extent() -> u64 {
    // SAFETY: a pure accessor over a compile-time constant. It takes no arguments,
    // reads no context, and cannot fail.
    (unsafe { mlir_sys::mlirShapedTypeGetDynamicSize() }) as u64
}

/// Map a resolved HIR type to its MLIR type.
///
/// Scalars map to their natural MLIR types and a tensor to a ranked MLIR tensor;
/// every other aggregate or reference type maps to an opaque LLVM pointer until
/// real struct lowering lands.
pub(crate) fn map_type<'c>(context: &'c Context, ty: &HirType) -> Result<Type<'c>, MlirError> {
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
        // A newtype is transparent: map it to its inner type.
        HirType::Newtype { inner, .. } => map_type(context, inner)?,
        HirType::String
        | HirType::Struct(_)
        | HirType::Enum(_)
        | HirType::Reference { .. }
        | HirType::Array { .. }
        | HirType::Tuple(_)
        | HirType::Collection { .. }
        // Address space 0; all LLVM pointers are opaque (`!llvm.ptr`) since LLVM 19.
        | HirType::Function { .. } => llvm::r#type::pointer(context, 0),
        HirType::Void => {
            return Err(MlirError::UnsupportedType(
                "void cannot appear in value position".to_string(),
            ))
        }
        // `dyn Trait` is unsized: it is only ever the referent of a
        // reference, which the arm above already maps to a pointer.
        HirType::DynObject(name) => {
            return Err(MlirError::UnsupportedType(format!(
                "unsized `dyn {name}` cannot appear in value position"
            )))
        }
        // A slice is unsized for the same reason: only `&[T]` / `&mut [T]` are
        // values, and the reference arm above already maps those to a pointer.
        HirType::Slice(element) => {
            return Err(MlirError::UnsupportedType(format!(
                "unsized `[{element}]` cannot appear in value position"
            )))
        }
        HirType::Tensor { element, shape, .. } => {
            let element_type = map_type(context, element)?;
            // An aggregate element would have mapped to `!llvm.ptr`, which is not a
            // type `tensor<...>` accepts; checking the MLIR type keeps this arm from
            // drifting as the scalar arms above change.
            if !element_type.is_integer() && !element_type.is_float() {
                return Err(MlirError::UnsupportedType(format!(
                    "tensor element `{element}` is not an MLIR scalar"
                )));
            }
            let extents: Vec<u64> = shape
                .iter()
                .map(|extent| extent.map_or_else(dynamic_extent, |extent| extent as u64))
                .collect();
            RankedTensorType::new(&extents, element_type, None).into()
        }
    };
    Ok(mapped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use neuro_hir::{HirCapture, HirClosure, HirFunction, HirImpl, HirMethod, HirParam};
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
    fn lowers_lifted_closure_with_environment_pointer() {
        // |x: i32| -> i32 { x + n }, lifted with `n` captured.
        let program = HirProgram {
            items: vec![HirItem::Closure(HirClosure {
                name: "__closure_0".to_string(),
                captures: vec![HirCapture {
                    name: "n".to_string(),
                    ty: HirType::I32,
                }],
                params: vec![param("x", HirType::I32)],
                return_type: HirType::I32,
                body: vec![],
                span: span(),
            })],
        };

        let ir = lower_program(&program).expect("scaffold should produce a verifiable module");
        assert!(
            ir.contains("@__closure_0"),
            "expected the lifted closure symbol:\n{ir}"
        );
        // The environment pointer is prepended to the user-facing parameters.
        assert!(
            ir.contains("(!llvm.ptr, i32) -> i32"),
            "expected the environment pointer as first parameter:\n{ir}"
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

    #[test]
    fn maps_aggregate_types_to_opaque_pointers() {
        // Every aggregate is a pointer in the scaffold. Collections are listed explicitly
        // because a new `HirType` variant must be routed here rather than silently
        // breaking this crate's build behind the off-by-default `mlir` feature.
        let program = HirProgram {
            items: vec![HirItem::Function(HirFunction {
                name: "aggregates".to_string(),
                params: vec![
                    param("a", HirType::String),
                    param(
                        "b",
                        HirType::Collection {
                            kind: neuro_hir::HirCollectionKind::Vec,
                            args: vec![HirType::I32],
                        },
                    ),
                ],
                return_type: HirType::Tuple(vec![HirType::I32, HirType::I32]),
                body: vec![],
                span: span(),
            })],
        };

        let ir = lower_program(&program).expect("scaffold should produce a verifiable module");
        assert!(
            ir.contains("(!llvm.ptr, !llvm.ptr) -> !llvm.ptr"),
            "expected aggregate types to map to opaque pointers:\n{ir}"
        );
    }
}
