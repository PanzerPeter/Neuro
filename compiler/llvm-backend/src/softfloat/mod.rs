// Self-contained soft-float half/bfloat conversion builtins.
//
// LLVM lowers `fpext`/`fptrunc` on `half`/`bfloat` — and f16/bf16 comparisons,
// which widen to f32 first — to runtime calls (`__extendhfsf2`, `__truncsfhf2`,
// `__truncdfhf2`, `__truncsfbf2`, `__truncdfbf2`) on targets without native
// half-precision instructions (generic x86-64). On Linux/macOS those come from
// libgcc/compiler-rt, which the C driver links automatically. On Windows we
// link via lld-link / MSVC, which provide no such runtime, so the symbols are
// undefined and linking fails.
//
// Rather than depend on a platform runtime, we emit our own definitions and
// link them into the module, making every binary self-contained. The
// implementations live in `builtins.ll` (provenance: `reference.c`). See those
// files for the verification notes.

use inkwell::context::Context;
use inkwell::memory_buffer::MemoryBuffer;
use inkwell::module::Module;

/// LLVM IR for the five soft-float conversion builtins, marked `weak_odr` so a
/// platform runtime, if present, may override them.
const BUILTINS_IR: &str = include_str!("builtins.ll");

/// Whether `module` references the `half`/`bfloat` LLVM types, i.e. contains an
/// f16/bf16 value whose lowering may emit a soft-float runtime call. When it
/// does not, linking the builtins would only add dead code, so we skip it.
pub(crate) fn module_uses_half_precision(module: &Module) -> bool {
    let ir = module.print_to_string();
    let text = ir.to_str().unwrap_or("");
    text.contains("half") || text.contains("bfloat")
}

/// Parse the embedded builtins and link them into `module` so the emitted object
/// resolves the half-precision conversion libcalls itself.
pub(crate) fn link_builtins<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    // inkwell's copying constructor still requires a NUL-terminated slice (the
    // IR parser reads a C string), so append the terminator the embedded text
    // lacks.
    let mut bytes = BUILTINS_IR.as_bytes().to_vec();
    bytes.push(0);
    let buffer = MemoryBuffer::create_from_memory_range_copy(&bytes, "softfloat_builtins");
    let builtins = context
        .create_module_from_ir(buffer)
        .map_err(|e| format!("failed to parse soft-float builtins IR: {}", e))?;
    module
        .link_in_module(builtins)
        .map_err(|e| format!("failed to link soft-float builtins: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The five conversion builtins LLVM may emit for f16/bf16 lowering.
    const NAMES: [&str; 5] = [
        "__extendhfsf2",
        "__truncsfhf2",
        "__truncdfhf2",
        "__truncsfbf2",
        "__truncdfbf2",
    ];

    #[test]
    fn builtins_ir_parses_links_and_verifies() {
        let context = Context::create();
        let module = context.create_module("host");
        link_builtins(&context, &module).expect("linking soft-float builtins");

        for name in NAMES {
            assert!(
                module.get_function(name).is_some(),
                "missing builtin definition: {name}"
            );
        }
        // The merged module must be valid IR for the linked LLVM version.
        assert!(module.verify().is_ok(), "linked module failed verification");
    }

    #[test]
    fn detects_half_precision_use() {
        let context = Context::create();

        let plain = context.create_module("plain");
        let f32_ty = context.f32_type();
        plain.add_function("f", f32_ty.fn_type(&[f32_ty.into()], false), None);
        assert!(!module_uses_half_precision(&plain));

        let halfy = context.create_module("halfy");
        let f16_ty = context.f16_type();
        halfy.add_function("g", f16_ty.fn_type(&[f16_ty.into()], false), None);
        assert!(module_uses_half_precision(&halfy));
    }
}
