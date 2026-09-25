// The gradient slot's three operations: `.grad()`, `.zero_grad()`, and the move a
// `.backward()` lowers to.
//
// `.backward()` itself never reaches the backend. The lowering turns the `@grad` call it
// pairs with into a call of the derivative, `__f__rev`, and the `.backward()` statement into
// one `__set_grad` per differentiated argument, each moving that argument's gradient out of
// the returned bundle. So the backend's share of the materialization layer is the slot and
// nothing else.

use inkwell::values::BasicValueEnum;
use neuro_hir::HirExpr;

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// The private method a `.backward()` statement lowers to, one per differentiated argument.
/// The language reserves `__` in every declared name, so no program can call it.
const SET_GRAD_METHOD: &str = "__set_grad";

const GRAD_METHOD: &str = "grad";
const ZERO_GRAD_METHOD: &str = "zero_grad";

/// One operation on a tensor's gradient slot.
pub(crate) enum GradSlotMethod {
    /// `.grad()`: a borrow of the gradient, or a panic when the slot is empty.
    Read,
    /// `.zero_grad()`: release the gradient and empty the slot.
    Clear,
    /// `__set_grad(g)`: move `g` into the slot, releasing what it held.
    Fill,
}

/// The slot operation `method` names on a tensor receiver, owned or borrowed.
pub(crate) fn resolve_grad_slot_method(recv: &Type, method: &str) -> Option<GradSlotMethod> {
    if !matches!(recv.referent(), Type::Tensor { .. }) {
        return None;
    }
    match method {
        GRAD_METHOD => Some(GradSlotMethod::Read),
        ZERO_GRAD_METHOD => Some(GradSlotMethod::Clear),
        SET_GRAD_METHOD => Some(GradSlotMethod::Fill),
        _ => None,
    }
}

impl<'ctx> CodegenContext<'ctx> {
    /// Lower one gradient-slot operation. `.grad()` yields a value; the other two are unit.
    pub(crate) fn codegen_grad_slot_method(
        &mut self,
        method: GradSlotMethod,
        receiver: &HirExpr,
        args: &[HirExpr],
    ) -> CodegenResult<Option<BasicValueEnum<'ctx>>> {
        let handle = self.tensor_receiver_handle(receiver, &Type::from_hir(&receiver.ty))?;
        match method {
            GradSlotMethod::Read => {
                let slot = self.dlpack_grad_slot(handle)?;
                let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
                let grad = self
                    .builder
                    .build_load(ptr_type, slot, "dlpack.grad")?
                    .into_pointer_value();
                let filled = self.builder.build_is_not_null(grad, "dlpack.grad.filled")?;
                self.codegen_guard_or_panic(
                    filled,
                    "`.grad()` read an empty gradient slot: no `.backward()` has filled it since the tensor was built or last `.zero_grad()`",
                    receiver.span.start,
                )?;
                // A `&Tensor` is the address of a cell holding the handle, and the slot is
                // such a cell, so the borrow costs nothing and aliases no copy.
                Ok(Some(slot.into()))
            }
            GradSlotMethod::Clear => {
                self.release_grad_slot(handle)?;
                Ok(None)
            }
            GradSlotMethod::Fill => {
                let [gradient] = args else {
                    return Err(CodegenError::InternalError(
                        "a gradient fill takes exactly the gradient".to_string(),
                    ));
                };
                let BasicValueEnum::PointerValue(value) = self.codegen_expr(gradient)? else {
                    return Err(CodegenError::InternalError(
                        "a gradient does not lower to a tensor handle".to_string(),
                    ));
                };
                self.mark_moved_for_drop(gradient);
                // A second `.backward()` replaces the gradient, so the old one is released
                // rather than leaked.
                self.release_grad_slot(handle)?;
                let slot = self.dlpack_grad_slot(handle)?;
                self.builder.build_store(slot, value)?;
                Ok(None)
            }
        }
    }
}
