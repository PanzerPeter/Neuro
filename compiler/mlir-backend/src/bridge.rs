use crate::{context::new_context, errors::MlirError, lower::build_module};

use melior::{
    ir::Module,
    pass::{conversion, PassManager},
    Context,
};
use neuro_hir::HirProgram;

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

/// Rewrite a module built from the `func` / `arith` / `index` dialects into the
/// `llvm` dialect, which is the only input `mlirTranslateModuleToLLVMIR` accepts.
///
/// `reconcile-unrealized-casts` runs last by necessity: each conversion above it
/// leaves `unrealized_conversion_cast` ops at its boundary with the dialects the
/// others own, and the translation rejects any that survive.
fn convert_to_llvm_dialect(context: &Context, module: &mut Module<'_>) -> Result<(), MlirError> {
    let manager = PassManager::new(context);
    manager.add_pass(conversion::create_func_to_llvm());
    manager.add_pass(conversion::create_arith_to_llvm());
    manager.add_pass(conversion::create_index_to_llvm());
    manager.add_pass(conversion::create_reconcile_unrealized_casts());

    manager
        .run(module)
        .map_err(|_| MlirError::PassPipelineFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::smoke::build_smoke_module;
    use neuro_hir::{HirFunction, HirItem, HirParam, HirProgram, HirType};
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
