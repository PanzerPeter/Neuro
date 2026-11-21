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
    guards::{At, Element, Lowering},
    lower::map_type,
    schedule,
    tensor_arithmetic::{
        Generic, OperandAxes, build_expression, cast_elements, dynamic_sizes, empty_tensor,
        fill_block, generic_op, half_precision, indexing_maps, iterator_types, result_extents,
        tensor_parts, with_element,
    },
    tensor_reduce::{append, apply, yield_value},
};

use ast_types::BinaryOp;
use melior::{
    Context,
    dialect::arith,
    ir::{
        Block, BlockLike, Location, Region, RegionLike, Type, Value,
        attribute::{FloatAttribute, IntegerAttribute},
    },
};
use neuro_hir::{HirExpr, HirExprKind, HirType};

/// Lower `einsum`, a `TensorEinsum`, into a tensor of type `result`: its own type, or the
/// one-element tensor a full contraction is boxed in.
pub(crate) fn build_einsum<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    einsum: &HirExpr,
    result: &HirType,
    scope: &[(String, Value<'c, 'a>)],
    lowering: &Lowering<'c, 'a>,
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
    let Some((narrow, result_shape)) = tensor_parts(result) else {
        return Ok(None);
    };
    // A half-precision contraction accumulates in `f32` in the same order and is rounded once
    // at the end, so its operands are widened and it runs as an `f32` one.
    let wide = HirType::F32;
    let half = half_precision(narrow);
    let element = if half { &wide } else { narrow };
    // A `...` over no axes leaves an empty output on a rank-0 tensor, which needs no box.
    let boxed = output.is_empty() && !result_shape.is_empty();
    let Some(kind) = Element::computed(element) else {
        return Ok(None);
    };
    if operands.is_empty()
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
        if tensor_parts(&operand.ty).map(|(e, _)| e) != Some(narrow) {
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
        let Some(value) = build_expression(context, location, block, operand, scope, lowering)?
        else {
            return Ok(None);
        };
        values.push(match half {
            true => cast_elements(
                context,
                location,
                block,
                value,
                (narrow, &with_element(&operand.ty, &wide)),
            )?,
            false => value,
        });
    }
    // A dynamic output axis is read off the operand that walks its letter.
    let walks: Vec<(Value, &OperandAxes)> = values.iter().copied().zip(&maps).collect();
    let Some(result_sizes) = result_extents(context, location, block, result_shape, &walks)? else {
        return Ok(None);
    };
    let written: OperandAxes = Some((0..result_shape.len()).map(Some).collect());
    maps.push(written.clone());

    let computed = with_element(result, element);
    let tensor_type = map_type(context, &computed)?;
    let element_type = map_type(context, element)?;
    let zero = match kind {
        Element::Float | Element::Half => FloatAttribute::new(context, element_type, 0.0).into(),
        _ => IntegerAttribute::new(element_type, 0).into(),
    };
    let zero = append(block, arith::constant(context, zero, location))?;
    let empty = append(
        block,
        empty_tensor(location, tensor_type, &dynamic_sizes(&result_sizes))?,
    )?;
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
    let checks = lowering.checks();
    let body = Region::new();
    body.append_block(product_block(
        location,
        element_type,
        values.len(),
        lowering,
        (kind, einsum.span.start),
    )?);
    let mut contraction = generic_op(
        context,
        location,
        Generic {
            inputs: &values,
            destination: seeded,
            indexing_maps: indexing_maps(context, rank, &map_refs)?,
            iterators: iterator_types(context, rank, contracted.len())?,
        },
        tensor_type,
        body,
    )?;
    // A letter repeated within one operand walks a diagonal, which no block reads as a vector.
    let diagonal = inputs.iter().any(|subscript| {
        subscript
            .iter()
            .enumerate()
            .any(|(at, letter)| subscript[..at].contains(letter))
    });
    // The schedule's blocks divide static extents, so a letter of a dynamic axis keeps the
    // naive nest.
    let dynamic_letter = |letter: &usize| {
        operands.iter().zip(inputs).any(|(operand, subscript)| {
            let shape = tensor_parts(&operand.ty).map(|(_, shape)| shape);
            subscript.iter().enumerate().any(|(axis, walked)| {
                walked == letter && shape.is_some_and(|shape| shape[axis].is_none())
            })
        })
    };
    let dynamic = output.iter().chain(&contracted).any(dynamic_letter);
    if !diagonal && !dynamic && lowering.checks() == checks {
        let loops: Vec<u64> = std::iter::repeat_n(1, lead)
            .chain(
                output
                    .iter()
                    .chain(&contracted)
                    .map(|&letter| extents[letter].map_or(1, |extent| extent as u64)),
            )
            .collect();
        schedule::tag(
            context,
            &mut contraction,
            lowering.side,
            (&loops, lead + output.len()),
            element,
        );
    }

    let total = block.append_operation(contraction).result(0)?.into();
    match half {
        true => cast_elements(context, location, block, total, (element, result)).map(Some),
        false => Ok(Some(total)),
    }
}

/// The body over `(operand elements..., accumulator)`: the elements multiplied left to
/// right, then added to the accumulator, each step with the host's checks at the `einsum`.
fn product_block<'c>(
    location: Location<'c>,
    element: Type<'c>,
    operands: usize,
    lowering: &Lowering<'c, '_>,
    (kind, offset): (Element, usize),
) -> Result<Block<'c>, MlirError> {
    let block = Block::new(&vec![(element, location); operands + 1]);
    let at = At {
        offset,
        position: None,
    };
    let step = |op, pair| {
        lowering
            .arith(&block, op, kind, pair, at)?
            .ok_or(MlirError::ModuleVerificationFailed)
    };
    let mut product: Value = block.argument(0)?.into();
    for index in 1..operands {
        product = step(BinaryOp::Multiply, (product, block.argument(index)?.into()))?;
    }
    let total = step(BinaryOp::Add, (block.argument(operands)?.into(), product))?;
    yield_value(&block, location, total)?;
    Ok(block)
}

#[cfg(test)]
mod tests {
    use crate::{
        GpuTarget,
        bridge::tests::{
            assert_launches_on_gpu, assert_lowered, mlir_of, operand, outlined, tensor_of,
        },
        context::new_context,
        lower::build_linkable_module,
        lower_for_gpu,
    };

    use neuro_hir::{HirExpr, HirExprKind, HirProgram, HirTarget, HirType};
    use shared_types::Span;

    fn program(body: &str) -> HirProgram {
        let source = format!(
            "enum Device {{\n    CPU,\n    GPU(i32)\n}}\n\nfunc main() -> i32 {{\n    val m: Tensor<f32, [6, 6]> = Tensor::ones()\n    val g = m.clone().to(Device::GPU(0))\n{body}\n    return 0\n}}\n"
        );
        let ast = syntax_parsing::parse(&source).expect("the program parses");
        hir_lowering::lower_program(&ast).expect("the program lowers to HIR")
    }

    fn device_module(program: &HirProgram) -> String {
        let context = new_context();
        let (module, _) = build_linkable_module(
            &context,
            program,
            (crate::Overflow::Checked, crate::guards::Side::Device),
            &|function| function.target == HirTarget::FollowsOperands,
        )
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
            crate::Overflow::Checked,
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

    /// `func f(a: A, b: B) -> R { return einsum(...) }`, with the subscript tables as the HIR
    /// holds them.
    fn contraction(
        (left, right): (Vec<Option<usize>>, Vec<Option<usize>>),
        result: Vec<Option<usize>>,
        extents: Vec<usize>,
    ) -> HirProgram {
        let span = Span::new(0, 1);
        let a = tensor_of(HirType::F32, left);
        let b = tensor_of(HirType::F32, right);
        let r = tensor_of(HirType::F32, result);
        let einsum = HirExpr::new(
            HirExprKind::TensorEinsum {
                operands: vec![operand("a", &a), operand("b", &b)],
                inputs: vec![vec![0, 1], vec![1, 2]],
                output: vec![0, 2],
                extents: extents.into_iter().map(Some).collect(),
            },
            r.clone(),
            span,
        );
        outlined(&[("a", a), ("b", b)], einsum, r)
    }

    /// The letter of a dynamic row axis is read off the operand that walks it.
    #[test]
    fn a_dynamic_letter_sizes_the_result_from_its_operand() {
        let program = contraction(
            (vec![None, Some(3)], vec![Some(3), Some(4)]),
            vec![None, Some(4)],
            vec![0, 3, 4],
        );
        let mlir = mlir_of(&program);
        assert!(
            mlir.contains("tensor.dim") && mlir.contains(r#""reduction""#),
            "{mlir}"
        );
        assert_lowered(&program);
        assert_launches_on_gpu(&program);
    }

    /// A dynamic contracted letter has no static trip count in the HIR, and the body takes it
    /// from the operands; the schedule keeps the naive nest.
    #[test]
    fn a_dynamic_contracted_letter_is_the_operands_trip_count() {
        let program = contraction(
            (vec![Some(2), None], vec![None, Some(4)]),
            vec![Some(2), Some(4)],
            vec![2, 0, 4],
        );
        let mlir = mlir_of(&program);
        assert!(mlir.contains(r#""reduction""#), "{mlir}");
        assert!(!mlir.contains("neuro.tiles"), "{mlir}");
        assert_lowered(&program);
        assert_launches_on_gpu(&program);
    }
}
