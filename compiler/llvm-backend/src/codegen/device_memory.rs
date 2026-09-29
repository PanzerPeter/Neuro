// Device memory for the tensor bodies a GPU computes.
//
// A GPU launcher reads and writes device memory only, so the wrapper around one stages
// every tensor it passes: each operand is copied into a device buffer, the kernels write
// a device result, and that result is copied back into the host tensor the caller
// receives. The buffers a launcher needs between two kernels come from the same
// allocator, because its IR allocates them through `_mlir_memref_to_llvm_alloc` /
// `_mlir_memref_to_llvm_free`, and those are the names this module defines.
//
// The allocator is a second linear arena, the host arena's rules over a chunk of device
// memory: bump allocation, one release for everything past a mark, and a spill to the GPU
// runtime's own allocator when the chunk is full. One call's staging is one region, so a
// call costs no device allocate / free pair per buffer, which matters because a device
// free synchronizes the whole device. A `pool` block in a program that runs GPU bodies
// takes a device mark as well and restores it after its sweep: that restore is the
// language's single batched release per device at pool exit.
//
// Every device operation goes through MLIR's GPU runtime ABI (`mgpuMemAlloc`,
// `mgpuMemFree`, `mgpuMemcpy`, the `mgpuStream*` family), the one the launchers call, so a
// single runtime library serves both.
//
// The runtime hands every `mgpuStreamCreate` the same in-order stream, so a call's copies,
// its kernels and the copy back queue in that order without a wait between them: the one
// synchronization is after the copy back, where the host is about to read the result.

use inkwell::module::Linkage;
use inkwell::values::{FunctionValue, IntValue, PointerValue};
use inkwell::AddressSpace;

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

use super::arena::Arena;
use super::context::CodegenContext;

/// Named for the ABI a GPU launcher allocates a scratch buffer through, so one allocator
/// serves the launchers and the staging around them.
pub(crate) const DEVICE_ALLOC_FN: &str = "_mlir_memref_to_llvm_alloc";
pub(crate) const DEVICE_RELEASE_FN: &str = "_mlir_memref_to_llvm_free";

/// What the GPU runtime calls on any failure, `(ptr, i64)` message included. Defined
/// here so a runtime failure is a Neuro panic: same `panic:` text, same abort, and
/// buffered standard output drained first.
pub(crate) const GPU_PANIC_FN: &str = "__neuro_gpu_panic";

const DEVICE_BUMP_FN: &str = "__neuro_device_bump";
const DEVICE_ARENA_BASE_GLOBAL: &str = "__neuro_device_arena_base";
const DEVICE_ARENA_OFFSET_GLOBAL: &str = "__neuro_device_arena_offset";

const MGPU_MEM_ALLOC: &str = "mgpuMemAlloc";
const MGPU_MEM_FREE: &str = "mgpuMemFree";
const MGPU_MEMCPY: &str = "mgpuMemcpy";
const MGPU_STREAM_CREATE: &str = "mgpuStreamCreate";
const MGPU_STREAM_SYNCHRONIZE: &str = "mgpuStreamSynchronize";

/// Device memory reserved the first time a run allocates any. Unlike host address space
/// it is resident from the moment it is reserved, so it is sized for staging a handful
/// of large tensors rather than generously; a call that outgrows it spills buffer by
/// buffer.
const DEVICE_ARENA_CAPACITY: u64 = 64 * 1024 * 1024;

/// What `cuMemAlloc` and `hipMalloc` guarantee, and what a coalesced load wants.
const DEVICE_ALIGN: u64 = 256;

/// `mgpuMemAlloc`'s `isHostShared`, false: plain device memory. Managed memory would let
/// the host read it too, which staging never needs and which a GPU without on-demand
/// paging serves slowly.
const DEVICE_ONLY: u64 = 0;

const DEVICE_ALLOC_FAILED: &str =
    "panic: device memory allocation failed: no GPU is available, or it is out of memory\n";

/// One call's device staging: the arena mark it restores, the stream its copies run on,
/// and the buffers it took, released one by one in case the chunk was full.
pub(crate) struct DeviceStaging<'ctx> {
    mark: IntValue<'ctx>,
    stream: PointerValue<'ctx>,
    buffers: Vec<PointerValue<'ctx>>,
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

    /// Read the device arena's mark, for
    /// [`restore_device_arena`](CodegenContext::restore_device_arena) to hand back.
    pub(crate) fn mark_device_arena(&self) -> CodegenResult<IntValue<'ctx>> {
        let offset = self.device_arena().offset;
        Ok(self
            .builder
            .build_load(
                self.context.i64_type(),
                offset.as_pointer_value(),
                "device.mark",
            )?
            .into_int_value())
    }

    /// Release every device allocation made since `mark` at once.
    pub(crate) fn restore_device_arena(&self, mark: IntValue<'ctx>) -> CodegenResult<()> {
        let offset = self.device_arena().offset;
        self.builder.build_store(offset.as_pointer_value(), mark)?;
        Ok(())
    }

    /// Start staging one call: take the device mark and a stream for its copies.
    pub(crate) fn open_device_staging(&mut self) -> CodegenResult<DeviceStaging<'ctx>> {
        let mark = self.mark_device_arena()?;
        let create = self.extern_fn(MGPU_STREAM_CREATE, self.ptr().fn_type(&[], false));
        let stream = self
            .builder
            .build_call(create, &[], "device.stream")?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| {
                CodegenError::InternalError(format!("{MGPU_STREAM_CREATE} returned void"))
            })?
            .into_pointer_value();
        Ok(DeviceStaging {
            mark,
            stream,
            buffers: Vec::new(),
        })
    }

    /// A device buffer sized for `tensor_ty`, for a kernel to write.
    pub(crate) fn device_buffer(
        &mut self,
        staging: &mut DeviceStaging<'ctx>,
        tensor_ty: &Type,
    ) -> CodegenResult<PointerValue<'ctx>> {
        // A zero-extent tensor still gets an address: the runtime answers a zero-byte
        // request with null, which the allocator would report as a failure.
        let bytes = self.type_mapper.tensor_buffer_bytes(tensor_ty)?.max(1);
        let alloc = self.device_alloc_fn()?;
        let buffer = self
            .builder
            .build_call(
                alloc,
                &[self.context.i64_type().const_int(bytes, false).into()],
                "device.buffer",
            )?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError(format!("{DEVICE_ALLOC_FN} returned void")))?
            .into_pointer_value();
        staging.buffers.push(buffer);
        Ok(buffer)
    }

    /// A device buffer holding a copy of the `tensor_ty` elements at `host`.
    pub(crate) fn copy_to_device(
        &mut self,
        staging: &mut DeviceStaging<'ctx>,
        tensor_ty: &Type,
        host: PointerValue<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let device = self.device_buffer(staging, tensor_ty)?;
        self.build_device_copy(staging, device, host, tensor_ty)?;
        Ok(device)
    }

    /// Copy the kernels' result at `device` back into `host`, then release everything
    /// the call staged: each buffer (a no-op for the ones in the chunk) and then the
    /// chunk itself back to the mark.
    pub(crate) fn close_device_staging(
        &mut self,
        staging: DeviceStaging<'ctx>,
        tensor_ty: &Type,
        host: PointerValue<'ctx>,
        device: PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        self.build_device_copy(&staging, host, device, tensor_ty)?;
        self.build_stream_call(MGPU_STREAM_SYNCHRONIZE, staging.stream)?;
        let release = self.device_release_fn()?;
        for buffer in &staging.buffers {
            self.builder.build_call(release, &[(*buffer).into()], "")?;
        }
        self.restore_device_arena(staging.mark)
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

    fn device_arena(&self) -> Arena<'ctx> {
        Arena {
            base: self.arena_base_global(DEVICE_ARENA_BASE_GLOBAL),
            offset: self.arena_offset_global(DEVICE_ARENA_OFFSET_GLOBAL),
            capacity: DEVICE_ARENA_CAPACITY,
        }
    }

    fn ptr(&self) -> inkwell::types::PointerType<'ctx> {
        self.context.ptr_type(AddressSpace::default())
    }

    fn mgpu_mem_alloc(&self) -> FunctionValue<'ctx> {
        let ptr = self.ptr();
        self.extern_fn(
            MGPU_MEM_ALLOC,
            ptr.fn_type(
                &[
                    self.context.i64_type().into(),
                    ptr.into(),
                    self.context.i8_type().into(),
                ],
                false,
            ),
        )
    }

    /// `_mlir_memref_to_llvm_alloc(i64) -> ptr`: reserve the chunk on the first call,
    /// bump from it, and abort rather than return null.
    ///
    /// Defined with external linkage so the launchers' declaration of the same name
    /// resolves to it when their IR is linked in; the link internalizes it afterwards.
    fn device_alloc_fn(&mut self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(DEVICE_ALLOC_FN) {
            return Ok(existing);
        }
        let i64_type = self.context.i64_type();
        let ptr = self.ptr();
        let function = self.module.add_function(
            DEVICE_ALLOC_FN,
            ptr.fn_type(&[i64_type.into()], false),
            None,
        );
        let bump = self.device_bump_fn()?;
        let failed = self.cold_panic_thunk(DEVICE_ALLOC_FAILED)?;
        let mem_alloc = self.mgpu_mem_alloc();
        self.detached(|| {
            let entry = self.context.append_basic_block(function, "entry");
            let reserve = self.context.append_basic_block(function, "reserve");
            let take = self.context.append_basic_block(function, "take");
            let fail = self.context.append_basic_block(function, "fail");
            let done = self.context.append_basic_block(function, "done");
            let arena = self.device_arena();
            let size = function
                .get_first_param()
                .ok_or_else(|| CodegenError::InternalError("device alloc lost its size".into()))?;

            self.builder.position_at_end(entry);
            let base = self
                .builder
                .build_load(ptr, arena.base.as_pointer_value(), "device.base")?
                .into_pointer_value();
            let missing = self.builder.build_is_null(base, "device.missing")?;
            self.builder
                .build_conditional_branch(missing, reserve, take)?;

            // A failed reservation leaves the base null, and the bump then spills every
            // request to the runtime, which is where a missing GPU is finally reported.
            self.builder.position_at_end(reserve);
            let chunk = self
                .builder
                .build_call(
                    mem_alloc,
                    &[
                        i64_type.const_int(DEVICE_ARENA_CAPACITY, false).into(),
                        ptr.const_null().into(),
                        self.context.i8_type().const_int(DEVICE_ONLY, false).into(),
                    ],
                    "device.chunk",
                )?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| {
                    CodegenError::InternalError(format!("{MGPU_MEM_ALLOC} returned void"))
                })?;
            self.builder
                .build_store(arena.base.as_pointer_value(), chunk)?;
            self.builder.build_unconditional_branch(take)?;

            self.builder.position_at_end(take);
            let buffer = self
                .builder
                .build_call(bump, &[size.into()], "device.ptr")?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| CodegenError::InternalError("device bump returned void".into()))?
                .into_pointer_value();
            let present = self.builder.build_is_not_null(buffer, "device.present")?;
            let branch = self.builder.build_conditional_branch(present, done, fail)?;
            self.mark_cold_branch(branch)?;

            self.builder.position_at_end(fail);
            self.builder.build_call(failed, &[], "")?;
            self.builder.build_unreachable()?;

            self.builder.position_at_end(done);
            self.builder.build_return(Some(&buffer))?;
            Ok(())
        })?;
        Ok(function)
    }

    fn device_bump_fn(&self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(DEVICE_BUMP_FN) {
            return Ok(existing);
        }
        let i64_type = self.context.i64_type();
        let ptr = self.ptr();
        let function = self.module.add_function(
            DEVICE_BUMP_FN,
            ptr.fn_type(&[i64_type.into()], false),
            Some(Linkage::Internal),
        );
        let mem_alloc = self.mgpu_mem_alloc();
        self.detached(|| {
            let size = function
                .get_first_param()
                .ok_or_else(|| CodegenError::InternalError("device bump lost its size".into()))?
                .into_int_value();
            self.build_bump_body(
                self.device_arena(),
                function,
                size,
                i64_type.const_int(DEVICE_ALIGN, false),
                mem_alloc,
                &[
                    size.into(),
                    ptr.const_null().into(),
                    self.context.i8_type().const_int(DEVICE_ONLY, false).into(),
                ],
            )
        })?;
        Ok(function)
    }

    /// `_mlir_memref_to_llvm_free(ptr)`: nothing for a buffer in the chunk, which the
    /// mark restore reclaims, and a runtime free for one that spilled. External until
    /// the link, like [`device_alloc_fn`](CodegenContext::device_alloc_fn).
    fn device_release_fn(&self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(DEVICE_RELEASE_FN) {
            return Ok(existing);
        }
        let ptr = self.ptr();
        let function = self.module.add_function(
            DEVICE_RELEASE_FN,
            self.context.void_type().fn_type(&[ptr.into()], false),
            None,
        );
        let mem_free = self.extern_fn(
            MGPU_MEM_FREE,
            self.context
                .void_type()
                .fn_type(&[ptr.into(), ptr.into()], false),
        );
        self.detached(|| {
            let entry = self.context.append_basic_block(function, "entry");
            let spilled = self.context.append_basic_block(function, "spilled");
            let done = self.context.append_basic_block(function, "done");
            let target = function
                .get_first_param()
                .ok_or_else(|| {
                    CodegenError::InternalError("device release lost its pointer".into())
                })?
                .into_pointer_value();

            self.builder.position_at_end(entry);
            let owned = self.build_arena_owns(self.device_arena(), target)?;
            self.builder
                .build_conditional_branch(owned, done, spilled)?;

            self.builder.position_at_end(spilled);
            self.builder
                .build_call(mem_free, &[target.into(), ptr.const_null().into()], "")?;
            self.builder.build_unconditional_branch(done)?;

            self.builder.position_at_end(done);
            self.builder.build_return(None)?;
            Ok(())
        })?;
        Ok(function)
    }
}
