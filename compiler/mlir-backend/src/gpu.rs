use crate::{
    bridge::{BUFFERIZE, LinkableBodies, llvm_descent, translate_llvm_dialect},
    context::new_context,
    errors::MlirError,
    lower::build_linkable_module,
    tensor_arithmetic::read_type,
};

use melior::{Context, ir::Module, pass::PassManager, utility::parse_pass_pipeline};
use neuro_hir::{HirFunction, HirItem, HirProgram, HirType};
use shared_types::Span;

/// The GPU a set of kernels is compiled for, and the chip that fixes its ISA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuTarget {
    /// NVIDIA through `nvvm`. `chip` is an `sm_NN` compute capability; the kernels ship
    /// as PTX, which the CUDA driver compiles for any GPU at or above it.
    Nvidia { chip: String },
    /// AMD through `rocdl`. `chip` is a `gfxNNN` architecture; the kernels ship as a
    /// code object for exactly that chip.
    Amd { chip: String },
}

/// The chips LLVM 22 has a processor model for (`llc -march=nvptx64 -mcpu=help`, and
/// `-march=amdgcn`, less the `gfxN-generic` families). LLVM only warns about any other name,
/// then crashes selecting AMD instructions without one, so a chip must be on its list.
const NVIDIA_CHIPS: &[&str] = &[
    "sm_20", "sm_21", "sm_30", "sm_32", "sm_35", "sm_37", "sm_50", "sm_52", "sm_53", "sm_60",
    "sm_61", "sm_62", "sm_70", "sm_72", "sm_75", "sm_80", "sm_86", "sm_87", "sm_88", "sm_89",
    "sm_90", "sm_90a", "sm_100", "sm_100a", "sm_100f", "sm_101", "sm_101a", "sm_101f", "sm_103",
    "sm_103a", "sm_103f", "sm_110", "sm_110a", "sm_110f", "sm_120", "sm_120a", "sm_120f", "sm_121",
    "sm_121a", "sm_121f",
];
const AMD_CHIPS: &[&str] = &[
    "gfx600", "gfx601", "gfx602", "gfx700", "gfx701", "gfx702", "gfx703", "gfx704", "gfx705",
    "gfx801", "gfx802", "gfx803", "gfx805", "gfx810", "gfx900", "gfx902", "gfx904", "gfx906",
    "gfx908", "gfx909", "gfx90a", "gfx90c", "gfx942", "gfx950", "gfx1010", "gfx1011", "gfx1012",
    "gfx1013", "gfx1030", "gfx1031", "gfx1032", "gfx1033", "gfx1034", "gfx1035", "gfx1036",
    "gfx1100", "gfx1101", "gfx1102", "gfx1103", "gfx1150", "gfx1151", "gfx1152", "gfx1153",
    "gfx1200", "gfx1201", "gfx1250", "gfx1251",
];

impl GpuTarget {
    fn chip(&self) -> &str {
        match self {
            GpuTarget::Nvidia { chip } | GpuTarget::Amd { chip } => chip,
        }
    }

    fn known_chips(&self) -> &'static [&'static str] {
        match self {
            GpuTarget::Nvidia { .. } => NVIDIA_CHIPS,
            GpuTarget::Amd { .. } => AMD_CHIPS,
        }
    }

    fn attach_target_pass(&self) -> &'static str {
        match self {
            GpuTarget::Nvidia { .. } => "nvvm-attach-target",
            GpuTarget::Amd { .. } => "rocdl-attach-target",
        }
    }

    fn kernel_conversion_pass(&self) -> &'static str {
        match self {
            GpuTarget::Nvidia { .. } => "convert-gpu-to-nvvm",
            GpuTarget::Amd { .. } => "convert-gpu-to-rocdl",
        }
    }

    /// What `gpu-module-to-binary` embeds. PTX for NVIDIA, because the driver JIT
    /// loads it and so a compile needs no CUDA toolkit. A code object for AMD,
    /// because HIP loads no assembly; linking one runs ROCm's `ld.lld`.
    fn object_format(&self) -> &'static str {
        match self {
            GpuTarget::Nvidia { .. } => "isa",
            GpuTarget::Amd { .. } => "bin",
        }
    }
}

/// Threads per block along the first two parallel axes. A launch where each block
/// runs one thread leaves all but one lane of every warp idle.
///
/// ponytail: one fixed tile for every shape and chip; a 1-D body gets 16 threads a
/// block. Pick per rank and chip once kernels run and can be measured.
const TILE_SIZES: &str = "16,16";

/// A buffer a launcher needs between two kernels is allocated through
/// `_mlir_memref_to_llvm_alloc` / `_mlir_memref_to_llvm_free` instead of libc, so
/// the caller can hand out device memory: only kernels ever touch it.
const DEVICE_MEMREF_TO_LLVM: &str = "finalize-memref-to-llvm{use-generic-functions=true}";

/// Lower every `@gpu` function, a `fallback: true` one included, into GPU kernels for
/// `target`, with host functions that launch them, or refuse the program if any of them
/// cannot become one.
///
/// A body qualifies exactly when [`lower_for_link`](crate::lower_for_link) would link
/// it were it not `@gpu`, and has no rank-0 tensor in it: an operation with no
/// parallel axis has no loop to map, so the launcher would run it on the host
/// against buffers that live on the device. Each symbol's signature is
/// `lower_for_link`'s: the host side takes the same exploded descriptors and
/// out-param, so the LLVM backend's wrapper serves either.
///
/// A symbol's body launches one kernel per `linalg` operation through MLIR's GPU
/// runtime ABI (`mgpuModuleLoad` / `mgpuModuleLoadJIT`, `mgpuLaunchKernel`, the
/// `mgpuStream*` family), which the returned IR declares and nothing here defines.
/// Every pointer in its descriptors must address device memory, and a buffer it
/// needs between two kernels comes from `_mlir_memref_to_llvm_alloc(i64) -> ptr`
/// and goes back through `_mlir_memref_to_llvm_free(ptr)`, both declared and left
/// for the caller to define as device allocations. The kernels are embedded in the
/// IR as device objects and loaded by a global constructor.
///
/// # Errors
///
/// [`MlirError::InvalidGpuChip`] for a chip LLVM has no processor model for, which
/// also keeps anything but a plain name out of the pass pipeline text it is spliced into. [`MlirError::GpuBodiesNotLowered`] naming every `@gpu` function whose body
/// does not qualify. [`MlirError::GpuSerializationFailed`] when a device object cannot be
/// produced, which for an AMD target means ROCm is not installed. Otherwise as
/// [`translate_to_llvm_ir`](crate::translate_to_llvm_ir).
pub fn lower_for_gpu(
    program: &HirProgram,
    target: &GpuTarget,
) -> Result<LinkableBodies, MlirError> {
    lower_with_format(program, target, target.object_format())
}

/// [`lower_for_gpu`] with the device object format chosen by the caller, so a test
/// can stop an AMD target at assembly on a machine without ROCm.
pub(crate) fn lower_with_format(
    program: &HirProgram,
    target: &GpuTarget,
    format: &str,
) -> Result<LinkableBodies, MlirError> {
    let chip = target.chip();
    if !target.known_chips().contains(&chip) {
        return Err(MlirError::InvalidGpuChip(chip.to_string()));
    }

    let context = new_context();
    let (mut module, functions) = build_linkable_module(&context, program, runs_on_gpu)?;
    let refused = refused_bodies(program, &functions);
    if !refused.is_empty() {
        return Err(MlirError::GpuBodiesNotLowered(refused));
    }
    if functions.is_empty() {
        return Ok(LinkableBodies {
            llvm_ir: String::new(),
            functions,
        });
    }

    run_pipeline(&context, &mut module, &gpu_lowering_pipeline(target))
        .map_err(|_| MlirError::PassPipelineFailed)?;
    run_pipeline(
        &context,
        &mut module,
        &format!("builtin.module(gpu-module-to-binary{{format={format}}})"),
    )
    .map_err(|_| MlirError::GpuSerializationFailed)?;

    Ok(LinkableBodies {
        llvm_ir: translate_llvm_dialect(&module)?,
        functions,
    })
}

fn runs_on_gpu(function: &HirFunction) -> bool {
    function.target.has_gpu_body() && launches_every_op(function)
}

/// Every `@gpu` function missing from `lowered`, with where it is declared.
fn refused_bodies(program: &HirProgram, lowered: &[(String, String)]) -> Vec<(String, Span)> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            HirItem::Function(function) if function.target.has_gpu_body() => Some(function),
            _ => None,
        })
        .filter(|function| !lowered.iter().any(|(name, _)| *name == function.name))
        .map(|function| (function.name.clone(), function.span))
        .collect()
}

/// Whether every operation in `function` has a parallel axis to launch over. An
/// operation's rank is its broadcast result's, so a rank-0 one needs every tensor
/// operand to be rank 0, and those come from the parameters: a body with no rank-0
/// tensor parameter or result computes nothing on the host.
fn launches_every_op(function: &HirFunction) -> bool {
    std::iter::once(&function.return_type)
        .chain(function.params.iter().map(|param| read_type(&param.ty)))
        .all(|ty| !matches!(ty, HirType::Tensor { shape, .. } if shape.is_empty()))
}

/// The CPU pipeline with its middle swapped: instead of sequential loops, each
/// `linalg` op becomes `scf.parallel` loops, tiled so the outer loop maps to blocks
/// and the inner one to threads, then a `gpu.launch` outlined into a kernel of its
/// own `gpu.module`. The kernels convert to the vendor dialect inside their
/// modules; `gpu-to-llvm` turns each launch into runtime calls on the host side.
///
/// `gpu-async-region` chains a body's launches on one stream with a single wait at
/// its end. Without it every launch creates a stream, waits for its kernel and
/// destroys the stream, so the host stalls between two kernels that need nothing
/// from it.
///
/// `lower-affine` is there for the index arithmetic `convert-parallel-loops-to-gpu`
/// writes as `affine.apply`, which the CPU path never produces. Serializing the
/// kernels is a separate run so a missing toolkit is told apart from a lowering
/// bug.
fn gpu_lowering_pipeline(target: &GpuTarget) -> String {
    format!(
        "builtin.module({BUFFERIZE},\
         func.func(convert-linalg-to-parallel-loops,\
         scf-parallel-loop-tiling{{parallel-loop-tile-sizes={TILE_SIZES} no-min-max-bounds=true}},\
         gpu-map-parallel-loops,convert-parallel-loops-to-gpu),\
         gpu-kernel-outlining,func.func(gpu-async-region),\
         {attach}{{chip={chip}}},\
         gpu.module({convert}),\
         lower-affine,{descent},gpu-to-llvm,reconcile-unrealized-casts)",
        attach = target.attach_target_pass(),
        chip = target.chip(),
        convert = target.kernel_conversion_pass(),
        descent = llvm_descent(DEVICE_MEMREF_TO_LLVM),
    )
}

fn run_pipeline(
    context: &Context,
    module: &mut Module<'_>,
    pipeline: &str,
) -> Result<(), melior::Error> {
    let manager = PassManager::new(context);
    parse_pass_pipeline(manager.as_operation_pass_manager(), pipeline)?;
    manager.run(module)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::bridge::tests::{program_with_tensor_operator, tensor};
    use crate::lower_for_link;
    use ast_types::BinaryOp;
    use neuro_hir::{HirExpr, HirExprKind, HirStmt, HirTarget, static_shape};

    fn nvidia() -> GpuTarget {
        GpuTarget::Nvidia {
            chip: "sm_80".to_string(),
        }
    }

    /// Every function in `program` marked `@gpu`.
    fn on_gpu(mut program: HirProgram) -> HirProgram {
        for item in &mut program.items {
            if let HirItem::Function(function) = item {
                function.target = HirTarget::Gpu;
            }
        }
        program
    }

    fn element_wise(shape: &[usize]) -> HirProgram {
        let ty = tensor(static_shape(shape));
        on_gpu(program_with_tensor_operator(
            BinaryOp::Add,
            ty.clone(),
            ty.clone(),
            ty,
        ))
    }

    fn host_matmul() -> HirProgram {
        program_with_tensor_operator(
            BinaryOp::MatMul,
            tensor(static_shape(&[2, 3])),
            tensor(static_shape(&[3, 4])),
            tensor(static_shape(&[2, 4])),
        )
    }

    fn matmul() -> HirProgram {
        on_gpu(host_matmul())
    }

    fn refused(program: &HirProgram) -> Vec<(String, Span)> {
        match lower_for_gpu(program, &nvidia()) {
            Err(MlirError::GpuBodiesNotLowered(functions)) => functions,
            other => panic!("expected the `@gpu` body refused, got {other:?}"),
        }
    }

    /// The host side is the launch and nothing of the computation: the arithmetic
    /// lives only in the embedded device object.
    fn assert_is_a_launcher(ir: &str) {
        assert!(
            ir.contains("define void @__neuro_mlir_f("),
            "expected the host symbol to be defined:\n{ir}"
        );
        assert!(
            ir.contains("@mgpuLaunchKernel") && ir.contains("@llvm.global_ctors"),
            "expected a kernel launch and a constructor loading the module:\n{ir}"
        );
        assert!(
            !ir.contains("gpu.launch") && !ir.contains("gpu.module") && !ir.contains("linalg."),
            "expected nothing above the llvm dialect to survive:\n{ir}"
        );
    }

    #[test]
    fn an_element_wise_body_becomes_a_ptx_kernel() {
        let bodies = lower_for_gpu(&element_wise(&[2, 3]), &nvidia())
            .expect("an element-wise body should lower to a kernel");

        let ir = &bodies.llvm_ir;
        assert_is_a_launcher(ir);
        assert!(
            ir.contains("@mgpuModuleLoadJIT"),
            "PTX is JIT loaded:\n{ir}"
        );
        assert!(ir.contains(".target sm_80"), "{ir}");
        assert!(
            ir.contains("add.rn.f32"),
            "expected the kernel's fadd:\n{ir}"
        );
        assert!(
            !ir.contains("fadd float"),
            "the host should not compute the body:\n{ir}"
        );
    }

    #[test]
    fn kernels_run_many_threads_a_block() {
        // Without tiling every block runs one thread, and the kernel says so.
        let ir = lower_for_gpu(&element_wise(&[64, 64]), &nvidia())
            .expect("the body should lower")
            .llvm_ir;

        assert!(ir.contains("%tid.x"), "expected per-thread indexing:\n{ir}");
        assert!(!ir.contains(".maxntid 1, 1, 1"), "{ir}");
    }

    #[test]
    fn a_matrix_product_becomes_a_fill_and_a_contraction_kernel() {
        let ir = lower_for_gpu(&matmul(), &nvidia())
            .expect("a matrix product should lower to kernels")
            .llvm_ir;

        assert_eq!(ir.matches("call void @mgpuLaunchKernel").count(), 2, "{ir}");
        assert!(
            ir.contains("fma.rn.f32") || ir.contains("mul.rn.f32"),
            "{ir}"
        );
    }

    #[test]
    fn a_rank_three_body_lowers() {
        // Tiling names two axes; a third must still map to the grid or a loop.
        let bodies = lower_for_gpu(&element_wise(&[2, 3, 4]), &nvidia())
            .expect("a rank-3 body should lower");
        assert_is_a_launcher(&bodies.llvm_ir);
    }

    #[test]
    fn the_symbols_match_the_cpu_path() {
        let gpu = lower_for_gpu(&matmul(), &nvidia()).expect("the GPU path should lower");
        let cpu = lower_for_link(&host_matmul()).expect("the CPU path should lower");

        assert_eq!(gpu.functions, cpu.functions);
        let signature = |ir: &str| {
            ir.lines()
                .find(|line| line.starts_with("define void @__neuro_mlir_f("))
                .map(str::to_string)
        };
        assert_eq!(signature(&gpu.llvm_ir), signature(&cpu.llvm_ir));
    }

    #[test]
    fn an_amd_target_lowers_through_rocdl() {
        // Assembly rather than a code object: linking one needs ROCm installed.
        let target = GpuTarget::Amd {
            chip: "gfx90a".to_string(),
        };
        let ir = lower_with_format(&element_wise(&[2, 3]), &target, "isa")
            .expect("an AMD target should lower")
            .llvm_ir;

        assert_is_a_launcher(&ir);
        assert!(ir.contains("amdgcn-amd-amdhsa--gfx90a"), "{ir}");
        assert!(
            ir.contains("v_add_f32"),
            "expected the kernel's fadd:\n{ir}"
        );
    }

    #[test]
    fn a_chip_name_cannot_reach_the_pipeline_text() {
        let target = GpuTarget::Nvidia {
            chip: "sm_80},func.func(canonicalize".to_string(),
        };
        let error = lower_for_gpu(&element_wise(&[2]), &target).expect_err("should refuse");
        assert!(matches!(error, MlirError::InvalidGpuChip(_)), "{error}");

        let empty = GpuTarget::Amd {
            chip: String::new(),
        };
        assert!(matches!(
            lower_for_gpu(&element_wise(&[2]), &empty),
            Err(MlirError::InvalidGpuChip(_))
        ));
    }

    #[test]
    fn a_chip_llvm_does_not_know_is_refused_before_it_can_crash_isel() {
        // An unknown AMD processor used to reach instruction selection and abort the compiler.
        for target in [
            GpuTarget::Amd {
                chip: "gfx9999".to_string(),
            },
            GpuTarget::Nvidia {
                chip: "sm_9999".to_string(),
            },
            // A real chip under the other vendor's dialect.
            GpuTarget::Amd {
                chip: "sm_80".to_string(),
            },
        ] {
            assert!(
                matches!(
                    lower_with_format(&element_wise(&[2]), &target, "isa"),
                    Err(MlirError::InvalidGpuChip(_))
                ),
                "{target:?}"
            );
        }
    }

    #[test]
    fn a_buffer_between_two_kernels_comes_from_the_callers_allocator() {
        // `(a + b) * b`: the sum lives only between the two launches.
        let ty = tensor(static_shape(&[37, 45]));
        let mut program = element_wise(&[37, 45]);
        let HirItem::Function(function) = &mut program.items[0] else {
            unreachable!("the fixture is one function");
        };
        let Some(HirStmt::Return {
            value: Some(sum), ..
        }) = function.body.pop()
        else {
            unreachable!("the fixture returns its operation");
        };
        let product = HirExpr::new(
            HirExprKind::Binary {
                op: BinaryOp::Multiply,
                left: Box::new(sum),
                right: Box::new(HirExpr::new(
                    HirExprKind::Variable("b".to_string()),
                    ty.clone(),
                    Span::new(0, 0),
                )),
            },
            ty,
            Span::new(0, 0),
        );
        function.body.push(HirStmt::Expr(product));

        let ir = lower_for_gpu(&program, &nvidia())
            .expect("a two-operation body should lower")
            .llvm_ir;

        assert_eq!(ir.matches("call void @mgpuLaunchKernel").count(), 2, "{ir}");
        assert!(
            ir.contains("call ptr @_mlir_memref_to_llvm_alloc(")
                && ir.contains("call void @_mlir_memref_to_llvm_free("),
            "expected the intermediate to go through the caller's allocator:\n{ir}"
        );
        assert!(
            !ir.contains("@malloc") && !ir.contains("@free("),
            "a host allocation would hand a kernel host memory:\n{ir}"
        );

        // Both launches queue on one stream and the host waits once, after the second:
        // a wait per launch stalls the host between kernels that need nothing from it.
        assert_eq!(ir.matches("call ptr @mgpuStreamCreate(").count(), 1, "{ir}");
        assert_eq!(
            ir.matches("call void @mgpuStreamSynchronize(").count(),
            1,
            "{ir}"
        );
        let last_launch = ir.rfind("call void @mgpuLaunchKernel").unwrap_or_default();
        let wait = ir
            .find("call void @mgpuStreamSynchronize(")
            .unwrap_or_default();
        let release = ir
            .find("call void @_mlir_memref_to_llvm_free(")
            .unwrap_or_default();
        assert!(
            last_launch < wait && wait < release,
            "the intermediate is released only once the kernel reading it is done:\n{ir}"
        );
    }

    #[test]
    fn a_rank_zero_body_is_refused() {
        // No parallel axis, so no kernel: the host would compute it against device
        // buffers, which `@gpu` forbids.
        let program = element_wise(&[]);
        let functions = refused(&program);
        assert_eq!(functions.len(), 1, "{functions:?}");
        assert_eq!(functions[0].0, "f");
    }

    #[test]
    fn an_integer_body_is_refused() {
        let ty = HirType::Tensor {
            element: Box::new(HirType::I32),
            shape: static_shape(&[2]),
            names: neuro_hir::AxisNames::default(),
        };
        let program = on_gpu(program_with_tensor_operator(
            BinaryOp::Add,
            ty.clone(),
            ty.clone(),
            ty,
        ));
        assert_eq!(refused(&program).len(), 1);
    }

    #[test]
    fn each_path_lowers_only_its_own_functions() {
        let mut program = element_wise(&[2, 3]);
        let HirItem::Function(gpu) = &program.items[0] else {
            unreachable!("the fixture is one function");
        };
        let mut host = gpu.clone();
        host.name = "g".to_string();
        host.target = HirTarget::Host;
        // A fallback's host copy is the LLVM backend's own body, not the CPU path's.
        let mut either = gpu.clone();
        either.name = "h".to_string();
        either.target = HirTarget::GpuOrHost;
        program.items.push(HirItem::Function(host));
        program.items.push(HirItem::Function(either));

        let gpu = lower_for_gpu(&program, &nvidia()).expect("the GPU path should lower");
        let cpu = lower_for_link(&program).expect("the CPU path should lower");
        assert_eq!(
            gpu.functions,
            [
                ("f".into(), "__neuro_mlir_f".into()),
                ("h".into(), "__neuro_mlir_h".into())
            ]
        );
        assert_eq!(cpu.functions, [("g".into(), "__neuro_mlir_g".into())]);
    }

    #[test]
    fn a_program_with_no_gpu_function_lowers_to_nothing() {
        let bodies = lower_for_gpu(&host_matmul(), &nvidia()).expect("nothing to refuse");
        assert!(bodies.functions.is_empty() && bodies.llvm_ir.is_empty());
    }
}
