// Codegen for tensor construction. A tensor value is a pointer to its own DLPack handle
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

use inkwell::intrinsics::Intrinsic;
use inkwell::module::Linkage;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, FunctionValue, IntValue, PointerValue};
use inkwell::IntPredicate;
use neuro_hir::HirExpr;

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

fn llvm_err(e: inkwell::builder::BuilderError) -> CodegenError {
    CodegenError::LlvmError(e.to_string())
}

impl<'ctx> CodegenContext<'ctx> {
    /// The element type and buffer length of a tensor type.
    fn tensor_layout(&self, ty: &Type) -> CodegenResult<(Type, usize)> {
        let Type::Tensor { element, shape } = ty else {
            return Err(CodegenError::InternalError(
                "tensor construction node does not carry a tensor type".to_string(),
            ));
        };
        Ok(((**element).clone(), shape.iter().product()))
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
                .map_err(llvm_err)
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
            self.builder.build_store(slot, value).map_err(llvm_err)?;
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
        let [rows, cols] = shape[..] else {
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
        self.builder
            .build_store(index, i64_type.const_zero())
            .map_err(llvm_err)?;

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

        self.builder
            .build_unconditional_branch(head)
            .map_err(llvm_err)?;
        self.builder.position_at_end(head);
        let i = self
            .builder
            .build_load(i64_type, index, "tensor.rand.idx")
            .map_err(llvm_err)?
            .into_int_value();
        let more = self
            .builder
            .build_int_compare(
                IntPredicate::ULT,
                i,
                i64_type.const_int(count as u64, false),
                "tensor.rand.more",
            )
            .map_err(llvm_err)?;
        self.builder
            .build_conditional_branch(more, body, done)
            .map_err(llvm_err)?;

        self.builder.position_at_end(body);
        let draw = self
            .builder
            .build_call(normal_fn, &[], "tensor.rand.draw")
            .map_err(llvm_err)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("rng helper returned void".to_string()))?
            .into_float_value();
        // The draw is standard normal in `f64`; narrowing before the affine transform
        // keeps the arithmetic at the element's own width, so an `f32` tensor rounds
        // once rather than twice.
        let draw = self
            .builder
            .build_float_cast(draw, elem_float, "tensor.rand.elem")
            .map_err(llvm_err)?;
        let scaled = self
            .builder
            .build_float_mul(std, draw, "tensor.rand.scaled")
            .map_err(llvm_err)?;
        let value = self
            .builder
            .build_float_add(mean, scaled, "tensor.rand.value")
            .map_err(llvm_err)?;
        // `i` is below `count` on this edge, because the loop head's `ULT` test is what
        // branches here, so the slot address stays inside the buffer.
        let slot = self.tensor_slot(buffer_ty, data, i)?;
        self.builder.build_store(slot, value).map_err(llvm_err)?;
        let next = self
            .builder
            .build_int_add(i, i64_type.const_int(1, false), "tensor.rand.next")
            .map_err(llvm_err)?;
        self.builder.build_store(index, next).map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(head)
            .map_err(llvm_err)?;

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
                )
                .map_err(llvm_err)?
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
            .build_extract_value(device_val, 0, "device.tag")
            .map_err(llvm_err)?
            .into_int_value();
        let host = self.enum_variant_tag(DEVICE_ENUM, DEVICE_HOST_VARIANT)?;
        let is_host = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                tag,
                self.context.i32_type().const_int(host as u64, false),
                "device.is_host",
            )
            .map_err(llvm_err)?;
        self.codegen_guard_or_panic(is_host, DEVICE_UNAVAILABLE, device.span.start)?;
        Ok(tensor)
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
            .build_load(i64_type, state.as_pointer_value(), "rng.s")
            .map_err(llvm_err)?
            .into_int_value();
        s = self.xorshift_step(s, XORSHIFT_A, true)?;
        s = self.xorshift_step(s, XORSHIFT_B, false)?;
        s = self.xorshift_step(s, XORSHIFT_C, true)?;
        self.builder
            .build_store(state.as_pointer_value(), s)
            .map_err(llvm_err)?;

        let mantissa = self
            .builder
            .build_right_shift(
                s,
                i64_type.const_int(64 - MANTISSA_BITS, false),
                false,
                "rng.mantissa",
            )
            .map_err(llvm_err)?;
        let as_float = self
            .builder
            .build_unsigned_int_to_float(mantissa, f64_type, "rng.float")
            .map_err(llvm_err)?;
        // `+1` before scaling lifts the draw off zero without shrinking the interval to
        // something a caller could distinguish: the result is `(0, 1]`.
        let shifted = self
            .builder
            .build_float_add(as_float, f64_type.const_float(1.0), "rng.shifted")
            .map_err(llvm_err)?;
        let scale = f64_type.const_float(1.0 / (1u64 << MANTISSA_BITS) as f64);
        let uniform = self
            .builder
            .build_float_mul(shifted, scale, "rng.uniform")
            .map_err(llvm_err)?;
        self.builder
            .build_return(Some(&uniform))
            .map_err(llvm_err)?;

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
            self.builder
                .build_left_shift(state, shift, "rng.shl")
                .map_err(llvm_err)?
        } else {
            self.builder
                .build_right_shift(state, shift, false, "rng.lshr")
                .map_err(llvm_err)?
        };
        self.builder
            .build_xor(state, shifted, "rng.xor")
            .map_err(llvm_err)
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
            .build_float_mul(f64_type.const_float(-2.0), ln, "rng.neg2ln")
            .map_err(llvm_err)?;
        let radius = self.call_f64(sqrt, &[scaled.into()], "rng.radius")?;
        let angle = self
            .builder
            .build_float_mul(f64_type.const_float(TWO_PI), u2, "rng.angle")
            .map_err(llvm_err)?;
        let cosine = self.call_f64(cos, &[angle.into()], "rng.cos")?;
        let normal = self
            .builder
            .build_float_mul(radius, cosine, "rng.normal")
            .map_err(llvm_err)?;
        self.builder.build_return(Some(&normal)).map_err(llvm_err)?;

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
            .build_call(callee, args, name)
            .map_err(llvm_err)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError(format!("`{name}` returned void")))?
            .into_float_value())
    }
}
