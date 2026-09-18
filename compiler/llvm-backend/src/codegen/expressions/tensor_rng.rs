// The xorshift64 generator behind `Tensor::random_normal`, and the float intrinsics its
// Box-Muller transform calls.
//
// Split from `tensors.rs`: nothing else in the backend draws a random number, and the
// generator is a self-contained pair of module-level functions rather than part of how a
// tensor is built.

use inkwell::intrinsics::Intrinsic;
use inkwell::module::Linkage;
use inkwell::values::{FunctionValue, IntValue};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};

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

impl<'ctx> CodegenContext<'ctx> {
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
    pub(super) fn get_or_define_rng_normal(&self) -> CodegenResult<FunctionValue<'ctx>> {
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
