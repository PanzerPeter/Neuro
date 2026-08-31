// The `(index, value)` position binding an enumerated `for` head introduces.
//
// Three loop lowerings can carry one — the range loop, the array loop, and the
// `Vec` loop — and each already computes the position it needs: the range loop's
// count of iterations, and the other two's induction variable. So the binding is
// not a second loop, only a named `u64` slot refreshed at the top of each body,
// and this module owns the scope bookkeeping the three would otherwise repeat.
//
// The slot is separate from any induction variable rather than aliasing it. The
// binding is immutable, so nothing in the body can write through it, but a slot
// the loop itself steps is one refactor away from being observably wrong — and
// `mem2reg` folds the extra store away before it reaches a register.

use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{IntValue, PointerValue};

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

use super::CodegenContext;

/// A live position binding: its slot, and the outer bindings its name shadows
/// for the duration of the loop.
pub(crate) struct LoopIndexBinding<'ctx> {
    name: String,
    slot: PointerValue<'ctx>,
    shadowed_var: Option<PointerValue<'ctx>>,
    shadowed_ty: Option<BasicTypeEnum<'ctx>>,
    shadowed_sem_ty: Option<Type>,
}

impl<'ctx> CodegenContext<'ctx> {
    /// Introduce `index` as a `u64` binding for the loop body about to be
    /// emitted. `None` in, `None` out: a plain `for` costs nothing.
    pub(crate) fn bind_loop_index(
        &mut self,
        index: Option<&str>,
    ) -> CodegenResult<Option<LoopIndexBinding<'ctx>>> {
        let Some(index) = index else {
            return Ok(None);
        };
        let i64_ty = self.context.i64_type();
        let slot = self.entry_alloca(i64_ty, index)?;
        let name = index.to_string();
        Ok(Some(LoopIndexBinding {
            shadowed_var: self.variables.insert(name.clone(), slot),
            shadowed_ty: self
                .variable_types
                .insert(name.clone(), i64_ty.as_basic_type_enum()),
            shadowed_sem_ty: self.type_env.insert(name.clone(), Type::U64),
            name,
            slot,
        }))
    }

    /// Publish the current position into the binding's slot. Called at the top of
    /// the loop body, where `position` is in scope and the body has not run yet.
    pub(crate) fn store_loop_index(
        &mut self,
        binding: &Option<LoopIndexBinding<'ctx>>,
        position: IntValue<'ctx>,
    ) -> CodegenResult<()> {
        let Some(binding) = binding else {
            return Ok(());
        };
        self.builder
            .build_store(binding.slot, position)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Drop the binding at the loop's exit block, restoring whatever its name
    /// meant outside the loop.
    pub(crate) fn unbind_loop_index(&mut self, binding: Option<LoopIndexBinding<'ctx>>) {
        let Some(binding) = binding else {
            return;
        };
        restore(&mut self.variables, &binding.name, binding.shadowed_var);
        restore(&mut self.variable_types, &binding.name, binding.shadowed_ty);
        restore(&mut self.type_env, &binding.name, binding.shadowed_sem_ty);
    }
}

/// Put `previous` back under `name`, or remove the entry when the name meant
/// nothing before the loop.
fn restore<V>(table: &mut std::collections::HashMap<String, V>, name: &str, previous: Option<V>) {
    match previous {
        Some(previous) => {
            table.insert(name.to_string(), previous);
        }
        None => {
            table.remove(name);
        }
    }
}
