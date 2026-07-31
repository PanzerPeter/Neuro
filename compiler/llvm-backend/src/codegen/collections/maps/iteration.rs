// Generated `keys(header) -> Vec<K>` helper: a snapshot of the live keys in order.
//
// One of the map-codegen modules under `maps`; each adds methods to the same
// `impl CodegenContext` block.

use inkwell::values::FunctionValue;
use inkwell::IntPredicate;

use super::STATE_FULL;
use crate::codegen::collections::{FIELD_CAP, FIELD_LEN};
use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::{CollectionKind, Type};

impl<'ctx> CodegenContext<'ctx> {
    /// Emit `keys(header, out_buffer)`: copy every live key into `out_buffer`, in slot
    /// order.
    pub(super) fn build_map_keys_helper(
        &mut self,
        kind: CollectionKind,
        key_ty: &Type,
        value_ty: &Type,
    ) -> CodegenResult<FunctionValue<'ctx>> {
        let name = self.map_helper_name(kind, "keys", key_ty, value_ty);
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = self
            .context
            .void_type()
            .fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let key_ty = key_ty.clone();
        let value_ty = value_ty.clone();

        self.get_or_build_helper(&name, fn_type, move |ctx, func| {
            ctx.emit_map_keys_body(func, kind, &key_ty, &value_ty)
        })
    }

    /// Body of the `keys` helper.
    pub(super) fn emit_map_keys_body(
        &mut self,
        func: FunctionValue<'ctx>,
        kind: CollectionKind,
        key_ty: &Type,
        value_ty: &Type,
    ) -> CodegenResult<()> {
        let i64_ty = self.context.i64_type();
        let entry = self.context.append_basic_block(func, "entry");
        let cond_bb = self.context.append_basic_block(func, "cond");
        let body_bb = self.context.append_basic_block(func, "body");
        let take_bb = self.context.append_basic_block(func, "take");
        let step_bb = self.context.append_basic_block(func, "step");
        let exit_bb = self.context.append_basic_block(func, "exit");
        self.builder.position_at_end(entry);

        let header = func
            .get_nth_param(0)
            .ok_or_else(|| CodegenError::InternalError("map keys arity".into()))?
            .into_pointer_value();
        let out = func
            .get_nth_param(1)
            .ok_or_else(|| CodegenError::InternalError("map keys arity".into()))?
            .into_pointer_value();

        // A hashed table must scan every slot (live ones are scattered); the ordered
        // one only has to walk its dense prefix.
        let limit = match kind {
            CollectionKind::HashMap => self.load_header_field(header, FIELD_CAP, "cap")?,
            _ => self.load_header_field(header, FIELD_LEN, "len")?,
        };
        let slot_slot = self.entry_alloca(i64_ty, "slot")?;
        let out_slot = self.entry_alloca(i64_ty, "out.i")?;
        for target in [slot_slot, out_slot] {
            self.builder
                .build_store(target, i64_ty.const_zero())
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        }
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(cond_bb);
        let slot = self
            .builder
            .build_load(i64_ty, slot_slot, "slot.val")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let more = self
            .builder
            .build_int_compare(IntPredicate::ULT, slot, limit, "more")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(more, body_bb, exit_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(body_bb);
        let live = match kind {
            CollectionKind::HashMap => {
                let state = self.load_slot_state(header, key_ty, value_ty, slot)?;
                self.builder
                    .build_int_compare(
                        IntPredicate::EQ,
                        state,
                        self.context.i8_type().const_int(STATE_FULL, false),
                        "live",
                    )
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            }
            _ => self.context.bool_type().const_int(1, false),
        };
        self.builder
            .build_conditional_branch(live, take_bb, step_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(take_bb);
        let key = self.load_slot_key(kind, header, key_ty, value_ty, slot)?;
        let out_index = self
            .builder
            .build_load(i64_ty, out_slot, "out.val")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let key_llvm = self.collection_value_type(key_ty)?;
        // SAFETY: `out` was allocated with the map's `used` count and `out_index` is
        // incremented once per live slot taken, so it never reaches that count.
        let dst = unsafe {
            self.builder
                .build_in_bounds_gep(key_llvm, out, &[out_index], "out.slot")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
        };
        self.builder
            .build_store(dst, key)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let out_next = self
            .builder
            .build_int_add(out_index, i64_ty.const_int(1, false), "out.next")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(out_slot, out_next)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unconditional_branch(step_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(step_bb);
        let next = self
            .builder
            .build_int_add(slot, i64_ty.const_int(1, false), "slot.next")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(slot_slot, next)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(exit_bb);
        self.builder
            .build_return(None)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }
}
