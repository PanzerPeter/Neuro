use std::collections::HashMap;

use inkwell::context::Context;
use inkwell::memory_buffer::MemoryBuffer;
use inkwell::module::{Linkage, Module};
use inkwell::types::{BasicMetadataTypeEnum, BasicTypeEnum};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PointerValue,
};
use inkwell::{AddressSpace, IntPredicate};
use neuro_hir::HirFunction;

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;
use crate::{BodyMemory, ExternalBodies};

use super::context::CodegenContext;
use super::device_memory::{
    DEVICE_ALLOC_FN, DEVICE_CHECK_FN, DEVICE_CLONE_FN, DEVICE_COPY_FN, DEVICE_JOIN_FN,
    DEVICE_MARK_FN, DEVICE_MOVE_FN, DEVICE_RELEASE_FN, DEVICE_RESTORE_FN, DEVICE_SWITCH_FN,
    DEVICE_TENSOR_ALLOC_FN, DEVICE_TENSOR_FREE_FN, DEVICE_UPLOAD_FN, GPU_FALLBACK_GLOBAL,
    GPU_PANIC_FN,
};

/// The GPU runtime ABI a device body calls, over CUDA and over HIP, one of them linked in
/// with the first set of device bodies. See `gpu_runtime.c`, the provenance of both.
const CUDA_RUNTIME_IR: &str = include_str!("gpu_runtime.ll");
const HIP_RUNTIME_IR: &str = include_str!("gpu_runtime_hip.ll");

/// The bits of a checked body's status word that number the check it failed. The bits
/// above them order failures for the body; see [`crate::ExternalBodies::guards`].
const STATUS_CHECK_MASK: u64 = 0xFFF;

/// The runtime's answer to which body a `@gpu(fallback: true)` function runs: nonzero
/// when it found a usable GPU.
const GPU_USABLE_FN: &str = "__neuro_gpu_usable";

/// A fallback function's two bodies are named after it with these. A `.` cannot appear in
/// a Neuro identifier, so neither can collide with a function the program declares.
const GPU_BODY_SUFFIX: &str = ".gpu";
const HOST_BODY_SUFFIX: &str = ".host";

/// What the runtime defines for the launchers, the staging and device tensors, made
/// internal once linked.
const GPU_RUNTIME_ENTRY_POINTS: [&str; 26] = [
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
    DEVICE_ALLOC_FN,
    DEVICE_RELEASE_FN,
    DEVICE_MARK_FN,
    DEVICE_RESTORE_FN,
    DEVICE_SWITCH_FN,
    DEVICE_JOIN_FN,
    DEVICE_TENSOR_ALLOC_FN,
    DEVICE_UPLOAD_FN,
    DEVICE_COPY_FN,
    DEVICE_CLONE_FN,
    DEVICE_MOVE_FN,
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
    /// after them (one per tensor of a tuple result), naming a buffer this side allocates. The function keeps Neuro's own
    /// ABI around it, so no caller can tell which backend computed the body.
    ///
    /// Ownership is the ordinary function's: a by-value tensor parameter was moved in,
    /// so it is released once the body has read it, and a `&Tensor` one is only read.
    ///
    /// When `memory` is a device's, every buffer `symbol` sees is device memory. A host
    /// tensor operand is copied there and a device one passed as it is. With every operand
    /// on the host, the call runs on GPU 0 and the result is written to a device buffer and
    /// copied back into the host tensor returned, so such a caller still passes and
    /// receives host tensors; with any operand on a device, the call runs on that device
    /// and the result is a device tensor there. Operands on two devices abort. Every staged
    /// copy is released before the function returns. A host body refuses a device tensor
    /// at run time.
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
        let params = (0..func_def.params.len())
            .map(|index| {
                function.get_nth_param(index as u32).ok_or_else(|| {
                    CodegenError::InternalError(format!("missing parameter {index}"))
                })
            })
            .collect::<CodegenResult<Vec<_>>>()?;
        match self.emit_external_call(func_def, symbol, memory, &params)? {
            Some(value) => self.builder.build_return(Some(&value))?,
            None => self.builder.build_return(None)?,
        };
        Ok(())
    }

    /// The call [`Self::codegen_external_body`] makes, emitted where the builder stands over
    /// `values`, the arguments of `func_def`'s parameters, and the value it returns. A call
    /// to a host body is expanded at its call site this way, so its result is allocated
    /// where the caller allocates, the arena of a `pool` body included.
    pub(crate) fn emit_external_call(
        &mut self,
        func_def: &HirFunction,
        symbol: &str,
        memory: BodyMemory,
        values: &[BasicValueEnum<'ctx>],
    ) -> CodegenResult<Option<BasicValueEnum<'ctx>>> {
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let mut arg_types: Vec<BasicMetadataTypeEnum<'ctx>> = Vec::new();
        let mut args: Vec<BasicMetadataValueEnum<'ctx>> = Vec::new();
        let mut consumed = Vec::new();

        // Every tensor handle is read before any is staged, because where the call runs,
        // and so where its first staged copy goes, depends on all of them.
        let mut operands = Vec::new();
        for (param, value) in func_def.params.iter().zip(values.iter().copied()) {
            let ty = Type::from_hir(&param.ty);
            let handle = match &ty {
                Type::Tensor { .. } => {
                    consumed.push(value.into_pointer_value());
                    Some(value.into_pointer_value())
                }
                // A `&Tensor` is the address of a cell holding the handle.
                Type::Reference { inner, .. } if matches!(**inner, Type::Tensor { .. }) => Some(
                    self.builder
                        .build_load(ptr_type, value.into_pointer_value(), "external.borrow")?
                        .into_pointer_value(),
                ),
                _ => None,
            };
            operands.push((value, ty, handle));
        }
        let mut staging = match memory {
            BodyMemory::Host => None,
            BodyMemory::Device => {
                let handles: Vec<_> = operands.iter().filter_map(|(_, _, h)| *h).collect();
                Some(self.open_device_staging(&handles)?)
            }
        };

        // What the kernels write back: a `&mut` tensor operand, and the result.
        let mut written_back = Vec::new();
        for (value, ty, handle) in operands {
            let Some(handle) = handle else {
                arg_types.push(value.get_type().into());
                args.push(value.into());
                continue;
            };
            let tensor_ty = ty.referent();
            let data = match staging.as_mut() {
                Some(staging) if matches!(ty, Type::Reference { mutable: true, .. }) => {
                    let output = self.stage_output(staging, tensor_ty, handle)?;
                    let written = output.written;
                    written_back.push((tensor_ty.clone(), output));
                    written
                }
                Some(staging) => self.stage_operand(staging, tensor_ty, handle)?,
                None => self.load_host_data(handle, func_def.span.start)?,
            };
            self.push_memref_descriptor(tensor_ty, data, &mut arg_types, &mut args)?;
        }

        // A checked body reports a failed check through a status word, which it takes
        // between its parameters and its results.
        let guards = self
            .external_guards
            .get(symbol)
            .cloned()
            .unwrap_or_default();
        let status = match guards.is_empty() {
            true => None,
            false => {
                let status_ty = status_type();
                let i64_type = self.context.i64_type();
                // All ones, above every key, which a failed check lowers. The slot sits
                // in the entry block: an expanded call is emitted where it is written,
                // and a slot allocated inside a loop body grows the frame per iteration.
                let slot = self.entry_alloca(i64_type, "external.status")?;
                self.builder.build_store(slot, i64_type.const_all_ones())?;
                let data = match staging.as_mut() {
                    Some(staging) => {
                        let staged = self.stage_status(staging, slot, &status_ty)?;
                        let written = staged.written;
                        written_back.push((status_ty.clone(), staged));
                        written
                    }
                    None => slot,
                };
                self.push_memref_descriptor(&status_ty, data, &mut arg_types, &mut args)?;
                Some(slot)
            }
        };

        // A `@kernel` returns nothing: it computes into its `&mut` tensors. A tuple
        // (`.topk`'s values and indices) is one out-param per tensor, in order.
        let result_ty = Type::from_hir(&func_def.return_type);
        let parts = match &result_ty {
            Type::Void => Vec::new(),
            Type::Tuple(parts) => parts.clone(),
            single => vec![single.clone()],
        };
        let mut results = Vec::with_capacity(parts.len());
        for part in &parts {
            let (handle, written) = match staging.as_mut() {
                Some(staging) => {
                    let staged = self.stage_result(staging, part)?;
                    let (handle, written) = (staged.handle, staged.written);
                    written_back.push((part.clone(), staged));
                    (handle, written)
                }
                None => {
                    let result = self.alloc_dlpack_tensor(part, "external.result")?;
                    (result, self.load_dlpack_data(result)?)
                }
            };
            self.push_memref_descriptor(part, written, &mut arg_types, &mut args)?;
            results.push(handle);
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
            self.close_device_staging(staging, &written_back)?;
        }
        if let Some(slot) = status {
            let i64_type = self.context.i64_type();
            let key = self
                .builder
                .build_load(i64_type, slot, "external.status")?
                .into_int_value();
            let failed = self.builder.build_and(
                key,
                i64_type.const_int(STATUS_CHECK_MASK, false),
                "external.failed",
            )?;
            for (index, guard) in guards.iter().enumerate() {
                let number = i64_type.const_int(index as u64 + 1, false);
                let ok = self.builder.build_int_compare(
                    IntPredicate::NE,
                    failed,
                    number,
                    "external.passed",
                )?;
                self.codegen_body_guard(ok, guard.kind, guard.offset)?;
            }
        }
        for handle in consumed {
            self.build_dlpack_release(handle)?;
        }
        match (&result_ty, results.as_slice()) {
            (Type::Void, _) => Ok(None),
            (Type::Tuple(_), handles) => {
                let BasicTypeEnum::StructType(tuple) = self.get_any_llvm_type(&result_ty)? else {
                    return Err(CodegenError::InternalError(
                        "a tuple result does not lower to a struct".to_string(),
                    ));
                };
                let mut packed = tuple.get_undef();
                for (index, handle) in handles.iter().enumerate() {
                    packed = self
                        .builder
                        .build_insert_value(packed, *handle, index as u32, "external.pair")?
                        .into_struct_value();
                }
                Ok(Some(packed.into()))
            }
            (_, [handle]) => Ok(Some((*handle).into())),
            _ => Err(CodegenError::InternalError(
                "a tensor result staged no buffer".to_string(),
            )),
        }
    }

    /// Define `func_def`, a `@gpu(fallback: true)` function whose kernels `symbol`
    /// launches, as a choice between two bodies: the staged device one where the runtime
    /// found a usable GPU, and the host one where it did not: `host`, a body lowered
    /// outside this backend, or else this backend's own. The runtime probes once, when the
    /// first module loads before `main`, so every call in a run takes the same branch.
    pub(crate) fn codegen_gpu_fallback(
        &mut self,
        func_def: &HirFunction,
        symbol: &str,
        host: Option<&str>,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        self.codegen_body_choice(func_def, (symbol, host), func_types, |this, _| {
            let i32_type = this.context.i32_type();
            let probe = this.extern_fn(GPU_USABLE_FN, i32_type.fn_type(&[], false));
            let usable = this
                .builder
                .build_call(probe, &[], "gpu.usable")?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| {
                    CodegenError::InternalError(format!("{GPU_USABLE_FN} returned void"))
                })?
                .into_int_value();
            Ok(this.builder.build_int_compare(
                IntPredicate::NE,
                usable,
                i32_type.const_zero(),
                "gpu.chosen",
            )?)
        })
    }

    /// Define `func_def`, a function outlined from one tensor operation whose kernels
    /// `symbol` launches, as a choice made per call: the staged device body when any tensor
    /// operand lives on a GPU, and the host body (`host`, as for
    /// [`Self::codegen_gpu_fallback`]) when every one is a host tensor.
    pub(crate) fn codegen_follows_operands(
        &mut self,
        func_def: &HirFunction,
        symbol: &str,
        host: Option<&str>,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        self.codegen_body_choice(func_def, (symbol, host), func_types, |this, function| {
            let ptr_type = this.context.ptr_type(AddressSpace::default());
            let mut resident = this.context.bool_type().const_zero();
            for (index, param) in func_def.params.iter().enumerate() {
                let value = function.get_nth_param(index as u32).ok_or_else(|| {
                    CodegenError::InternalError(format!("missing parameter {index}"))
                })?;
                let handle = match Type::from_hir(&param.ty) {
                    Type::Tensor { .. } => value.into_pointer_value(),
                    Type::Reference { inner, .. } if matches!(*inner, Type::Tensor { .. }) => this
                        .builder
                        .build_load(ptr_type, value.into_pointer_value(), "operand.handle")?
                        .into_pointer_value(),
                    _ => continue,
                };
                let on_host = this.dlpack_on_host(handle)?;
                let here = this.builder.build_not(on_host, "operand.resident")?;
                resident = this.builder.build_or(resident, here, "device.chosen")?;
            }
            Ok(resident)
        })
    }

    /// Define `func_def` as a branch between two bodies of its own: the staged device one
    /// launching `device`'s kernels where `choose` answers true, and the host one elsewhere,
    /// `host`'s if there is one. `choose` is emitted at the entry of the function being
    /// defined.
    fn codegen_body_choice(
        &mut self,
        func_def: &HirFunction,
        (device, host): (&str, Option<&str>),
        func_types: &HashMap<String, Type>,
        choose: impl FnOnce(&mut Self, FunctionValue<'ctx>) -> CodegenResult<IntValue<'ctx>>,
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
        self.codegen_external_body(&gpu, device, BodyMemory::Device)?;
        let host_copy = self.declare_body_copy(func_def, function, HOST_BODY_SUFFIX);
        match host {
            Some(symbol) => self.codegen_external_body(&host_copy, symbol, BodyMemory::Host)?,
            None => self.codegen_function(
                &host_copy,
                &HashMap::from([(host_copy.name.clone(), signature)]),
            )?,
        }

        let entry = self.context.append_basic_block(function, "entry");
        let on_gpu = self.context.append_basic_block(function, "on_gpu");
        let on_host = self.context.append_basic_block(function, "on_host");
        self.builder.position_at_end(entry);
        self.current_function = Some(function);
        let chosen = choose(self, function)?;
        self.builder
            .build_conditional_branch(chosen, on_gpu, on_host)?;

        let args: Vec<BasicMetadataValueEnum<'ctx>> =
            function.get_param_iter().map(Into::into).collect();
        for (block, body) in [(on_gpu, &gpu.name), (on_host, &host_copy.name)] {
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

/// The status word a checked body reports through, as the one-element tensor its descriptor
/// describes.
fn status_type() -> Type {
    Type::Tensor {
        element: Box::new(Type::I64),
        shape: vec![Some(1)],
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
    let parsed = parse_ir(context, &bodies.llvm_ir, "external bodies")?;
    // A host body is the program's tensor arithmetic, so it is optimized whatever `-O` the
    // program is built at, the way a library it calls would be. It computes the same bits:
    // nothing here licenses reassociating float arithmetic, and its checks are code of its
    // own.
    if bodies.memory == BodyMemory::Host {
        let (machine, triple) = crate::host_target_machine(crate::OptimizationLevelSetting::O2)?;
        parsed.set_data_layout(&machine.get_target_data().get_data_layout());
        parsed.set_triple(&triple);
        parsed
            .run_passes(
                "default<O2>",
                &machine,
                inkwell::passes::PassBuilderOptions::create(),
            )
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to optimize external bodies: {e}"))
            })?;
    }
    module
        .link_in_module(parsed)
        .map_err(|e| CodegenError::LlvmError(format!("failed to link external bodies: {e}")))?;

    for (_, symbol) in &bodies.functions {
        let function = module.get_function(symbol).ok_or_else(|| {
            CodegenError::LlvmError(format!("external bodies do not define `{symbol}`"))
        })?;
        function.set_linkage(Linkage::Internal);
    }
    Ok(())
}

/// Link `vendor`'s GPU runtime into `module`, whose device bodies and staging call it,
/// and make its entry points internal: nothing outside the program calls them.
pub(crate) fn link_gpu_runtime<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    vendor: crate::GpuVendor,
) -> CodegenResult<()> {
    let runtime = match vendor {
        crate::GpuVendor::Nvidia => CUDA_RUNTIME_IR,
        crate::GpuVendor::Amd => HIP_RUNTIME_IR,
    };
    link_ir(context, module, runtime, "the GPU runtime")?;
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
    module
        .link_in_module(parse_ir(context, ir, what)?)
        .map_err(|e| CodegenError::LlvmError(format!("failed to link {what}: {e}")))
}

fn parse_ir<'ctx>(context: &'ctx Context, ir: &str, what: &str) -> CodegenResult<Module<'ctx>> {
    // The IR parser reads a C string, so the text needs the terminator it lacks.
    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = MemoryBuffer::create_from_memory_range_copy(&bytes, what);
    context
        .create_module_from_ir(buffer)
        .map_err(|e| CodegenError::LlvmError(format!("failed to parse {what}: {e}")))
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
    use crate::{BodyMemory, ExternalBodies, GpuVendor, OptimizationLevelSetting, build_module};
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
            guards: Vec::new(),
        }
    }

    fn linked_ir(source: &str, bodies_ir: &str, memory: BodyMemory) -> String {
        linked(source, &[both(bodies_ir, memory)], GpuVendor::Nvidia)
    }

    fn linked(source: &str, external: &[ExternalBodies], gpu: GpuVendor) -> String {
        let ast = syntax_parsing::parse(source).expect("parsing failed");
        let mut hir = hir_lowering::lower_program(&ast).expect("HIR lowering failed");
        // The functions under test have bodies of their own here, so nothing calls the
        // operations lowering outlined out of them.
        hir.items
            .retain(|item| !matches!(item, neuro_hir::HirItem::Function(f) if f.target.outlined()));
        let context = Context::create();
        let codegen_ctx = build_module(
            &context,
            &hir,
            OptimizationLevelSetting::O0,
            source,
            "external.nr",
            external,
            gpu,
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

    /// The device stand-ins beside host ones of their own, as a fallback function has both.
    fn fallback_ir(source: &str) -> String {
        let host = ExternalBodies {
            llvm_ir: BODIES.replace("@ext_", "@ext_host_"),
            functions: vec![
                ("scale".to_string(), "ext_host_scale".to_string()),
                ("consume".to_string(), "ext_host_consume".to_string()),
            ],
            memory: BodyMemory::Host,
            guards: Vec::new(),
        };
        linked(
            source,
            &[host, both(DEVICE_BODIES, BodyMemory::Device)],
            GpuVendor::Nvidia,
        )
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

        // The call's device is settled before its first allocation: the stream and the
        // arena are the current device's.
        let joined = position(
            scale,
            "%device.index = call i32 @__neuro_device_join(i32 -1, i32 %device.operand.index)",
            0,
        );
        let switched = position(
            scale,
            "%device.previous = call i32 @__neuro_device_switch(i32 %device.index)",
            joined,
        );
        let marked = position(
            scale,
            "%device.mark = call i64 @__neuro_device_mark()",
            switched,
        );
        let stream = position(scale, "call ptr @mgpuStreamCreate()", marked);
        let placed = position(scale, "label %device.stage, label %device.staged", stream);
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
        let restored = position(
            scale,
            "call void @__neuro_device_restore(i64 %device.mark)",
            settled,
        );
        position(
            scale,
            "call i32 @__neuro_device_switch(i32 %device.previous)",
            restored,
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
                && on_device
                    .contains("insertvalue { i32, i32 } { i32 2, i32 0 }, i32 %device.index, 1"),
            "a result left on the device is a kDLCUDA tensor on the call's device, \
             released by the runtime:\n{on_device}"
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
    fn a_kernel_returns_nothing_and_copies_its_mut_tensor_back() {
        const KERNEL: &str = r#"
            @kernel(threads: [2])
            func fill(a: &Tensor<f32, [2]>, out: &mut Tensor<f32, [2]>) {
                out[thread_id.x] = a[thread_id.x]
            }
        "#;
        const LAUNCHER: &str = r#"
            define void @ext_fill(ptr %0, ptr %1, i64 %2, i64 %3, i64 %4, ptr %5, ptr %6, i64 %7, i64 %8, i64 %9) {
              ret void
            }
        "#;
        let ast = syntax_parsing::parse(KERNEL).expect("parsing failed");
        let hir = hir_lowering::lower_program(&ast).expect("HIR lowering failed");
        let context = Context::create();
        let external = [ExternalBodies {
            llvm_ir: LAUNCHER.to_string(),
            functions: vec![("fill".to_string(), "ext_fill".to_string())],
            memory: BodyMemory::Device,
            guards: Vec::new(),
        }];
        let ir = build_module(
            &context,
            &hir,
            OptimizationLevelSetting::O0,
            KERNEL,
            "kernel.nr",
            &external,
            GpuVendor::Nvidia,
        )
        .expect("a module with a kernel launcher should build and verify")
        .module
        .print_to_string()
        .to_string();

        let start = ir
            .find("define void @fill(")
            .unwrap_or_else(|| panic!("a kernel is a void function:\n{ir}"));
        let fill = &ir[start..start + ir[start..].find("\n}").unwrap_or(0)];
        let call = position(fill, "call void @ext_fill(", 0);
        let copy_back = position(fill, "call void @mgpuMemcpy(ptr %device.written_back", call);
        position(fill, "call void @mgpuStreamSynchronize(", copy_back);
        assert_eq!(
            fill.matches("call void @mgpuMemcpy(").count(),
            3,
            "each tensor is staged in and only the `&mut` one is copied back:\n{fill}"
        );
        assert!(
            !fill.contains("__neuro_device_alloc(") && fill.contains("ret void"),
            "a kernel builds no result tensor:\n{fill}"
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
                "expected `{name}` defined by the runtime and internalized after the link:\n{ir}"
            );
        }
        assert!(
            ir.contains("device memory allocation failed"),
            "an allocation that fails must abort rather than hand a kernel null:\n{ir}"
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
        let marked = position(main, "%device.mark = call i64 @__neuro_device_mark()", 0);
        let released = position(main, "call void @__neuro_arena_release(", marked);
        position(
            main,
            "call void @__neuro_device_restore(i64 %device.mark)",
            released,
        );

        let host = linked_ir(&source, BODIES, BodyMemory::Host);
        assert!(
            !host.contains("__neuro_device_mark"),
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
                    guards: Vec::new(),
                },
                ExternalBodies {
                    llvm_ir: HOST_CONSUME.to_string(),
                    functions: vec![("consume".to_string(), "ext_consume".to_string())],
                    memory: BodyMemory::Host,
                    guards: Vec::new(),
                },
            ],
            GpuVendor::Nvidia,
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

    #[test]
    fn an_amd_build_brings_the_hip_runtime() {
        let device = both(DEVICE_BODIES, BodyMemory::Device);
        let ir = linked(SOURCE, &[device], GpuVendor::Amd);
        for name in super::GPU_RUNTIME_ENTRY_POINTS {
            let line = definition(&ir, name).unwrap_or_else(|| {
                panic!(
                    "expected `{name}` defined by the runtime:
{ir}"
                )
            });
            assert!(line.starts_with("define internal"), "{line}");
        }
        assert!(
            ir.contains("libamdhip64.so")
                && ir.contains("needs an AMD GPU")
                && ir.contains("hipModuleLaunchKernel")
                && !ir.contains("libcuda"),
            "HIP is opened at run time in place of the CUDA driver:
{ir}"
        );
        assert!(
            body(&ir, "scale")
                .contains("insertvalue { i32, i32 } { i32 10, i32 0 }, i32 %device.index, 1"),
            "a result left on an AMD GPU is a kDLROCM tensor:
{ir}"
        );
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
        let ir = fallback_ir(&attributed(fallback, fallback));
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
            host.starts_with("define internal")
                && host.contains("call void @ext_host_scale(")
                && !host.contains("mgpu"),
            "the host body is the one computed for the host:\n{host}"
        );
        assert!(
            ir.contains("@__neuro_gpu_fallback = internal constant i8 1"),
            "with every function falling back, a missing GPU is not fatal:\n{ir}"
        );
    }

    #[test]
    fn one_bare_gpu_function_keeps_a_missing_gpu_fatal() {
        let ir = fallback_ir(&attributed("@gpu", "@gpu(fallback: true)"));
        assert!(
            ir.contains("@__neuro_gpu_fallback = internal constant i8 0"),
            "{ir}"
        );
        assert!(!body(&ir, "scale").contains("__neuro_gpu_usable"), "{ir}");
        assert!(body(&ir, "consume").contains("__neuro_gpu_usable"), "{ir}");
    }

    #[test]
    fn a_checked_body_takes_a_status_word_and_its_caller_panics_for_it() {
        // `consume`'s body takes the status word, one more rank-1 descriptor, between its
        // operand and its result.
        const CHECKED: &str = r#"
            define void @ext_consume(ptr %0, ptr %1, i64 %2, i64 %3, i64 %4, ptr %5, ptr %6, i64 %7, i64 %8, i64 %9, ptr %10, ptr %11, i64 %12, i64 %13, i64 %14) {
              ret void
            }
        "#;
        let guards = vec![(
            "ext_consume".to_string(),
            vec![
                crate::BodyGuard {
                    kind: crate::BodyGuardKind::DivisionByZero,
                    offset: 0,
                },
                crate::BodyGuard {
                    kind: crate::BodyGuardKind::Overflow,
                    offset: 0,
                },
            ],
        )];
        for memory in [BodyMemory::Host, BodyMemory::Device] {
            let external = ExternalBodies {
                llvm_ir: CHECKED.to_string(),
                functions: vec![("consume".to_string(), "ext_consume".to_string())],
                memory,
                guards: guards.clone(),
            };
            let ast = syntax_parsing::parse(SOURCE).expect("parsing failed");
            let mut hir = hir_lowering::lower_program(&ast).expect("HIR lowering failed");
            // Only `consume` is under test, and its body is the stand-in.
            hir.items.retain(|item| {
                !matches!(item, neuro_hir::HirItem::Function(f)
                    if f.name == "scale" || f.target.outlined())
            });
            let context = Context::create();
            let ir = build_module(
                &context,
                &hir,
                OptimizationLevelSetting::O0,
                SOURCE,
                "checked.nr",
                &[external],
                GpuVendor::Nvidia,
            )
            .expect("a checked body links")
            .module
            .print_to_string()
            .to_string();
            let consume = body(&ir, "consume");
            let filled = position(consume, "store i64 -1, ptr %external.status", 0);
            let called = position(consume, "call void @ext_consume(", filled);
            let read = position(consume, "load i64, ptr %external.status", called);
            position(consume, "and i64 %external.status", read);
            assert!(
                ir.contains("panic: division by zero at checked.nr")
                    && ir.contains("panic: integer overflow at checked.nr"),
                "each check panics with the backend's own words:\n{ir}"
            );
            if memory == BodyMemory::Device {
                let staged = position(consume, "call void @mgpuMemcpy(", filled);
                assert!(
                    staged < called,
                    "the word reaches the device first:\n{consume}"
                );
            }
        }
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
