use crate::errors::MlirError;

use melior::{
    dialect::{arith, func},
    ir::{
        attribute::{StringAttribute, TypeAttribute},
        operation::OperationLike,
        r#type::FunctionType,
        Block, BlockLike, Location, Module, Region, RegionLike, Type,
    },
    Context,
};

/// Build a trivial, verifiable MLIR module in a caller-owned context.
///
/// It defines `func.func @neuro_smoke(index, index) -> index` returning the sum of
/// its arguments, which exercises dialect registration (`func`, `arith`) and the
/// MLIR verifier end-to-end and so confirms `melior` is wired correctly against the
/// active MLIR 20 toolchain. The context is the caller's so `bridge` can carry this
/// module, which has a real *body* rather than only declarations, across to LLVM IR.
pub(crate) fn build_smoke_module(context: &Context) -> Result<Module<'_>, MlirError> {
    let location = Location::unknown(context);
    let module = Module::new(location);

    let index_type = Type::index(context);

    let block = Block::new(&[(index_type, location), (index_type, location)]);
    let lhs = block.argument(0)?.into();
    let rhs = block.argument(1)?.into();
    let sum = block
        .append_operation(arith::addi(lhs, rhs, location))
        .result(0)?
        .into();
    block.append_operation(func::r#return(&[sum], location));

    let region = Region::new();
    region.append_block(block);

    let function = func::func(
        context,
        StringAttribute::new(context, "neuro_smoke"),
        TypeAttribute::new(
            FunctionType::new(context, &[index_type, index_type], &[index_type]).into(),
        ),
        region,
        &[],
        location,
    );
    module.body().append_operation(function);

    if !module.as_operation().verify() {
        return Err(MlirError::ModuleVerificationFailed);
    }

    Ok(module)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::context::new_context;

    #[test]
    fn smoke_module_verifies_and_defines_function() {
        let context = new_context();
        let ir = build_smoke_module(&context)
            .expect("melior should build a verifiable module")
            .as_operation()
            .to_string();
        assert!(ir.contains("func.func"), "expected a func.func op:\n{ir}");
        assert!(
            ir.contains("neuro_smoke"),
            "expected the named function:\n{ir}"
        );
        assert!(
            ir.contains("arith.addi"),
            "expected the addi body op:\n{ir}"
        );
    }
}
