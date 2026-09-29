use inkwell::context::Context;
use inkwell::memory_buffer::MemoryBuffer;
use inkwell::module::{Linkage, Module};
use inkwell::types::BasicMetadataTypeEnum;
use inkwell::values::{BasicMetadataValueEnum, PointerValue};
use inkwell::AddressSpace;
use neuro_hir::HirFunction;

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;
use crate::{BodyMemory, ExternalBodies};

use super::context::CodegenContext;
use super::device_memory::{DEVICE_ALLOC_FN, DEVICE_RELEASE_FN};

impl<'ctx> CodegenContext<'ctx> {
    /// Define `func_def` as a call to `symbol`, a body lowered outside this backend.
    ///
    /// `symbol` takes MLIR's calling convention: each tensor as an exploded row-major
    /// `memref` descriptor, each scalar as itself, and the result as one more descriptor
    /// after them, naming a buffer this side allocates. The function keeps Neuro's own
    /// ABI around it, so no caller can tell which backend computed the body.
    ///
    /// Ownership is the ordinary function's: a by-value tensor parameter was moved in,
    /// so it is released once the body has read it, and a `&Tensor` one is only read.
    ///
    /// When the bodies run on a device, every buffer `symbol` sees is staged: each
    /// tensor operand is copied into device memory, the result is written to a device
    /// buffer and copied back into the host tensor returned, and all of it is released
    /// before the function returns. A caller still passes and receives host tensors.
    pub(crate) fn codegen_external_body(
        &mut self,
        func_def: &HirFunction,
        symbol: &str,
    ) -> CodegenResult<()> {
        let function = *self
            .functions
            .get(&func_def.name)
            .ok_or_else(|| CodegenError::UndefinedFunction(func_def.name.clone()))?;
        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);
        self.current_function = Some(function);

        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let mut arg_types: Vec<BasicMetadataTypeEnum<'ctx>> = Vec::new();
        let mut args: Vec<BasicMetadataValueEnum<'ctx>> = Vec::new();
        let mut consumed = Vec::new();
        let mut staging = match self.body_memory {
            BodyMemory::Host => None,
            BodyMemory::Device => Some(self.open_device_staging()?),
        };

        for (index, param) in func_def.params.iter().enumerate() {
            let value = function
                .get_nth_param(index as u32)
                .ok_or_else(|| CodegenError::InternalError(format!("missing parameter {index}")))?;
            let ty = Type::from_hir(&param.ty);
            let handle = match &ty {
                Type::Tensor { .. } => {
                    consumed.push(value.into_pointer_value());
                    value.into_pointer_value()
                }
                // A `&Tensor` is the address of a cell holding the handle.
                Type::Reference { inner, .. } if matches!(**inner, Type::Tensor { .. }) => self
                    .builder
                    .build_load(ptr_type, value.into_pointer_value(), "external.borrow")?
                    .into_pointer_value(),
                _ => {
                    arg_types.push(value.get_type().into());
                    args.push(value.into());
                    continue;
                }
            };
            let host = self.load_dlpack_data(handle)?;
            let data = match staging.as_mut() {
                Some(staging) => self.copy_to_device(staging, ty.referent(), host)?,
                None => host,
            };
            self.push_memref_descriptor(ty.referent(), data, &mut arg_types, &mut args)?;
        }

        let result_ty = Type::from_hir(&func_def.return_type);
        let result = self.alloc_dlpack_tensor(&result_ty, "external.result")?;
        let host_result = self.load_dlpack_data(result)?;
        let written = match staging.as_mut() {
            Some(staging) => self.device_buffer(staging, &result_ty)?,
            None => host_result,
        };
        self.push_memref_descriptor(&result_ty, written, &mut arg_types, &mut args)?;
        if let Some(staging) = &staging {
            self.await_device_staging(staging)?;
        }

        let callee = self.module.get_function(symbol).unwrap_or_else(|| {
            self.module.add_function(
                symbol,
                self.context.void_type().fn_type(&arg_types, false),
                None,
            )
        });
        self.builder.build_call(callee, &args, "")?;

        if let Some(staging) = staging {
            self.close_device_staging(staging, &result_ty, host_result, written)?;
        }
        for handle in consumed {
            self.build_dlpack_release(handle)?;
        }
        self.builder.build_return(Some(&result))?;
        Ok(())
    }

    /// Append the `memref` descriptor for a `tensor_ty` buffer at `data`: the buffer as
    /// both the allocated and the aligned pointer (the callee frees neither), a zero
    /// offset, then the extents and the row-major element strides.
    fn push_memref_descriptor(
        &self,
        tensor_ty: &Type,
        data: PointerValue<'ctx>,
        arg_types: &mut Vec<BasicMetadataTypeEnum<'ctx>>,
        args: &mut Vec<BasicMetadataValueEnum<'ctx>>,
    ) -> CodegenResult<()> {
        let Type::Tensor { shape, .. } = tensor_ty else {
            return Err(CodegenError::InternalError(
                "a memref descriptor is only built for a tensor".to_string(),
            ));
        };
        let extents = crate::types::static_extents(shape)?;
        let i64_type = self.context.i64_type();

        let mut strides = vec![1u64; extents.len()];
        for axis in (0..extents.len().saturating_sub(1)).rev() {
            strides[axis] = strides[axis + 1] * extents[axis + 1] as u64;
        }

        for pointer in [data, data] {
            arg_types.push(pointer.get_type().into());
            args.push(pointer.into());
        }
        let integers = std::iter::once(0)
            .chain(extents.iter().map(|&extent| extent as u64))
            .chain(strides);
        for integer in integers {
            arg_types.push(i64_type.into());
            args.push(i64_type.const_int(integer, false).into());
        }
        Ok(())
    }
}

/// Parse the bodies' IR and link it into `module`, then make each symbol internal:
/// they exist only to be called by the functions
/// [`CodegenContext::codegen_external_body`] wraps around them, so the optimizer may
/// inline them and drop what remains.
pub(crate) fn link_external_bodies<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    bodies: &ExternalBodies,
) -> CodegenResult<()> {
    // The IR parser reads a C string, so the text needs the terminator it lacks.
    let mut bytes = bodies.llvm_ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = MemoryBuffer::create_from_memory_range_copy(&bytes, "external_bodies");
    let external = context
        .create_module_from_ir(buffer)
        .map_err(|e| CodegenError::LlvmError(format!("failed to parse external bodies: {e}")))?;
    module
        .link_in_module(external)
        .map_err(|e| CodegenError::LlvmError(format!("failed to link external bodies: {e}")))?;

    for (_, symbol) in &bodies.functions {
        let function = module.get_function(symbol).ok_or_else(|| {
            CodegenError::LlvmError(format!("external bodies do not define `{symbol}`"))
        })?;
        function.set_linkage(Linkage::Internal);
    }
    // The device allocator stayed external only so the launchers' declarations of its
    // names would resolve to it.
    for name in [DEVICE_ALLOC_FN, DEVICE_RELEASE_FN] {
        if let Some(function) = module.get_function(name) {
            function.set_linkage(Linkage::Internal);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{build_module, BodyMemory, ExternalBodies, OptimizationLevelSetting};
    use inkwell::context::Context;
    use inkwell::module::Linkage;

    const SOURCE: &str = r#"
        func scale(a: &Tensor<f32, [2]>, s: f32) -> Tensor<f32, [2]> {
            return a * s
        }
        func consume(a: Tensor<f32, [2]>) -> Tensor<f32, [2]> {
            return a * 2.0f32
        }
    "#;

    /// Stand-ins for what MLIR emits: one rank-1 descriptor is two pointers and three
    /// integers, and the result's descriptor trails the parameters.
    const BODIES: &str = r#"
        define void @ext_scale(ptr %0, ptr %1, i64 %2, i64 %3, i64 %4, float %5, ptr %6, ptr %7, i64 %8, i64 %9, i64 %10) {
          ret void
        }
        define void @ext_consume(ptr %0, ptr %1, i64 %2, i64 %3, i64 %4, ptr %5, ptr %6, i64 %7, i64 %8, i64 %9) {
          ret void
        }
    "#;

    /// The same stand-ins as a GPU launcher: `ext_consume` also takes a scratch buffer
    /// between two kernels through the allocator the LLVM backend defines.
    const DEVICE_BODIES: &str = r#"
        declare ptr @_mlir_memref_to_llvm_alloc(i64)
        declare void @_mlir_memref_to_llvm_free(ptr)
        define void @ext_scale(ptr %0, ptr %1, i64 %2, i64 %3, i64 %4, float %5, ptr %6, ptr %7, i64 %8, i64 %9, i64 %10) {
          ret void
        }
        define void @ext_consume(ptr %0, ptr %1, i64 %2, i64 %3, i64 %4, ptr %5, ptr %6, i64 %7, i64 %8, i64 %9) {
          %scratch = call ptr @_mlir_memref_to_llvm_alloc(i64 72)
          call void @_mlir_memref_to_llvm_free(ptr %scratch)
          ret void
        }
    "#;

    fn linked_ir(source: &str, bodies_ir: &str, memory: BodyMemory) -> String {
        let ast = syntax_parsing::parse(source).expect("parsing failed");
        let hir = hir_lowering::lower_program(&ast).expect("HIR lowering failed");
        let bodies = ExternalBodies {
            llvm_ir: bodies_ir.to_string(),
            functions: vec![
                ("scale".to_string(), "ext_scale".to_string()),
                ("consume".to_string(), "ext_consume".to_string()),
            ],
            memory,
        };
        let context = Context::create();
        let codegen_ctx = build_module(
            &context,
            &hir,
            OptimizationLevelSetting::O0,
            source,
            "external.nr",
            Some(&bodies),
        )
        .expect("a module with linked bodies should build and verify");
        for symbol in ["ext_scale", "ext_consume"] {
            let function = codegen_ctx
                .module
                .get_function(symbol)
                .unwrap_or_else(|| panic!("`{symbol}` should be linked in"));
            assert_eq!(function.get_linkage(), Linkage::Internal, "{symbol}");
        }
        codegen_ctx.module.print_to_string().to_string()
    }

    fn host_ir() -> String {
        linked_ir(SOURCE, BODIES, BodyMemory::Host)
    }

    fn device_ir(source: &str) -> String {
        linked_ir(source, DEVICE_BODIES, BodyMemory::Device)
    }

    fn body<'a>(ir: &'a str, name: &str) -> &'a str {
        let start = ir
            .find(&format!("define ptr @{name}("))
            .unwrap_or_else(|| panic!("no definition of `{name}`:\n{ir}"));
        let rest = &ir[start..];
        &rest[..rest.find("\n}").unwrap_or(rest.len())]
    }

    #[test]
    fn a_borrowed_tensor_is_passed_by_descriptor_and_kept() {
        let ir = host_ir();
        let scale = body(&ir, "scale");
        assert!(
            scale.contains(
                "call void @ext_scale(ptr %dlpack.data, ptr %dlpack.data, i64 0, i64 2, i64 1, float %1"
            ),
            "expected the borrowed buffer, its extent and stride, then the scalar:\n{scale}"
        );
        assert!(
            !scale.contains("dlpack.deleter"),
            "a borrowed tensor is the caller's to release:\n{scale}"
        );
        assert!(
            !scale.contains("fmul"),
            "the LLVM backend's own body must not be emitted beside the linked one:\n{scale}"
        );
    }

    #[test]
    fn an_owned_tensor_is_released_after_the_call() {
        let ir = host_ir();
        let consume = body(&ir, "consume");
        let call = consume
            .find("call void @ext_consume(")
            .unwrap_or_else(|| panic!("expected the linked body to be called:\n{consume}"));
        assert!(
            consume[call..].contains("dlpack.deleter"),
            "a tensor moved in is released once the body has read it:\n{consume}"
        );
    }

    /// The text of `needle`'s first occurrence at or after `from`, or a panic naming it.
    fn position(haystack: &str, needle: &str, from: usize) -> usize {
        haystack[from..]
            .find(needle)
            .map(|at| from + at)
            .unwrap_or_else(|| panic!("expected `{needle}` after byte {from} in:\n{haystack}"))
    }

    #[test]
    fn a_device_body_is_handed_only_staged_buffers() {
        let ir = device_ir(SOURCE);
        let scale = body(&ir, "scale");

        let stream = position(scale, "call ptr @mgpuStreamCreate()", 0);
        let copy_in = position(scale, "call void @mgpuMemcpy(", stream);
        let settled = position(scale, "call void @mgpuStreamSynchronize(", copy_in);
        let call = position(scale, "call void @ext_scale(ptr %device.buffer", settled);
        let copy_out = position(scale, "call void @mgpuMemcpy(", call);
        position(scale, "call void @mgpuStreamDestroy(", copy_out);
        position(scale, "call void @_mlir_memref_to_llvm_free(", copy_out);
        position(
            scale,
            "store i64 %device.mark, ptr @__neuro_device_arena_offset",
            copy_out,
        );

        assert_eq!(
            scale
                .matches("call ptr @_mlir_memref_to_llvm_alloc(")
                .count(),
            2,
            "one device buffer for the operand and one for the result:\n{scale}"
        );
        assert!(
            !scale[call..].contains("%dlpack.data,"),
            "a host buffer must not reach the kernel:\n{scale}"
        );
    }

    #[test]
    fn a_launchers_scratch_buffer_resolves_to_the_device_allocator() {
        let ir = device_ir(SOURCE);
        for name in ["_mlir_memref_to_llvm_alloc", "_mlir_memref_to_llvm_free"] {
            assert!(
                ir.lines().any(|line| line.starts_with("define internal")
                    && line.contains(&format!("@{name}("))),
                "expected `{name}` defined here and internalized after the link:\n{ir}"
            );
        }
        assert!(
            ir.contains("device memory allocation failed"),
            "an allocation that fails must abort rather than hand a kernel null:\n{ir}"
        );
        assert!(
            ir.contains("call ptr @mgpuMemAlloc(i64 67108864, ptr null, i8 0)"),
            "expected the chunk reserved as plain device memory:\n{ir}"
        );
    }

    #[test]
    fn a_pool_releases_the_device_arena_after_its_sweep() {
        let source = format!(
            "{SOURCE}
            func main() -> i32 {{
                pool {{
                    val n = 1
                }}
                return 0
            }}"
        );
        let device = device_ir(&source);
        let main = &device[position(&device, "define i32 @main(", 0)..];
        let released = position(main, "call void @__neuro_arena_release(", 0);
        position(
            main,
            "store i64 %device.mark, ptr @__neuro_device_arena_offset",
            released,
        );

        let host = linked_ir(&source, BODIES, BodyMemory::Host);
        assert!(
            !host.contains("__neuro_device_arena"),
            "a host-only program owes the device nothing:\n{host}"
        );
    }
}
