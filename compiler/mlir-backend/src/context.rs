use melior::{
    dialect::DialectRegistry,
    utility::{register_all_dialects, register_all_llvm_translations},
    Context,
};

/// Build the MLIR context every path in this slice works in.
///
/// `register_all_llvm_translations` is unconditional rather than reserved for the
/// translating path: the LLVM-IR translation interfaces must be on the context
/// that *built* the module, so a context that can produce a module must already
/// carry them.
pub(crate) fn new_context() -> Context {
    let registry = DialectRegistry::new();
    register_all_dialects(&registry);

    let context = Context::new();
    context.append_dialect_registry(&registry);
    context.load_all_available_dialects();
    register_all_llvm_translations(&context);

    context
}
