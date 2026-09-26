// The derivative slots' operations: `.grad()`, `.hessian()`, `.zero_grad()`, and the moves a
// `.backward()` lowers to.
//
// `.backward()` itself never reaches the backend. The lowering turns the `@grad` call it
// pairs with into a call of the derivative, `__f__rev`, and the `.backward()` statement into
// one `__set_grad` per differentiated argument, each moving that argument's gradient out of
// the returned bundle, followed under `order: 2` by a `__set_hessian`. So the backend's share
// of the materialization layer is the slots and nothing else.

use inkwell::values::BasicValueEnum;
use neuro_hir::HirExpr;

use crate::codegen::context::CodegenContext;
use crate::codegen::dlpack::DerivativeSlot;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// The private methods a `.backward()` statement lowers to, one per differentiated argument
/// and slot. The language reserves `__` in every declared name, so no program can call them.
const SET_GRAD_METHOD: &str = "__set_grad";
const SET_HESSIAN_METHOD: &str = "__set_hessian";

const GRAD_METHOD: &str = "grad";
const HESSIAN_METHOD: &str = "hessian";
const ZERO_GRAD_METHOD: &str = "zero_grad";

/// One operation on a tensor's derivative slots.
pub(crate) enum GradSlotMethod {
    /// `.grad()` or `.hessian()`: a borrow of the derivative, or a panic when the slot is
    /// empty.
    Read(DerivativeSlot),
    /// `.zero_grad()`: release both derivatives and empty both slots.
    Clear,
    /// `__set_grad(g)` or `__set_hessian(h)`: move the derivative into its slot, releasing
    /// what it held. A new gradient also empties the Hessian slot, so a first-order
    /// `.backward()` never leaves a second derivative of an earlier point behind.
    Fill(DerivativeSlot),
}

/// The slot operation `method` names on a tensor receiver, owned or borrowed.
pub(crate) fn resolve_grad_slot_method(recv: &Type, method: &str) -> Option<GradSlotMethod> {
    if !matches!(recv.referent(), Type::Tensor { .. }) {
        return None;
    }
    match method {
        GRAD_METHOD => Some(GradSlotMethod::Read(DerivativeSlot::Gradient)),
        HESSIAN_METHOD => Some(GradSlotMethod::Read(DerivativeSlot::Hessian)),
        ZERO_GRAD_METHOD => Some(GradSlotMethod::Clear),
        SET_GRAD_METHOD => Some(GradSlotMethod::Fill(DerivativeSlot::Gradient)),
        SET_HESSIAN_METHOD => Some(GradSlotMethod::Fill(DerivativeSlot::Hessian)),
        _ => None,
    }
}

/// The panic an empty slot's read raises.
fn empty_slot_message(slot: DerivativeSlot) -> &'static str {
    match slot {
        DerivativeSlot::Gradient => "`.grad()` read an empty gradient slot: no `.backward()` has filled it since the tensor was built or last `.zero_grad()`",
        DerivativeSlot::Hessian => "`.hessian()` read an empty Hessian slot: no `.backward()` of a `@grad(order: 2)` function has filled it since the tensor was built, last `.zero_grad()`, or last took a first-order gradient",
    }
}

impl<'ctx> CodegenContext<'ctx> {
    /// Lower one derivative-slot operation. A read yields a value; the others are unit.
    pub(crate) fn codegen_grad_slot_method(
        &mut self,
        method: GradSlotMethod,
        receiver: &HirExpr,
        args: &[HirExpr],
    ) -> CodegenResult<Option<BasicValueEnum<'ctx>>> {
        let handle = self.tensor_receiver_handle(receiver, &Type::from_hir(&receiver.ty))?;
        match method {
            GradSlotMethod::Read(slot) => {
                let cell = self.dlpack_derivative_slot(handle, slot)?;
                let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
                let label = slot.label();
                let held = self
                    .builder
                    .build_load(ptr_type, cell, label)?
                    .into_pointer_value();
                let filled = self
                    .builder
                    .build_is_not_null(held, &format!("{label}.filled"))?;
                self.codegen_guard_or_panic(filled, empty_slot_message(slot), receiver.span.start)?;
                // A `&Tensor` is the address of a cell holding the handle, and the slot is
                // such a cell, so the borrow costs nothing and aliases no copy.
                Ok(Some(cell.into()))
            }
            GradSlotMethod::Clear => {
                self.release_derivatives(handle)?;
                Ok(None)
            }
            GradSlotMethod::Fill(slot) => {
                let [derivative] = args else {
                    return Err(CodegenError::InternalError(
                        "a derivative fill takes exactly the derivative".to_string(),
                    ));
                };
                let BasicValueEnum::PointerValue(value) = self.codegen_expr(derivative)? else {
                    return Err(CodegenError::InternalError(
                        "a derivative does not lower to a tensor handle".to_string(),
                    ));
                };
                self.mark_moved_for_drop(derivative);
                // A second `.backward()` replaces the derivative, so the old one is released
                // rather than leaked.
                match slot {
                    DerivativeSlot::Gradient => self.release_derivatives(handle)?,
                    DerivativeSlot::Hessian => self.release_derivative_slot(handle, slot)?,
                }
                let cell = self.dlpack_derivative_slot(handle, slot)?;
                self.builder.build_store(cell, value)?;
                Ok(None)
            }
        }
    }
}
