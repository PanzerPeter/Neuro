//! The owner bit of a `string` and the releases it settles.
//!
//! A `string` is `{ ptr, i64 }`, and the top bit of the `i64` says whether the value owns
//! its buffer. A literal and a view have it clear; an allocation has it set. Every reader
//! of the length masks the bit, and every release tests it, so a release of a value that
//! owns nothing is a no-op and ownership needs no compile-time proof.

use inkwell::values::{BasicValueEnum, IntValue, PointerValue, StructValue};
use neuro_hir::{HirExpr, HirExprKind, HirType};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

use super::is_fresh_value;

/// Bit 63 of a `string`'s stored length: set when the value owns its buffer.
pub(crate) const STRING_OWNER_BIT: u64 = 1 << 63;

/// Whether this expression shape allocates the tensor buffer it hands back, so that
/// nothing else can still own it.
///
/// Deliberately a whitelist rather than "not a place": an `if`, a `match` or a block
/// yields whatever its branch yields, which may be a buffer a binding still owns, and
/// releasing that would free it twice. A slice qualifies because it is a copy, while an
/// index that reads one element yields no buffer at all. A shape cast and a detach
/// consume their receiver, so the buffer they hand on has no other owner left.
pub(crate) fn builds_its_own_buffer(expr: &HirExpr) -> bool {
    if let HirExprKind::TensorIndex { .. } = expr.kind {
        return matches!(expr.ty, HirType::Tensor { .. });
    }
    matches!(
        expr.kind,
        HirExprKind::Binary { .. }
            | HirExprKind::TensorShapeCast { .. }
            | HirExprKind::TensorDetach { .. }
            | HirExprKind::Call { .. }
            | HirExprKind::TensorLiteral { .. }
            | HirExprKind::TensorFill { .. }
            | HirExprKind::TensorIdentity
            | HirExprKind::TensorRandomNormal { .. }
            | HirExprKind::TensorReduce { .. }
            | HirExprKind::TensorEinsum { .. }
            | HirExprKind::TensorApply { .. }
            | HirExprKind::TensorSort { .. }
            | HirExprKind::Math { .. }
    )
}

/// Whether `expr` yields a `string` no place still holds, so whoever consumes it is its
/// last owner: a call, a concatenation, an interpolation, or a branch, block or loop,
/// whose yielded place was moved out. A place, a literal and anything else answer
/// `false`; a literal owns nothing anyway, and a place is released by its own scope.
pub(crate) fn is_fresh_string(expr: &HirExpr) -> bool {
    if !matches!(Type::from_hir(&expr.ty), Type::String) {
        return false;
    }
    match &expr.kind {
        HirExprKind::Call { .. }
        | HirExprKind::Binary { .. }
        | HirExprKind::InterpString { .. }
        | HirExprKind::If { .. }
        | HirExprKind::Match { .. }
        | HirExprKind::Block { .. }
        | HirExprKind::Unsafe { .. }
        | HirExprKind::Loop { .. } => true,
        HirExprKind::NewtypeConstruct { value, .. } => is_fresh_string(value),
        HirExprKind::FieldAccess { .. }
        | HirExprKind::TupleIndex { .. }
        | HirExprKind::Index { .. } => is_fresh_value(expr),
        _ => false,
    }
}

impl<'ctx> CodegenContext<'ctx> {
    /// The byte count of `fat`, with the owner bit masked off.
    pub(crate) fn string_len(&self, fat: StructValue<'ctx>) -> CodegenResult<IntValue<'ctx>> {
        let raw = self
            .builder
            .build_extract_value(fat, 1, "str.len.raw")?
            .into_int_value();
        let mask = self.context.i64_type().const_int(!STRING_OWNER_BIT, false);
        Ok(self.builder.build_and(raw, mask, "str.len")?)
    }

    /// Whether `fat` owns its buffer, as an `i1`.
    pub(crate) fn string_owns(&self, fat: StructValue<'ctx>) -> CodegenResult<IntValue<'ctx>> {
        let raw = self
            .builder
            .build_extract_value(fat, 1, "str.len.raw")?
            .into_int_value();
        Ok(self.builder.build_int_compare(
            inkwell::IntPredicate::SLT,
            raw,
            self.context.i64_type().const_zero(),
            "str.owns",
        )?)
    }

    /// `fat` as a view: the same bytes, owning nothing.
    pub(crate) fn string_view(&self, fat: StructValue<'ctx>) -> CodegenResult<StructValue<'ctx>> {
        let len = self.string_len(fat)?;
        Ok(self
            .builder
            .build_insert_value(fat, len, 1, "str.view")?
            .into_struct_value())
    }

    /// A `string` that owns the `len` bytes at `ptr`, a buffer just allocated for it.
    pub(crate) fn build_owned_string_value(
        &self,
        ptr: PointerValue<'ctx>,
        len: IntValue<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let bit = self.context.i64_type().const_int(STRING_OWNER_BIT, false);
        let tagged = self.builder.build_or(len, bit, "str.owned.len")?;
        self.build_string_value(ptr, tagged)
    }

    /// Release `fat`'s buffer when it owns one. A select rather than a branch: a null
    /// pointer is a no-op for the release.
    pub(crate) fn release_string_value(&self, fat: StructValue<'ctx>) -> CodegenResult<()> {
        let owns = self.string_owns(fat)?;
        self.release_if(owns, fat)
    }

    /// Release `fat`'s buffer when `owns` holds.
    pub(crate) fn release_if(
        &self,
        owns: IntValue<'ctx>,
        fat: StructValue<'ctx>,
    ) -> CodegenResult<()> {
        let buffer = self
            .builder
            .build_extract_value(fat, 0, "str.buf")?
            .into_pointer_value();
        let null = self
            .context
            .ptr_type(inkwell::AddressSpace::default())
            .const_null();
        let target = self
            .builder
            .build_select(owns, buffer, null, "str.release")?;
        let free_fn = self.release_fn()?;
        self.builder.build_call(free_fn, &[target.into()], "")?;
        Ok(())
    }

    /// Release the `string` `expr` evaluated to, once a consumer that copies the bytes
    /// out has read it, when no place still holds it ([`is_fresh_string`]).
    pub(crate) fn release_string_temporary(
        &self,
        expr: &HirExpr,
        value: BasicValueEnum<'ctx>,
    ) -> CodegenResult<()> {
        if !is_fresh_string(expr) {
            return Ok(());
        }
        let BasicValueEnum::StructValue(fat) = value else {
            return Err(CodegenError::InternalError(
                "a string temporary reached its release as something other than a fat pointer"
                    .to_string(),
            ));
        };
        self.release_string_value(fat)
    }

    /// Release the buffer of a tensor receiver that no binding owns.
    ///
    /// A reduction or a sort READS its receiver and builds a fresh result, so a receiver
    /// that was constructed for the call (an operator result, a value a call returned, a
    /// constructor, another reduction) has no drop entry to release it at scope exit and
    /// would otherwise leak once per evaluation. Only the shapes that provably allocate a
    /// buffer of their own qualify: a place expression names a binding whose own entry
    /// covers it, and every other shape may hand back a buffer something else still owns.
    pub(crate) fn release_receiver_temporary(
        &mut self,
        receiver: &HirExpr,
        handle: PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        if matches!(Type::from_hir(&receiver.ty), Type::Reference { .. }) {
            return Ok(());
        }
        if !builds_its_own_buffer(receiver) {
            return Ok(());
        }
        self.build_dlpack_release(handle)
    }
}
