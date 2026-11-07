// The derivative slots' operations: `.grad()`, `.hessian()`, `.zero_grad()`, `.detach()`, and
// the moves a `.backward()` lowers to.
//
// `.backward()` itself never reaches the backend. The lowering turns the `@grad` call it
// pairs with into a call of the derivative, `__f__rev`, and the `.backward()` statement into
// one `__set_grad` per differentiated argument, each moving that argument's gradient out of
// the returned bundle, followed under `order: 2` by a `__set_hessian`. So the backend's share
// of the materialization layer is the slots and nothing else.

use inkwell::module::Linkage;
use inkwell::values::{BasicValueEnum, FunctionValue};
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
        DerivativeSlot::Gradient => {
            "`.grad()` read an empty gradient slot: no `.backward()` has filled it since the tensor was built or last `.zero_grad()`"
        }
        DerivativeSlot::Hessian => {
            "`.hessian()` read an empty Hessian slot: no `.backward()` of a `@grad(order: 2)` function has filled it since the tensor was built, last `.zero_grad()`, or last took a first-order gradient"
        }
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
                self.mark_moved_for_drop(derivative)?;
                let home = self.derivative_home_fn(&Type::from_hir(&derivative.ty));
                self.builder
                    .build_call(home, &[handle.into(), value.into()], "")?;
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

    /// The private `void (param, derivative)` that moves a derivative of `derivative_ty` to
    /// its parameter's device. The reverse of a reduction builds a host tensor whatever the
    /// parameter's device, so the rule's last operation cannot decide where it lands.
    fn derivative_home_fn(&mut self, derivative_ty: &Type) -> FunctionValue<'ctx> {
        if let Some((function, _)) = self
            .derivative_homes
            .iter()
            .find(|(_, ty)| ty == derivative_ty)
        {
            return *function;
        }
        let ptr = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = self
            .context
            .void_type()
            .fn_type(&[ptr.into(), ptr.into()], false);
        let name = format!("__neuro_derivative_home_{}", self.derivative_homes.len());
        let function = self
            .module
            .add_function(&name, fn_type, Some(Linkage::Private));
        self.derivative_homes
            .push((function, derivative_ty.clone()));
        function
    }

    /// Give each [`Self::derivative_home_fn`] its body. Run after every function body,
    /// since only then is it known whether a tensor can leave the host: a program with no
    /// GPU runtime moves nothing, and an empty body is all it pays.
    pub(crate) fn define_derivative_homes(&mut self) -> CodegenResult<()> {
        let moves_tensors = self.requires_gpu_runtime() && !cfg!(target_os = "windows");
        for (function, derivative_ty) in self.derivative_homes.clone() {
            let entry = self.context.append_basic_block(function, "entry");
            self.builder.position_at_end(entry);
            self.current_function = Some(function);
            if moves_tensors {
                let handle_of = |index| {
                    function
                        .get_nth_param(index)
                        .map(BasicValueEnum::into_pointer_value)
                        .ok_or_else(|| {
                            CodegenError::InternalError("derivative home lost a handle".to_string())
                        })
                };
                let (param, derivative) = (handle_of(0)?, handle_of(1)?);
                let to_host = self.dlpack_on_host(param)?;
                let index = self.dlpack_device_index(param)?;
                self.transfer_tensor(derivative, &derivative_ty, to_host, index)?;
            }
            self.builder.build_return(None)?;
        }
        self.current_function = None;
        Ok(())
    }

    /// `.detach()`: the receiver's handle, consumed and handed on with empty derivative
    /// slots, as an order-preserving shape cast hands it on. The gradient fence itself is
    /// the derivative transform's; at run time a detach only forgets the derivatives.
    pub(crate) fn codegen_tensor_detach(
        &mut self,
        receiver: &HirExpr,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let BasicValueEnum::PointerValue(handle) = self.codegen_expr(receiver)? else {
            return Err(CodegenError::InternalError(
                "a detached receiver does not lower to a tensor handle".to_string(),
            ));
        };
        self.mark_moved_for_drop(receiver)?;
        self.release_derivatives(handle)?;
        Ok(handle.into())
    }
}
