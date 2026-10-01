// Elementwise math (`.exp()`, `.log()`, `.sqrt()`, `.tanh()`, `.abs()`, `.pow(p)`) as `linalg`,
// for GPU bodies.
//
// One `linalg.generic` per call: the operand walked with the identity map, `.pow`'s exponent a
// scalar every point reads, and the `math` dialect op of the function's name as the body. The
// GPU conversion turns each into the vendor's device math library (libdevice on NVIDIA, ocml
// on AMD). `sqrt` and `abs` are exact there as on the host; the four transcendental functions
// are the device library's, which may differ from the host's C library in the last bits.
// `sign`, which only a derivative writes, is two compares.

use crate::{
    errors::MlirError,
    lower::map_type,
    tensor_arithmetic::{
        Generic, OperandAxes, build_expression, empty_tensor, indexing_maps, iterator_types,
        tensor_parts,
    },
    tensor_reduce::{append, apply, yield_value},
};

use melior::{
    Context,
    dialect::arith::{self, CmpfPredicate},
    ir::{
        Block, BlockLike, Location, Type, Value, attribute::FloatAttribute,
        operation::OperationBuilder,
    },
};
use neuro_hir::{HirExpr, HirExprKind, HirMathOp, HirTarget, HirType};

/// Lower `math`, a `Math` over a float tensor, into a fresh tensor of the operand's shape.
/// A scalar operand is the LLVM backend's.
pub(crate) fn build_math<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    math: &HirExpr,
    scope: &[(String, Value<'c, 'a>)],
    target: HirTarget,
) -> Result<Option<Value<'c, 'a>>, MlirError> {
    let HirExprKind::Math {
        op,
        operand,
        exponent,
    } = &math.kind
    else {
        return Ok(None);
    };
    let Some((element, shape)) = tensor_parts(&math.ty) else {
        return Ok(None);
    };
    if !matches!(element, HirType::F32 | HirType::F64)
        || shape.iter().any(Option::is_none)
        || (*op == HirMathOp::Pow) != exponent.is_some()
    {
        return Ok(None);
    }
    let Some(source) = build_expression(context, location, block, operand, scope, target)? else {
        return Ok(None);
    };
    let mut inputs = vec![source];
    if let Some(exponent) = exponent {
        let Some(power) = build_expression(context, location, block, exponent, scope, target)?
        else {
            return Ok(None);
        };
        inputs.push(power);
    }

    let tensor_type = map_type(context, &math.ty)?;
    let element_type = map_type(context, element)?;
    let rank = shape.len();
    let own: OperandAxes = Some((0..rank).map(Some).collect());
    let scalar: OperandAxes = None;
    let mut maps = vec![&own];
    if exponent.is_some() {
        maps.push(&scalar);
    }
    maps.push(&own);
    let empty = append(block, empty_tensor(location, tensor_type, &[])?)?;
    Ok(Some(apply(
        context,
        location,
        block,
        Generic {
            inputs: &inputs,
            destination: empty,
            indexing_maps: indexing_maps(context, rank, &maps)?,
            iterators: iterator_types(context, rank, 0)?,
        },
        tensor_type,
        math_block(context, location, element_type, *op, inputs.len())?,
    )?))
}

/// The body over `(element[, exponent], destination)`: the function applied to the element.
fn math_block<'c>(
    context: &'c Context,
    location: Location<'c>,
    element: Type<'c>,
    op: HirMathOp,
    inputs: usize,
) -> Result<Block<'c>, MlirError> {
    let block = Block::new(&vec![(element, location); inputs + 1]);
    let value: Value = block.argument(0)?.into();
    let result = match op {
        HirMathOp::Sign => sign(context, location, &block, element, value)?,
        HirMathOp::Pow => {
            let power = block.argument(1)?.into();
            append(
                &block,
                math_op("math.powf", &[value, power], element, location)?,
            )?
        }
        unary => {
            let name = match unary {
                HirMathOp::Exp => "math.exp",
                HirMathOp::Log => "math.log",
                HirMathOp::Sqrt => "math.sqrt",
                HirMathOp::Tanh => "math.tanh",
                _ => "math.absf",
            };
            append(&block, math_op(name, &[value], element, location)?)?
        }
    };
    yield_value(&block, location, result)?;
    Ok(block)
}

fn math_op<'c>(
    name: &str,
    operands: &[Value<'c, '_>],
    element: Type<'c>,
    location: Location<'c>,
) -> Result<melior::ir::Operation<'c>, MlirError> {
    Ok(OperationBuilder::new(name, location)
        .add_operands(operands)
        .add_results(&[element])
        .build()?)
}

/// `1` above zero, `-1` below, and the value itself otherwise (either zero, or NaN), as the
/// LLVM backend computes it: both compares are ordered, so NaN fails each.
fn sign<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    element: Type<'c>,
    value: Value<'c, 'a>,
) -> Result<Value<'c, 'a>, MlirError> {
    let constant = |number: f64| {
        append(
            block,
            arith::constant(
                context,
                FloatAttribute::new(context, element, number).into(),
                location,
            ),
        )
    };
    let zero = constant(0.0)?;
    let one = constant(1.0)?;
    let minus_one = constant(-1.0)?;
    let above = append(
        block,
        arith::cmpf(context, CmpfPredicate::Ogt, value, zero, location),
    )?;
    let below = append(
        block,
        arith::cmpf(context, CmpfPredicate::Olt, value, zero, location),
    )?;
    let negative = append(block, arith::select(below, minus_one, value, location))?;
    append(block, arith::select(above, one, negative, location))
}

#[cfg(test)]
mod tests {
    use crate::{
        GpuTarget, context::new_context, gpu::device_math, lower::build_linkable_module,
        lower_for_gpu,
    };

    use neuro_hir::{HirProgram, HirTarget};

    fn program(body: &str) -> HirProgram {
        let source = format!(
            "enum Device {{\n    CPU,\n    GPU(i32)\n}}\n\nfunc main() -> i32 {{\n    val m: Tensor<f32, [37, 19]> = Tensor::ones()\n    val g = m.clone().to(Device::GPU(0))\n{body}\n    return 0\n}}\n"
        );
        let ast = syntax_parsing::parse(&source).expect("the program parses");
        hir_lowering::lower_program(&ast).expect("the program lowers to HIR")
    }

    /// The device bodies before they are lowered for any GPU.
    fn device_module(program: &HirProgram) -> String {
        let context = new_context();
        let (module, _) = build_linkable_module(&context, program, &|function| {
            function.target == HirTarget::FollowsOperands
        })
        .expect("the bodies build");
        module.as_operation().to_string()
    }

    #[test]
    fn each_function_is_the_math_op_of_its_name() {
        for (call, op) in [
            ("g.exp()", "math.exp"),
            ("g.log()", "math.log"),
            ("g.sqrt()", "math.sqrt"),
            ("g.tanh()", "math.tanh"),
            ("g.abs()", "math.absf"),
        ] {
            let text = device_module(&program(&format!("    val r = {call}")));
            assert!(text.contains(op), "`{call}`:\n{text}");
        }
    }

    #[test]
    fn a_power_reads_its_exponent_at_every_point() {
        let text = device_module(&program("    val r = g.pow(1.5f32)"));
        assert!(text.contains("math.powf"), "{text}");
        assert!(
            text.contains("affine_map<(d0, d1) -> ()>"),
            "the exponent is a scalar every point reads:\n{text}"
        );
    }

    #[test]
    fn a_math_function_runs_on_a_gpu_exactly_when_its_library_is_there() {
        let target = GpuTarget::Nvidia {
            chip: "sm_80".to_string(),
        };
        let available = device_math(&new_context(), &target, "isa");
        let bodies = lower_for_gpu(&program("    val r = g.exp()"), &target)
            .expect("no library is no error for an outlined body");
        assert_eq!(bodies.functions.is_empty(), !available);
    }
}
