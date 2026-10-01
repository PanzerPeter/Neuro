// `einsum` as `linalg`, for GPU bodies.
//
// One contracting `linalg.generic`, the shape `@` already has, over an index space of the
// output letters (`parallel`, one GPU thread per result element) followed by the contracted
// letters in letter order (`reduction`, a loop inside the thread, the last letter fastest).
// That is the LLVM backend's own order: it walks the contracted letters with one counter whose
// last letter is its lowest digit, and it multiplies the operands left to right before adding
// the product to an accumulator that starts at zero. So the device answer is the host's, bit
// for bit. Each operand's map names the dimension of every letter on its axes, which is what
// makes a letter repeated within one operand walk its diagonal.
//
// A full contraction (`"ii->"`) arrives boxed in a one-element tensor and gets one leading
// parallel dimension of extent 1, so its loop still maps onto a GPU thread.

use crate::{
    errors::MlirError,
    lower::map_type,
    tensor_arithmetic::{
        Generic, OperandAxes, build_expression, empty_tensor, fill_block, indexing_maps,
        iterator_types, tensor_parts,
    },
    tensor_reduce::{append, apply, yield_value},
};

use melior::{
    Context,
    dialect::arith,
    ir::{Block, BlockLike, Location, Type, Value, attribute::FloatAttribute},
};
use neuro_hir::{HirExpr, HirExprKind, HirTarget, HirType};

/// Lower `einsum`, a `TensorEinsum`, into a tensor of type `result`: its own type, or the
/// one-element tensor a full contraction is boxed in.
pub(crate) fn build_einsum<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    einsum: &HirExpr,
    result: &HirType,
    scope: &[(String, Value<'c, 'a>)],
    target: HirTarget,
) -> Result<Option<Value<'c, 'a>>, MlirError> {
    let HirExprKind::TensorEinsum {
        operands,
        inputs,
        output,
        extents,
    } = &einsum.kind
    else {
        return Ok(None);
    };
    let Some((element, result_shape)) = tensor_parts(result) else {
        return Ok(None);
    };
    let boxed = output.is_empty();
    if !matches!(element, HirType::F32 | HirType::F64)
        || operands.is_empty()
        || operands.len() != inputs.len()
        || result_shape.len() != output.len().max(usize::from(boxed))
        || inputs
            .iter()
            .flatten()
            .chain(output)
            .any(|letter| *letter >= extents.len())
    {
        return Ok(None);
    }
    let contracted: Vec<usize> = (0..extents.len())
        .filter(|letter| !output.contains(letter))
        .filter(|letter| inputs.iter().any(|subscript| subscript.contains(letter)))
        .collect();
    let lead = usize::from(boxed);
    let rank = lead + output.len() + contracted.len();
    let dimension = |letter: &usize| {
        output
            .iter()
            .position(|walked| walked == letter)
            .map(|at| lead + at)
            .or_else(|| {
                contracted
                    .iter()
                    .position(|summed| summed == letter)
                    .map(|at| lead + output.len() + at)
            })
    };

    let mut maps = Vec::with_capacity(operands.len() + 1);
    let mut values = Vec::with_capacity(operands.len());
    for (operand, subscript) in operands.iter().zip(inputs) {
        if tensor_parts(&operand.ty).map(|(e, _)| e) != Some(element) {
            return Ok(None);
        }
        let Some(axes) = subscript
            .iter()
            .map(&dimension)
            .map(|d| d.map(Some))
            .collect()
        else {
            return Ok(None);
        };
        maps.push(Some(axes));
        let Some(value) = build_expression(context, location, block, operand, scope, target)?
        else {
            return Ok(None);
        };
        values.push(value);
    }
    let written: OperandAxes = Some((0..result_shape.len()).map(Some).collect());
    maps.push(written.clone());

    let tensor_type = map_type(context, result)?;
    let element_type = map_type(context, element)?;
    let zero = append(
        block,
        arith::constant(
            context,
            FloatAttribute::new(context, element_type, 0.0).into(),
            location,
        ),
    )?;
    let empty = append(block, empty_tensor(location, tensor_type, &[])?)?;
    let seeded = apply(
        context,
        location,
        block,
        Generic {
            inputs: &[zero],
            destination: empty,
            indexing_maps: indexing_maps(context, result_shape.len(), &[&None, &written])?,
            iterators: iterator_types(context, result_shape.len(), 0)?,
        },
        tensor_type,
        fill_block(location, element_type)?,
    )?;
    let map_refs: Vec<&OperandAxes> = maps.iter().collect();
    Ok(Some(apply(
        context,
        location,
        block,
        Generic {
            inputs: &values,
            destination: seeded,
            indexing_maps: indexing_maps(context, rank, &map_refs)?,
            iterators: iterator_types(context, rank, contracted.len())?,
        },
        tensor_type,
        product_block(location, element_type, values.len())?,
    )?))
}

/// The body over `(operand elements..., accumulator)`: the elements multiplied left to
/// right, then added to the accumulator.
fn product_block<'c>(
    location: Location<'c>,
    element: Type<'c>,
    operands: usize,
) -> Result<Block<'c>, MlirError> {
    let block = Block::new(&vec![(element, location); operands + 1]);
    let mut product: Value = block.argument(0)?.into();
    for index in 1..operands {
        product = append(
            &block,
            arith::mulf(product, block.argument(index)?.into(), location),
        )?;
    }
    let total = append(
        &block,
        arith::addf(block.argument(operands)?.into(), product, location),
    )?;
    yield_value(&block, location, total)?;
    Ok(block)
}

#[cfg(test)]
mod tests {
    use crate::{GpuTarget, context::new_context, lower::build_linkable_module, lower_for_gpu};

    use neuro_hir::{HirProgram, HirTarget};

    fn program(body: &str) -> HirProgram {
        let source = format!(
            "enum Device {{\n    CPU,\n    GPU(i32)\n}}\n\nfunc main() -> i32 {{\n    val m: Tensor<f32, [6, 6]> = Tensor::ones()\n    val g = m.clone().to(Device::GPU(0))\n{body}\n    return 0\n}}\n"
        );
        let ast = syntax_parsing::parse(&source).expect("the program parses");
        hir_lowering::lower_program(&ast).expect("the program lowers to HIR")
    }

    fn device_module(program: &HirProgram) -> String {
        let context = new_context();
        let (module, _) = build_linkable_module(&context, program, &|function| {
            function.target == HirTarget::FollowsOperands
        })
        .expect("the bodies build");
        module.as_operation().to_string()
    }

    #[test]
    fn a_contraction_walks_output_letters_then_contracted_ones() {
        let text = device_module(&program("    val e = einsum(\"ij,kj->ik\", &g, &g)"));
        assert!(
            text.contains("affine_map<(d0, d1, d2) -> (d0, d2)>")
                && text.contains("affine_map<(d0, d1, d2) -> (d1, d2)>")
                && text.contains("affine_map<(d0, d1, d2) -> (d0, d1)>"),
            "{text}"
        );
        assert!(
            text.contains(r#"["parallel", "parallel", "reduction"]"#),
            "{text}"
        );
    }

    #[test]
    fn a_repeated_letter_walks_the_diagonal() {
        let text = device_module(&program("    val d = einsum(\"ii->i\", &g)"));
        assert!(text.contains("affine_map<(d0) -> (d0, d0)>"), "{text}");
    }

    #[test]
    fn a_full_contraction_gets_one_parallel_dimension() {
        let program = program("    val t = einsum(\"ij,ij->\", &g, &g)");
        let text = device_module(&program);
        assert!(
            text.contains("affine_map<(d0, d1, d2) -> (d0)>"),
            "the boxed result is walked by a leading dimension:\n{text}"
        );
        let bodies = lower_for_gpu(
            &program,
            &GpuTarget::Nvidia {
                chip: "sm_80".to_string(),
            },
        )
        .expect("the body lowers");
        assert_eq!(
            bodies
                .llvm_ir
                .matches("call void @mgpuLaunchKernel")
                .count(),
            2,
            "a zero fill, then the contraction"
        );
    }
}
