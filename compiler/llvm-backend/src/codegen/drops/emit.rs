//! Emitting the destruction itself: scope-exit drops under their flags, user
//! destructors, enum payload switches, and the heap and tensor releases.

use inkwell::basic_block::BasicBlock;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};

use crate::codegen::context::{CodegenContext, DropTarget};
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

impl<'ctx> CodegenContext<'ctx> {
    /// Release the heap buffer behind a `string` fat pointer held in `storage_ptr`.
    ///
    /// Emitted only for a binding registered as [`DropTarget::HeapString`], whose
    /// initializer [`produces_owned_string`] proved allocates.
    pub(super) fn emit_heap_string_free(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        let fat_ptr_ty = self.string_fat_ptr_type();
        let buffer =
            self.builder
                .build_struct_gep(fat_ptr_ty, storage_ptr, 0, "str.drop.buf.addr")?;
        let buffer = self.builder.build_load(
            self.context.ptr_type(inkwell::AddressSpace::default()),
            buffer,
            "str.drop.buf",
        )?;
        let free_fn = self.release_fn()?;
        self.builder
            .build_call(free_fn, &[buffer.into()], "")
            .map_err(|e| CodegenError::LlvmError(format!("failed to free string: {}", e)))?;
        Ok(())
    }

    /// Release the tensor a binding in `storage_ptr` owns, through the DLPack `deleter`
    /// its own handle carries.
    ///
    /// The binding's storage holds the handle itself, so it is one load away. Dispatching
    /// through the field rather than calling `free` here is what makes the release a
    /// scope exit performs the same one a foreign owner of the handle performs. Emitted
    /// only for a binding registered as [`DropTarget::TensorBuffer`], whose flag a move
    /// at any of the move sites has already cleared.
    pub(super) fn emit_tensor_buffer_free(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        let handle = self
            .builder
            .build_load(
                self.context.ptr_type(inkwell::AddressSpace::default()),
                storage_ptr,
                "tensor.drop.handle",
            )?
            .into_pointer_value();
        self.build_dlpack_release(handle)
    }

    /// Release whatever the active variant of an enum value holds, by switching on its
    /// tag and destroying that variant's owning payload slots.
    ///
    /// A payload field was written into its slot through memory, bit-exactly, so the
    /// slot's address is the field's address and each destructor reads it in place. The
    /// builder is left on the join block, which is what lets the flag-guarded caller
    /// finish its own branch around this.
    pub(super) fn emit_enum_payload_drop(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
        enum_name: &str,
    ) -> CodegenResult<()> {
        let variants = self
            .type_mapper
            .enum_payload_types(enum_name)
            .cloned()
            .unwrap_or_default();
        let parent_fn = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("enum payload drop emitted outside function".to_string())
        })?;

        let enum_llvm = self.type_mapper.enum_struct_type(enum_name)?;
        let payload_array_ty = enum_llvm
            .get_field_type_at_index(1)
            .ok_or_else(|| {
                CodegenError::InternalError(format!("enum '{}' has no payload field", enum_name))
            })?
            .into_array_type();
        let tag_ptr =
            self.builder
                .build_struct_gep(enum_llvm, storage_ptr, 0, "enum.drop.tag.ptr")?;
        let tag = self
            .builder
            .build_load(self.context.i32_type(), tag_ptr, "enum.drop.tag")?
            .into_int_value();
        let payload_ptr =
            self.builder
                .build_struct_gep(enum_llvm, storage_ptr, 1, "enum.drop.payload")?;

        let join_bb = self.context.append_basic_block(parent_fn, "enum.drop.cont");
        let owning: Vec<usize> = variants
            .iter()
            .enumerate()
            .filter(|(_, fields)| fields.iter().any(|field| self.holds_owner(field)))
            .map(|(tag_value, _)| tag_value)
            .collect();
        let cases: Vec<(IntValue<'ctx>, BasicBlock<'ctx>)> = owning
            .iter()
            .map(|tag_value| {
                (
                    self.context.i32_type().const_int(*tag_value as u64, false),
                    self.context
                        .append_basic_block(parent_fn, &format!("enum.drop.v{}", tag_value)),
                )
            })
            .collect();
        self.builder.build_switch(tag, join_bb, &cases)?;

        for (tag_value, (_, case_bb)) in owning.iter().zip(cases.iter()) {
            self.builder.position_at_end(*case_bb);
            for (slot, field_ty) in variants[*tag_value].iter().enumerate() {
                if !self.holds_owner(field_ty) {
                    continue;
                }
                let slot_ptr = self.aggregate_position_ptr(
                    payload_array_ty.into(),
                    payload_ptr,
                    slot as u32,
                    "enum.drop.slot",
                )?;
                self.emit_value_destructor(slot_ptr, field_ty)?;
            }
            self.builder.build_unconditional_branch(join_bb)?;
        }

        self.builder.position_at_end(join_bb);
        Ok(())
    }

    /// Destroy the value at `storage_ptr`, and everything it holds, unconditionally.
    ///
    /// The flagged path is the one bindings take; this is for a position no flag can
    /// guard, namely an enum payload slot, where the tag the caller switched on has
    /// already established that the value is live.
    pub(super) fn emit_value_destructor(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
        ty: &Type,
    ) -> CodegenResult<()> {
        match self.drop_target_of(ty) {
            Some(DropTarget::UserDrop(struct_name)) => {
                self.emit_user_drop_call(storage_ptr, &struct_name)?
            }
            Some(DropTarget::Collection(collection_ty)) => {
                self.emit_collection_free(storage_ptr, &collection_ty)?
            }
            Some(DropTarget::TensorBuffer) => self.emit_tensor_buffer_free(storage_ptr)?,
            Some(DropTarget::EnumPayload(enum_name)) => {
                self.emit_enum_payload_drop(storage_ptr, &enum_name)?
            }
            _ => {}
        }

        let positions = self.held_positions(ty);
        if positions.is_empty() {
            return Ok(());
        }
        let holder_llvm = self.get_any_llvm_type(ty)?;
        for (index, (segment, position_ty)) in positions.into_iter().enumerate() {
            if !self.holds_owner(&position_ty) {
                continue;
            }
            let position_ptr = self.aggregate_position_ptr(
                holder_llvm,
                storage_ptr,
                index as u32,
                &format!("held.{}.ptr", segment),
            )?;
            self.emit_value_destructor(position_ptr, &position_ty)?;
        }
        Ok(())
    }

    /// Call a type's `impl Drop` destructor against the value at `storage_ptr`, which
    /// is the `&mut self` receiver this backend passes by address.
    pub(super) fn emit_user_drop_call(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
        struct_name: &str,
    ) -> CodegenResult<()> {
        let mangled = format!("{}__drop", struct_name);
        let drop_fn = *self
            .functions
            .get(&mangled)
            .ok_or_else(|| CodegenError::UndefinedFunction(mangled.clone()))?;
        let receiver: BasicValueEnum<'ctx> = storage_ptr.into();
        self.builder
            .build_call(drop_fn, &[receiver.into()], "")
            .map_err(|e| CodegenError::LlvmError(format!("failed to build drop call: {}", e)))?;
        Ok(())
    }

    /// Emit the destructor calls for the innermost scope, in reverse declaration
    /// order, then leave the scope in place (the caller pops it). Used at the normal
    /// fall-through end of a lexical block.
    pub(crate) fn emit_top_scope_drops(&mut self) -> CodegenResult<()> {
        let depth = self.drop_scopes.len();
        if depth == 0 {
            return Ok(());
        }
        self.emit_drops_through(depth - 1)
    }

    /// Emit destructor calls for every open scope from the innermost down to and
    /// including `min_index`, in LIFO order, without popping any scope. Used at
    /// `return` (`min_index = 0`) and at `break`/`continue` (the loop's body scope).
    pub(crate) fn emit_drops_through(&mut self, min_index: usize) -> CodegenResult<()> {
        if min_index >= self.drop_scopes.len() {
            return Ok(());
        }
        // Snapshot the entries first so the destructor calls below can borrow `self`
        // mutably without aliasing the scope stack. Innermost scope first, reverse
        // declaration order within each scope.
        let mut pending: Vec<(PointerValue<'ctx>, PointerValue<'ctx>, DropTarget)> = Vec::new();
        for scope in self.drop_scopes[min_index..].iter().rev() {
            for entry in scope.iter().rev() {
                // A pool-registered value outlives its own scope on purpose: its
                // release is the arena's LIFO sweep at the pool's closing brace, so every
                // such binding in the block is released in one ordered pass.
                if matches!(entry.target, DropTarget::PoolRegistered) {
                    continue;
                }
                pending.extend(Self::snapshot_entry(entry));
            }
        }
        for (storage_ptr, flag_ptr, target) in pending {
            self.emit_one_drop(storage_ptr, flag_ptr, &target)?;
        }
        Ok(())
    }

    /// Emit a single flag-guarded destructor:
    /// `if drop_flag { destroy(&storage); drop_flag = false }`.
    pub(super) fn emit_one_drop(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
        flag_ptr: PointerValue<'ctx>,
        target: &DropTarget,
    ) -> CodegenResult<()> {
        if self.current_block_terminated() {
            return Ok(());
        }
        // A holder that only holds has no release of its own; its held entries carry
        // the work and are emitted beside this call, not through it.
        if matches!(target, DropTarget::Aggregate) {
            return Ok(());
        }
        let parent_fn = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("drop emitted outside function".to_string())
        })?;

        let bool_ty = self.context.bool_type();
        let flag = self
            .builder
            .build_load(bool_ty, flag_ptr, "drop.flag.load")?
            .into_int_value();

        let run_bb = self.context.append_basic_block(parent_fn, "drop.run");
        let cont_bb = self.context.append_basic_block(parent_fn, "drop.cont");
        self.builder
            .build_conditional_branch(flag, run_bb, cont_bb)?;

        self.builder.position_at_end(run_bb);
        match target {
            DropTarget::UserDrop(struct_name) => {
                let struct_name = struct_name.clone();
                self.emit_user_drop_call(storage_ptr, &struct_name)?
            }
            DropTarget::Collection(collection_ty) => {
                let collection_ty = collection_ty.clone();
                self.emit_collection_free(storage_ptr, &collection_ty)?
            }
            DropTarget::HeapString => self.emit_heap_string_free(storage_ptr)?,
            DropTarget::TensorBuffer => self.emit_tensor_buffer_free(storage_ptr)?,
            DropTarget::EnumPayload(enum_name) => {
                let enum_name = enum_name.clone();
                self.emit_enum_payload_drop(storage_ptr, &enum_name)?
            }
            DropTarget::Aggregate => {
                return Err(CodegenError::InternalError(
                    "a holder with no release of its own reached the destructor path".to_string(),
                ));
            }
            DropTarget::PoolRegistered => {
                return Err(CodegenError::InternalError(
                    "a pool-registered value reached the per-scope drop path".to_string(),
                ));
            }
        }
        // Clear the flag so a re-reachable drop site cannot run the destructor twice.
        self.builder.build_store(flag_ptr, bool_ty.const_zero())?;
        self.builder.build_unconditional_branch(cont_bb)?;

        self.builder.position_at_end(cont_bb);
        Ok(())
    }
}
