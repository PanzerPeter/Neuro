use crate::{
    context::new_context,
    errors::MlirError,
    guards::{Guard, Overflow, Side},
    lower::{build_linkable_module, build_module},
};

use melior::{Context, ir::Module, pass::PassManager, utility::parse_pass_pipeline};
use neuro_hir::{HirItem, HirProgram, HirTarget, HirType};

/// The route from the dialects this slice builds in down to the `llvm` dialect.
///
/// The first four entries are what carry a `linalg` body: `one-shot-bufferize`
/// rewrites the tensor value semantics into `memref` buffers (function boundaries
/// included, or a `func.func` would keep tensors in its signature and never
/// convert), `buffer-results-to-out-params` turns each returned buffer into a
/// trailing parameter the caller allocates, `buffer-deallocation-pipeline` gives
/// every buffer still allocated inside an owner and a release, and only then can
/// `convert-linalg-to-loops` turn the structured op into `scf` loops over loads and
/// stores.
///
/// The two options on the first pair fix the boundary the LLVM backend calls
/// across. `identity-layout-map` makes a parameter a plain row-major `memref`,
/// which is what a DLPack buffer is; left at its default, the layout is fully
/// strided and a copy into it lowers to a runtime-library call nothing links.
/// Out-params (`modify-public-functions`, since every definition is public, and
/// `hoist-static-allocs`, so the body writes straight into the caller's buffer
/// rather than into its own and copying) leave the result's allocation to the
/// caller, which is the only side that knows how a Neuro tensor is allocated.
///
/// The rest is the descent those loops land in: `scf` becomes `cf` branches,
/// `memref` becomes pointer arithmetic against `malloc`, and each remaining
/// dialect converts on its own. `reconcile-unrealized-casts` runs last by
/// necessity: every conversion above it leaves `unrealized_conversion_cast` ops
/// at its boundary with the dialects the others own, and the translation rejects
/// any that survive.
///
/// It is named in text rather than assembled from `melior`'s typed pass
/// constructors because two entries have no usable one: melior's
/// `one-shot-bufferize` constructor takes no options, so it cannot set
/// `bufferize-function-boundaries`, and `buffer-deallocation-pipeline` is a
/// pipeline, not a pass. Half the pipeline typed and half in text would be two
/// spellings of one sequence.
///
/// The GPU pipeline shares both halves around its own middle, so they are named
/// once here as [`BUFFERIZE`] and [`llvm_descent`].
fn llvm_lowering_pipeline() -> String {
    format!(
        "builtin.module({BUFFERIZE},func.func(convert-linalg-to-loops),{descent},reconcile-unrealized-casts)",
        descent = llvm_descent(HOST_MEMREF_TO_LLVM)
    )
}

/// Tensor values into buffers, with the boundary the LLVM backend calls across.
///
/// `linalg-fuse-elementwise-ops` first folds each operation whose one use is the next
/// into it, so `(a + b) * c` is one loop nest (one kernel on a GPU) and its sum never
/// becomes a buffer. The fused body runs the same operations in the same order, and
/// neither backend contracts a multiply and an add without fast-math flags, so the
/// bits are the unfused ones.
pub(crate) const BUFFERIZE: &str = "\
    linalg-fuse-elementwise-ops,\
    one-shot-bufferize{bufferize-function-boundaries=true function-boundary-type-conversion=identity-layout-map},\
    buffer-results-to-out-params{hoist-static-allocs=true modify-public-functions=true},\
    buffer-deallocation-pipeline";

/// A buffer the body allocates for itself comes from libc `malloc`.
const HOST_MEMREF_TO_LLVM: &str = "finalize-memref-to-llvm";

/// Loops over buffers into the `llvm` dialect, short of reconciling the casts.
///
/// `memref_to_llvm` is where the CPU and GPU paths part: it decides which allocator
/// a buffer the body allocates for itself calls.
pub(crate) fn llvm_descent(memref_to_llvm: &str) -> String {
    format!(
        "convert-scf-to-cf,{memref_to_llvm},convert-func-to-llvm,convert-math-to-llvm,convert-arith-to-llvm,\
         convert-cf-to-llvm,convert-index-to-llvm"
    )
}

/// Lower a typed HIR program through MLIR all the way to verified LLVM IR.
///
/// The full 2C path in one call: HIR → `melior` module → the `llvm` dialect via an
/// MLIR conversion pipeline → `mlirTranslateModuleToLLVMIR` → an inkwell
/// [`Module`](inkwell::module::Module) → LLVM's verifier → textual LLVM IR.
///
/// This is what makes the two independent bindings one pipeline: `mlir-sys` builds
/// the LLVM module *inside* an `LLVMContext` that inkwell owns, so a mismatched
/// `libLLVM` between them cannot go unnoticed: the handoff fails here rather
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

/// Function bodies lowered through MLIR for the LLVM backend to link in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkableBodies {
    /// A textual LLVM module defining every symbol in `functions`, and nothing else.
    pub llvm_ir: String,
    /// `(function, symbol)`: a HIR function, and the symbol in `llvm_ir` that computes its
    /// body.
    pub functions: Vec<(String, String)>,
    /// `(symbol, checks)` for each symbol whose integer arithmetic is checked. Such a symbol
    /// takes one more parameter after the others, before its results: a `memref<1xi64>`
    /// status word the caller fills with all ones. A failed check lowers it to a key whose
    /// low 12 bits are the check's position in `checks`, counted from 1, and whose higher
    /// bits order failures as the LLVM backend meets them, so the one left is the one it
    /// would have stopped at. The caller panics with that check's message at its offset once
    /// the call returns.
    pub guards: Vec<(String, Vec<Guard>)>,
}

/// Lower every function this path computes exactly as the LLVM backend would into
/// linkable LLVM IR: element-wise arithmetic and matrix products over numeric tensors of
/// static shape, straight-line, with owned or `&` operands. Integer arithmetic carries the
/// LLVM backend's checks, its overflow ones only where `overflow` says they panic. A
/// `@gpu` function is never one of them: it is [`lower_for_gpu`](crate::lower_for_gpu)'s.
///
/// Each symbol has MLIR's calling convention rather than Neuro's. A tensor
/// parameter crosses as an exploded row-major `memref` descriptor (allocated
/// pointer, aligned pointer, offset, then one size and one stride per axis), a
/// scalar crosses as itself, and the result is one more descriptor appended after
/// the parameters, naming a buffer the caller allocated; the function returns
/// nothing. Wrapping each in a Neuro-ABI function is the caller's side.
///
/// # Errors
///
/// As [`translate_to_llvm_ir`].
pub fn lower_for_link(
    program: &HirProgram,
    overflow: Overflow,
) -> Result<LinkableBodies, MlirError> {
    let context = new_context();
    let (mut module, (functions, guards)) =
        build_linkable_module(&context, program, (overflow, Side::Host), &|function| {
            matches!(
                function.target,
                HirTarget::FollowsOperands | HirTarget::HostOperation | HirTarget::GpuOrHost
            )
        })?;
    if functions.is_empty() {
        return Ok(LinkableBodies {
            llvm_ir: String::new(),
            functions,
            guards,
        });
    }
    // A body with a tensor result only reads its operands and writes a buffer its caller
    // has just allocated, so no two of its buffers alias in a way that matters. Saying so
    // lets LLVM keep an accumulator in a register once the body is inlined. A compound
    // assignment's body writes its target in place, and is left as it is.
    let exclusive: Vec<&str> = functions
        .iter()
        .filter(|(function, _)| {
            program.items.iter().any(|item| {
                matches!(item, HirItem::Function(f)
                    if f.name == *function && f.return_type != HirType::Void)
            })
        })
        .map(|(_, symbol)| symbol.as_str())
        .collect();
    convert_to_llvm_dialect(&context, &mut module)?;
    let llvm_ir = translate_finishing(std::slice::from_ref(&module), &|llvm_module| {
        for symbol in &exclusive {
            if let Some(function) = llvm_module.get_function(symbol) {
                mark_noalias(llvm_module.get_context(), function);
            }
        }
    })?;

    Ok(LinkableBodies {
        llvm_ir,
        functions,
        guards,
    })
}

/// Mark every pointer parameter of `function` `noalias`.
fn mark_noalias(
    context: inkwell::context::ContextRef<'_>,
    function: inkwell::values::FunctionValue<'_>,
) {
    let kind = inkwell::attributes::Attribute::get_named_enum_kind_id("noalias");
    for (index, param) in function.get_param_iter().enumerate() {
        if param.is_pointer_value() {
            function.add_attribute(
                inkwell::attributes::AttributeLoc::Param(index as u32),
                context.create_enum_attribute(kind, 0),
            );
        }
    }
}

/// Run the `llvm`-dialect conversion and the LLVM-IR translation over a built module.
pub(crate) fn translate_module(
    context: &Context,
    module: &mut Module<'_>,
) -> Result<String, MlirError> {
    convert_to_llvm_dialect(context, module)?;
    translate_llvm_dialect(module)
}

/// Translate a module already wholly in the `llvm` dialect (plus any `gpu.binary`
/// it embeds) to LLVM IR, verified by LLVM.
pub(crate) fn translate_llvm_dialect(module: &Module<'_>) -> Result<String, MlirError> {
    translate_llvm_dialects(std::slice::from_ref(module))
}

/// [`translate_llvm_dialect`] over several modules, linked into one LLVM module. Modules
/// that went through different pass pipelines meet here rather than as MLIR.
pub(crate) fn translate_llvm_dialects(modules: &[Module<'_>]) -> Result<String, MlirError> {
    translate_finishing(modules, &|_| {})
}

/// [`translate_llvm_dialects`], with `finish` given the linked LLVM module before it is
/// verified and printed.
fn translate_finishing(
    modules: &[Module<'_>],
    finish: &dyn Fn(&inkwell::module::Module<'_>),
) -> Result<String, MlirError> {
    let llvm_context = inkwell::context::Context::create();
    let mut linked: Option<inkwell::module::Module<'_>> = None;
    for module in modules {
        let translated = translate_into(&llvm_context, module)?;
        match &linked {
            Some(first) => first
                .link_in_module(translated)
                .map_err(|error| MlirError::LlvmVerificationFailed(error.to_string()))?,
            None => linked = Some(translated),
        }
    }
    let llvm_module = linked.ok_or(MlirError::TranslationFailed)?;
    finish(&llvm_module);

    llvm_module
        .verify()
        .map_err(|error| MlirError::LlvmVerificationFailed(error.to_string()))?;

    Ok(llvm_module.print_to_string().to_string())
}

fn translate_into<'ctx>(
    llvm_context: &'ctx inkwell::context::Context,
    module: &Module<'_>,
) -> Result<inkwell::module::Module<'ctx>, MlirError> {
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
    Ok(unsafe { inkwell::module::Module::new(raw.cast()) })
}

/// Rewrite a module built from the `func` / `arith` / `index` / `linalg` /
/// `tensor` dialects into the `llvm` dialect, which is the only input
/// `mlirTranslateModuleToLLVMIR` accepts.
fn convert_to_llvm_dialect(context: &Context, module: &mut Module<'_>) -> Result<(), MlirError> {
    let manager = PassManager::new(context);
    parse_pass_pipeline(
        manager.as_operation_pass_manager(),
        &llvm_lowering_pipeline(),
    )?;

    manager
        .run(module)
        .map_err(|_| MlirError::PassPipelineFailed)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    use crate::smoke::build_smoke_module;
    use ast_types::BinaryOp;
    use neuro_hir::{
        AxisNames, HirExpr, HirExprKind, HirFunction, HirItem, HirParam, HirProgram, HirStmt,
        HirType, static_shape,
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
                target: neuro_hir::HirTarget::FollowsOperands,
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

    pub(crate) fn tensor(shape: Vec<Option<usize>>) -> HirType {
        HirType::Tensor {
            element: Box::new(HirType::F32),
            shape,
            names: AxisNames::default(),
        }
    }

    /// `func f(a: Tensor<f32, L>, b: Tensor<f32, R>) -> Tensor<f32, Out> { return a <op> b }`
    pub(crate) fn program_with_tensor_operator(
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
                target: neuro_hir::HirTarget::FollowsOperands,
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

    fn borrowed(ty: HirType) -> HirType {
        HirType::Reference {
            inner: Box::new(ty),
            mutable: false,
        }
    }

    #[test]
    fn a_float_body_is_linked_under_its_own_symbol_with_an_out_param() {
        let shape = static_shape(&[2, 3]);
        let bodies = lower_for_link(
            &program_with_tensor_operator(
                BinaryOp::Add,
                tensor(shape.clone()),
                tensor(shape.clone()),
                tensor(shape),
            ),
            crate::Overflow::Checked,
        )
        .expect("a float body should lower for linking");

        assert_eq!(
            bodies.functions,
            vec![("f".to_string(), "__neuro_mlir_f".to_string())]
        );
        // Two rank-2 operands and the out-param: three descriptors of seven fields.
        let ir = &bodies.llvm_ir;
        let signature = ir
            .lines()
            .find(|line| line.starts_with("define void @__neuro_mlir_f("))
            .unwrap_or_else(|| {
                panic!("expected a void body taking its result as a parameter:\n{ir}")
            });
        assert_eq!(signature.matches("ptr").count(), 6, "{signature}");
        assert_eq!(signature.matches("i64").count(), 15, "{signature}");
        assert!(
            !ir.contains("@f("),
            "the Neuro-ABI name is the LLVM backend's to define:\n{ir}"
        );
    }

    #[test]
    fn borrowed_operands_are_read_as_tensors() {
        let ty = tensor(static_shape(&[2, 2]));
        let bodies = lower_for_link(
            &program_with_tensor_operator(
                BinaryOp::MatMul,
                borrowed(ty.clone()),
                borrowed(ty.clone()),
                ty,
            ),
            crate::Overflow::Checked,
        )
        .expect("borrowed operands should lower for linking");

        assert_eq!(bodies.functions.len(), 1, "{}", bodies.llvm_ir);
        assert!(bodies.llvm_ir.contains("fmul float"), "{}", bodies.llvm_ir);
    }

    fn integer_program(op: BinaryOp) -> HirProgram {
        let ty = HirType::Tensor {
            element: Box::new(HirType::I32),
            shape: static_shape(&[2]),
            names: AxisNames::default(),
        };
        program_with_tensor_operator(op, ty.clone(), ty.clone(), ty)
    }

    #[test]
    fn an_integer_body_links_with_its_overflow_check_on_the_debug_tier() {
        let bodies = lower_for_link(&integer_program(BinaryOp::Add), crate::Overflow::Checked)
            .expect("an integer body lowers");
        assert_eq!(bodies.functions.len(), 1, "{}", bodies.llvm_ir);
        let overflow = crate::Guard {
            kind: crate::GuardKind::Overflow,
            offset: 0,
        };
        assert_eq!(
            bodies.guards,
            vec![("__neuro_mlir_f".to_string(), vec![overflow])]
        );
        assert!(
            bodies.llvm_ir.contains("@llvm.sadd.with.overflow.i32")
                && bodies.llvm_ir.contains("atomicrmw umin ptr"),
            "an overflow lowers the status word to its check's number:\n{}",
            bodies.llvm_ir
        );

        let release = lower_for_link(&integer_program(BinaryOp::Add), crate::Overflow::Wrapping)
            .expect("an integer body lowers");
        assert!(release.guards.is_empty(), "{:?}", release.guards);
        assert!(
            !release.llvm_ir.contains("with.overflow") && release.llvm_ir.contains("add i32"),
            "the release tier wraps:\n{}",
            release.llvm_ir
        );
    }

    #[test]
    fn an_integer_division_checks_its_divisor_on_every_tier() {
        let release = lower_for_link(
            &integer_program(BinaryOp::Divide),
            crate::Overflow::Wrapping,
        )
        .expect("an integer division lowers");
        let kinds: Vec<_> = release.guards[0].1.iter().map(|guard| guard.kind).collect();
        assert_eq!(kinds, vec![crate::GuardKind::DivisionByZero]);
        assert!(release.llvm_ir.contains("sdiv i32"), "{}", release.llvm_ir);

        let debug = lower_for_link(&integer_program(BinaryOp::Divide), crate::Overflow::Checked)
            .expect("an integer division lowers");
        let kinds: Vec<_> = debug.guards[0].1.iter().map(|guard| guard.kind).collect();
        assert_eq!(
            kinds,
            vec![crate::GuardKind::DivisionByZero, crate::GuardKind::Overflow],
            "`MIN / -1` is an overflow on the debug tier"
        );
    }

    #[test]
    fn a_body_handing_back_its_argument_is_not_linked() {
        let ty = tensor(static_shape(&[2]));
        let mut program =
            program_with_tensor_operator(BinaryOp::Add, ty.clone(), ty.clone(), ty.clone());
        let HirItem::Function(function) = &mut program.items[0] else {
            unreachable!("the fixture is one function");
        };
        function.body = vec![HirStmt::Expr(HirExpr::new(
            HirExprKind::Variable("a".to_string()),
            ty,
            Span::new(0, 0),
        ))];

        let bodies = lower_for_link(&program, crate::Overflow::Checked)
            .expect("the program should still lower");
        assert!(bodies.functions.is_empty(), "{}", bodies.llvm_ir);
    }

    #[test]
    fn a_tail_expression_is_a_body() {
        let ty = tensor(static_shape(&[2]));
        let mut program =
            program_with_tensor_operator(BinaryOp::Subtract, ty.clone(), ty.clone(), ty);
        let HirItem::Function(function) = &mut program.items[0] else {
            unreachable!("the fixture is one function");
        };
        let Some(HirStmt::Return {
            value: Some(value), ..
        }) = function.body.pop()
        else {
            unreachable!("the fixture returns its operation");
        };
        function.body.push(HirStmt::Expr(value));

        let bodies = lower_for_link(&program, crate::Overflow::Checked)
            .expect("a tail expression should lower");
        assert_eq!(bodies.functions.len(), 1, "{}", bodies.llvm_ir);
        assert!(bodies.llvm_ir.contains("fsub float"), "{}", bodies.llvm_ir);
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
