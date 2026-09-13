use crate::{context::new_context, errors::MlirError};

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

/// Builds a trivial, verifiable MLIR module and returns its textual form.
///
/// The module defines `func.func @neuro_smoke(index, index) -> index` returning
/// the sum of its arguments. It exercises dialect registration (`func`, `arith`)
/// and the MLIR verifier end-to-end, confirming that `melior` is wired correctly
/// against the active MLIR 20 toolchain. Used as the Phase 1.8 integration smoke
/// test until real HIR lowering exists.
pub fn emit_smoke_module() -> Result<String, MlirError> {
    let context = new_context();
    let module = build_smoke_module(&context)?;

    Ok(module.as_operation().to_string())
}

/// Build the smoke module in a caller-owned context.
///
/// Split out of [`emit_smoke_module`] so the translating path can run a module
/// that carries a real *body* through the crossing, not only declarations.
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

    #[test]
    fn smoke_module_verifies_and_defines_function() {
        let ir = emit_smoke_module().expect("melior should build a verifiable module");
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
