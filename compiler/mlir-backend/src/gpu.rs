use crate::{
    bridge::{translate_llvm_dialect, LinkableBodies, BUFFERIZE, LLVM_DESCENT},
    context::new_context,
    errors::MlirError,
    lower::build_linkable_module,
};

use melior::{ir::Module, pass::PassManager, utility::parse_pass_pipeline, Context};
use neuro_hir::HirProgram;

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

impl GpuTarget {
    fn chip(&self) -> &str {
        match self {
            GpuTarget::Nvidia { chip } | GpuTarget::Amd { chip } => chip,
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

/// Lower the bodies [`lower_for_link`](crate::lower_for_link) would link into
/// GPU kernels for `target`, with host functions that launch them.
///
/// Each `(function, symbol)` pair and each symbol's signature are exactly
/// `lower_for_link`'s: the host side of a symbol takes the same exploded
/// descriptors and out-param, so the LLVM backend's wrapper serves either. Its
/// body launches one kernel per `linalg` operation through MLIR's GPU runtime ABI
/// (`mgpuModuleLoad` / `mgpuModuleLoadJIT`, `mgpuLaunchKernel`, the `mgpuStream*`
/// family), which the returned IR declares and nothing here defines. The kernels
/// are embedded in the IR as device objects and loaded by a global constructor.
///
/// # Errors
///
/// [`MlirError::InvalidGpuChip`] for a chip name that is not letters, digits and
/// `_`. [`MlirError::GpuSerializationFailed`] when a device object cannot be
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
    if chip.is_empty() || !chip.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(MlirError::InvalidGpuChip(chip.to_string()));
    }

    let context = new_context();
    let (mut module, functions) = build_linkable_module(&context, program)?;
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

/// The CPU pipeline with its middle swapped: instead of sequential loops, each
/// `linalg` op becomes `scf.parallel` loops, tiled so the outer loop maps to blocks
/// and the inner one to threads, then a `gpu.launch` outlined into a kernel of its
/// own `gpu.module`. The kernels convert to the vendor dialect inside their
/// modules; `gpu-to-llvm` turns each launch into runtime calls on the host side.
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
         gpu-kernel-outlining,\
         {attach}{{chip={chip}}},\
         gpu.module({convert}),\
         lower-affine,{LLVM_DESCENT},gpu-to-llvm,reconcile-unrealized-casts)",
        attach = target.attach_target_pass(),
        chip = target.chip(),
        convert = target.kernel_conversion_pass(),
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
    use neuro_hir::{static_shape, HirType};

    fn nvidia() -> GpuTarget {
        GpuTarget::Nvidia {
            chip: "sm_80".to_string(),
        }
    }

    fn element_wise(shape: &[usize]) -> neuro_hir::HirProgram {
        let ty = tensor(static_shape(shape));
        program_with_tensor_operator(BinaryOp::Add, ty.clone(), ty.clone(), ty)
    }

    fn matmul() -> neuro_hir::HirProgram {
        program_with_tensor_operator(
            BinaryOp::MatMul,
            tensor(static_shape(&[2, 3])),
            tensor(static_shape(&[3, 4])),
            tensor(static_shape(&[2, 4])),
        )
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
        let program = matmul();
        let gpu = lower_for_gpu(&program, &nvidia()).expect("the GPU path should lower");
        let cpu = lower_for_link(&program).expect("the CPU path should lower");

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
    fn an_integer_body_launches_nothing() {
        let ty = HirType::Tensor {
            element: Box::new(HirType::I32),
            shape: static_shape(&[2]),
            names: neuro_hir::AxisNames::default(),
        };
        let program = program_with_tensor_operator(BinaryOp::Add, ty.clone(), ty.clone(), ty);
        let bodies = lower_for_gpu(&program, &nvidia()).expect("the program should still lower");

        assert!(bodies.functions.is_empty());
        assert!(bodies.llvm_ir.is_empty());
    }
}
