// Generated `drop_elems(header)` helper for both map shapes: release the `string`
// buffers the live slots own.
//
// One of the map-codegen modules under `maps`; each adds methods to the same
// `impl CodegenContext` block.

use inkwell::values::PointerValue;
use inkwell::IntPredicate;

use super::{SlotField, STATE_FULL};
use crate::codegen::collections::elements::element_release_helper_name;
use crate::codegen::collections::{collection_arg, FIELD_CAP, FIELD_LEN};
use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::{CollectionKind, Type};

impl<'ctx> CodegenContext<'ctx> {
    /// Call the instantiation's slot-release helper, building it on first use.
    pub(in crate::codegen::collections) fn emit_map_string_release(
        &mut self,
        kind: CollectionKind,
        header: PointerValue<'ctx>,
        args: &[Type],
    ) -> CodegenResult<()> {
        let key_ty = collection_arg(args, 0)?;
        let value_ty = collection_arg(args, 1)?;
        let name = element_release_helper_name(kind, args);
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = self.context.void_type().fn_type(&[ptr_ty.into()], false);

        let helper = self.get_or_build_helper(&name, fn_type, move |ctx, func| {
            ctx.emit_map_release_body(func, kind, &key_ty, &value_ty)
        })?;
        self.builder
            .build_call(helper, &[header.into()], "")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Body of the slot-release helper: walk the slots this shape can hold live entries
    /// in and release each one's `string` fields.
    fn emit_map_release_body(
        &mut self,
        func: inkwell::values::FunctionValue<'ctx>,
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
            .ok_or_else(|| CodegenError::InternalError("map release arity".into()))?
            .into_pointer_value();

        // A hashed table scatters its live slots across the whole buffer; the ordered
        // one keeps them in a dense prefix, exactly as `keys()` walks them.
        let limit = match kind {
            CollectionKind::HashMap => self.load_header_field(header, FIELD_CAP, "cap")?,
            _ => self.load_header_field(header, FIELD_LEN, "len")?,
        };
        let cursor = self.entry_alloca(i64_ty, "rel.slot")?;
        self.builder
            .build_store(cursor, i64_ty.const_zero())
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(cond_bb);
        let slot = self
            .builder
            .build_load(i64_ty, cursor, "rel.slot.val")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let more = self
            .builder
            .build_int_compare(IntPredicate::ULT, slot, limit, "rel.more")
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
                        "rel.live",
                    )
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            }
            _ => self.context.bool_type().const_int(1, false),
        };
        self.builder
            .build_conditional_branch(live, take_bb, step_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(take_bb);
        self.release_slot_strings(kind, header, key_ty, value_ty, slot)?;
        self.builder
            .build_unconditional_branch(step_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(step_bb);
        let next = self
            .builder
            .build_int_add(slot, i64_ty.const_int(1, false), "rel.next")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(cursor, next)
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

    /// Release whichever of a live slot's key and value is a `string`. Shared with
    /// `remove`, which gives up one slot rather than all of them.
    pub(super) fn release_slot_strings(
        &mut self,
        kind: CollectionKind,
        header: PointerValue<'ctx>,
        key_ty: &Type,
        value_ty: &Type,
        slot: inkwell::values::IntValue<'ctx>,
    ) -> CodegenResult<()> {
        for (field_ty, field) in [(key_ty, SlotField::Key), (value_ty, SlotField::Value)] {
            if !matches!(field_ty, Type::String) {
                continue;
            }
            let field_ptr = self.map_slot_field_ptr(kind, header, key_ty, value_ty, slot, field)?;
            self.release_slot_string(field_ptr)?;
        }
        Ok(())
    }
}
