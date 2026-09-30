use std::collections::HashMap;

use inkwell::context::Context;
use inkwell::memory_buffer::MemoryBuffer;
use inkwell::module::{Linkage, Module};
use inkwell::types::BasicMetadataTypeEnum;
use inkwell::values::{BasicMetadataValueEnum, FunctionValue, PointerValue};
use inkwell::{AddressSpace, IntPredicate};
use neuro_hir::HirFunction;

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;
use crate::{BodyMemory, ExternalBodies};

use super::context::CodegenContext;
use super::device_memory::{
    DEVICE_ALLOC_FN, DEVICE_CHECK_FN, DEVICE_DOWNLOAD_FN, DEVICE_RELEASE_FN,
    DEVICE_TENSOR_ALLOC_FN, DEVICE_TENSOR_FREE_FN, DEVICE_UPLOAD_FN, GPU_FALLBACK_GLOBAL,
    GPU_PANIC_FN,
};

/// The CUDA implementation of the GPU runtime ABI a device body calls, linked in with
/// the first set of device bodies. See `gpu_runtime.c`, its provenance.
const GPU_RUNTIME_IR: &str = include_str!("gpu_runtime.ll");

/// The runtime's answer to which body a `@gpu(fallback: true)` function runs: nonzero
/// when it found a usable GPU.
const GPU_USABLE_FN: &str = "__neuro_gpu_usable";

/// A fallback function's two bodies are named after it with these. A `.` cannot appear in
/// a Neuro identifier, so neither can collide with a function the program declares.
const GPU_BODY_SUFFIX: &str = ".gpu";
const HOST_BODY_SUFFIX: &str = ".host";

/// What the runtime defines for the launchers, the staging and device tensors, made
/// internal once linked.
const GPU_RUNTIME_ENTRY_POINTS: [&str; 18] = [
    "mgpuModuleLoad",
    "mgpuModuleLoadJIT",
    "mgpuModuleUnload",
    "mgpuModuleGetFunction",
    "mgpuLaunchKernel",
    "mgpuStreamCreate",
    "mgpuStreamSynchronize",
    "mgpuStreamDestroy",
    "mgpuMemAlloc",
    "mgpuMemFree",
    "mgpuMemcpy",
    DEVICE_TENSOR_ALLOC_FN,
    DEVICE_UPLOAD_FN,
    DEVICE_DOWNLOAD_FN,
    DEVICE_TENSOR_FREE_FN,
    DEVICE_CHECK_FN,
    GPU_USABLE_FN,
    GPU_PANIC_FN,
];

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
    /// When `memory` is a device's, every buffer `symbol` sees is device memory. A host
    /// tensor operand is copied there and a device one passed as it is. With every operand
    /// on the host, the result is written to a device buffer and copied back into the host
    /// tensor returned, so such a caller still passes and receives host tensors; with any
    /// operand on the device, the result is a device tensor. Every staged copy is released
    /// before the function returns. A host body refuses a device tensor at run time.
    pub(crate) fn codegen_external_body(
        &mut self,
        func_def: &HirFunction,
        symbol: &str,
        memory: BodyMemory,
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
        let mut staging = match memory {
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
            let data = match staging.as_mut() {
                Some(staging) => self.stage_operand(staging, ty.referent(), handle)?,
                None => self.load_host_data(handle, func_def.span.start)?,
            };
            self.push_memref_descriptor(ty.referent(), data, &mut arg_types, &mut args)?;
        }

        let result_ty = Type::from_hir(&func_def.return_type);
        let staged_result = match staging.as_mut() {
            Some(staging) => Some(self.stage_result(staging, &result_ty)?),
            None => None,
        };
        let (result, written) = match &staged_result {
            Some(staged) => (staged.handle, staged.written),
            None => {
                let result = self.alloc_dlpack_tensor(&result_ty, "external.result")?;
                (result, self.load_dlpack_data(result)?)
            }
        };
        self.push_memref_descriptor(&result_ty, written, &mut arg_types, &mut args)?;

        let callee = self.module.get_function(symbol).unwrap_or_else(|| {
            self.module.add_function(
                symbol,
                self.context.void_type().fn_type(&arg_types, false),
                None,
            )
        });
        self.builder.build_call(callee, &args, "")?;

        if let (Some(staging), Some(staged)) = (staging, staged_result) {
            self.close_device_staging(staging, &result_ty, &staged)?;
        }
        for handle in consumed {
            self.build_dlpack_release(handle)?;
        }
        self.builder.build_return(Some(&result))?;
        Ok(())
    }

    /// Define `func_def`, a `@gpu(fallback: true)` function whose kernels `symbol`
    /// launches, as a choice between two bodies: the staged device one where the runtime
    /// found a usable GPU, and this backend's own host body where it did not. The runtime
    /// probes once, when the first module loads before `main`, so every call in a run takes
    /// the same branch.
    pub(crate) fn codegen_gpu_fallback(
        &mut self,
        func_def: &HirFunction,
        symbol: &str,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        let function = *self
            .functions
            .get(&func_def.name)
            .ok_or_else(|| CodegenError::UndefinedFunction(func_def.name.clone()))?;
        let signature = func_types
            .get(&func_def.name)
            .cloned()
            .ok_or_else(|| CodegenError::UndefinedFunction(func_def.name.clone()))?;

        let gpu = self.declare_body_copy(func_def, function, GPU_BODY_SUFFIX);
        self.codegen_external_body(&gpu, symbol, BodyMemory::Device)?;
        let host = self.declare_body_copy(func_def, function, HOST_BODY_SUFFIX);
        self.codegen_function(&host, &HashMap::from([(host.name.clone(), signature)]))?;

        let entry = self.context.append_basic_block(function, "entry");
        let on_gpu = self.context.append_basic_block(function, "on_gpu");
        let on_host = self.context.append_basic_block(function, "on_host");
        self.builder.position_at_end(entry);
        self.current_function = Some(function);
        let i32_type = self.context.i32_type();
        let probe = self.extern_fn(GPU_USABLE_FN, i32_type.fn_type(&[], false));
        let usable = self
            .builder
            .build_call(probe, &[], "gpu.usable")?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError(format!("{GPU_USABLE_FN} returned void")))?
            .into_int_value();
        let chosen = self.builder.build_int_compare(
            IntPredicate::NE,
            usable,
            i32_type.const_zero(),
            "gpu.chosen",
        )?;
        self.builder
            .build_conditional_branch(chosen, on_gpu, on_host)?;

        let args: Vec<BasicMetadataValueEnum<'ctx>> =
            function.get_param_iter().map(Into::into).collect();
        for (block, body) in [(on_gpu, &gpu.name), (on_host, &host.name)] {
            self.builder.position_at_end(block);
            let callee = *self
                .functions
                .get(body)
                .ok_or_else(|| CodegenError::UndefinedFunction(body.clone()))?;
            let result = self.builder.build_call(callee, &args, "")?;
            match result.try_as_basic_value().basic() {
                Some(value) => self.builder.build_return(Some(&value))?,
                None => self.builder.build_return(None)?,
            };
        }
        Ok(())
    }

    /// `func_def` renamed with `suffix`, declared internal with `function`'s signature.
    fn declare_body_copy(
        &mut self,
        func_def: &HirFunction,
        function: FunctionValue<'ctx>,
        suffix: &str,
    ) -> HirFunction {
        let copy = HirFunction {
            name: format!("{}{suffix}", func_def.name),
            ..func_def.clone()
        };
        let declared =
            self.module
                .add_function(&copy.name, function.get_type(), Some(Linkage::Internal));
        self.functions.insert(copy.name.clone(), declared);
        copy
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
    link_ir(context, module, &bodies.llvm_ir, "external bodies")?;

    for (_, symbol) in &bodies.functions {
        let function = module.get_function(symbol).ok_or_else(|| {
            CodegenError::LlvmError(format!("external bodies do not define `{symbol}`"))
        })?;
        function.set_linkage(Linkage::Internal);
    }
    // The device allocator stayed external only so the launchers' declarations of its
    // names would resolve to it.
    internalize(module, &[DEVICE_ALLOC_FN, DEVICE_RELEASE_FN]);
    Ok(())
}

/// Link the GPU runtime into `module`, whose device bodies and staging call it, and
/// make its entry points internal: nothing outside the program calls them.
pub(crate) fn link_gpu_runtime<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> CodegenResult<()> {
    link_ir(context, module, GPU_RUNTIME_IR, "the GPU runtime")?;
    internalize(module, &GPU_RUNTIME_ENTRY_POINTS);
    if let Some(flag) = module.get_global(GPU_FALLBACK_GLOBAL) {
        flag.set_linkage(Linkage::Internal);
    }
    Ok(())
}

fn link_ir<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    ir: &str,
    what: &str,
) -> CodegenResult<()> {
    // The IR parser reads a C string, so the text needs the terminator it lacks.
    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = MemoryBuffer::create_from_memory_range_copy(&bytes, what);
    let parsed = context
        .create_module_from_ir(buffer)
        .map_err(|e| CodegenError::LlvmError(format!("failed to parse {what}: {e}")))?;
    module
        .link_in_module(parsed)
        .map_err(|e| CodegenError::LlvmError(format!("failed to link {what}: {e}")))
}

fn internalize(module: &Module<'_>, names: &[&str]) {
    for name in names {
        if let Some(function) = module.get_function(name) {
            function.set_linkage(Linkage::Internal);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{BodyMemory, ExternalBodies, OptimizationLevelSetting, build_module};
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

    fn both(bodies_ir: &str, memory: BodyMemory) -> ExternalBodies {
        ExternalBodies {
            llvm_ir: bodies_ir.to_string(),
            functions: vec![
                ("scale".to_string(), "ext_scale".to_string()),
                ("consume".to_string(), "ext_consume".to_string()),
            ],
            memory,
        }
    }

    fn linked_ir(source: &str, bodies_ir: &str, memory: BodyMemory) -> String {
        linked(source, &[both(bodies_ir, memory)])
    }

    fn linked(source: &str, external: &[ExternalBodies]) -> String {
        let ast = syntax_parsing::parse(source).expect("parsing failed");
        let hir = hir_lowering::lower_program(&ast).expect("HIR lowering failed");
        let context = Context::create();
        let codegen_ctx = build_module(
            &context,
            &hir,
            OptimizationLevelSetting::O0,
            source,
            "external.nr",
            external,
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
    fn a_device_body_stages_a_host_operand_and_passes_a_device_one_as_it_is() {
        let ir = device_ir(SOURCE);
        let scale = body(&ir, "scale");

        let stream = position(scale, "call ptr @mgpuStreamCreate()", 0);
        let placed = position(
            scale,
            "br i1 %dlpack.on_host, label %device.stage, label %device.staged",
            stream,
        );
        let copy_in = position(
            scale,
            "call void @mgpuMemcpy(ptr %device.buffer, ptr %dlpack.data,",
            placed,
        );
        position(
            scale,
            "%device.operand = phi ptr [ %device.buffer, %device.stage ], [ %dlpack.data, %entry ]",
            copy_in,
        );
        let call = position(scale, "call void @ext_scale(ptr %device.operand", copy_in);
        let copy_out = position(
            scale,
            "call void @mgpuMemcpy(ptr %device.copy_back, ptr %device.written,",
            call,
        );
        let settled = position(scale, "call void @mgpuStreamSynchronize(", copy_out);
        assert!(
            !scale[..copy_out].contains("@mgpuStreamSynchronize("),
            "the kernels queue behind the copies on one stream, so nothing waits before them:\n{scale}"
        );
        position(scale, "call void @_mlir_memref_to_llvm_free(", settled);
        position(
            scale,
            "store i64 %device.mark, ptr @__neuro_device_arena_offset",
            settled,
        );

        assert_eq!(
            scale
                .matches("call ptr @_mlir_memref_to_llvm_alloc(")
                .count(),
            2,
            "one scratch buffer for the operand and one for a host result:\n{scale}"
        );
    }

    #[test]
    fn a_device_operand_leaves_the_result_on_the_device() {
        let ir = device_ir(SOURCE);
        let scale = body(&ir, "scale");

        let choice = position(
            scale,
            "br i1 %device.any_resident, label %device.result, label %host.result",
            0,
        );
        let resident = position(scale, "call ptr @__neuro_device_alloc(i64 8)", choice);
        let returned = position(scale, "host.result:", resident);
        let on_device = &scale[resident..returned];
        assert!(
            on_device.contains("store ptr @__neuro_dlpack_device_deleter")
                && on_device.contains("store { i32, i32 } { i32 2, i32 0 }"),
            "a result left on the device is a kDLCUDA tensor released by the runtime:\n{on_device}"
        );
        position(
            scale,
            "%device.copy_back = phi ptr [ null, %device.result ]",
            returned,
        );
        position(
            scale,
            "br i1 %device.returning, label %device.copy_out, label %device.settle",
            returned,
        );
    }

    #[test]
    fn a_host_body_refuses_a_device_tensor() {
        let ir = host_ir();
        let consume = body(&ir, "consume");
        let guard = position(consume, "%dlpack.on_host = icmp eq i32", 0);
        position(consume, "call void @ext_consume(ptr %dlpack.data", guard);
        assert!(
            ir.contains("this tensor lives on a GPU"),
            "a host body must not read device memory:\n{consume}"
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

    /// A defined function's linkage line, or `None` when `name` is only declared.
    fn definition<'a>(ir: &'a str, name: &str) -> Option<&'a str> {
        ir.lines()
            .find(|line| line.starts_with("define") && line.contains(&format!("@{name}(")))
    }

    #[test]
    fn host_and_device_bodies_link_side_by_side() {
        const DEVICE_SCALE: &str = r#"
            define void @ext_scale(ptr %0, ptr %1, i64 %2, i64 %3, i64 %4, float %5, ptr %6, ptr %7, i64 %8, i64 %9, i64 %10) {
              ret void
            }
        "#;
        const HOST_CONSUME: &str = r#"
            define void @ext_consume(ptr %0, ptr %1, i64 %2, i64 %3, i64 %4, ptr %5, ptr %6, i64 %7, i64 %8, i64 %9) {
              ret void
            }
        "#;
        let ir = linked(
            SOURCE,
            &[
                ExternalBodies {
                    llvm_ir: DEVICE_SCALE.to_string(),
                    functions: vec![("scale".to_string(), "ext_scale".to_string())],
                    memory: BodyMemory::Device,
                },
                ExternalBodies {
                    llvm_ir: HOST_CONSUME.to_string(),
                    functions: vec![("consume".to_string(), "ext_consume".to_string())],
                    memory: BodyMemory::Host,
                },
            ],
        );
        assert!(
            body(&ir, "scale").contains("call void @ext_scale(ptr %device.operand"),
            "the device body is handed device buffers:\n{ir}"
        );
        let consume = body(&ir, "consume");
        assert!(
            consume.contains("call void @ext_consume(ptr %dlpack.data")
                && !consume.contains("mgpu"),
            "the host body is handed its own buffers:\n{consume}"
        );
    }

    #[test]
    fn a_device_body_brings_the_gpu_runtime() {
        let ir = device_ir(SOURCE);
        for name in super::GPU_RUNTIME_ENTRY_POINTS {
            let line = definition(&ir, name)
                .unwrap_or_else(|| panic!("expected `{name}` defined by the runtime:\n{ir}"));
            assert!(line.starts_with("define internal"), "{line}");
        }
        assert!(
            ir.contains("libcuda.so.1") && ir.contains("needs an NVIDIA GPU"),
            "the driver is opened at run time, and its absence is reported:\n{ir}"
        );
        assert!(
            ir.contains("cannot load this program's kernels"),
            "a module the driver refuses makes the GPU unusable, for a fallback to take:\n{ir}"
        );

        let panic = &ir[position(&ir, "@__neuro_gpu_panic(ptr", 0)..];
        let write = position(panic, "call i64 @write(", 0);
        position(panic, "call void @abort()", write);
    }

    /// `SOURCE` with an attribute on each function.
    fn attributed(scale: &str, consume: &str) -> String {
        SOURCE
            .replace("func scale", &format!("{scale}\n        func scale"))
            .replace("func consume", &format!("{consume}\n        func consume"))
    }

    /// The whole definition of `name`, whatever its linkage.
    fn any_body<'a>(ir: &'a str, name: &str) -> &'a str {
        let line = definition(ir, name).unwrap_or_else(|| panic!("no `{name}`:\n{ir}"));
        let rest = &ir[position(ir, line, 0)..];
        &rest[..rest.find("\n}").unwrap_or(rest.len())]
    }

    #[test]
    fn a_fallback_function_chooses_its_device_or_host_body_per_run() {
        let fallback = "@gpu(fallback: true)";
        let ir = device_ir(&attributed(fallback, fallback));
        let scale = body(&ir, "scale");
        let probe = position(scale, "call i32 @__neuro_gpu_usable()", 0);
        position(scale, "call ptr @scale.gpu(", probe);
        position(scale, "call ptr @scale.host(", probe);

        let device = any_body(&ir, "scale.gpu");
        assert!(
            device.starts_with("define internal") && device.contains("call void @ext_scale("),
            "{device}"
        );
        let host = any_body(&ir, "scale.host");
        assert!(
            host.starts_with("define internal") && host.contains("fmul") && !host.contains("mgpu"),
            "the host body is this backend's own:\n{host}"
        );
        assert!(
            ir.contains("@__neuro_gpu_fallback = internal constant i8 1"),
            "with every function falling back, a missing GPU is not fatal:\n{ir}"
        );
    }

    #[test]
    fn one_bare_gpu_function_keeps_a_missing_gpu_fatal() {
        let ir = device_ir(&attributed("@gpu", "@gpu(fallback: true)"));
        assert!(
            ir.contains("@__neuro_gpu_fallback = internal constant i8 0"),
            "{ir}"
        );
        assert!(!body(&ir, "scale").contains("__neuro_gpu_usable"), "{ir}");
        assert!(body(&ir, "consume").contains("__neuro_gpu_usable"), "{ir}");
    }

    #[test]
    fn a_host_body_brings_no_gpu_runtime() {
        let ir = host_ir();
        assert!(
            definition(&ir, "mgpuModuleLoadJIT").is_none() && !ir.contains("__neuro_gpu_panic"),
            "{ir}"
        );
    }
}
