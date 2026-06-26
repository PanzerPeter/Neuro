use crate::errors::MlirError;

use melior::{
    dialect::{arith, func, DialectRegistry},
    ir::{
        attribute::{StringAttribute, TypeAttribute},
        operation::OperationLike,
        r#type::FunctionType,
        Block, BlockLike, Location, Module, Region, RegionLike, Type,
    },
    utility::register_all_dialects,
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
    let registry = DialectRegistry::new();
    register_all_dialects(&registry);

    let context = Context::new();
    context.append_dialect_registry(&registry);
    context.load_all_available_dialects();

    let location = Location::unknown(&context);
    let module = Module::new(location);

    let index_type = Type::index(&context);

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
        &context,
        StringAttribute::new(&context, "neuro_smoke"),
        TypeAttribute::new(
            FunctionType::new(&context, &[index_type, index_type], &[index_type]).into(),
        ),
        region,
        &[],
        location,
    );
    module.body().append_operation(function);

    if !module.as_operation().verify() {
        return Err(MlirError::ModuleVerificationFailed);
    }

    Ok(module.as_operation().to_string())
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
