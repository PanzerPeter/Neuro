//! `@kernel` functions as GPU launchers.
//!
//! A `@gpu` body is whole-tensor arithmetic the `linalg` path maps onto a grid for it; a
//! `@kernel` body is the per-thread code itself, so it is lowered statement by statement
//! into the region of a `gpu.launch`, which the GPU pipeline outlines into a kernel of its
//! own. The launcher is written as MLIR text and parsed: `gpu.launch` takes a dozen
//! segmented operands and a region with twelve arguments, which the text states plainly
//! and a builder would have to reassemble by hand.
//!
//! This is device code the LLVM backend cannot emit, so it is not the second copy of the
//! host's scalar codegen that the rest of this crate refuses to grow.

mod body;

use neuro_hir::{HirFunction, HirItem, HirProgram, HirTarget, HirType};

use crate::{errors::KernelRefusal, gpu::GpuTarget, lower::LINKED_SYMBOL_PREFIX};

use body::{BodyEmitter, Refused, memref_type, scalar_type};

/// How a body stops a thread that broke a runtime rule (an index past an extent, a zero
/// divisor). NVIDIA lowers `cf.assert` to its device assertion, which prints the message
/// before the kernel fails; ROCDL has no lowering for it, so AMD traps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuardStyle {
    Assert,
    Trap,
}

impl GuardStyle {
    fn for_target(target: &GpuTarget) -> Self {
        match target {
            GpuTarget::Nvidia { .. } => GuardStyle::Assert,
            GpuTarget::Amd { .. } => GuardStyle::Trap,
        }
    }
}

/// The launchers of a program's kernels: one MLIR module in text, and the `(function,
/// symbol)` pairs it defines.
pub(crate) struct KernelLaunchers {
    pub(crate) text: String,
    pub(crate) functions: Vec<(String, String)>,
}

/// Every `@kernel` function in `program` as a `func.func` that launches its body, in MLIR
/// text, with the `(function, symbol)` pairs it defines. Each symbol takes the tensors as
/// exploded `memref` descriptors and the scalars as themselves, in declaration order, and
/// returns nothing: what a kernel computes is what it writes through its `&mut` tensors.
///
/// # Errors
///
/// A body construct the lowering does not cover, one [`KernelRefusal`] per function.
pub(crate) fn kernel_launchers(
    program: &HirProgram,
    target: &GpuTarget,
) -> Result<KernelLaunchers, Vec<KernelRefusal>> {
    let guard = GuardStyle::for_target(target);
    let mut text = String::from("module {\n");
    let mut functions = Vec::new();
    let mut refusals = Vec::new();
    for item in &program.items {
        let HirItem::Function(function) = item else {
            continue;
        };
        let HirTarget::Kernel { threads } = function.target else {
            continue;
        };
        let symbol = format!("{LINKED_SYMBOL_PREFIX}{}", function.name);
        match launcher(function, &symbol, threads, guard) {
            Ok(launcher) => {
                text.push_str(&launcher);
                functions.push((function.name.clone(), symbol));
            }
            Err(refused) => refusals.push(KernelRefusal {
                function: function.name.clone(),
                span: refused.span,
                what: refused.what,
            }),
        }
    }
    text.push_str("}\n");
    if !refusals.is_empty() {
        return Err(refusals);
    }
    Ok(KernelLaunchers { text, functions })
}

/// One kernel's launcher: the grid sizes, then a `gpu.launch` whose region is the body.
fn launcher(
    function: &HirFunction,
    symbol: &str,
    threads: [u32; 3],
    guard: GuardStyle,
) -> Result<String, Refused> {
    let mut params = Vec::with_capacity(function.params.len());
    for (index, param) in function.params.iter().enumerate() {
        let ty = match &param.ty {
            HirType::Reference { inner, .. } => memref_type(inner),
            other => scalar_type(other).map(str::to_string),
        }
        .ok_or_else(|| Refused::new(param.span, "a parameter of this type"))?;
        params.push(format!("%arg{index}: {ty}"));
    }
    let Some(extents) = grid_extents(function) else {
        return Err(Refused::new(
            function.span,
            "a kernel without a `&mut` tensor of static shape",
        ));
    };

    let mut text = format!("  func.func @\"{symbol}\"({}) {{\n", params.join(", "));
    let blocks = grid_blocks(&extents, threads);
    // A grid tensor with no elements has no threads to run, and a launch of zero blocks
    // is an error on every GPU.
    if blocks.contains(&0) {
        text.push_str("    return\n  }\n");
        return Ok(text);
    }
    let region = BodyEmitter::new(function, guard, threads, extents).emit()?;
    for (axis, (count, per_block)) in blocks.iter().zip(threads).enumerate() {
        text.push_str(&format!(
            "    %grid{axis} = arith.constant {count} : index\n    %block{axis} = arith.constant {per_block} : index\n"
        ));
    }
    text.push_str(
        "    gpu.launch blocks(%bx, %by, %bz) in (%gx = %grid0, %gy = %grid1, %gz = %grid2) \
         threads(%tx, %ty, %tz) in (%sx = %block0, %sy = %block1, %sz = %block2) {\n",
    );
    text.push_str(&region);
    text.push_str("    }\n    return\n  }\n");
    Ok(text)
}

/// The extents of the first `&mut` tensor parameter, the tensor the grid covers.
fn grid_extents(function: &HirFunction) -> Option<Vec<usize>> {
    function.params.iter().find_map(|param| match &param.ty {
        HirType::Reference {
            inner,
            mutable: true,
        } => match inner.as_ref() {
            HirType::Tensor { shape, .. } => shape.iter().copied().collect(),
            _ => None,
        },
        _ => None,
    })
}

/// Blocks along each grid axis: enough to give every element of the grid tensor a thread,
/// the last one overhanging where `threads` does not divide the extent. An axis past the
/// tensor's rank has one block.
fn grid_blocks(extents: &[usize], threads: [u32; 3]) -> [u64; 3] {
    let mut blocks = [1u64; 3];
    for ((count, &extent), per_block) in blocks.iter_mut().zip(extents).zip(threads) {
        *count = (extent as u64).div_ceil(u64::from(per_block.max(1)));
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{MlirError, gpu::lower_with_format};

    const KERNEL: &str = r#"
@kernel(threads: [16, 16])
func add_relu(a: &Tensor<f32, [37, 45]>, s: f32, out: &mut Tensor<f32, [37, 45]>) {
    val row = thread_id.x
    val col = thread_id.y
    if row < 37 && col < 45 {
        mut sum = 0.0f32
        for k in 0..3 {
            sum += a[row, col] * s
        }
        out[row, col] = if sum > 0.0 { sum } else { 0.0 }
    }
}

func main() -> i32 {
    val a: Tensor<f32, [37, 45]> = Tensor::ones()
    mut r: Tensor<f32, [37, 45]> = Tensor::zeros()
    add_relu(&a, 2.0f32, &mut r)
    return 0
}
"#;

    fn program(source: &str) -> HirProgram {
        let ast = syntax_parsing::parse(source).expect("the kernel parses");
        hir_lowering::lower_program(&ast).expect("the kernel lowers to HIR")
    }

    fn nvidia() -> GpuTarget {
        GpuTarget::Nvidia {
            chip: "sm_80".to_string(),
        }
    }

    #[test]
    fn the_grid_rounds_each_axis_up_to_whole_blocks() {
        assert_eq!(grid_blocks(&[37, 45], [16, 16, 1]), [3, 3, 1]);
        assert_eq!(grid_blocks(&[32], [16, 1, 1]), [2, 1, 1]);
        assert_eq!(grid_blocks(&[0, 4], [16, 16, 1]), [0, 1, 1]);
        assert_eq!(grid_blocks(&[5, 6, 7], [2, 2, 2]), [3, 3, 4]);
    }

    #[test]
    fn a_kernel_launches_over_its_grid_behind_a_descriptor_signature() {
        let KernelLaunchers { text, functions } =
            kernel_launchers(&program(KERNEL), &nvidia()).expect("the body lowers");
        assert_eq!(
            functions,
            vec![("add_relu".to_string(), "__neuro_mlir_add_relu".to_string())]
        );
        assert!(
            text.contains("%grid0 = arith.constant 3 : index")
                && text.contains("%grid1 = arith.constant 3 : index")
                && text.contains("%block0 = arith.constant 16 : index"),
            "37 x 45 in 16 x 16 blocks is a 3 x 3 grid:\n{text}"
        );

        let ir = lower_with_format(&program(KERNEL), &nvidia(), "isa")
            .expect("the kernel lowers for NVIDIA")
            .llvm_ir;
        // Two exploded rank-2 descriptors (two pointers and five integers each) around
        // the scalar, and nothing returned.
        assert!(
            ir.contains(
                "define void @__neuro_mlir_add_relu(ptr %0, ptr %1, i64 %2, i64 %3, i64 %4, i64 %5, i64 %6, float %7, ptr %8,"
            ),
            "{ir}"
        );
        assert!(ir.contains("@mgpuLaunchKernel"), "{ir}");
        // LLVM may fold a check the body's own guard already implies, so the launcher
        // text is where every index is seen to be checked.
        assert_eq!(
            text.matches("cf.assert").count(),
            4,
            "each of the two indices of the read and of the write is bounds-checked:\n{text}"
        );
    }

    #[test]
    fn an_amd_kernel_traps_instead_of_asserting() {
        let target = GpuTarget::Amd {
            chip: "gfx90a".to_string(),
        };
        let text = kernel_launchers(&program(KERNEL), &target)
            .expect("the body lowers")
            .text;
        assert!(
            text.contains("llvm.intr.trap") && !text.contains("cf.assert"),
            "{text}"
        );
        let ir = lower_with_format(&program(KERNEL), &target, "isa")
            .expect("the kernel lowers for AMD")
            .llvm_ir;
        assert!(ir.contains("amdgcn-amd-amdhsa--gfx90a"), "{ir}");
    }

    #[test]
    fn a_body_construct_the_lowering_lacks_is_refused_where_it_is() {
        let source = KERNEL
            .replace(
                "out[row, col] = if sum > 0.0 { sum } else { 0.0 }",
                "out[row, col] = relu(sum)",
            )
            .replace(
                "func main()",
                "func relu(x: f32) -> f32 {\n    if x > 0.0 { x } else { 0.0 }\n}\n\nfunc main()",
            );
        let Err(MlirError::KernelBodiesNotLowered(refusals)) =
            lower_with_format(&program(&source), &nvidia(), "isa")
        else {
            panic!("expected the body refused");
        };
        let [refusal] = refusals.as_slice() else {
            panic!("expected one refusal, got {refusals:?}");
        };
        assert_eq!(refusal.function, "add_relu");
        assert_eq!(
            refusal.span.start,
            source.find("relu(sum)").expect("the call in the source")
        );
    }

    const PARTITION: &str = r#"
@kernel(threads: [4])
func split<M>(a: Tensor<f32, [6]>, out: KernelOut<Tensor<f32, [6]>>, wide: KernelOut<Tensor<i64, [M]>>) {
    out.partition(|base, slice| {
        for i in 0u64..slice.len() {
            slice[i] = a.flat(base + i)
        }
    })
    wide.partition(|base, s| {
        s[1] = base as i64
    })
}

func main() -> i32 {
    val a: Tensor<f32, [6]> = Tensor::ones()
    mut r: Tensor<f32, [6]> = Tensor::zeros()
    mut w: Tensor<i64, [18]> = Tensor::zeros()
    split(a, &mut r, &mut w)
    return 0
}
"#;

    #[test]
    fn a_partition_runs_in_each_thread_that_owns_a_grid_element() {
        let text = kernel_launchers(&program(PARTITION), &nvidia())
            .expect("the body lowers")
            .text;
        // Threads 6 and 7 of the two blocks of 4 own no element of the grid tensor.
        assert!(
            text.contains("arith.constant 6 : index"),
            "the overhang test against the grid extent:\n{text}"
        );
        // 18 elements over 6 threads is a run of 3, which both `.len()` and the
        // slice's own bounds check read.
        assert!(text.contains("arith.constant 3 : i64"), "{text}");
        assert!(text.contains("arith.constant 1 : i64"), "{text}");
        lower_with_format(&program(PARTITION), &nvidia(), "isa").expect("it lowers for NVIDIA");
    }

    #[test]
    fn a_partition_the_grid_cannot_share_is_refused_per_instance() {
        let source = PARTITION.replace("[18]", "[20]");
        let Err(MlirError::KernelBodiesNotLowered(refusals)) =
            lower_with_format(&program(&source), &nvidia(), "isa")
        else {
            panic!("expected the partition refused");
        };
        let [refusal] = refusals.as_slice() else {
            panic!("expected one refusal, got {refusals:?}");
        };
        assert!(
            refusal.what.contains("20 elements") && refusal.what.contains("6 threads"),
            "{}",
            refusal.what
        );
        assert_eq!(
            refusal.span.start,
            source.find("wide.partition").expect("the call")
        );
    }

    #[test]
    fn a_grid_tensor_with_no_elements_launches_nothing() {
        let source = r#"
@kernel(threads: [4])
func empty(out: &mut Tensor<i32, [0]>) {
    out[thread_id.x] = 1
}

func main() -> i32 {
    mut t: Tensor<i32, [0]> = Tensor::zeros()
    empty(&mut t)
    return 0
}
"#;
        let text = kernel_launchers(&program(source), &nvidia())
            .expect("the body lowers")
            .text;
        assert!(!text.contains("gpu.launch"), "{text}");
    }
}
