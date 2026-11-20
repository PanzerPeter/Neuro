// Codegen for tensor construction and in-place update. A tensor value is a pointer to its
// own DLPack handle
// (`codegen/dlpack.rs`), whose `data` field addresses a flat, row-major
// `[d0*d1*... x T]` run of elements. Every node here therefore builds two things: the
// handle it hands back, and the buffer it writes elements into.
//
// The buffer is out of line rather than a first-class LLVM aggregate because the language
// has a tensor *own* its buffer, and promises that buffer a stable address across an
// in-place update. Neither is expressible for an SSA value, which has no address at all.
// It is also what makes a large tensor compilable: an aggregate copy is a whole-buffer
// `load`/`store` pair that only `-O1`'s SROA can turn into a `memcpy`, and SelectionDAG
// crashes legalizing one above ~50k elements at `-O0`.
//
// A literal whose elements are themselves constant folds to an LLVM constant in `.rodata`
// and reaches the buffer as one `memcpy`; its size is the source's. A fill, an identity
// matrix and `random_normal` are written by a run-time loop straight into the heap buffer
// instead, because their element count comes from the type alone: a constant per element
// made the compiler's memory and the binary grow with the shape.
//
// The compound assignment at the end of the file allocates nothing at all: it reuses the
// buffer the target already owns, which is the guarantee the language makes about a
// tensor's handle across an in-place update.
use inkwell::IntPredicate;
use inkwell::module::Linkage;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use neuro_hir::{HirExpr, HirExtent};

use crate::codegen::context::CodegenContext;
use crate::codegen::dlpack::TensorHome;
use crate::codegen::extents::ExtentOperand;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// The prelude enum `.to(device)` takes, and its host variant; every other variant is a
/// GPU, carrying its index as the one payload field.
const DEVICE_ENUM: &str = "Device";
const DEVICE_HOST_VARIANT: &str = "CPU";

/// Windows has no GPU runtime to move a buffer with (it opens the driver with `dlopen`),
/// and a transfer that left the buffer where it was would leave the program wrong about
/// where its compute runs.
const DEVICE_UNAVAILABLE: &str = "a tensor transfer to a GPU is not supported on Windows yet";

impl<'ctx> CodegenContext<'ctx> {
    /// The element type and buffer length of a static tensor type. Only a literal reaches
    /// this: tensor literals are static by construction.
    pub(super) fn tensor_layout(&self, ty: &Type) -> CodegenResult<(Type, usize)> {
        let Type::Tensor { element, shape } = ty else {
            return Err(CodegenError::InternalError(
                "tensor node does not carry a tensor type".to_string(),
            ));
        };
        let extents = crate::types::static_extents(shape)?;
        Ok(((**element).clone(), extents.iter().product()))
    }

    /// The element type of a tensor type.
    pub(super) fn tensor_element(&self, ty: &Type) -> CodegenResult<Type> {
        let Type::Tensor { element, .. } = ty else {
            return Err(CodegenError::InternalError(
                "tensor node does not carry a tensor type".to_string(),
            ));
        };
        Ok((**element).clone())
    }

    /// Each `dims` expression of a construction, evaluated to its run-time extent in order.
    pub(super) fn codegen_extent_values(
        &mut self,
        dims: &[HirExpr],
    ) -> CodegenResult<Vec<IntValue<'ctx>>> {
        dims.iter()
            .map(|dim| Ok(self.codegen_expr(dim)?.into_int_value()))
            .collect()
    }

    /// Allocate a tensor's DLPack handle and its element buffer, returning both: the
    /// handle *is* the tensor value, and the buffer is where elements are written. `dims` and
    /// `bytes` are as for [`alloc_dlpack_tensor`](CodegenContext::alloc_dlpack_tensor).
    ///
    /// Every construction node routes through here, so there is exactly one place the
    /// allocator is chosen, the hook 2D's arena replaces.
    pub(super) fn alloc_tensor(
        &mut self,
        tensor_ty: &Type,
        name: &str,
        dims: &[IntValue<'ctx>],
        bytes: IntValue<'ctx>,
    ) -> CodegenResult<(PointerValue<'ctx>, PointerValue<'ctx>)> {
        let handle = self.alloc_dlpack_tensor(tensor_ty, name, dims, bytes)?;
        let data = self.load_dlpack_data(handle)?;
        Ok((handle, data))
    }

    /// Copy a compile-time-constant buffer into a freshly allocated tensor.
    ///
    /// The constant is emitted once as a private `.rodata` global and `memcpy`'d into the
    /// tensor's own buffer, so a `zeros()` of any size costs one call rather than an
    /// instruction per element, and the tensor still owns writable storage afterwards.
    fn emit_const_tensor_buffer(
        &mut self,
        tensor_ty: &Type,
        constant: BasicValueEnum<'ctx>,
        name: &str,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let global = self
            .module
            .add_global(constant.get_type(), None, "tensor.const");
        global.set_linkage(Linkage::Private);
        global.set_constant(true);
        global.set_initializer(&constant);

        // A literal is static, so its run is a constant and its offset is never reported.
        let size = self.element_run(tensor_ty, &[], 0)?.bytes;
        let (handle, data) = self.alloc_tensor(tensor_ty, name, &[], size)?;
        self.build_memcpy_call(data, global.as_pointer_value(), size)?;
        Ok(handle.into())
    }

    /// The address of buffer slot `index`, for a buffer of `buffer_ty`.
    pub(super) fn tensor_slot(
        &self,
        buffer_ty: BasicTypeEnum<'ctx>,
        buffer: PointerValue<'ctx>,
        index: IntValue<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        // SAFETY: every caller derives `index` from the buffer's own element count, so
        // the GEP stays inside the allocation.
        unsafe {
            self.builder
                .build_in_bounds_gep(
                    buffer_ty,
                    buffer,
                    &[self.context.i64_type().const_zero(), index],
                    "tensor.slot",
                )
                .map_err(CodegenError::from)
        }
    }

    /// Lower a tensor literal (a coerced nested array literal, `Tensor::from(...)`, or
    /// `Tensor::scalar(v)`) into a fresh buffer. `elements` is already in row-major
    /// order, so the element index is the buffer index.
    ///
    /// A literal whose elements are all constants becomes one `.rodata` blob and one
    /// `memcpy`; a literal mentioning a runtime value is written slot by slot instead.
    pub(crate) fn codegen_tensor_literal(
        &mut self,
        elements: &[HirExpr],
        tensor_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let (element_ty, count) = self.tensor_layout(tensor_ty)?;
        if elements.len() != count {
            return Err(CodegenError::InternalError(format!(
                "tensor literal holds {} element(s) for a buffer of {}",
                elements.len(),
                count
            )));
        }
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        let mut values = Vec::with_capacity(count);
        for element in elements {
            let value = self.codegen_expr(element)?;
            values.push(self.coerce_if_needed(value, elem_llvm, &element_ty)?);
        }

        let all_constant = values.iter().all(|value| match value {
            BasicValueEnum::IntValue(v) => v.is_const(),
            BasicValueEnum::FloatValue(v) => v.is_const(),
            _ => false,
        });
        if all_constant {
            let constant = Self::const_array_of(elem_llvm, values.into_iter())?;
            return self.emit_const_tensor_buffer(tensor_ty, constant, "tensor.literal");
        }

        let size = self.element_run(tensor_ty, &[], 0)?.bytes;
        let (handle, data) = self.alloc_tensor(tensor_ty, "tensor.literal", &[], size)?;
        let buffer_ty = self.type_mapper.tensor_buffer_type(tensor_ty)?;
        let i64_type = self.context.i64_type();
        for (index, value) in values.into_iter().enumerate() {
            let slot =
                self.tensor_slot(buffer_ty, data, i64_type.const_int(index as u64, false))?;
            self.builder.build_store(slot, value)?;
        }
        Ok(handle.into())
    }

    /// Lower `zeros()` / `ones()`: one constant repeated across the buffer. `dims` holds the
    /// run-time extent of each dynamic axis, and is empty for a static tensor.
    pub(crate) fn codegen_tensor_fill(
        &mut self,
        value: &HirExpr,
        dims: &[HirExpr],
        tensor_ty: &Type,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let element_ty = self.tensor_element(tensor_ty)?;
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        let value = self.codegen_expr(value)?;
        let value = self.coerce_if_needed(value, elem_llvm, &element_ty)?;
        let dims = self.codegen_extent_values(dims)?;
        self.write_tensor_by_loop(tensor_ty, "tensor.fill", &dims, offset, |_, _| Ok(value))
    }

    /// Lower `identity()`: ones on the diagonal of a square rank-2 buffer, zeros
    /// elsewhere. Squareness was established before lowering, so both extents are the same
    /// value at run time too.
    pub(crate) fn codegen_tensor_identity(
        &mut self,
        dims: &[HirExpr],
        tensor_ty: &Type,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let Type::Tensor { element, shape } = tensor_ty else {
            return Err(CodegenError::InternalError(
                "identity node does not carry a tensor type".to_string(),
            ));
        };
        if shape.len() != 2 {
            return Err(CodegenError::InternalError(
                "identity is a rank-2 construction".to_string(),
            ));
        }
        let dims = self.codegen_extent_values(dims)?;
        let extents = self.axis_extents(shape, &dims)?;
        // Squareness was established before lowering, so a zero-sized matrix has no slot
        // to write and the loop never divides by its column count.
        let cols = self.at_least_one(extents[1])?;
        let elem_llvm = self.get_any_llvm_type(element)?;
        let (zero, one): (BasicValueEnum<'ctx>, BasicValueEnum<'ctx>) = match elem_llvm {
            BasicTypeEnum::IntType(int_ty) => (
                int_ty.const_zero().into(),
                int_ty.const_int(1, false).into(),
            ),
            BasicTypeEnum::FloatType(float_ty) => (
                float_ty.const_zero().into(),
                float_ty.const_float(1.0).into(),
            ),
            _ => {
                return Err(CodegenError::InternalError(
                    "a tensor element is a scalar".to_string(),
                ));
            }
        };
        self.write_tensor_by_loop(tensor_ty, "tensor.identity", &dims, offset, |ctx, i| {
            let row = ctx
                .builder
                .build_int_unsigned_div(i, cols, "tensor.identity.row")?;
            let col = ctx
                .builder
                .build_int_unsigned_rem(i, cols, "tensor.identity.col")?;
            let diagonal = ctx.builder.build_int_compare(
                IntPredicate::EQ,
                row,
                col,
                "tensor.identity.diag",
            )?;
            Ok(ctx
                .builder
                .build_select(diagonal, one, zero, "tensor.identity.value")?)
        })
    }

    /// Allocate a tensor of `tensor_ty` and write `value_at(i)` into each buffer slot `i`
    /// in a counted loop. The buffer is written through its pointer rather than built by
    /// `insertvalue` or a constant, because the element count does not scale with an
    /// instruction or a constant per element. `dims` are the run-time extents of its dynamic
    /// axes, and `offset` locates the size trap a dynamic count can hit.
    fn write_tensor_by_loop(
        &mut self,
        tensor_ty: &Type,
        name: &str,
        dims: &[IntValue<'ctx>],
        offset: usize,
        mut value_at: impl FnMut(&mut Self, IntValue<'ctx>) -> CodegenResult<BasicValueEnum<'ctx>>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let run = self.element_run(tensor_ty, dims, offset)?;
        let count = run.count;
        let buffer_ty = self.type_mapper.tensor_buffer_type(tensor_ty)?;
        let (handle, data) = self.alloc_tensor(tensor_ty, name, dims, run.bytes)?;
        let i64_type = self.context.i64_type();
        let index = self.entry_alloca(i64_type, &format!("{name}.i"))?;
        self.builder.build_store(index, i64_type.const_zero())?;

        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("tensor construction outside a function".to_string())
        })?;
        let head = self
            .context
            .append_basic_block(function, &format!("{name}.head"));
        let body = self
            .context
            .append_basic_block(function, &format!("{name}.body"));
        let done = self
            .context
            .append_basic_block(function, &format!("{name}.done"));

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let i = self
            .builder
            .build_load(i64_type, index, &format!("{name}.idx"))?
            .into_int_value();
        let more =
            self.builder
                .build_int_compare(IntPredicate::ULT, i, count, &format!("{name}.more"))?;
        self.builder.build_conditional_branch(more, body, done)?;

        self.builder.position_at_end(body);
        let value = value_at(self, i)?;
        // `i` is below `count` on this edge, because the loop head's `ULT` test is what
        // branches here, so the slot address stays inside the buffer.
        let slot = self.tensor_slot(buffer_ty, data, i)?;
        self.builder.build_store(slot, value)?;
        let next =
            self.builder
                .build_int_add(i, i64_type.const_int(1, false), &format!("{name}.next"))?;
        self.builder.build_store(index, next)?;
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(done);
        Ok(handle.into())
    }

    /// Lower `random_normal(mean, std)`: a counted loop that writes one draw per buffer
    /// slot.
    pub(crate) fn codegen_tensor_random_normal(
        &mut self,
        mean: &HirExpr,
        std: &HirExpr,
        dims: &[HirExpr],
        tensor_ty: &Type,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let element_ty = self.tensor_element(tensor_ty)?;
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        let BasicTypeEnum::FloatType(elem_float) = elem_llvm else {
            return Err(CodegenError::InternalError(
                "`random_normal` draws into a floating-point tensor".to_string(),
            ));
        };
        let mean = self.codegen_expr(mean)?.into_float_value();
        let std = self.codegen_expr(std)?.into_float_value();
        let dims = self.codegen_extent_values(dims)?;
        let normal_fn = self.get_or_define_rng_normal()?;
        self.write_tensor_by_loop(tensor_ty, "tensor.rand", &dims, offset, |ctx, _| {
            let draw = ctx
                .builder
                .build_call(normal_fn, &[], "tensor.rand.draw")?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| CodegenError::InternalError("rng helper returned void".to_string()))?
                .into_float_value();
            // The draw is standard normal in `f64`; narrowing before the affine transform
            // keeps the arithmetic at the element's own width, so an `f32` tensor rounds
            // once rather than twice.
            let draw = ctx
                .builder
                .build_float_cast(draw, elem_float, "tensor.rand.elem")?;
            let scaled = ctx
                .builder
                .build_float_mul(std, draw, "tensor.rand.scaled")?;
            Ok(ctx
                .builder
                .build_float_add(mean, scaled, "tensor.rand.value")?
                .into())
        })
    }

    /// Lower `tensor.clone()`: a second buffer holding the same elements.
    ///
    /// The clone allocates and `memcpy`s, so the result owns storage of its own and the
    /// receiver keeps its address: the deep copy the language specifies, rather than a
    /// second name for one buffer. A device tensor's clone is a buffer on the same GPU,
    /// copied there without passing through the host.
    ///
    /// An owned receiver lowers to the tensor pointer itself; a `&Tensor<T, S>` receiver
    /// lowers to the *address of* that pointer, so it is loaded through first. Both are
    /// `ptr` in LLVM, so the distinction comes from `recv_ty`, not from the value.
    pub(crate) fn codegen_tensor_clone(
        &mut self,
        recv_ty: &Type,
        receiver: &HirExpr,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let value = self.codegen_expr(receiver)?;
        let BasicValueEnum::PointerValue(ptr) = value else {
            return Err(CodegenError::InternalError(
                "a tensor receiver does not lower to a pointer".to_string(),
            ));
        };
        let tensor_ty = recv_ty.referent();
        let source = if matches!(recv_ty, Type::Reference { .. }) {
            self.builder
                .build_load(
                    self.context.ptr_type(inkwell::AddressSpace::default()),
                    ptr,
                    "tensor.clone.src",
                )?
                .into_pointer_value()
        } else {
            ptr
        };
        // A clone takes the source's extents and byte length, so its own dynamic axes come from
        // the source handle, which the copy never outgrows.
        let Type::Tensor { shape, .. } = tensor_ty else {
            return Err(CodegenError::InternalError(
                "a tensor clone receiver is not a tensor".to_string(),
            ));
        };
        let cloned = self.split_on_home(
            source,
            |this| {
                let source_data = this.load_dlpack_data(source)?;
                let dims = this.handle_dynamic_extents(source, shape)?;
                let size = this.dlpack_copy_length(source, tensor_ty)?;
                let (handle, data) = this.alloc_tensor(tensor_ty, "tensor.clone", &dims, size)?;
                this.build_memcpy_call(data, source_data, size)?;
                Ok(Some(handle.into()))
            },
            |this, index| {
                let source_data = this.load_dlpack_data(source)?;
                let dims = this.handle_dynamic_extents(source, shape)?;
                let size = this.dlpack_copy_length(source, tensor_ty)?;
                let data = this.device_clone(source_data, size, index)?;
                let handle =
                    this.device_tensor_over(data, tensor_ty, "tensor.clone", index, &dims, size)?;
                Ok(Some(handle.into()))
            },
        )?;
        cloned.ok_or_else(|| CodegenError::InternalError("a clone produced no tensor".into()))
    }

    /// Lower `.reshape(...)` / `.flatten(...)`, which keep the element order: the
    /// receiver's own handle comes back with its rank, extents, and strides rewritten. The
    /// elements are already where the result wants them, so nothing is copied and the
    /// DLPack `data` pointer does not move. A permuting cast is computed in MLIR.
    ///
    /// `extents` gives the run-time extent of each dynamic axis of the result, evaluated against
    /// the receiver (operand 0) before its shape is rewritten.
    pub(crate) fn codegen_tensor_redescribe(
        &mut self,
        receiver: &HirExpr,
        result_ty: &Type,
        extents: &[HirExtent],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let receiver_ty = Type::from_hir(&receiver.ty);
        let BasicValueEnum::PointerValue(source) = self.codegen_expr(receiver)? else {
            return Err(CodegenError::InternalError(
                "a tensor receiver does not lower to a pointer".to_string(),
            ));
        };
        self.mark_moved_for_drop(receiver)?;
        let Type::Tensor {
            shape: receiver_shape,
            ..
        } = receiver_ty.referent()
        else {
            return Err(CodegenError::InternalError(
                "a tensor receiver does not carry a tensor type".to_string(),
            ));
        };
        let operands = [ExtentOperand::Tensor {
            handle: source,
            shape: receiver_shape.clone(),
        }];
        let dims = extents
            .iter()
            .map(|extent| self.eval_extent(extent, &operands))
            .collect::<CodegenResult<Vec<_>>>()?;
        self.build_dlpack_redescribe(source, &receiver_ty, result_ty, &dims)?;
        // The derivatives belonged to the consumed receiver and are shaped like it, so the
        // result starts without them.
        self.release_derivatives(source)?;
        Ok(source.into())
    }

    /// Lower `tensor.to(device)`: the consuming device transfer.
    ///
    /// Both the device and where the tensor lives now are run-time values, so the choice
    /// is made at run time: a tensor already where it is asked to go is left alone, and
    /// one elsewhere, another GPU included, has its elements copied into a buffer at the
    /// destination and its old buffer released. Either way the result is the receiver's own handle, re-pointed at
    /// the new buffer, so a transfer allocates no second tensor. The GPU runtime checks the
    /// index, and aborts with a diagnostic when no usable GPU answers to it.
    ///
    /// The transfer hands ownership on: the receiver's drop flag is cleared here, or the
    /// one handle would be released twice.
    pub(crate) fn codegen_tensor_to(
        &mut self,
        receiver: &HirExpr,
        args: &[HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let BasicValueEnum::PointerValue(handle) = self.codegen_expr(receiver)? else {
            return Err(CodegenError::InternalError(
                "a tensor receiver does not lower to a pointer".to_string(),
            ));
        };
        self.mark_moved_for_drop(receiver)?;
        let device = args.first().ok_or_else(|| {
            CodegenError::InternalError("`.to` reached codegen without a device".to_string())
        })?;
        let BasicValueEnum::StructValue(device_val) = self.codegen_expr(device)? else {
            return Err(CodegenError::InternalError(
                "`.to` device argument is not an enum value".to_string(),
            ));
        };
        let tag = self
            .builder
            .build_extract_value(device_val, 0, "device.tag")?
            .into_int_value();
        let host = self.enum_variant_tag(DEVICE_ENUM, DEVICE_HOST_VARIANT)?;
        let to_host = self.builder.build_int_compare(
            IntPredicate::EQ,
            tag,
            self.context.i32_type().const_int(host as u64, false),
            "device.to_host",
        )?;
        if cfg!(target_os = "windows") {
            self.codegen_guard_or_panic(to_host, DEVICE_UNAVAILABLE, device.span.start)?;
            return Ok(handle.into());
        }

        let tensor_ty = Type::from_hir(&receiver.ty);
        let index = self.device_index(device_val)?;
        self.transfer_tensor(handle, &tensor_ty, to_host, index)?;
        Ok(handle.into())
    }

    /// Re-point `handle` at a buffer on the host when `to_host` is set, and on GPU `index`
    /// otherwise, copying its elements and releasing the old buffer unless it is already
    /// there.
    pub(crate) fn transfer_tensor(
        &mut self,
        handle: inkwell::values::PointerValue<'ctx>,
        tensor_ty: &Type,
        to_host: IntValue<'ctx>,
        index: IntValue<'ctx>,
    ) -> CodegenResult<()> {
        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("a device transfer outside a function".to_string())
        })?;
        let on_host = self.dlpack_on_host(handle)?;
        let to_gpu = self.context.append_basic_block(function, "to.gpu");
        let upload = self.context.append_basic_block(function, "to.upload");
        let resident = self.context.append_basic_block(function, "to.resident");
        let to_cpu = self.context.append_basic_block(function, "to.cpu");
        let download = self.context.append_basic_block(function, "to.download");
        let done = self.context.append_basic_block(function, "to.done");
        self.builder
            .build_conditional_branch(to_host, to_cpu, to_gpu)?;

        self.builder.position_at_end(to_gpu);
        self.builder
            .build_conditional_branch(on_host, upload, resident)?;

        self.builder.position_at_end(upload);
        let host_data = self.load_dlpack_data(handle)?;
        let size = self.dlpack_copy_length(handle, tensor_ty)?;
        let device_data = self.device_upload(host_data, size, index)?;
        let release_host = self.aligned_release_fn()?;
        self.builder
            .build_call(release_host, &[host_data.into()], "")?;
        self.set_dlpack_home(handle, device_data, TensorHome::Gpu(index))?;
        self.builder.build_unconditional_branch(done)?;

        self.builder.position_at_end(resident);
        let device_data = self.load_dlpack_data(handle)?;
        let from = self.dlpack_device_index(handle)?;
        let size = self.dlpack_copy_length(handle, tensor_ty)?;
        let moved = self.device_move(device_data, size, from, index)?;
        self.set_dlpack_home(handle, moved, TensorHome::Gpu(index))?;
        self.builder.build_unconditional_branch(done)?;

        self.builder.position_at_end(to_cpu);
        self.builder
            .build_conditional_branch(on_host, done, download)?;

        self.builder.position_at_end(download);
        let device_data = self.load_dlpack_data(handle)?;
        let from = self.dlpack_device_index(handle)?;
        let size = self.dlpack_copy_length(handle, tensor_ty)?;
        let host_data = self.alloc_host_buffer(size)?;
        self.device_download(host_data, device_data, from, size)?;
        let release_device = self.device_tensor_free_fn()?;
        self.builder
            .build_call(release_device, &[device_data.into(), from.into()], "")?;
        self.set_dlpack_home(handle, host_data, TensorHome::Host)?;
        self.builder.build_unconditional_branch(done)?;

        self.builder.position_at_end(done);
        Ok(())
    }

    /// The `i32` index a `Device::GPU(n)` value carries, read back out of its payload's
    /// raw words the way a `match` binding reads it.
    fn device_index(
        &self,
        device_val: inkwell::values::StructValue<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        let payload = self
            .builder
            .build_extract_value(device_val, 1, "device.payload")?
            .into_array_value();
        let words = self
            .builder
            .build_extract_value(payload, 0, "device.words")?;
        let cell = self.enum_payload_cell(self.type_mapper.enum_slot_type(DEVICE_ENUM)?)?;
        self.builder.build_store(cell, words)?;
        Ok(self
            .builder
            .build_load(self.context.i32_type(), cell, "device.index")?
            .into_int_value())
    }

    /// An LLVM constant array over `values`, which must themselves be constants.
    ///
    /// `elem_llvm` is what makes an empty run representable: a shape carrying a `0` extent
    /// is a legal tensor type with no element to take a type from.
    fn const_array_of(
        elem_llvm: BasicTypeEnum<'ctx>,
        values: impl Iterator<Item = BasicValueEnum<'ctx>>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let mut ints = Vec::new();
        let mut floats = Vec::new();
        for value in values {
            match value {
                BasicValueEnum::IntValue(v) if v.is_const() => ints.push(v),
                BasicValueEnum::FloatValue(v) if v.is_const() => floats.push(v),
                _ => {
                    return Err(CodegenError::InternalError(
                        "a tensor fill element is not a scalar constant".to_string(),
                    ));
                }
            }
        }
        match elem_llvm {
            BasicTypeEnum::IntType(int_ty) if floats.is_empty() => {
                Ok(int_ty.const_array(&ints).into())
            }
            BasicTypeEnum::FloatType(float_ty) if ints.is_empty() => {
                Ok(float_ty.const_array(&floats).into())
            }
            _ => Err(CodegenError::InternalError(
                "a tensor fill produced elements of a type the buffer does not hold".to_string(),
            )),
        }
    }
}
