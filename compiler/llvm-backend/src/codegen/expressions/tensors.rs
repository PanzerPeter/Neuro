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
// Three of the four construction nodes still fold to an LLVM constant: a fill, an
// identity matrix, and a literal whose elements are themselves constant all land in
// `.rodata` and reach the buffer as one `memcpy`. Only `random_normal` needs a runtime
// loop, and it now writes straight into the heap buffer.
//
// The compound assignment at the end of the file allocates nothing at all: it reuses the
// buffer the target already owns, which is the guarantee the language makes about a
// tensor's handle across an in-place update.

use ast_types::BinaryOp;
use inkwell::intrinsics::Intrinsic;
use inkwell::module::Linkage;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, FunctionValue, IntValue, PointerValue};
use inkwell::IntPredicate;
use neuro_hir::HirExpr;

use super::row_major_strides;
use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// The xorshift64 state every `random_normal` draw advances. Private to the module and
/// seeded with a fixed constant: the language offers no seed, and a fixed one makes a
/// compiled program reproducible run to run, which is what a test can assert on.
const RNG_STATE_GLOBAL: &str = "__neuro_rng_state";
/// The golden-ratio constant `2^64 / phi`, a conventional non-zero xorshift seed. Any
/// non-zero value works; zero is the one state xorshift cannot leave.
const RNG_SEED: u64 = 0x9E37_79B9_7F4A_7C15;
const RNG_UNIFORM_FN: &str = "__neuro_rng_uniform_f64";
const RNG_NORMAL_FN: &str = "__neuro_rng_normal_f64";
/// xorshift64 triple, as published by Marsaglia.
const XORSHIFT_A: u64 = 13;
const XORSHIFT_B: u64 = 7;
const XORSHIFT_C: u64 = 17;
/// A `double` has a 53-bit significand, so the top 53 bits of the state are exactly the
/// bits a uniform draw can carry without rounding twice.
const MANTISSA_BITS: u64 = 53;
const TWO_PI: f64 = std::f64::consts::TAU;

/// The prelude enum `.to(device)` takes, and the one variant this backend can lower a
/// transfer to. Any other device is a run-time abort rather than a silent no-op: the
/// buffer would still be host memory, and a program that believed otherwise would be
/// wrong about where its compute runs.
const DEVICE_ENUM: &str = "Device";
const DEVICE_HOST_VARIANT: &str = "CPU";
const DEVICE_UNAVAILABLE: &str =
    "tensor transfer to a non-host device requires the GPU backend, which this compiler \
     does not have yet";

/// A tensor operand as the emitted code reaches it: the DLPack handle that IS the tensor
/// value, the element buffer it addresses, and whether the operation consumes it.
struct TensorBuffer<'ctx> {
    handle: PointerValue<'ctx>,
    data: PointerValue<'ctx>,
    owned: bool,
}

/// One operand of an element-wise tensor operation, resolved to what the loop body reads.
enum ElementSource<'ctx> {
    /// A tensor buffer, plus the flat stride to advance per axis of the RESULT. A stride
    /// of zero is a stretched axis: every result coordinate along it reads the same
    /// element, which is the whole of broadcasting.
    Buffer {
        handle: PointerValue<'ctx>,
        data: PointerValue<'ctx>,
        strides: Vec<usize>,
        /// Whether the source has the result's own shape, so its slot index is the loop
        /// counter and no coordinate arithmetic is needed.
        contiguous: bool,
        /// Whether the operand is consumed, and so its buffer released once the result is
        /// built. A borrowed operand is only read.
        owned: bool,
    },
    /// A scalar broadcast across every element of the result.
    Scalar(BasicValueEnum<'ctx>),
}

/// The destination and extents of one matrix-product loop, bundled because emitting it
/// needs every one of them and they are meaningless apart.
struct Contraction<'a, 'ctx> {
    /// The destination's element count, `M * N`: the flat loop's trip count.
    count: usize,
    /// `N`, the destination's row length, which recovers a row and a column from the
    /// flat counter and is also the right operand's own row length.
    columns: usize,
    /// `K`, the axis read away by the product.
    contracted: usize,
    buffer_ty: BasicTypeEnum<'ctx>,
    elem_llvm: BasicTypeEnum<'ctx>,
    element_ty: &'a Type,
    destination: PointerValue<'ctx>,
    /// The source position keying the diagnostic of any run-time guard the element
    /// arithmetic needs.
    offset: usize,
}

/// The destination and shape of one element-wise loop, bundled because emitting it needs
/// every one of them and they are meaningless apart.
struct ElementwiseLoop<'a, 'ctx> {
    result_shape: &'a [usize],
    count: usize,
    buffer_ty: BasicTypeEnum<'ctx>,
    elem_llvm: BasicTypeEnum<'ctx>,
    element_ty: &'a Type,
    op: BinaryOp,
    destination: PointerValue<'ctx>,
    /// The source position keying the diagnostic of any run-time guard the element
    /// arithmetic needs.
    offset: usize,
    /// The prefix every block and value name in the loop carries, naming the node the
    /// loop was emitted for.
    name: &'a str,
}

/// The flat stride an operand advances by per step along each axis of the result.
///
/// Zero where the operand is stretched: an axis it does not have at all (a lower-rank
/// operand aligns at the trailing end), or one whose extent is 1 against a wider result.
fn broadcast_strides(result_shape: &[usize], operand_shape: &[usize]) -> Vec<usize> {
    let own = row_major_strides(operand_shape);
    let rank = result_shape.len();
    (0..rank)
        .map(|axis| {
            let depth = rank - 1 - axis;
            match operand_shape.len().checked_sub(depth + 1) {
                Some(source) if operand_shape[source] == result_shape[axis] => own[source],
                _ => 0,
            }
        })
        .collect()
}

impl<'ctx> CodegenContext<'ctx> {
    /// The element type and buffer length of a tensor type.
    fn tensor_layout(&self, ty: &Type) -> CodegenResult<(Type, usize)> {
        let Type::Tensor { element, shape } = ty else {
            return Err(CodegenError::InternalError(
                "tensor node does not carry a tensor type".to_string(),
            ));
        };
        let extents = crate::types::static_extents(shape)?;
        Ok(((**element).clone(), extents.iter().product()))
    }

    /// Allocate a tensor's DLPack handle and its element buffer, returning both: the
    /// handle *is* the tensor value, and the buffer is where elements are written.
    ///
    /// Every construction node routes through here, so there is exactly one place the
    /// allocator is chosen, the hook 2D's arena replaces.
    fn alloc_tensor(
        &mut self,
        tensor_ty: &Type,
        name: &str,
    ) -> CodegenResult<(PointerValue<'ctx>, PointerValue<'ctx>)> {
        let handle = self.alloc_dlpack_tensor(tensor_ty, name)?;
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

        let (handle, data) = self.alloc_tensor(tensor_ty, name)?;
        let size = self.dlpack_copy_length(tensor_ty)?;
        self.build_memcpy_call(data, global.as_pointer_value(), size)?;
        Ok(handle.into())
    }

    /// The address of buffer slot `index`, for a buffer of `buffer_ty`.
    fn tensor_slot(
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

        let (handle, data) = self.alloc_tensor(tensor_ty, "tensor.literal")?;
        let buffer_ty = self.tensor_buffer_type(tensor_ty)?;
        let i64_type = self.context.i64_type();
        for (index, value) in values.into_iter().enumerate() {
            let slot =
                self.tensor_slot(buffer_ty, data, i64_type.const_int(index as u64, false))?;
            self.builder.build_store(slot, value)?;
        }
        Ok(handle.into())
    }

    /// Lower `zeros()` / `ones()`: one constant repeated across the buffer.
    pub(crate) fn codegen_tensor_fill(
        &mut self,
        value: &HirExpr,
        tensor_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let (element_ty, count) = self.tensor_layout(tensor_ty)?;
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        let value = self.codegen_expr(value)?;
        let value = self.coerce_if_needed(value, elem_llvm, &element_ty)?;
        let constant = Self::const_array_of(elem_llvm, std::iter::repeat_n(value, count))?;
        self.emit_const_tensor_buffer(tensor_ty, constant, "tensor.fill")
    }

    /// Lower `identity()`: ones on the diagonal of a square rank-2 buffer, zeros
    /// elsewhere. Squareness was established before lowering.
    pub(crate) fn codegen_tensor_identity(
        &mut self,
        tensor_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let Type::Tensor { element, shape } = tensor_ty else {
            return Err(CodegenError::InternalError(
                "identity node does not carry a tensor type".to_string(),
            ));
        };
        let [rows, cols] = crate::types::static_extents(shape)?[..] else {
            return Err(CodegenError::InternalError(
                "identity is a rank-2 construction".to_string(),
            ));
        };
        let elem_llvm = self.get_any_llvm_type(element)?;
        let (zero, one) = match elem_llvm {
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
                ))
            }
        };
        let values = (0..rows * cols).map(|i| if i / cols == i % cols { one } else { zero });
        let constant = Self::const_array_of(elem_llvm, values)?;
        self.emit_const_tensor_buffer(tensor_ty, constant, "tensor.identity")
    }

    /// Lower `random_normal(mean, std)`: a counted loop that writes one draw per buffer
    /// slot. The buffer is written through a stack slot rather than built by
    /// `insertvalue`, because a weight tensor has as many elements as it has parameters
    /// and an instruction per element does not scale.
    pub(crate) fn codegen_tensor_random_normal(
        &mut self,
        mean: &HirExpr,
        std: &HirExpr,
        tensor_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let (element_ty, count) = self.tensor_layout(tensor_ty)?;
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        let BasicTypeEnum::FloatType(elem_float) = elem_llvm else {
            return Err(CodegenError::InternalError(
                "`random_normal` draws into a floating-point tensor".to_string(),
            ));
        };
        let mean = self.codegen_expr(mean)?.into_float_value();
        let std = self.codegen_expr(std)?.into_float_value();

        let normal_fn = self.get_or_define_rng_normal()?;
        let buffer_ty = self.tensor_buffer_type(tensor_ty)?;
        let (handle, data) = self.alloc_tensor(tensor_ty, "tensor.rand")?;
        let i64_type = self.context.i64_type();
        let index = self.entry_alloca(i64_type, "tensor.rand.i")?;
        self.builder.build_store(index, i64_type.const_zero())?;

        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("tensor construction outside a function".to_string())
        })?;
        let head = self
            .context
            .append_basic_block(function, "tensor.rand.head");
        let body = self
            .context
            .append_basic_block(function, "tensor.rand.body");
        let done = self
            .context
            .append_basic_block(function, "tensor.rand.done");

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let i = self
            .builder
            .build_load(i64_type, index, "tensor.rand.idx")?
            .into_int_value();
        let more = self.builder.build_int_compare(
            IntPredicate::ULT,
            i,
            i64_type.const_int(count as u64, false),
            "tensor.rand.more",
        )?;
        self.builder.build_conditional_branch(more, body, done)?;

        self.builder.position_at_end(body);
        let draw = self
            .builder
            .build_call(normal_fn, &[], "tensor.rand.draw")?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("rng helper returned void".to_string()))?
            .into_float_value();
        // The draw is standard normal in `f64`; narrowing before the affine transform
        // keeps the arithmetic at the element's own width, so an `f32` tensor rounds
        // once rather than twice.
        let draw = self
            .builder
            .build_float_cast(draw, elem_float, "tensor.rand.elem")?;
        let scaled = self
            .builder
            .build_float_mul(std, draw, "tensor.rand.scaled")?;
        let value = self
            .builder
            .build_float_add(mean, scaled, "tensor.rand.value")?;
        // `i` is below `count` on this edge, because the loop head's `ULT` test is what
        // branches here, so the slot address stays inside the buffer.
        let slot = self.tensor_slot(buffer_ty, data, i)?;
        self.builder.build_store(slot, value)?;
        let next =
            self.builder
                .build_int_add(i, i64_type.const_int(1, false), "tensor.rand.next")?;
        self.builder.build_store(index, next)?;
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(done);
        Ok(handle.into())
    }

    /// Lower `tensor.clone()`: a second buffer holding the same elements.
    ///
    /// The clone allocates and `memcpy`s, so the result owns storage of its own and the
    /// receiver keeps its address: the deep copy the language specifies, rather than a
    /// second name for one buffer.
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
        let (handle, data) = self.alloc_tensor(tensor_ty, "tensor.clone")?;
        let source_data = self.load_dlpack_data(source)?;
        let size = self.dlpack_copy_length(tensor_ty)?;
        self.build_memcpy_call(data, source_data, size)?;
        Ok(handle.into())
    }

    /// Lower `.t()` / `.reshape(...)` / `.permute(...)` / `.flatten(...)`.
    ///
    /// Both halves consume the receiver, so exactly one buffer is alive afterwards.
    /// An order-preserving cast (`permutation` is `None`) hands the receiver's own handle
    /// back with its rank, extents, and strides rewritten: the elements are already where
    /// the result wants them, so there is nothing to copy and the DLPack `data` pointer
    /// does not move. A permuting cast has to build the result's buffer, because the
    /// element order genuinely differs, and then releases the receiver's handle — the
    /// deleter, not a private free, so a `pool`-allocated tensor stays correct.
    pub(crate) fn codegen_tensor_shape_cast(
        &mut self,
        receiver: &HirExpr,
        permutation: Option<&[usize]>,
        result_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let BasicValueEnum::PointerValue(source) = self.codegen_expr(receiver)? else {
            return Err(CodegenError::InternalError(
                "a tensor receiver does not lower to a pointer".to_string(),
            ));
        };
        self.mark_moved_for_drop(receiver);

        let Some(permutation) = permutation else {
            self.build_dlpack_redescribe(source, result_ty)?;
            return Ok(source.into());
        };

        let source_ty = Type::from_hir(receiver.ty.referent());
        let Type::Tensor {
            shape: src_shape, ..
        } = &source_ty
        else {
            return Err(CodegenError::InternalError(
                "a shape cast's receiver does not carry a tensor type".to_string(),
            ));
        };
        let src_shape = crate::types::static_extents(src_shape)?;
        let (_, count) = self.tensor_layout(result_ty)?;
        let Type::Tensor {
            shape: dst_shape, ..
        } = result_ty
        else {
            return Err(CodegenError::InternalError(
                "a shape cast does not produce a tensor type".to_string(),
            ));
        };
        let dst_shape = crate::types::static_extents(dst_shape)?;
        if permutation.len() != dst_shape.len() || permutation.len() != src_shape.len() {
            return Err(CodegenError::InternalError(
                "a shape cast's permutation does not match its ranks".to_string(),
            ));
        }

        let buffer_ty = self.tensor_buffer_type(result_ty)?;
        let source_data = self.load_dlpack_data(source)?;
        let (handle, data) = self.alloc_tensor(result_ty, "tensor.permute")?;
        self.emit_permuted_copy(
            buffer_ty,
            source_data,
            data,
            count,
            &src_shape,
            &dst_shape,
            permutation,
        )?;
        self.build_dlpack_release(source)?;
        Ok(handle.into())
    }

    /// Copy `count` elements from `source` into `destination`, reading each result slot
    /// from the receiver slot the permutation points it at.
    ///
    /// One flat loop over the result's linear index rather than a nest of `rank` loops:
    /// the extents and both stride vectors are compile-time constants, so a result index
    /// decomposes into coordinates with constant divisions and recomposes into a source
    /// offset with constant multiplies. The IR is then the same size whatever the rank is.
    #[allow(clippy::too_many_arguments)]
    fn emit_permuted_copy(
        &mut self,
        buffer_ty: BasicTypeEnum<'ctx>,
        source: PointerValue<'ctx>,
        destination: PointerValue<'ctx>,
        count: usize,
        src_shape: &[usize],
        dst_shape: &[usize],
        permutation: &[usize],
    ) -> CodegenResult<()> {
        let src_strides = row_major_strides(src_shape);
        let dst_strides = row_major_strides(dst_shape);
        let i64_type = self.context.i64_type();
        let BasicTypeEnum::ArrayType(buffer_array) = buffer_ty else {
            return Err(CodegenError::InternalError(
                "a tensor buffer is not an array type".to_string(),
            ));
        };
        let element_ty = buffer_array.get_element_type();

        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("a shape cast outside a function".to_string())
        })?;
        let index = self.entry_alloca(i64_type, "tensor.permute.i")?;
        self.builder.build_store(index, i64_type.const_zero())?;
        let head = self
            .context
            .append_basic_block(function, "tensor.permute.head");
        let body = self
            .context
            .append_basic_block(function, "tensor.permute.body");
        let done = self
            .context
            .append_basic_block(function, "tensor.permute.done");

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let i = self
            .builder
            .build_load(i64_type, index, "tensor.permute.idx")?
            .into_int_value();
        let more = self.builder.build_int_compare(
            IntPredicate::ULT,
            i,
            i64_type.const_int(count as u64, false),
            "tensor.permute.more",
        )?;
        self.builder.build_conditional_branch(more, body, done)?;

        self.builder.position_at_end(body);
        let mut offset = i64_type.const_zero();
        for axis in 0..dst_shape.len() {
            let coord = self.builder.build_int_unsigned_div(
                i,
                i64_type.const_int(dst_strides[axis] as u64, false),
                "tensor.permute.div",
            )?;
            let coord = self.builder.build_int_unsigned_rem(
                coord,
                i64_type.const_int(dst_shape[axis] as u64, false),
                "tensor.permute.coord",
            )?;
            let scaled = self.builder.build_int_mul(
                coord,
                i64_type.const_int(src_strides[permutation[axis]] as u64, false),
                "tensor.permute.scaled",
            )?;
            offset = self
                .builder
                .build_int_add(offset, scaled, "tensor.permute.offset")?;
        }

        // Both indices are below `count` on this edge: `i` by the loop head's test, and
        // `offset` because a permutation is a bijection over the same element run.
        let from = self.tensor_slot(buffer_ty, source, offset)?;
        let value = self
            .builder
            .build_load(element_ty, from, "tensor.permute.value")?;
        let into = self.tensor_slot(buffer_ty, destination, i)?;
        self.builder.build_store(into, value)?;

        let next =
            self.builder
                .build_int_add(i, i64_type.const_int(1, false), "tensor.permute.next")?;
        self.builder.build_store(index, next)?;
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(done);
        Ok(())
    }

    /// Lower `tensor.to(device)`: the consuming device transfer.
    ///
    /// Every buffer this backend can build is host memory, so a transfer to the host is
    /// the move itself and costs nothing. A transfer anywhere else has no lowering at all,
    /// and the device is an ordinary run-time value, so the mismatch is caught where the
    /// value is known: a guard on the discriminant that aborts with a diagnostic rather
    /// than letting the program run somewhere it did not ask for.
    ///
    /// The result is the receiver's own buffer pointer, so the transfer hands ownership on:
    /// the receiver's drop flag is cleared here, or the one buffer would be freed twice.
    pub(crate) fn codegen_tensor_to(
        &mut self,
        receiver: &HirExpr,
        args: &[HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let tensor = self.codegen_expr(receiver)?;
        self.mark_moved_for_drop(receiver);
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
        let is_host = self.builder.build_int_compare(
            IntPredicate::EQ,
            tag,
            self.context.i32_type().const_int(host as u64, false),
            "device.is_host",
        )?;
        self.codegen_guard_or_panic(is_host, DEVICE_UNAVAILABLE, device.span.start)?;
        Ok(tensor)
    }

    /// Lower `target OP= value` on a tensor: an element-wise update written straight
    /// into the buffer `target`'s handle already addresses.
    ///
    /// Nothing is allocated. That is the point of the node rather than an optimization
    /// of it: the DLPack handle and its `data` pointer are unchanged across the
    /// statement, so a raw pointer held by an optimizer, by the runtime, or by a foreign
    /// consumer stays valid. The desugaring this node exists to avoid would build a second
    /// tensor and rebind the name to it, invalidating both.
    ///
    /// The evaluation order is fixed by the language: the right-hand side is evaluated
    /// before the target is touched.
    ///
    /// The right-hand side broadcasts under the by-value operator's rule, with the
    /// one asymmetry the in-place write forces: it may be stretched UP to the target's
    /// shape and no further, because the result goes back into the target's own buffer.
    pub(crate) fn codegen_tensor_compound_assign(
        &mut self,
        target: &str,
        op: BinaryOp,
        value: &HirExpr,
        ty: &neuro_hir::HirType,
        offset: usize,
    ) -> CodegenResult<()> {
        let tensor_ty = Type::from_hir(ty);
        let (element_ty, count) = self.tensor_layout(&tensor_ty)?;
        let Type::Tensor { shape, .. } = &tensor_ty else {
            return Err(CodegenError::InternalError(
                "a tensor compound assignment does not carry a tensor type".to_string(),
            ));
        };
        let result_shape = crate::types::static_extents(shape)?;

        let rhs = self.codegen_operand_source(value, &result_shape)?;

        let target_ptr = *self
            .variables
            .get(target)
            .ok_or_else(|| CodegenError::UndefinedVariable(target.to_string()))?;
        let lhs_handle = self
            .builder
            .build_load(
                self.context.ptr_type(inkwell::AddressSpace::default()),
                target_ptr,
                "tensor.op.lhs",
            )?
            .into_pointer_value();
        let lhs_data = self.load_dlpack_data(lhs_handle)?;
        // The target is both the left operand and the destination, so it is walked slot
        // for slot: it is the shape everything else broadcasts to.
        let lhs = ElementSource::Buffer {
            handle: lhs_handle,
            data: lhs_data,
            strides: broadcast_strides(&result_shape, &result_shape),
            contiguous: true,
            owned: false,
        };

        let buffer_ty = self.tensor_buffer_type(&tensor_ty)?;
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        self.emit_elementwise_loop(
            ElementwiseLoop {
                result_shape: &result_shape,
                count,
                buffer_ty,
                elem_llvm,
                element_ty: &element_ty,
                op,
                destination: lhs_data,
                offset,
                name: "tensor.op",
            },
            &lhs,
            &rhs,
        )?;

        // An owned operand is consumed by the update, so its buffer is released here:
        // it has no binding left to free it at scope exit.
        self.release_consumed_operand(value, &rhs)
    }

    /// Lower a by-value tensor operator: `a + b`, `&a + &b`, and the scalar broadcast
    ///
    /// Unlike the compound assignment above, this ALLOCATES: the operator's contract is a
    /// fresh tensor, which is what lets it read two borrows and what makes `w = w + g`
    /// observably different from `w -= g`. An owned operand is consumed, so its buffer is
    /// released once the result is built; a borrowed one is only read.
    pub(crate) fn codegen_tensor_binary(
        &mut self,
        left: &HirExpr,
        op: BinaryOp,
        right: &HirExpr,
        result_ty: &Type,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        // `@` contracts an axis instead of walking the result element for element, so it
        // is a different loop shape rather than a different body.
        if matches!(op, BinaryOp::MatMul) {
            return self.codegen_tensor_matmul(left, right, result_ty, offset);
        }
        let (element_ty, count) = self.tensor_layout(result_ty)?;
        let Type::Tensor { shape, .. } = result_ty else {
            return Err(CodegenError::InternalError(
                "a tensor operator does not carry a tensor result type".to_string(),
            ));
        };
        let result_shape = crate::types::static_extents(shape)?;

        // Left to right: the operands are two ordinary expressions and the language
        // evaluates them in source order.
        let lhs = self.codegen_operand_source(left, &result_shape)?;
        let rhs = self.codegen_operand_source(right, &result_shape)?;

        let (handle, data) = self.alloc_tensor(result_ty, "tensor.bin")?;
        let buffer_ty = self.tensor_buffer_type(result_ty)?;
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        self.emit_elementwise_loop(
            ElementwiseLoop {
                result_shape: &result_shape,
                count,
                buffer_ty,
                elem_llvm,
                element_ty: &element_ty,
                op,
                destination: data,
                offset,
                name: "tensor.bin",
            },
            &lhs,
            &rhs,
        )?;

        self.release_consumed_operand(left, &lhs)?;
        self.release_consumed_operand(right, &rhs)?;
        Ok(handle.into())
    }

    /// Lower `a @ b`: the matrix product of an `[M, K]` and a `[K, N]` into a fresh
    /// `[M, N]` tensor.
    ///
    /// The destination is walked flat, one iteration per output element, and the
    /// contraction is the inner loop over K — two loops rather than three, since the
    /// row and column are recovered from the flat counter the destination already needs.
    /// The accumulator lives in an entry `alloca` because the element arithmetic may
    /// split the body around an overflow guard, which a `phi` over the loop would then
    /// have to chase.
    fn codegen_tensor_matmul(
        &mut self,
        left: &HirExpr,
        right: &HirExpr,
        result_ty: &Type,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let (element_ty, count) = self.tensor_layout(result_ty)?;
        let Type::Tensor { shape, .. } = result_ty else {
            return Err(CodegenError::InternalError(
                "a tensor operator does not carry a tensor result type".to_string(),
            ));
        };
        let [_rows, columns] = crate::types::static_extents(shape)?[..] else {
            return Err(CodegenError::InternalError(
                "`@` produces a rank-2 tensor".to_string(),
            ));
        };

        // Left to right: the operands are two ordinary expressions and the language
        // evaluates them in source order.
        let Some((lhs, lhs_shape)) = self.codegen_tensor_operand(left)? else {
            return Err(CodegenError::InternalError(
                "`@` takes two tensor operands".to_string(),
            ));
        };
        let Some((rhs, _)) = self.codegen_tensor_operand(right)? else {
            return Err(CodegenError::InternalError(
                "`@` takes two tensor operands".to_string(),
            ));
        };
        // The contracted extent is the left operand's trailing axis; the checker has
        // already established the right operand's leading axis equals it.
        let [_, contracted] = lhs_shape[..] else {
            return Err(CodegenError::InternalError(
                "`@` takes two rank-2 tensor operands".to_string(),
            ));
        };

        let (handle, destination) = self.alloc_tensor(result_ty, "tensor.mm")?;
        let buffer_ty = self.tensor_buffer_type(result_ty)?;
        let elem_llvm = self.get_any_llvm_type(&element_ty)?;
        self.emit_contraction_loop(
            Contraction {
                count,
                columns,
                contracted,
                buffer_ty,
                elem_llvm,
                element_ty: &element_ty,
                destination,
                offset,
            },
            &lhs,
            &rhs,
        )?;

        self.release_consumed_buffer(left, &lhs)?;
        self.release_consumed_buffer(right, &rhs)?;
        Ok(handle.into())
    }

    /// Walk every slot of the destination, accumulating the dot product of the left
    /// operand's row and the right operand's column that meet there.
    fn emit_contraction_loop(
        &mut self,
        spec: Contraction<'_, 'ctx>,
        lhs: &TensorBuffer<'ctx>,
        rhs: &TensorBuffer<'ctx>,
    ) -> CodegenResult<()> {
        let Contraction {
            count,
            columns,
            contracted,
            buffer_ty,
            elem_llvm,
            element_ty,
            destination,
            offset,
        } = spec;
        let i64_type = self.context.i64_type();
        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("a tensor operation outside a function".to_string())
        })?;

        let slot_index = self.entry_alloca(i64_type, "tensor.mm.i")?;
        let step = self.entry_alloca(i64_type, "tensor.mm.k")?;
        let total = self.entry_alloca(elem_llvm, "tensor.mm.acc")?;
        self.builder
            .build_store(slot_index, i64_type.const_zero())?;

        let head = self.context.append_basic_block(function, "tensor.mm.head");
        let body = self.context.append_basic_block(function, "tensor.mm.body");
        let inner_head = self
            .context
            .append_basic_block(function, "tensor.mm.inner.head");
        let inner_body = self
            .context
            .append_basic_block(function, "tensor.mm.inner.body");
        let store = self.context.append_basic_block(function, "tensor.mm.store");
        let done = self.context.append_basic_block(function, "tensor.mm.done");

        self.builder.build_unconditional_branch(head)?;
        self.builder.position_at_end(head);
        let i = self
            .builder
            .build_load(i64_type, slot_index, "tensor.mm.idx")?
            .into_int_value();
        let more = self.builder.build_int_compare(
            IntPredicate::ULT,
            i,
            i64_type.const_int(count as u64, false),
            "tensor.mm.more",
        )?;
        self.builder.build_conditional_branch(more, body, done)?;

        // The destination is row-major, so its flat counter carries both coordinates: the
        // row selects the left operand's row, the column the right operand's column.
        self.builder.position_at_end(body);
        let width = i64_type.const_int(columns as u64, false);
        let row = self
            .builder
            .build_int_unsigned_div(i, width, "tensor.mm.row")?;
        let column = self
            .builder
            .build_int_unsigned_rem(i, width, "tensor.mm.col")?;
        self.builder.build_store(total, elem_llvm.const_zero())?;
        self.builder.build_store(step, i64_type.const_zero())?;
        self.builder.build_unconditional_branch(inner_head)?;

        self.builder.position_at_end(inner_head);
        let k = self
            .builder
            .build_load(i64_type, step, "tensor.mm.step")?
            .into_int_value();
        let stepping = self.builder.build_int_compare(
            IntPredicate::ULT,
            k,
            i64_type.const_int(contracted as u64, false),
            "tensor.mm.stepping",
        )?;
        self.builder
            .build_conditional_branch(stepping, inner_body, store)?;

        self.builder.position_at_end(inner_body);
        // `row` and `k` are below their own extents on this edge, so both addresses stay
        // inside the operand buffers the checker sized them against.
        let left_at = self.builder.build_int_add(
            self.builder.build_int_mul(
                row,
                i64_type.const_int(contracted as u64, false),
                "tensor.mm.lrow",
            )?,
            k,
            "tensor.mm.lhs",
        )?;
        let right_at = self.builder.build_int_add(
            self.builder.build_int_mul(k, width, "tensor.mm.rrow")?,
            column,
            "tensor.mm.rhs",
        )?;
        let a = self.load_buffer_element(lhs.data, left_at, buffer_ty, elem_llvm, "tensor.mm.a")?;
        let b =
            self.load_buffer_element(rhs.data, right_at, buffer_ty, elem_llvm, "tensor.mm.b")?;
        let product = self.tensor_element_arith(BinaryOp::Multiply, a, b, element_ty, offset)?;
        let running = self
            .builder
            .build_load(elem_llvm, total, "tensor.mm.running")?;
        let summed =
            self.tensor_element_arith(BinaryOp::Add, running, product, element_ty, offset)?;
        self.builder.build_store(total, summed)?;
        let next_step =
            self.builder
                .build_int_add(k, i64_type.const_int(1, false), "tensor.mm.next.k")?;
        self.builder.build_store(step, next_step)?;
        // The element arithmetic may have split the body around an overflow guard, so the
        // back edge leaves whichever block is current now.
        self.builder.build_unconditional_branch(inner_head)?;

        self.builder.position_at_end(store);
        let value = self.builder.build_load(elem_llvm, total, "tensor.mm.sum")?;
        let slot = self.tensor_slot(buffer_ty, destination, i)?;
        self.builder.build_store(slot, value)?;
        let next = self
            .builder
            .build_int_add(i, i64_type.const_int(1, false), "tensor.mm.next")?;
        self.builder.build_store(slot_index, next)?;
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(done);
        Ok(())
    }

    /// One element read out of a tensor buffer at a flat index its caller has bounded.
    fn load_buffer_element(
        &self,
        buffer: PointerValue<'ctx>,
        index: IntValue<'ctx>,
        buffer_ty: BasicTypeEnum<'ctx>,
        elem_llvm: BasicTypeEnum<'ctx>,
        name: &str,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let slot = self.tensor_slot(buffer_ty, buffer, index)?;
        self.builder
            .build_load(elem_llvm, slot, name)
            .map_err(CodegenError::from)
    }

    /// Resolve one operand of an element-wise tensor operation to what the loop body
    /// reads from it: a buffer walked at broadcast strides, or a scalar.
    fn codegen_operand_source(
        &mut self,
        operand: &HirExpr,
        result_shape: &[usize],
    ) -> CodegenResult<ElementSource<'ctx>> {
        let Some((buffer, operand_shape)) = self.codegen_tensor_operand(operand)? else {
            return Ok(ElementSource::Scalar(self.codegen_expr(operand)?));
        };
        Ok(ElementSource::Buffer {
            handle: buffer.handle,
            data: buffer.data,
            strides: broadcast_strides(result_shape, &operand_shape),
            contiguous: operand_shape == result_shape,
            owned: buffer.owned,
        })
    }

    /// Lower one operand of a tensor operation to its handle, its element buffer and its
    /// own extents, or `None` when the operand is not a tensor at all.
    ///
    /// A scalar operand is deliberately NOT lowered here: answering `None` before emitting
    /// anything lets the caller decide what a non-tensor operand means for its operation,
    /// which is a broadcast for the element-wise family and an error for `@`.
    fn codegen_tensor_operand(
        &mut self,
        operand: &HirExpr,
    ) -> CodegenResult<Option<(TensorBuffer<'ctx>, Vec<usize>)>> {
        let operand_ty = Type::from_hir(&operand.ty);
        let Type::Tensor { shape, .. } = operand_ty.referent() else {
            return Ok(None);
        };
        let operand_shape = crate::types::static_extents(shape)?;
        let BasicValueEnum::PointerValue(ptr) = self.codegen_expr(operand)? else {
            return Err(CodegenError::InternalError(
                "a tensor operand does not lower to a pointer".to_string(),
            ));
        };
        // A borrowed operand lowers to the address of the handle pointer, an owned one to
        // the handle pointer itself; both are `ptr`, so the type is what tells them apart.
        let owned = !matches!(operand_ty, Type::Reference { .. });
        let handle = if owned {
            ptr
        } else {
            self.builder
                .build_load(
                    self.context.ptr_type(inkwell::AddressSpace::default()),
                    ptr,
                    "tensor.operand",
                )?
                .into_pointer_value()
        };
        Ok(Some((
            TensorBuffer {
                data: self.load_dlpack_data(handle)?,
                handle,
                owned,
            },
            operand_shape,
        )))
    }

    /// Release the buffer of an operand the operation consumed. A borrowed operand and a
    /// scalar own nothing, so both are left alone.
    fn release_consumed_operand(
        &mut self,
        operand: &HirExpr,
        source: &ElementSource<'ctx>,
    ) -> CodegenResult<()> {
        let ElementSource::Buffer {
            handle,
            owned: true,
            ..
        } = source
        else {
            return Ok(());
        };
        self.mark_moved_for_drop(operand);
        self.build_dlpack_release(*handle)
    }

    /// Release the buffer of a matmul operand the operator consumed.
    fn release_consumed_buffer(
        &mut self,
        operand: &HirExpr,
        buffer: &TensorBuffer<'ctx>,
    ) -> CodegenResult<()> {
        if !buffer.owned {
            return Ok(());
        }
        self.mark_moved_for_drop(operand);
        self.build_dlpack_release(buffer.handle)
    }

    /// Walk every slot of the destination buffer, applying `op` to the two sources at the
    /// broadcast position each one reads.
    ///
    /// The destination is written contiguously, so the loop counter IS its slot index; a
    /// source is addressed by decomposing that counter into result coordinates and
    /// recombining them at the source's own strides. A stretched axis carries stride 0,
    /// which is what makes broadcasting a matter of arithmetic rather than a second loop
    /// shape.
    fn emit_elementwise_loop(
        &mut self,
        loop_spec: ElementwiseLoop<'_, 'ctx>,
        lhs: &ElementSource<'ctx>,
        rhs: &ElementSource<'ctx>,
    ) -> CodegenResult<()> {
        let ElementwiseLoop {
            result_shape,
            count,
            buffer_ty,
            elem_llvm,
            element_ty,
            op,
            destination,
            offset,
            name,
        } = loop_spec;
        let i64_type = self.context.i64_type();
        let result_strides = row_major_strides(result_shape);
        let function = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("a tensor operation outside a function".to_string())
        })?;
        let index = self.entry_alloca(i64_type, &format!("{name}.i"))?;
        self.builder.build_store(index, i64_type.const_zero())?;
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
        let more = self.builder.build_int_compare(
            IntPredicate::ULT,
            i,
            i64_type.const_int(count as u64, false),
            &format!("{name}.more"),
        )?;
        self.builder.build_conditional_branch(more, body, done)?;

        self.builder.position_at_end(body);
        // Computed once and shared by both sources: a rank-2 result decomposes its
        // counter twice, not once per operand.
        let stretched = |source: &ElementSource<'ctx>| {
            matches!(
                source,
                ElementSource::Buffer {
                    contiguous: false,
                    ..
                }
            )
        };
        let coords = if stretched(lhs) || stretched(rhs) {
            self.result_coordinates(i, result_shape, &result_strides, name)?
        } else {
            Vec::new()
        };

        let lhs_elem = self.load_source_element(lhs, i, &coords, buffer_ty, elem_llvm, name)?;
        let rhs_elem = self.load_source_element(rhs, i, &coords, buffer_ty, elem_llvm, name)?;
        let updated = self.tensor_element_arith(op, lhs_elem, rhs_elem, element_ty, offset)?;
        // `i` is below `count` on this edge, because the head's `ULT` test is what
        // branches here, so the slot stays inside the destination buffer.
        let slot = self.tensor_slot(buffer_ty, destination, i)?;
        self.builder.build_store(slot, updated)?;

        let next =
            self.builder
                .build_int_add(i, i64_type.const_int(1, false), &format!("{name}.next"))?;
        self.builder.build_store(index, next)?;
        // The element arithmetic may have split the body around an overflow or
        // divide-by-zero guard, so the back edge leaves whichever block is current now.
        self.builder.build_unconditional_branch(head)?;

        self.builder.position_at_end(done);
        Ok(())
    }

    /// Decompose a flat result index into one coordinate per result axis.
    fn result_coordinates(
        &self,
        index: IntValue<'ctx>,
        result_shape: &[usize],
        result_strides: &[usize],
        name: &str,
    ) -> CodegenResult<Vec<IntValue<'ctx>>> {
        let i64_type = self.context.i64_type();
        result_shape
            .iter()
            .enumerate()
            .map(|(axis, extent)| {
                let scaled = self.builder.build_int_unsigned_div(
                    index,
                    i64_type.const_int(result_strides[axis] as u64, false),
                    &format!("{name}.div"),
                )?;
                self.builder
                    .build_int_unsigned_rem(
                        scaled,
                        i64_type.const_int(*extent as u64, false),
                        &format!("{name}.coord"),
                    )
                    .map_err(CodegenError::from)
            })
            .collect()
    }

    /// The element one source contributes at the current result position.
    fn load_source_element(
        &mut self,
        source: &ElementSource<'ctx>,
        index: IntValue<'ctx>,
        coords: &[IntValue<'ctx>],
        buffer_ty: BasicTypeEnum<'ctx>,
        elem_llvm: BasicTypeEnum<'ctx>,
        name: &str,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let ElementSource::Buffer {
            data,
            strides,
            contiguous,
            ..
        } = source
        else {
            let ElementSource::Scalar(value) = source else {
                return Err(CodegenError::InternalError(
                    "an element source is a buffer or a scalar".to_string(),
                ));
            };
            return Ok(*value);
        };
        let i64_type = self.context.i64_type();
        // A source of the result's own shape is walked slot for slot, which is both the
        // common case and the one where the coordinate arithmetic buys nothing.
        let source_index = if *contiguous {
            index
        } else {
            let mut address = i64_type.const_zero();
            for (coord, stride) in coords.iter().zip(strides) {
                let scaled = self.builder.build_int_mul(
                    *coord,
                    i64_type.const_int(*stride as u64, false),
                    &format!("{name}.scaled"),
                )?;
                address = self
                    .builder
                    .build_int_add(address, scaled, &format!("{name}.src"))?;
            }
            address
        };
        // Every stride above is the source's own and is zero on an axis the source does
        // not walk, so the address stays inside the source's own buffer.
        let slot = self.tensor_slot(buffer_ty, *data, source_index)?;
        self.builder
            .build_load(elem_llvm, slot, &format!("{name}.elem"))
            .map_err(CodegenError::from)
    }

    /// One element of a tensor compound assignment, with the same guards the scalar
    /// operator carries: a tensor's arithmetic is its element's arithmetic, so an
    /// overflowing element panics exactly where an overflowing scalar would.
    fn tensor_element_arith(
        &mut self,
        op: BinaryOp,
        lhs: BasicValueEnum<'ctx>,
        rhs: BasicValueEnum<'ctx>,
        element_ty: &Type,
        offset: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        if let (BasicValueEnum::FloatValue(a), BasicValueEnum::FloatValue(b)) = (lhs, rhs) {
            let value = match op {
                BinaryOp::Add => self.builder.build_float_add(a, b, "tensor.op.add"),
                BinaryOp::Subtract => self.builder.build_float_sub(a, b, "tensor.op.sub"),
                BinaryOp::Multiply => self.builder.build_float_mul(a, b, "tensor.op.mul"),
                BinaryOp::Divide => self.builder.build_float_div(a, b, "tensor.op.div"),
                BinaryOp::Modulo => self.builder.build_float_rem(a, b, "tensor.op.rem"),
                _ => {
                    return Err(CodegenError::InternalError(
                        "a compound assignment carries an arithmetic operator".to_string(),
                    ))
                }
            };
            return Ok(value?.into());
        }
        let (BasicValueEnum::IntValue(a), BasicValueEnum::IntValue(b)) = (lhs, rhs) else {
            return Err(CodegenError::InternalError(
                "a tensor element is an integer or a float".to_string(),
            ));
        };
        let unsigned = crate::type_mapping::TypeMapper::is_unsigned_int(element_ty);
        let value = match op {
            BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply => {
                self.codegen_int_arith(op, a, b, unsigned, offset, "tensor.op.arith")?
            }
            BinaryOp::Divide | BinaryOp::Modulo => {
                self.codegen_int_div_rem(op, a, b, unsigned, offset, "tensor.op.divrem")?
            }
            _ => {
                return Err(CodegenError::InternalError(
                    "a compound assignment carries an arithmetic operator".to_string(),
                ))
            }
        };
        Ok(value.into())
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
                    ))
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

    /// The module's xorshift64 state, reserved on first use.
    fn get_or_create_rng_state(&self) -> inkwell::values::GlobalValue<'ctx> {
        if let Some(existing) = self.module.get_global(RNG_STATE_GLOBAL) {
            return existing;
        }
        let i64_type = self.context.i64_type();
        let global = self.module.add_global(i64_type, None, RNG_STATE_GLOBAL);
        global.set_linkage(Linkage::Private);
        global.set_initializer(&i64_type.const_int(RNG_SEED, false));
        global
    }

    /// `double __neuro_rng_uniform_f64()`, one xorshift64 step rendered as a uniform
    /// draw in `(0, 1]`. The interval excludes zero because the normal transform takes
    /// its logarithm.
    fn get_or_define_rng_uniform(&self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(RNG_UNIFORM_FN) {
            return Ok(existing);
        }
        let f64_type = self.context.f64_type();
        let i64_type = self.context.i64_type();
        let func = self.module.add_function(
            RNG_UNIFORM_FN,
            f64_type.fn_type(&[], false),
            Some(Linkage::Internal),
        );
        let saved = self.builder.get_insert_block();
        let entry = self.context.append_basic_block(func, "entry");
        self.builder.position_at_end(entry);

        let state = self.get_or_create_rng_state();
        let mut s = self
            .builder
            .build_load(i64_type, state.as_pointer_value(), "rng.s")?
            .into_int_value();
        s = self.xorshift_step(s, XORSHIFT_A, true)?;
        s = self.xorshift_step(s, XORSHIFT_B, false)?;
        s = self.xorshift_step(s, XORSHIFT_C, true)?;
        self.builder.build_store(state.as_pointer_value(), s)?;

        let mantissa = self.builder.build_right_shift(
            s,
            i64_type.const_int(64 - MANTISSA_BITS, false),
            false,
            "rng.mantissa",
        )?;
        let as_float = self
            .builder
            .build_unsigned_int_to_float(mantissa, f64_type, "rng.float")?;
        // `+1` before scaling lifts the draw off zero without shrinking the interval to
        // something a caller could distinguish: the result is `(0, 1]`.
        let shifted =
            self.builder
                .build_float_add(as_float, f64_type.const_float(1.0), "rng.shifted")?;
        let scale = f64_type.const_float(1.0 / (1u64 << MANTISSA_BITS) as f64);
        let uniform = self
            .builder
            .build_float_mul(shifted, scale, "rng.uniform")?;
        self.builder.build_return(Some(&uniform))?;

        if let Some(block) = saved {
            self.builder.position_at_end(block);
        }
        Ok(func)
    }

    /// One `s ^= s << n` / `s ^= s >> n` step of the xorshift64 generator.
    fn xorshift_step(
        &self,
        state: IntValue<'ctx>,
        amount: u64,
        left: bool,
    ) -> CodegenResult<IntValue<'ctx>> {
        let shift = self.context.i64_type().const_int(amount, false);
        let shifted = if left {
            self.builder.build_left_shift(state, shift, "rng.shl")?
        } else {
            self.builder
                .build_right_shift(state, shift, false, "rng.lshr")?
        };
        self.builder
            .build_xor(state, shifted, "rng.xor")
            .map_err(CodegenError::from)
    }

    /// `double __neuro_rng_normal_f64()`, one standard-normal draw by the Box-Muller
    /// transform. Both uniforms are consumed per call rather than caching the second
    /// output, so a draw depends on nothing but the generator state.
    fn get_or_define_rng_normal(&self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(RNG_NORMAL_FN) {
            return Ok(existing);
        }
        let uniform = self.get_or_define_rng_uniform()?;
        let f64_type = self.context.f64_type();
        let func = self.module.add_function(
            RNG_NORMAL_FN,
            f64_type.fn_type(&[], false),
            Some(Linkage::Internal),
        );
        let saved = self.builder.get_insert_block();
        let entry = self.context.append_basic_block(func, "entry");
        self.builder.position_at_end(entry);

        let log = self.float_intrinsic("llvm.log")?;
        let sqrt = self.float_intrinsic("llvm.sqrt")?;
        let cos = self.float_intrinsic("llvm.cos")?;

        let u1 = self.call_f64(uniform, &[], "rng.u1")?;
        let u2 = self.call_f64(uniform, &[], "rng.u2")?;
        let ln = self.call_f64(log, &[u1.into()], "rng.ln")?;
        let scaled = self
            .builder
            .build_float_mul(f64_type.const_float(-2.0), ln, "rng.neg2ln")?;
        let radius = self.call_f64(sqrt, &[scaled.into()], "rng.radius")?;
        let angle = self
            .builder
            .build_float_mul(f64_type.const_float(TWO_PI), u2, "rng.angle")?;
        let cosine = self.call_f64(cos, &[angle.into()], "rng.cos")?;
        let normal = self.builder.build_float_mul(radius, cosine, "rng.normal")?;
        self.builder.build_return(Some(&normal))?;

        if let Some(block) = saved {
            self.builder.position_at_end(block);
        }
        Ok(func)
    }

    /// The `f64` overload of an LLVM floating-point intrinsic.
    fn float_intrinsic(&self, name: &str) -> CodegenResult<FunctionValue<'ctx>> {
        let intrinsic = Intrinsic::find(name)
            .ok_or_else(|| CodegenError::InternalError(format!("no `{name}` intrinsic")))?;
        intrinsic
            .get_declaration(&self.module, &[self.context.f64_type().into()])
            .ok_or_else(|| CodegenError::InternalError(format!("`{name}` has no `f64` overload")))
    }

    fn call_f64(
        &self,
        callee: FunctionValue<'ctx>,
        args: &[inkwell::values::BasicMetadataValueEnum<'ctx>],
        name: &str,
    ) -> CodegenResult<inkwell::values::FloatValue<'ctx>> {
        Ok(self
            .builder
            .build_call(callee, args, name)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError(format!("`{name}` returned void")))?
            .into_float_value())
    }
}
