use std::sync::Once;

use melior::{
    dialect::DialectRegistry,
    utility::{register_all_dialects, register_all_llvm_translations, register_all_passes},
    Context,
};

/// The pass registry is process-global, and registering a pipeline twice is a
/// duplicate-registration error rather than a no-op.
static PASSES: Once = Once::new();

/// Build the MLIR context every path in this slice works in.
///
/// `register_all_llvm_translations` is unconditional rather than reserved for the
/// translating path: the LLVM-IR translation interfaces must be on the context
/// that *built* the module, so a context that can produce a module must already
/// carry them. `register_all_passes` is here for the same reason: the lowering
/// pipeline is named in text, and a pass absent from the registry is a parse
/// failure rather than a missing rewrite.
pub(crate) fn new_context() -> Context {
    PASSES.call_once(register_all_passes);

    let registry = DialectRegistry::new();
    register_all_dialects(&registry);

    let context = Context::new();
    context.append_dialect_registry(&registry);
    context.load_all_available_dialects();
    register_all_llvm_translations(&context);

    context
}
