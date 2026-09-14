use crate::{context::new_context, errors::MlirError, lower::build_module};

use melior::{ir::Module, pass::PassManager, utility::parse_pass_pipeline, Context};
use neuro_hir::HirProgram;

/// The route from the dialects this slice builds in down to the `llvm` dialect.
///
/// The first three entries are what carry a `linalg` body: `one-shot-bufferize`
/// rewrites the tensor value semantics into `memref` buffers (function boundaries
/// included, or a `func.func` would keep tensors in its signature and never
/// convert), `buffer-deallocation-pipeline` gives every buffer it allocated an
/// owner and a release, and only then can `convert-linalg-to-loops` turn the
/// structured op into `scf` loops over loads and stores.
///
/// The rest is the descent those loops land in: `scf` becomes `cf` branches,
/// `memref` becomes pointer arithmetic against `malloc`, and each remaining
/// dialect converts on its own. `reconcile-unrealized-casts` runs last by
/// necessity: every conversion above it leaves `unrealized_conversion_cast` ops
/// at its boundary with the dialects the others own, and the translation rejects
/// any that survive.
///
/// It is named in text rather than assembled from `melior`'s typed pass
/// constructors because `one-shot-bufferize` has none: `melior 0.25` wraps the
/// conversion and linalg passes but not the bufferization ones. Half the pipeline
/// typed and half in text would be two spellings of one sequence.
const LLVM_LOWERING_PIPELINE: &str = "builtin.module(\
    one-shot-bufferize{bufferize-function-boundaries=true},\
    buffer-deallocation-pipeline,\
    func.func(convert-linalg-to-loops),\
    convert-scf-to-cf,\
    finalize-memref-to-llvm,\
    convert-func-to-llvm,\
    convert-arith-to-llvm,\
    convert-cf-to-llvm,\
    convert-index-to-llvm,\
    reconcile-unrealized-casts)";

/// Lower a typed HIR program through MLIR all the way to verified LLVM IR.
///
/// The full 2C path in one call: HIR → `melior` module → the `llvm` dialect via an
/// MLIR conversion pipeline → `mlirTranslateModuleToLLVMIR` → an inkwell
/// [`Module`](inkwell::module::Module) → LLVM's verifier → textual LLVM IR.
///
/// This is what makes the two independent bindings one pipeline: `mlir-sys` builds
/// the LLVM module *inside* an `LLVMContext` that inkwell owns, so a mismatched
/// `libLLVM-20` between them cannot go unnoticed: the handoff fails here rather
/// than miscompiling downstream.
///
/// # Errors
///
/// Returns [`MlirError::PassPipelineFailed`] if the module does not survive
/// conversion to the `llvm` dialect, [`MlirError::TranslationFailed`] if MLIR
/// declines to translate it, and [`MlirError::LlvmVerificationFailed`] if the
/// resulting LLVM module is ill-formed. Errors from building the MLIR module
/// itself propagate unchanged from [`lower_program`](crate::lower_program).
pub fn translate_to_llvm_ir(program: &HirProgram) -> Result<String, MlirError> {
    let context = new_context();
    let mut module = build_module(&context, program)?;

    translate_module(&context, &mut module)
}

/// Run the `llvm`-dialect conversion and the LLVM-IR translation over a built module.
pub(crate) fn translate_module(
    context: &Context,
    module: &mut Module<'_>,
) -> Result<String, MlirError> {
    convert_to_llvm_dialect(context, module)?;

    let llvm_context = inkwell::context::Context::create();

    // SAFETY: the operation is the module's own, alive for this call, and the
    // context pointer is the live inkwell context's. Both bindings generate their
    // own opaque `LLVMContextRef` alias over the same C type, hence the cast. The
    // returned module is owned by the caller per the MLIR C API contract.
    let raw = unsafe {
        mlir_sys::mlirTranslateModuleToLLVMIR(
            module.as_operation().to_raw(),
            llvm_context.raw().cast(),
        )
    };

    if raw.is_null() {
        return Err(MlirError::TranslationFailed);
    }

    // SAFETY: `raw` is the non-null module MLIR just built in `llvm_context`, and
    // ownership transfers here: the inkwell `Module` is the single owner and
    // disposes of it on drop, before the context it lives in is dropped.
    let llvm_module = unsafe { inkwell::module::Module::new(raw.cast()) };

    llvm_module
        .verify()
        .map_err(|error| MlirError::LlvmVerificationFailed(error.to_string()))?;

    Ok(llvm_module.print_to_string().to_string())
}

/// Rewrite a module built from the `func` / `arith` / `index` / `linalg` /
/// `tensor` dialects into the `llvm` dialect, which is the only input
/// `mlirTranslateModuleToLLVMIR` accepts.
fn convert_to_llvm_dialect(context: &Context, module: &mut Module<'_>) -> Result<(), MlirError> {
    let manager = PassManager::new(context);
    parse_pass_pipeline(manager.as_operation_pass_manager(), LLVM_LOWERING_PIPELINE)?;

    manager
        .run(module)
        .map_err(|_| MlirError::PassPipelineFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::smoke::build_smoke_module;
    use ast_types::BinaryOp;
    use neuro_hir::{
        static_shape, AxisNames, HirExpr, HirExprKind, HirFunction, HirItem, HirParam, HirProgram,
        HirStmt, HirType,
    };
    use shared_types::Span;

    fn program_with_one_function() -> HirProgram {
        HirProgram {
            items: vec![HirItem::Function(HirFunction {
                name: "add".to_string(),
                params: vec![
                    HirParam {
                        name: "a".to_string(),
                        ty: HirType::I32,
                        span: Span::new(0, 0),
                    },
                    HirParam {
                        name: "b".to_string(),
                        ty: HirType::I32,
                        span: Span::new(0, 0),
                    },
                ],
                return_type: HirType::I32,
                body: Vec::new(),
                span: Span::new(0, 0),
            })],
        }
    }

    #[test]
    fn hir_reaches_llvm_ir_through_mlir() {
        let ir = translate_to_llvm_ir(&program_with_one_function())
            .expect("the HIR scaffold should translate to LLVM IR");

        assert!(
            ir.contains("declare") && ir.contains("@add"),
            "expected an LLVM declaration for the lowered function:\n{ir}"
        );
    }

    #[test]
    fn function_bodies_survive_the_crossing() {
        let context = new_context();
        let mut module = build_smoke_module(&context).expect("the smoke module should build");
        let ir = translate_module(&context, &mut module)
            .expect("the smoke module should translate to LLVM IR");

        assert!(
            ir.contains("define") && ir.contains("@neuro_smoke"),
            "expected a defined function, not a declaration:\n{ir}"
        );
        assert!(
            ir.contains(" add "),
            "expected arith.addi to have become an LLVM add:\n{ir}"
        );
    }

    fn tensor(shape: Vec<Option<usize>>) -> HirType {
        HirType::Tensor {
            element: Box::new(HirType::F32),
            shape,
            names: AxisNames::default(),
        }
    }

    /// `func f(a: Tensor<f32, L>, b: Tensor<f32, R>) -> Tensor<f32, Out> { return a <op> b }`
    fn program_with_tensor_operator(
        op: BinaryOp,
        left: HirType,
        right: HirType,
        result: HirType,
    ) -> HirProgram {
        let operand = |name: &str, ty: &HirType| {
            HirExpr::new(
                HirExprKind::Variable(name.to_string()),
                ty.clone(),
                Span::new(0, 0),
            )
        };
        let param = |name: &str, ty: &HirType| HirParam {
            name: name.to_string(),
            ty: ty.clone(),
            span: Span::new(0, 0),
        };

        HirProgram {
            items: vec![HirItem::Function(HirFunction {
                name: "f".to_string(),
                params: vec![param("a", &left), param("b", &right)],
                return_type: result.clone(),
                body: vec![HirStmt::Return {
                    value: Some(HirExpr::new(
                        HirExprKind::Binary {
                            op,
                            left: Box::new(operand("a", &left)),
                            right: Box::new(operand("b", &right)),
                        },
                        result,
                        Span::new(0, 0),
                    )),
                    span: Span::new(0, 0),
                }],
                span: Span::new(0, 0),
            })],
        }
    }

    /// The whole point of bufferizing: a `linalg` body is a definition on the far
    /// side of the crossing, and the loads and stores prove it is the loop nest
    /// rather than an emptied-out shell.
    fn assert_is_a_lowered_body(ir: &str) {
        assert!(
            ir.contains("define") && ir.contains("@f"),
            "expected a defined function, not a declaration:\n{ir}"
        );
        assert!(
            ir.contains("load float") && ir.contains("store float"),
            "expected the linalg body to have become element loads and stores:\n{ir}"
        );
        assert!(
            !ir.contains("linalg.") && !ir.contains("memref"),
            "expected nothing above the llvm dialect to survive:\n{ir}"
        );
    }

    #[test]
    fn an_element_wise_linalg_body_reaches_llvm_ir() {
        let shape = static_shape(&[2, 3]);
        let ir = translate_to_llvm_ir(&program_with_tensor_operator(
            BinaryOp::Add,
            tensor(shape.clone()),
            tensor(shape.clone()),
            tensor(shape),
        ))
        .expect("an element-wise body should bufferize and translate");

        assert_is_a_lowered_body(&ir);
        assert!(
            ir.contains("fadd float"),
            "expected arith.addf to have become an LLVM fadd:\n{ir}"
        );
    }

    #[test]
    fn a_contracting_linalg_body_reaches_llvm_ir() {
        let ir = translate_to_llvm_ir(&program_with_tensor_operator(
            BinaryOp::MatMul,
            tensor(static_shape(&[2, 3])),
            tensor(static_shape(&[3, 4])),
            tensor(static_shape(&[2, 4])),
        ))
        .expect("a matrix product should bufferize and translate");

        assert_is_a_lowered_body(&ir);
        assert!(
            ir.contains("fmul float"),
            "expected the multiply-accumulate body:\n{ir}"
        );
    }

    #[test]
    fn a_dynamic_extent_survives_the_crossing() {
        // The destination is sized by `tensor.dim` on an operand, so bufferizing
        // it has to keep that extent as a run-time value feeding the allocation
        // rather than needing it as a constant.
        let shape = vec![None];
        let ir = translate_to_llvm_ir(&program_with_tensor_operator(
            BinaryOp::Multiply,
            tensor(shape.clone()),
            tensor(shape.clone()),
            tensor(shape),
        ))
        .expect("a dynamic extent should bufferize and translate");

        assert_is_a_lowered_body(&ir);
        assert!(
            ir.contains("@malloc"),
            "expected the destination buffer to be allocated at run time:\n{ir}"
        );
    }

    #[test]
    fn empty_program_translates_to_an_empty_module() {
        let ir = translate_to_llvm_ir(&HirProgram { items: Vec::new() })
            .expect("an empty program should still produce a verifiable LLVM module");

        assert!(
            !ir.contains("define") && !ir.contains("declare"),
            "expected no functions:\n{ir}"
        );
    }
}
