use crate::{
    bridge::{BUFFERIZE, LinkableBodies, llvm_descent, translate_llvm_dialects},
    context::new_context,
    errors::MlirError,
    guards::{Overflow, Side},
    kernel::kernel_launchers,
    lower::build_linkable_module,
    schedule,
    tensor_arithmetic::read_type,
};

use std::cell::OnceCell;

use melior::{
    Context,
    ir::{BlockLike, Module, attribute::StringAttribute, operation::OperationLike},
    pass::PassManager,
    utility::parse_pass_pipeline,
};
use neuro_hir::{HirFunction, HirItem, HirProgram, HirTarget, HirType};
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

/// The chips LLVM 23 has a processor model for (`llc -march=nvptx64 -mcpu=help`, and
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
    "gfx1154", "gfx1170", "gfx1171", "gfx1172", "gfx1200", "gfx1201", "gfx1250", "gfx1251",
    "gfx1310",
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

/// How a function's parallel loops become blocks of threads: the tile sizes, read by
/// loop axis from the outermost, and the `gpu-map-parallel-loops` policy that lays the
/// tiled axes onto x, y and z.
///
/// One list fits one rank only, so each function gets the one its widest tensor needs,
/// and functions that need different ones lower as separate modules. A rank-N tiling
/// handed a loop of lower rank (a reduction's result) still launches, with fewer threads
/// a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tiling {
    tiles: &'static str,
    policy: &'static str,
}

/// The innermost axis on thread x, 32 wide, so a warp reads 128 contiguous bytes of a
/// row-major buffer, with 256 threads a block.
const VECTOR: Tiling = Tiling {
    tiles: "256",
    policy: "innermost-first",
};
const MATRIX: Tiling = Tiling {
    tiles: "8,32",
    policy: "innermost-first",
};
const VOLUME: Tiling = Tiling {
    tiles: "1,8,32",
    policy: "innermost-first",
};
/// The outermost axis on x, where the grid has no practical limit: what is left when an
/// outer axis is too long for grid y or z, and for rank 4 and up, whose innermost axis
/// no policy maps.
const OUTERMOST: Tiling = Tiling {
    tiles: "16,16",
    policy: "outermost-first",
};

/// Blocks a launch may have along grid y or z, on NVIDIA and AMD alike.
const MAX_GRID_YZ: usize = 65_535;

/// The tiling for `function`'s loops, from the longest extent each axis has over its
/// tensors, counted from the innermost: broadcasting aligns there, so no operation's
/// loop reaches further.
fn tiling(function: &HirFunction) -> Tiling {
    let mut extents: Vec<usize> = Vec::new();
    for shape in signature_shapes(function) {
        for (axis, extent) in shape.iter().rev().enumerate() {
            let extent = extent.unwrap_or(0);
            match extents.get_mut(axis) {
                Some(longest) => *longest = (*longest).max(extent),
                None => extents.push(extent),
            }
        }
    }
    let fits = |axis: usize, tile: usize| extents[axis].div_ceil(tile) <= MAX_GRID_YZ;
    match extents.len() {
        0 | 1 => VECTOR,
        2 if fits(1, 8) => MATRIX,
        3 if fits(1, 8) && fits(2, 1) => VOLUME,
        _ => OUTERMOST,
    }
}

/// The shapes of `function`'s tensor parameters and results.
fn signature_shapes(function: &HirFunction) -> Vec<&[Option<usize>]> {
    let results = match &function.return_type {
        HirType::Tuple(parts) => parts.iter().collect(),
        result => vec![result],
    };
    function
        .params
        .iter()
        .map(|param| read_type(&param.ty))
        .chain(results)
        .filter_map(|ty| match ty {
            HirType::Tensor { shape, .. } => Some(shape.as_slice()),
            _ => None,
        })
        .collect()
}

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
    overflow: Overflow,
) -> Result<LinkableBodies, MlirError> {
    lower_with_format(program, target, overflow, target.object_format())
}

/// [`lower_for_gpu`] with the device object format chosen by the caller, so a test
/// can stop an AMD target at assembly on a machine without ROCm.
pub(crate) fn lower_with_format(
    program: &HirProgram,
    target: &GpuTarget,
    overflow: Overflow,
    format: &str,
) -> Result<LinkableBodies, MlirError> {
    let chip = target.chip();
    if !target.known_chips().contains(&chip) {
        return Err(MlirError::InvalidGpuChip(chip.to_string()));
    }

    let context = new_context();
    // Asked once, and only of a program whose GPU code calls a math function.
    let probed = OnceCell::new();
    let math = || *probed.get_or_init(|| device_math(&context, target, format));
    let (module, (mut functions, mut guards)) =
        build_linkable_module(&context, program, (overflow, Side::Device), &runs_on_gpu)?;
    let calling_math = math_functions(&module, &functions);
    let without_math = !calling_math.is_empty() && !math();
    let admit = |function: &HirFunction| {
        runs_on_gpu(function) && !(without_math && calling_math.contains(&function.name))
    };
    if without_math {
        (functions, guards) =
            build_linkable_module(&context, program, (overflow, Side::Device), &admit)?.1;
    }
    let refused = refused_bodies(program, &functions);
    if !refused.is_empty() {
        return Err(MlirError::GpuBodiesNotLowered(refused));
    }
    let kernels = kernel_launchers(program, target, overflow, &math)
        .map_err(MlirError::KernelBodiesNotLowered)?;

    let mut tilings = Vec::new();
    for function in program.items.iter().filter_map(|item| match item {
        HirItem::Function(function) if functions.iter().any(|(name, _)| *name == function.name) => {
            Some(function)
        }
        _ => None,
    }) {
        let tiling = tiling(function);
        if !tilings.contains(&tiling) {
            tilings.push(tiling);
        }
    }
    let mut lowered = Vec::with_capacity(tilings.len() + 1);
    for tiling in tilings {
        let (mut module, _) =
            build_linkable_module(&context, program, (overflow, Side::Device), &|function| {
                admit(function) && self::tiling(function) == tiling
            })?;
        schedule::apply(&context, &module, Side::Device)?;
        lower_module(
            &context,
            &mut module,
            &gpu_lowering_pipeline(target, tiling),
            format,
        )?;
        lowered.push(module);
    }
    // Kernels take a pipeline of their own: their buffers are the caller's from the start,
    // so there is nothing to bufferize, and the deallocation pass that follows
    // bufferization refuses the loops a kernel body branches through.
    if !kernels.functions.is_empty() {
        let mut kernel_module =
            Module::parse(&context, &kernels.text).ok_or(MlirError::ModuleVerificationFailed)?;
        if !kernel_module.as_operation().verify() {
            return Err(MlirError::ModuleVerificationFailed);
        }
        lower_module(
            &context,
            &mut kernel_module,
            &kernel_lowering_pipeline(target),
            format,
        )?;
        lowered.push(kernel_module);
        functions.extend(kernels.functions);
        guards.extend(kernels.guards);
    }
    if lowered.is_empty() {
        return Ok(LinkableBodies {
            llvm_ir: String::new(),
            functions,
            guards,
        });
    }

    Ok(LinkableBodies {
        llvm_ir: translate_llvm_dialects(&lowered)?,
        functions,
        guards,
    })
}

/// A GPU module calling one math function, which the vendor's conversion turns into a call
/// into its device math library: libdevice, found through the CUDA toolkit, on NVIDIA, and
/// ROCm's device libraries on AMD.
const MATH_PROBE: &str = r#"module attributes {gpu.container_module} {
  gpu.module @probe {
    gpu.func @probe(%a: memref<1xf32>) kernel {
      %c0 = arith.constant 0 : index
      %x = memref.load %a[%c0] : memref<1xf32>
      %y = math.exp %x : f32
      memref.store %y, %a[%c0] : memref<1xf32>
      gpu.return
    }
  }
}"#;

/// Whether this compile can link the device math library a math function calls on
/// `target`. Without the CUDA toolkit, `gpu-module-to-binary` leaves libdevice's functions
/// as `.extern` declarations in the PTX, which the driver then cannot load, along with every
/// other kernel of the program; a missing ROCm library fails the AMD link, or leaves the
/// `__ocml_` call in assembly.
pub(crate) fn device_math(context: &Context, target: &GpuTarget, format: &str) -> bool {
    let Some(mut module) = Module::parse(context, MATH_PROBE) else {
        return false;
    };
    let pipeline = format!(
        "builtin.module({attach}{{chip={chip}}},gpu.module({convert}),reconcile-unrealized-casts)",
        attach = target.attach_target_pass(),
        chip = target.chip(),
        convert = target.kernel_conversion_pass(),
    );
    // A missing library is an answer here, not an error to print: handled, so MLIR's
    // default handler never sees it.
    let quiet = context.attach_diagnostic_handler(|_| true);
    let lowered = lower_module(context, &mut module, &pipeline, format);
    context.detach_diagnostic_handler(quiet);
    if lowered.is_err() {
        return false;
    }
    let object = module.as_operation().to_string();
    !object.contains(".extern") && !object.contains("__ocml_")
}

/// The functions of `functions` whose definition in `module` calls into the vendor's device
/// math library: a math function, or a float `%`, which the conversion turns into its exact
/// `fmod` (`__nv_fmodf`, `__ocml_fmod_f32`) rather than a division the hardware rounds.
fn math_functions(module: &Module<'_>, functions: &[(String, String)]) -> Vec<String> {
    let mut calling = Vec::new();
    let mut next = module.body().first_operation();
    while let Some(operation) = next {
        let symbol = operation
            .attribute("sym_name")
            .ok()
            .and_then(|name| StringAttribute::try_from(name).ok())
            .map(|name| name.value().to_string());
        if let Some(symbol) = symbol
            && {
                let text = operation.to_string();
                text.contains("math.") || text.contains("arith.remf")
            }
            && let Some((function, _)) = functions.iter().find(|(_, linked)| *linked == symbol)
        {
            calling.push(function.clone());
        }
        next = operation.next_in_block();
    }
    calling
}

/// Run `pipeline` over `module`, then embed its kernels as `format` device objects.
fn lower_module(
    context: &Context,
    module: &mut Module<'_>,
    pipeline: &str,
    format: &str,
) -> Result<(), MlirError> {
    run_pipeline(context, module, pipeline).map_err(|_| MlirError::PassPipelineFailed)?;
    run_pipeline(
        context,
        module,
        &format!("builtin.module(gpu-module-to-binary{{format={format}}})"),
    )
    .map_err(|_| MlirError::GpuSerializationFailed)
}

/// Whether `function` is a `@gpu` one whose whole-tensor body the `linalg` path maps
/// onto a grid. A `@kernel` body is the per-thread code itself, lowered apart.
fn is_gpu_body(function: &HirFunction) -> bool {
    matches!(function.target, HirTarget::Gpu | HirTarget::GpuOrHost)
}

/// A `FollowsOperands` function joins the `@gpu` ones, but leniently: one the `linalg`
/// path cannot take keeps its host body alone, which is no error.
fn runs_on_gpu(function: &HirFunction) -> bool {
    (is_gpu_body(function) || function.target == HirTarget::FollowsOperands)
        && launches_every_op(function)
        && exact_on_gpu(function)
}

/// Whether a GPU computes `function` with the host's bits: no half-precision tensor, whose
/// conversions not every chip has, and no `bool` tensor.
fn exact_on_gpu(function: &HirFunction) -> bool {
    let half = |ty: &HirType| {
        matches!(
            read_type(ty),
            HirType::Tensor { element, .. }
                if matches!(**element, HirType::F16 | HirType::BF16 | HirType::Bool)
        )
    };
    !std::iter::once(&function.return_type)
        .chain(function.params.iter().map(|param| &param.ty))
        .any(half)
}

/// Every `@gpu` function missing from `lowered`, with where it is declared.
fn refused_bodies(program: &HirProgram, lowered: &[(String, String)]) -> Vec<(String, Span)> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            HirItem::Function(function) if is_gpu_body(function) => Some(function),
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
/// `linalg` op becomes `scf.parallel` loops, tiled by `tiling` so the outer loop maps
/// to blocks and the inner one to threads, then a `gpu.launch` outlined into a kernel
/// of its own `gpu.module`. The kernels convert to the vendor dialect inside their
/// modules; `gpu-to-llvm` turns each launch into runtime calls on the host side.
///
/// `gpu-async-region` chains a body's launches on one stream with a single wait at
/// its end. Without it every launch creates a stream, waits for its kernel and
/// destroys the stream, so the host stalls between two kernels that need nothing
/// from it.
///
/// A scheduled contraction (`schedule`) arrives as an `scf.forall` over each thread's
/// register block, which becomes one more parallel loop to map, its vector transfers
/// unrolled first. Inside each kernel module its tiles' subviews become index arithmetic
/// and its vectors convert before the vendor conversion, which handles neither.
///
/// `lower-affine` is there for the index arithmetic `convert-parallel-loops-to-gpu`
/// writes as `affine.apply`, which the CPU path never produces. Serializing the
/// kernels is a separate run so a missing toolkit is told apart from a lowering
/// bug.
fn gpu_lowering_pipeline(target: &GpuTarget, tiling: Tiling) -> String {
    format!(
        "builtin.module({BUFFERIZE},\
         func.func(scf-forall-to-parallel,convert-linalg-to-parallel-loops,\
         convert-vector-to-scf{{full-unroll=true}},\
         scf-parallel-loop-tiling{{parallel-loop-tile-sizes={tiles} no-min-max-bounds=true}},\
         gpu-map-parallel-loops{{mapping-policy={policy}}},convert-parallel-loops-to-gpu),\
         gpu-kernel-outlining,func.func(gpu-async-region),\
         {attach}{{chip={chip}}},\
         gpu.module(expand-strided-metadata,lower-affine,convert-vector-to-llvm,{convert}),\
         lower-affine,{descent},gpu-to-llvm,reconcile-unrealized-casts)",
        tiles = tiling.tiles,
        policy = tiling.policy,
        attach = target.attach_target_pass(),
        chip = target.chip(),
        convert = target.kernel_conversion_pass(),
        descent = llvm_descent(DEVICE_MEMREF_TO_LLVM),
    )
}

/// A `@kernel` launcher's descent: the `gpu.launch` outlined into a kernel of its own
/// module, converted for the vendor, and the host side turned into runtime calls, as in
/// [`gpu_lowering_pipeline`] after its `linalg` mapping.
fn kernel_lowering_pipeline(target: &GpuTarget) -> String {
    format!(
        "builtin.module(gpu-kernel-outlining,func.func(gpu-async-region),\
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
        let mut program = program_with_tensor_operator(
            BinaryOp::MatMul,
            tensor(static_shape(&[2, 3])),
            tensor(static_shape(&[3, 4])),
            tensor(static_shape(&[2, 4])),
        );
        for item in &mut program.items {
            if let HirItem::Function(function) = item {
                function.target = HirTarget::Host;
            }
        }
        program
    }

    fn matmul() -> HirProgram {
        on_gpu(host_matmul())
    }

    fn refused(program: &HirProgram) -> Vec<(String, Span)> {
        match lower_for_gpu(program, &nvidia(), crate::Overflow::Checked) {
            Err(MlirError::GpuBodiesNotLowered(functions)) => functions,
            other => panic!("expected the `@gpu` body refused, got {other:?}"),
        }
    }

    /// The host side is the launch and nothing of the computation: the arithmetic
    /// lives only in the embedded device object.
    fn assert_is_a_launcher(ir: &str) {
        assert!(
            ir.contains("define void @__neuro_gpu_f("),
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
        let bodies = lower_for_gpu(&element_wise(&[2, 3]), &nvidia(), crate::Overflow::Checked)
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
        let ir = lower_for_gpu(
            &element_wise(&[64, 64]),
            &nvidia(),
            crate::Overflow::Checked,
        )
        .expect("the body should lower")
        .llvm_ir;

        assert!(ir.contains("%tid.x"), "expected per-thread indexing:\n{ir}");
        assert!(!ir.contains(".maxntid 1, 1, 1"), "{ir}");
    }

    #[test]
    fn each_rank_gets_the_tiling_that_fits_it() {
        let of = |shape: &[usize]| {
            let program = element_wise(shape);
            let HirItem::Function(function) = &program.items[0] else {
                unreachable!("the fixture is one function");
            };
            tiling(function)
        };
        assert_eq!(of(&[1 << 24]), VECTOR);
        assert_eq!(of(&[64, 64]), MATRIX);
        assert_eq!(of(&[8, 64, 64]), VOLUME);
        assert_eq!(of(&[2, 3, 4, 5]), OUTERMOST);
        // Rows past what grid y holds in blocks of 8 go to grid x instead.
        assert_eq!(of(&[2_000_000, 4]), OUTERMOST);
        assert_eq!(of(&[70_000, 4, 4]), OUTERMOST);
    }

    #[test]
    fn a_warp_walks_the_innermost_axis_and_a_wide_tensor_launches() {
        // With the outermost axis on x, 2,000,000 columns in blocks of 16 asked grid y
        // for 125,000 blocks, past its limit, and the launch failed.
        let ir = lower_for_gpu(
            &element_wise(&[4, 2_000_000]),
            &nvidia(),
            crate::Overflow::Checked,
        )
        .expect("a wide body should lower")
        .llvm_ir;

        // Grid (62500, 1, 1), blocks of (32, 8, 1).
        assert!(
            ir.contains("i64 62500, i64 1, i64 1, i64 32, i64 8, i64 1"),
            "{ir}"
        );
    }

    #[test]
    fn functions_of_different_ranks_lower_together() {
        let mut program = element_wise(&[2, 3]);
        let HirItem::Function(mut vector) = element_wise(&[5]).items.remove(0) else {
            unreachable!("the fixture is one function");
        };
        vector.name = "g".to_string();
        program.items.push(HirItem::Function(vector));

        let bodies = lower_for_gpu(&program, &nvidia(), crate::Overflow::Checked)
            .expect("both bodies should lower");
        assert_eq!(bodies.functions.len(), 2, "{:?}", bodies.functions);
        assert!(
            bodies.llvm_ir.contains("define void @__neuro_gpu_f(")
                && bodies.llvm_ir.contains("define void @__neuro_gpu_g("),
            "{}",
            bodies.llvm_ir
        );
    }

    #[test]
    fn a_matrix_product_becomes_a_fill_and_a_contraction_kernel() {
        let ir = lower_for_gpu(&matmul(), &nvidia(), crate::Overflow::Checked)
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
        let bodies = lower_for_gpu(
            &element_wise(&[2, 3, 4]),
            &nvidia(),
            crate::Overflow::Checked,
        )
        .expect("a rank-3 body should lower");
        assert_is_a_launcher(&bodies.llvm_ir);
    }

    #[test]
    fn the_symbols_match_the_cpu_path() {
        let gpu = lower_for_gpu(&matmul(), &nvidia(), crate::Overflow::Checked)
            .expect("the GPU path should lower");
        let mut fallback = matmul();
        if let HirItem::Function(function) = &mut fallback.items[0] {
            function.target = HirTarget::GpuOrHost;
        }
        let cpu =
            lower_for_link(&fallback, crate::Overflow::Checked).expect("the CPU path should lower");

        assert_eq!(gpu.functions, [("f".into(), "__neuro_gpu_f".into())]);
        assert_eq!(cpu.functions, [("f".into(), "__neuro_mlir_f".into())]);
        let signature = |ir: &str, symbol: &str| {
            ir.lines()
                .find(|line| line.starts_with(&format!("define void @{symbol}(")))
                // The host body also promises its buffers do not alias.
                .map(|line| line.replacen(symbol, "f", 1).replace(" noalias", ""))
        };
        assert_eq!(
            signature(&gpu.llvm_ir, "__neuro_gpu_f"),
            signature(&cpu.llvm_ir, "__neuro_mlir_f")
        );
    }

    #[test]
    fn an_amd_target_lowers_through_rocdl() {
        // Assembly rather than a code object: linking one needs ROCm installed.
        let target = GpuTarget::Amd {
            chip: "gfx90a".to_string(),
        };
        let ir = lower_with_format(
            &element_wise(&[2, 3]),
            &target,
            crate::Overflow::Checked,
            "isa",
        )
        .expect("an AMD target should lower")
        .llvm_ir;

        assert_is_a_launcher(&ir);
        assert!(ir.contains("amdgcn-amd-amdhsa-unknown-gfx90a"), "{ir}");
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
        let error = lower_for_gpu(&element_wise(&[2]), &target, crate::Overflow::Checked)
            .expect_err("should refuse");
        assert!(matches!(error, MlirError::InvalidGpuChip(_)), "{error}");

        let empty = GpuTarget::Amd {
            chip: String::new(),
        };
        assert!(matches!(
            lower_for_gpu(&element_wise(&[2]), &empty, crate::Overflow::Checked),
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
                    lower_with_format(
                        &element_wise(&[2]),
                        &target,
                        crate::Overflow::Checked,
                        "isa"
                    ),
                    Err(MlirError::InvalidGpuChip(_))
                ),
                "{target:?}"
            );
        }
    }

    /// `program`'s one function with its result multiplied by `b`, of type `ty`.
    fn times_b(mut program: HirProgram, ty: HirType) -> HirProgram {
        let HirItem::Function(function) = &mut program.items[0] else {
            unreachable!("the fixture is one function");
        };
        let Some(HirStmt::Return {
            value: Some(result),
            ..
        }) = function.body.pop()
        else {
            unreachable!("the fixture returns its operation");
        };
        let product = HirExpr::new(
            HirExprKind::Binary {
                op: BinaryOp::Multiply,
                left: Box::new(result),
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
        program
    }

    #[test]
    fn element_wise_operations_fuse_into_one_kernel() {
        // `(a + b) * b`: the sum is computed where it is used and never stored.
        let ty = tensor(static_shape(&[37, 45]));
        let ir = lower_for_gpu(
            &times_b(element_wise(&[37, 45]), ty),
            &nvidia(),
            crate::Overflow::Checked,
        )
        .expect("a two-operation body should lower")
        .llvm_ir;

        assert_eq!(ir.matches("call void @mgpuLaunchKernel").count(), 1, "{ir}");
        assert!(!ir.contains("@_mlir_memref_to_llvm_alloc("), "{ir}");
    }

    #[test]
    fn a_buffer_between_two_kernels_comes_from_the_callers_allocator() {
        // `(a @ b) * b`: a contraction does not fuse into its consumer, so the product
        // lives only between the launches.
        let ty = tensor(static_shape(&[37, 37]));
        let program = times_b(
            on_gpu(program_with_tensor_operator(
                BinaryOp::MatMul,
                ty.clone(),
                ty.clone(),
                ty.clone(),
            )),
            ty,
        );

        let ir = lower_for_gpu(&program, &nvidia(), crate::Overflow::Checked)
            .expect("a two-operation body should lower")
            .llvm_ir;

        // The fill, the contraction and the product.
        assert_eq!(ir.matches("call void @mgpuLaunchKernel").count(), 3, "{ir}");
        assert!(
            ir.contains("call ptr @_mlir_memref_to_llvm_alloc(")
                && ir.contains("call void @_mlir_memref_to_llvm_free("),
            "expected the intermediate to go through the caller's allocator:\n{ir}"
        );
        assert!(
            !ir.contains("@malloc") && !ir.contains("@free("),
            "a host allocation would hand a kernel host memory:\n{ir}"
        );

        // Every launch queues on one stream and the host waits once, after the last:
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
    fn an_integer_body_becomes_a_kernel_with_its_checks() {
        let ty = HirType::Tensor {
            element: Box::new(HirType::I32),
            shape: static_shape(&[2]),
            names: neuro_hir::AxisNames::default(),
        };
        let program = on_gpu(program_with_tensor_operator(
            BinaryOp::Divide,
            ty.clone(),
            ty.clone(),
            ty,
        ));
        let bodies = lower_for_gpu(&program, &nvidia(), crate::Overflow::Checked)
            .expect("an integer `@gpu` body lowers");
        assert_is_a_launcher(&bodies.llvm_ir);
        assert_eq!(bodies.guards.len(), 1, "{:?}", bodies.guards);
        // The status word is one more descriptor before the result's.
        assert!(
            bodies.llvm_ir.contains(
                "define void @__neuro_gpu_f(ptr %0, ptr %1, i64 %2, i64 %3, i64 %4, ptr %5, ptr %6, i64 %7, i64 %8, i64 %9, ptr %10, ptr %11, i64 %12, i64 %13, i64 %14, ptr %15,"
            ),
            "{}",
            bodies.llvm_ir
        );
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
        // A host function is the LLVM backend's, its tensor operations outlined; a fallback's
        // host copy is the CPU path's.
        let mut either = gpu.clone();
        either.name = "h".to_string();
        either.target = HirTarget::GpuOrHost;
        program.items.push(HirItem::Function(host));
        program.items.push(HirItem::Function(either));

        let gpu = lower_for_gpu(&program, &nvidia(), crate::Overflow::Checked)
            .expect("the GPU path should lower");
        let cpu =
            lower_for_link(&program, crate::Overflow::Checked).expect("the CPU path should lower");
        assert_eq!(
            gpu.functions,
            [
                ("f".into(), "__neuro_gpu_f".into()),
                ("h".into(), "__neuro_gpu_h".into())
            ]
        );
        assert_eq!(cpu.functions, [("h".into(), "__neuro_mlir_h".into())]);
    }

    #[test]
    fn a_program_with_no_gpu_function_lowers_to_nothing() {
        let bodies = lower_for_gpu(&host_matmul(), &nvidia(), crate::Overflow::Checked)
            .expect("nothing to refuse");
        assert!(bodies.functions.is_empty() && bodies.llvm_ir.is_empty());
    }
}
