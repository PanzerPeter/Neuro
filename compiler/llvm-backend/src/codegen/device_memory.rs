// Device memory for the tensor bodies a GPU computes.
//
// A GPU launcher reads and writes device memory only, so the wrapper around one stages
// every host tensor it passes: each is copied into a device buffer, the kernels write a
// device result, and that result is copied back into the host tensor the caller receives.
// A tensor `.to(Device::GPU(n))` already moved is passed as it is, and a call with any such
// operand leaves its result on the device as well, in a device tensor of its own that
// outlives the call. The call runs on the GPU its device operands live on (GPU 0 when
// there are none): the staging makes that device the runtime's current one before its
// first allocation and switches back after its last.
//
// Staging allocates from the current device's arena in the GPU runtime, a linear arena
// over a chunk of device memory: bump allocation, one release for everything past a mark,
// and a spill to the driver when the chunk is full. One call's staging is one region, so a
// call costs no device allocate / free pair per buffer, which matters because a device
// free synchronizes the whole device. The buffers a launcher needs between two kernels come
// from the same arena, because its IR allocates them through `_mlir_memref_to_llvm_alloc` /
// `_mlir_memref_to_llvm_free`, which the runtime defines. A `pool` block in a program that
// runs GPU bodies takes a device mark as well and restores it after its sweep.
//
// Every device operation goes through MLIR's GPU runtime ABI (`mgpuMemAlloc`,
// `mgpuMemFree`, `mgpuMemcpy`, the `mgpuStream*` family), the one the launchers call, so a
// single runtime library serves both.
//
// The runtime hands every `mgpuStreamCreate` on a device the same in-order stream, so a
// call's copies, its kernels and the copy back queue in that order without a wait between
// them: the one synchronization is after the copy back, where the host is about to read the
// result.

use inkwell::AddressSpace;
use inkwell::basic_block::BasicBlock;
use inkwell::values::{FunctionValue, IntValue, PointerValue};

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

use super::context::CodegenContext;

/// The device arena's allocator, in the GPU runtime. Named for the ABI a GPU launcher
/// allocates a scratch buffer through, so one allocator serves the launchers and the
/// staging around them.
pub(crate) const DEVICE_ALLOC_FN: &str = "_mlir_memref_to_llvm_alloc";
pub(crate) const DEVICE_RELEASE_FN: &str = "_mlir_memref_to_llvm_free";

/// What the GPU runtime calls on any failure, `(ptr, i64)` message included. Defined
/// here so a runtime failure is a Neuro panic: same `panic:` text, same abort, and
/// buffered standard output drained first.
pub(crate) const GPU_PANIC_FN: &str = "__neuro_gpu_panic";

/// An `i8` the GPU runtime reads before a module load: 1 when every `@gpu` function has a
/// host fallback, so a missing GPU leaves the program to run its host bodies instead of
/// aborting at startup.
pub(crate) const GPU_FALLBACK_GLOBAL: &str = "__neuro_gpu_fallback";

/// The runtime's device-tensor calls (see `gpu_runtime.c`): a buffer that outlives the
/// call making it, the transfers, its release, and the device-index check the transfers
/// make.
pub(crate) const DEVICE_TENSOR_ALLOC_FN: &str = "__neuro_device_alloc";
pub(crate) const DEVICE_UPLOAD_FN: &str = "__neuro_device_upload";
pub(crate) const DEVICE_COPY_FN: &str = "__neuro_device_copy";
pub(crate) const DEVICE_CLONE_FN: &str = "__neuro_device_clone";
pub(crate) const DEVICE_MOVE_FN: &str = "__neuro_device_move";
pub(crate) const DEVICE_TENSOR_FREE_FN: &str = "__neuro_device_free";
pub(crate) const DEVICE_CHECK_FN: &str = "__neuro_device_check";

/// The runtime's current device: which one a call runs on, the fold that picks it from
/// the operands, and the current device's arena mark and release.
pub(crate) const DEVICE_SWITCH_FN: &str = "__neuro_device_switch";
pub(crate) const DEVICE_JOIN_FN: &str = "__neuro_device_join";
pub(crate) const DEVICE_MARK_FN: &str = "__neuro_device_mark";
pub(crate) const DEVICE_RESTORE_FN: &str = "__neuro_device_restore";

const MGPU_MEMCPY: &str = "mgpuMemcpy";
const MGPU_STREAM_CREATE: &str = "mgpuStreamCreate";
const MGPU_STREAM_SYNCHRONIZE: &str = "mgpuStreamSynchronize";

/// What [`DEVICE_JOIN_FN`] reads as a host operand, and starts from.
const NO_DEVICE: i32 = -1;

/// One call's device staging: the device it runs on and the one current before it, the
/// arena mark it restores, the stream its copies run on, the buffers it took (null where
/// an operand needed none), released one by one in case the chunk was full, and whether
/// any operand already lived on the device.
pub(crate) struct DeviceStaging<'ctx> {
    device: IntValue<'ctx>,
    previous: IntValue<'ctx>,
    mark: IntValue<'ctx>,
    stream: PointerValue<'ctx>,
    buffers: Vec<PointerValue<'ctx>>,
    on_device: IntValue<'ctx>,
}

/// Whether a kernel only reads a staged operand, or also writes it through `&mut`.
#[derive(Clone, Copy)]
enum OperandUse {
    Read,
    Write,
}

/// Where a staged call's kernels write their result, and the tensor that hands it back.
pub(crate) struct StagedResult<'ctx> {
    /// The tensor the call returns.
    pub(crate) handle: PointerValue<'ctx>,
    /// The device buffer the kernels write.
    pub(crate) written: PointerValue<'ctx>,
    /// The host buffer `written` is copied back into, or null when the result stays on
    /// the device.
    copy_back: PointerValue<'ctx>,
}

impl<'ctx> CodegenContext<'ctx> {
    /// Define [`GPU_PANIC_FN`] for the GPU runtime to call. External until the runtime is
    /// linked, like [`device_alloc_fn`](CodegenContext::device_alloc_fn), and emitted
    /// before the module is finished so the stdout drain lands ahead of its message.
    pub(crate) fn define_gpu_panic(&mut self) -> CodegenResult<()> {
        let ptr = self.ptr();
        let function = self.module.add_function(
            GPU_PANIC_FN,
            self.context
                .void_type()
                .fn_type(&[ptr.into(), self.context.i64_type().into()], false),
            None,
        );
        let entry = self.context.append_basic_block(function, "entry");
        let resume_at = self.builder.get_insert_block();
        self.builder.position_at_end(entry);
        let message = function
            .get_first_param()
            .ok_or_else(|| CodegenError::InternalError("GPU panic lost its message".into()))?;
        let length = function
            .get_nth_param(1)
            .ok_or_else(|| CodegenError::InternalError("GPU panic lost its length".into()))?
            .into_int_value();
        self.emit_write_cstr("panic: ")?;
        self.emit_write(message, length)?;
        self.emit_write_cstr("\n")?;
        self.emit_abort_unreachable()?;
        if let Some(first) = entry.get_first_instruction() {
            self.process_exit_points.push(first);
        }
        if let Some(block) = resume_at {
            self.builder.position_at_end(block);
        }
        Ok(())
    }

    /// Make the module carry the GPU runtime, for a device tensor in a program that may
    /// have no `@gpu` body to bring it: define what the runtime calls back into. A module
    /// that defines [`GPU_PANIC_FN`] is one the build links the runtime into.
    pub(crate) fn require_gpu_runtime(&mut self) -> CodegenResult<()> {
        if self.requires_gpu_runtime() {
            return Ok(());
        }
        self.define_gpu_panic()?;
        // A `@gpu` body would have defined both before any function was generated, so
        // there is none, and a missing GPU is fatal only to the transfer that needs one.
        self.define_gpu_fallback_flag(true);
        Ok(())
    }

    pub(crate) fn requires_gpu_runtime(&self) -> bool {
        self.module.get_function(GPU_PANIC_FN).is_some()
    }

    /// `__neuro_device_free(ptr, i32 device)`, the release a device tensor's deleter makes.
    pub(crate) fn device_tensor_free_fn(&mut self) -> CodegenResult<FunctionValue<'ctx>> {
        self.require_gpu_runtime()?;
        Ok(self.extern_fn(
            DEVICE_TENSOR_FREE_FN,
            self.context
                .void_type()
                .fn_type(&[self.ptr().into(), self.context.i32_type().into()], false),
        ))
    }

    /// A device buffer for a `tensor_ty` tensor's elements, released only by its deleter.
    pub(crate) fn alloc_device_tensor_buffer(
        &mut self,
        tensor_ty: &Type,
    ) -> CodegenResult<PointerValue<'ctx>> {
        self.require_gpu_runtime()?;
        let i64_type = self.context.i64_type();
        let alloc = self.extern_fn(
            DEVICE_TENSOR_ALLOC_FN,
            self.ptr().fn_type(&[i64_type.into()], false),
        );
        let bytes = self.dlpack_copy_length(tensor_ty)?;
        self.pointer_call(alloc, &[bytes.into()], "device.tensor")
    }

    /// Copy a `tensor_ty` buffer at `host` to a new device buffer on GPU `index`, and
    /// return it. The runtime checks `index` and waits for the copy, so `host` may be
    /// released as soon as this returns.
    pub(crate) fn device_upload(
        &mut self,
        host: PointerValue<'ctx>,
        tensor_ty: &Type,
        index: IntValue<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        self.require_gpu_runtime()?;
        let i64_type = self.context.i64_type();
        let upload = self.extern_fn(
            DEVICE_UPLOAD_FN,
            self.ptr().fn_type(
                &[
                    self.ptr().into(),
                    i64_type.into(),
                    self.context.i32_type().into(),
                ],
                false,
            ),
        );
        let bytes = self.dlpack_copy_length(tensor_ty)?;
        self.pointer_call(
            upload,
            &[host.into(), bytes.into(), index.into()],
            "device.upload",
        )
    }

    /// Copy the `tensor_ty` elements at `buffer`, on GPU `index`, into `host`, once every
    /// kernel queued before it has written them.
    pub(crate) fn device_download(
        &mut self,
        host: PointerValue<'ctx>,
        buffer: PointerValue<'ctx>,
        index: IntValue<'ctx>,
        tensor_ty: &Type,
    ) -> CodegenResult<()> {
        let bytes = self.dlpack_copy_length(tensor_ty)?;
        self.device_copy(host, buffer, bytes, index)
    }

    /// Copy `bytes` from `from` to `to`, one of them memory on GPU `index` and the other
    /// host memory, in the order of every kernel queued there, and wait for it.
    pub(crate) fn device_copy(
        &mut self,
        to: PointerValue<'ctx>,
        from: PointerValue<'ctx>,
        bytes: IntValue<'ctx>,
        index: IntValue<'ctx>,
    ) -> CodegenResult<()> {
        self.require_gpu_runtime()?;
        let ptr = self.ptr();
        let copy = self.extern_fn(
            DEVICE_COPY_FN,
            self.context.void_type().fn_type(
                &[
                    ptr.into(),
                    ptr.into(),
                    self.context.i64_type().into(),
                    self.context.i32_type().into(),
                ],
                false,
            ),
        );
        self.builder.build_call(
            copy,
            &[to.into(), from.into(), bytes.into(), index.into()],
            "",
        )?;
        Ok(())
    }

    /// A new buffer on GPU `index` holding the `tensor_ty` elements at `buffer`, which lives
    /// there too.
    pub(crate) fn device_clone(
        &mut self,
        buffer: PointerValue<'ctx>,
        tensor_ty: &Type,
        index: IntValue<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        self.require_gpu_runtime()?;
        let clone = self.extern_fn(
            DEVICE_CLONE_FN,
            self.ptr().fn_type(
                &[
                    self.ptr().into(),
                    self.context.i64_type().into(),
                    self.context.i32_type().into(),
                ],
                false,
            ),
        );
        let bytes = self.dlpack_copy_length(tensor_ty)?;
        self.pointer_call(
            clone,
            &[buffer.into(), bytes.into(), index.into()],
            "device.clone",
        )
    }

    /// The `tensor_ty` buffer at `buffer`, on GPU `from`, moved to GPU `to`: the same
    /// buffer when they are one device, and otherwise a copy there, with `buffer` released.
    /// The runtime checks `to` first.
    pub(crate) fn device_move(
        &mut self,
        buffer: PointerValue<'ctx>,
        tensor_ty: &Type,
        from: IntValue<'ctx>,
        to: IntValue<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        self.require_gpu_runtime()?;
        let i32_type = self.context.i32_type();
        let move_fn = self.extern_fn(
            DEVICE_MOVE_FN,
            self.ptr().fn_type(
                &[
                    self.ptr().into(),
                    self.context.i64_type().into(),
                    i32_type.into(),
                    i32_type.into(),
                ],
                false,
            ),
        );
        let bytes = self.dlpack_copy_length(tensor_ty)?;
        self.pointer_call(
            move_fn,
            &[buffer.into(), bytes.into(), from.into(), to.into()],
            "device.moved",
        )
    }

    fn pointer_call(
        &self,
        function: FunctionValue<'ctx>,
        args: &[inkwell::values::BasicMetadataValueEnum<'ctx>],
        name: &str,
    ) -> CodegenResult<PointerValue<'ctx>> {
        Ok(self
            .builder
            .build_call(function, args, name)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| {
                CodegenError::InternalError(format!(
                    "{} returned void",
                    function.get_name().to_string_lossy()
                ))
            })?
            .into_pointer_value())
    }

    /// Define [`GPU_FALLBACK_GLOBAL`], external until the runtime is linked like
    /// [`GPU_PANIC_FN`].
    pub(crate) fn define_gpu_fallback_flag(&self, every_function_falls_back: bool) {
        let i8_type = self.context.i8_type();
        let flag = self.module.add_global(i8_type, None, GPU_FALLBACK_GLOBAL);
        flag.set_constant(true);
        flag.set_initializer(&i8_type.const_int(u64::from(every_function_falls_back), false));
    }

    /// Read the current device's arena mark, for
    /// [`restore_device_arena`](CodegenContext::restore_device_arena) to hand back.
    pub(crate) fn mark_device_arena(&self) -> CodegenResult<IntValue<'ctx>> {
        let i64_type = self.context.i64_type();
        let mark = self.extern_fn(DEVICE_MARK_FN, i64_type.fn_type(&[], false));
        self.int_call(mark, &[], "device.mark")
    }

    /// Release every allocation made on the current device's arena since `mark` at once.
    pub(crate) fn restore_device_arena(&self, mark: IntValue<'ctx>) -> CodegenResult<()> {
        let restore = self.extern_fn(
            DEVICE_RESTORE_FN,
            self.context
                .void_type()
                .fn_type(&[self.context.i64_type().into()], false),
        );
        self.builder.build_call(restore, &[mark.into()], "")?;
        Ok(())
    }

    /// Start staging one call over its tensor operands' `handles`: pick the device it
    /// runs on, the one every device operand lives on, make it current, then take its
    /// arena mark and a stream for the copies.
    pub(crate) fn open_device_staging(
        &mut self,
        handles: &[PointerValue<'ctx>],
    ) -> CodegenResult<DeviceStaging<'ctx>> {
        let i32_type = self.context.i32_type();
        let join = self.extern_fn(
            DEVICE_JOIN_FN,
            i32_type.fn_type(&[i32_type.into(), i32_type.into()], false),
        );
        let none = i32_type.const_int(NO_DEVICE as u64, true);
        let mut device = none;
        for &handle in handles {
            let on_host = self.dlpack_on_host(handle)?;
            let index = self.dlpack_device_index(handle)?;
            let operand = self
                .builder
                .build_select(on_host, none, index, "device.operand.index")?
                .into_int_value();
            device = self.int_call(join, &[device.into(), operand.into()], "device.index")?;
        }
        let switch = self.extern_fn(
            DEVICE_SWITCH_FN,
            i32_type.fn_type(&[i32_type.into()], false),
        );
        let previous = self.int_call(switch, &[device.into()], "device.previous")?;
        let mark = self.mark_device_arena()?;
        let create = self.extern_fn(MGPU_STREAM_CREATE, self.ptr().fn_type(&[], false));
        let stream = self.pointer_call(create, &[], "device.stream")?;
        Ok(DeviceStaging {
            device,
            previous,
            mark,
            stream,
            buffers: Vec::new(),
            on_device: self.context.bool_type().const_zero(),
        })
    }

    /// The device buffer a kernel reads for the `tensor_ty` operand `handle`: its own
    /// buffer when the tensor already lives on the device, and a staged copy otherwise.
    pub(crate) fn stage_operand(
        &mut self,
        staging: &mut DeviceStaging<'ctx>,
        tensor_ty: &Type,
        handle: PointerValue<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        Ok(self
            .stage_buffer(staging, tensor_ty, handle, OperandUse::Read)?
            .written)
    }

    /// The device buffer a kernel writes for the `&mut` tensor operand `handle`, whose old
    /// elements it may also read. A host tensor's staged copy goes back over it when the
    /// call closes; a device tensor is written in place.
    pub(crate) fn stage_output(
        &mut self,
        staging: &mut DeviceStaging<'ctx>,
        tensor_ty: &Type,
        handle: PointerValue<'ctx>,
    ) -> CodegenResult<StagedResult<'ctx>> {
        self.stage_buffer(staging, tensor_ty, handle, OperandUse::Write)
    }

    /// `handle`'s buffer where the kernels can reach it. For a written operand, `copy_back`
    /// is the host buffer it was copied from (null when it already lived on the device);
    /// a read one is never copied back, and its `copy_back` is null.
    fn stage_buffer(
        &mut self,
        staging: &mut DeviceStaging<'ctx>,
        tensor_ty: &Type,
        handle: PointerValue<'ctx>,
        operand_use: OperandUse,
    ) -> CodegenResult<StagedResult<'ctx>> {
        let function = self.staging_function()?;
        let on_host = self.dlpack_on_host(handle)?;
        let data = self.load_dlpack_data(handle)?;
        let resident = self.current_block()?;
        let stage = self.context.append_basic_block(function, "device.stage");
        let staged = self.context.append_basic_block(function, "device.staged");
        self.builder
            .build_conditional_branch(on_host, stage, staged)?;

        self.builder.position_at_end(stage);
        let copy = self.device_scratch(tensor_ty)?;
        self.build_device_copy(staging, copy, data, tensor_ty)?;
        let copied = self.current_block()?;
        self.builder.build_unconditional_branch(staged)?;

        self.builder.position_at_end(staged);
        let null = self.ptr().const_null();
        let operand =
            self.build_pointer_phi(&[(copy, copied), (data, resident)], "device.operand")?;
        let scratch =
            self.build_pointer_phi(&[(copy, copied), (null, resident)], "device.scratch")?;
        let copy_back = match operand_use {
            OperandUse::Read => null,
            OperandUse::Write => {
                self.build_pointer_phi(&[(data, copied), (null, resident)], "device.written_back")?
            }
        };
        staging.buffers.push(scratch);
        let resident_here = self.builder.build_not(on_host, "device.resident")?;
        staging.on_device =
            self.builder
                .build_or(staging.on_device, resident_here, "device.any_resident")?;
        Ok(StagedResult {
            handle,
            written: operand,
            copy_back,
        })
    }

    /// Where the kernels write a `tensor_ty` result. With any operand on the device the
    /// result stays there too, in a device tensor of its own; otherwise it goes to scratch
    /// and is copied back into a host tensor, so an all-host call is what it always was.
    pub(crate) fn stage_result(
        &mut self,
        staging: &mut DeviceStaging<'ctx>,
        tensor_ty: &Type,
    ) -> CodegenResult<StagedResult<'ctx>> {
        let function = self.staging_function()?;
        let resident = self.context.append_basic_block(function, "device.result");
        let returned = self.context.append_basic_block(function, "host.result");
        let join = self.context.append_basic_block(function, "result.staged");
        self.builder
            .build_conditional_branch(staging.on_device, resident, returned)?;

        // An operand on the device is what brought the call here, so `device` names it.
        self.builder.position_at_end(resident);
        let device_handle =
            self.alloc_device_tensor(tensor_ty, "external.result", staging.device)?;
        let device_data = self.load_dlpack_data(device_handle)?;
        let resident_end = self.current_block()?;
        self.builder.build_unconditional_branch(join)?;

        self.builder.position_at_end(returned);
        let host_handle = self.alloc_dlpack_tensor(tensor_ty, "external.result")?;
        let host_data = self.load_dlpack_data(host_handle)?;
        let scratch = self.device_scratch(tensor_ty)?;
        let returned_end = self.current_block()?;
        self.builder.build_unconditional_branch(join)?;

        self.builder.position_at_end(join);
        let null = self.ptr().const_null();
        let handle = self.build_pointer_phi(
            &[(device_handle, resident_end), (host_handle, returned_end)],
            "external.result",
        )?;
        let written = self.build_pointer_phi(
            &[(device_data, resident_end), (scratch, returned_end)],
            "device.written",
        )?;
        let copy_back = self.build_pointer_phi(
            &[(null, resident_end), (host_data, returned_end)],
            "device.copy_back",
        )?;
        let scratch = self.build_pointer_phi(
            &[(null, resident_end), (scratch, returned_end)],
            "device.scratch",
        )?;
        staging.buffers.push(scratch);
        Ok(StagedResult {
            handle,
            written,
            copy_back,
        })
    }

    /// Copy each of the kernels' `results` back to the host when it is not staying on the
    /// device, wait for the stream, then release everything the call staged: each buffer
    /// (a no-op for the ones in the chunk) and then the chunk itself back to the mark. The
    /// device current before the call is current again after it.
    ///
    /// The wait comes before the release even for a result left on the device: a buffer
    /// that spilled out of the chunk goes back through the runtime's free, which must not
    /// run under a kernel still reading it.
    pub(crate) fn close_device_staging(
        &mut self,
        staging: DeviceStaging<'ctx>,
        results: &[(Type, StagedResult<'ctx>)],
    ) -> CodegenResult<()> {
        let function = self.staging_function()?;
        for (tensor_ty, result) in results {
            let copy = self.context.append_basic_block(function, "device.copy_out");
            let settle = self.context.append_basic_block(function, "device.settle");
            let returning = self
                .builder
                .build_is_not_null(result.copy_back, "device.returning")?;
            self.builder
                .build_conditional_branch(returning, copy, settle)?;

            self.builder.position_at_end(copy);
            self.build_device_copy(&staging, result.copy_back, result.written, tensor_ty)?;
            self.builder.build_unconditional_branch(settle)?;
            self.builder.position_at_end(settle);
        }
        self.build_stream_call(MGPU_STREAM_SYNCHRONIZE, staging.stream)?;
        let release = self.device_release_fn();
        for buffer in &staging.buffers {
            self.builder.build_call(release, &[(*buffer).into()], "")?;
        }
        self.restore_device_arena(staging.mark)?;
        let i32_type = self.context.i32_type();
        let switch = self.extern_fn(
            DEVICE_SWITCH_FN,
            i32_type.fn_type(&[i32_type.into()], false),
        );
        self.builder
            .build_call(switch, &[staging.previous.into()], "")?;
        Ok(())
    }

    /// A device buffer sized for `tensor_ty` from the device arena, for one call.
    fn device_scratch(&mut self, tensor_ty: &Type) -> CodegenResult<PointerValue<'ctx>> {
        // A zero-extent tensor still gets an address: the runtime answers a zero-byte
        // request with null, which the allocator would report as a failure.
        let bytes = self.type_mapper.tensor_buffer_bytes(tensor_ty)?.max(1);
        let alloc = self.device_alloc_fn();
        self.pointer_call(
            alloc,
            &[self.context.i64_type().const_int(bytes, false).into()],
            "device.buffer",
        )
    }

    fn build_pointer_phi(
        &self,
        incoming: &[(PointerValue<'ctx>, BasicBlock<'ctx>)],
        name: &str,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let phi = self.builder.build_phi(self.ptr(), name)?;
        for (value, block) in incoming {
            phi.add_incoming(&[(value, *block)]);
        }
        Ok(phi.as_basic_value().into_pointer_value())
    }

    fn current_block(&self) -> CodegenResult<BasicBlock<'ctx>> {
        self.builder
            .get_insert_block()
            .ok_or_else(|| CodegenError::InternalError("device staging outside a block".into()))
    }

    fn staging_function(&self) -> CodegenResult<FunctionValue<'ctx>> {
        self.current_function
            .ok_or_else(|| CodegenError::InternalError("device staging outside a function".into()))
    }

    fn build_device_copy(
        &self,
        staging: &DeviceStaging<'ctx>,
        dst: PointerValue<'ctx>,
        src: PointerValue<'ctx>,
        tensor_ty: &Type,
    ) -> CodegenResult<()> {
        let ptr = self.ptr();
        let memcpy = self.extern_fn(
            MGPU_MEMCPY,
            self.context.void_type().fn_type(
                &[
                    ptr.into(),
                    ptr.into(),
                    self.context.i64_type().into(),
                    ptr.into(),
                ],
                false,
            ),
        );
        let bytes = self.dlpack_copy_length(tensor_ty)?;
        self.builder.build_call(
            memcpy,
            &[dst.into(), src.into(), bytes.into(), staging.stream.into()],
            "",
        )?;
        Ok(())
    }

    fn build_stream_call(&self, name: &str, stream: PointerValue<'ctx>) -> CodegenResult<()> {
        let function = self.extern_fn(
            name,
            self.context
                .void_type()
                .fn_type(&[self.ptr().into()], false),
        );
        self.builder.build_call(function, &[stream.into()], "")?;
        Ok(())
    }

    fn ptr(&self) -> inkwell::types::PointerType<'ctx> {
        self.context.ptr_type(AddressSpace::default())
    }

    fn int_call(
        &self,
        function: FunctionValue<'ctx>,
        args: &[inkwell::values::BasicMetadataValueEnum<'ctx>],
        name: &str,
    ) -> CodegenResult<IntValue<'ctx>> {
        Ok(self
            .builder
            .build_call(function, args, name)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| {
                CodegenError::InternalError(format!(
                    "{} returned void",
                    function.get_name().to_string_lossy()
                ))
            })?
            .into_int_value())
    }

    /// `_mlir_memref_to_llvm_alloc(i64) -> ptr`: a buffer from the current device's arena,
    /// never null.
    fn device_alloc_fn(&self) -> FunctionValue<'ctx> {
        self.extern_fn(
            DEVICE_ALLOC_FN,
            self.ptr().fn_type(&[self.context.i64_type().into()], false),
        )
    }

    /// `_mlir_memref_to_llvm_free(ptr)`: nothing for a buffer in the chunk or for null, a
    /// driver free for one that spilled.
    fn device_release_fn(&self) -> FunctionValue<'ctx> {
        self.extern_fn(
            DEVICE_RELEASE_FN,
            self.context
                .void_type()
                .fn_type(&[self.ptr().into()], false),
        )
    }
}
