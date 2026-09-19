// Ownership of the `string` values a collection stores.
//
// A slot holds a `string` as a plain `{ ptr, len }` fat pointer, so nothing in the slot
// itself records who owns the bytes. What makes it decidable is that the boundary always
// copies: a `string` written into a slot becomes the collection's own buffer, and a
// `string` read out of one becomes the reader's. The collection then releases every live
// slot when it is destroyed, and no value it handed out can be left dangling by that.
//
// The one asymmetry is a slot the collection GIVES UP: `pop`, whose element is gone from
// the buffer by the time the reader sees it. That transfers rather than copies, so the
// buffer is neither duplicated nor released twice.

use inkwell::values::{BasicValueEnum, PointerValue};
use inkwell::IntPredicate;
use neuro_hir::HirExpr;

use super::{collection_arg, FIELD_LEN};
use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::{CollectionKind, Type};

/// How a `string` leaves a collection slot.
#[derive(Clone, Copy)]
pub(crate) enum SlotTransfer {
    /// The slot keeps its buffer, so the reader is handed a copy of the bytes.
    Copied,
    /// The slot is given up by the read itself, so its buffer moves to the reader.
    Moved,
}

impl<'ctx> CodegenContext<'ctx> {
    /// The bytes of a `string` value in a buffer of their own.
    ///
    /// `malloc(0)` may hand back null, which would make an empty copy indistinguishable
    /// from a failed allocation, so an empty string still takes a byte, the same spare
    /// byte `String::to_string` reserves.
    pub(crate) fn copy_string_bytes(
        &mut self,
        value: BasicValueEnum<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let i64_ty = self.context.i64_type();
        let fat_ptr = value.into_struct_value();
        let source = self
            .builder
            .build_extract_value(fat_ptr, 0, "str.src")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let len = self
            .builder
            .build_extract_value(fat_ptr, 1, "str.src.len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();

        let empty = self
            .builder
            .build_int_compare(IntPredicate::EQ, len, i64_ty.const_zero(), "str.dup.empty")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let bytes = self
            .builder
            .build_select(empty, i64_ty.const_int(1, false), len, "str.dup.size")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let copy = self.build_malloc(bytes, "str.dup")?;
        self.build_memcpy_call(copy, source, len)?;
        self.build_string_value(copy, len)
    }

    /// The value to write into a collection slot of type `slot_ty`, evaluated from
    /// `expr`.
    ///
    /// A `string` is copied so the slot owns bytes independent of whatever the operand
    /// names: a `.rodata` literal, a live binding, a borrowed view. The one exception
    /// is an operand [`produces_owned_string`](CodegenContext::produces_owned_string)
    /// proves allocated for this expression and nothing else can reach: the slot adopts
    /// that buffer instead, which is what keeps `v.push("item {i}")` to one allocation.
    pub(crate) fn value_for_collection_slot(
        &mut self,
        expr: &HirExpr,
        value: BasicValueEnum<'ctx>,
        slot_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        if !matches!(slot_ty, Type::String) {
            self.mark_moved_for_drop(expr);
            return Ok(value);
        }
        if self.produces_owned_string(expr) {
            return Ok(value);
        }
        self.copy_string_bytes(value)
    }

    /// The value an element read hands back.
    pub(crate) fn value_from_collection_slot(
        &mut self,
        value: BasicValueEnum<'ctx>,
        slot_ty: &Type,
        transfer: SlotTransfer,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        match transfer {
            SlotTransfer::Moved => Ok(value),
            SlotTransfer::Copied if matches!(slot_ty, Type::String) => {
                self.copy_string_bytes(value)
            }
            SlotTransfer::Copied => Ok(value),
        }
    }

    /// Release the buffer behind the `string` fat pointer stored at `slot_ptr`.
    pub(super) fn release_slot_string(
        &mut self,
        slot_ptr: PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        let fat_ptr_ty = self.string_fat_ptr_type();
        let buffer_ptr = self
            .builder
            .build_struct_gep(fat_ptr_ty, slot_ptr, 0, "slot.str.addr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let buffer = self
            .builder
            .build_load(
                self.context.ptr_type(inkwell::AddressSpace::default()),
                buffer_ptr,
                "slot.str.buf",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let release = self.release_fn()?;
        self.builder
            .build_call(release, &[buffer.into()], "")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Release every `string` a collection of type `ty` still holds. Emitted before the
    /// buffer itself is freed, and again by `clear()`, which gives up the same slots
    /// while keeping the buffer.
    ///
    /// A collection with no `string` slot emits nothing at all, so a `Vec<i32>` costs
    /// exactly what it did before.
    pub(crate) fn emit_collection_string_release(
        &mut self,
        header: PointerValue<'ctx>,
        ty: &Type,
    ) -> CodegenResult<()> {
        let Type::Collection { kind, args } = ty.referent() else {
            return Ok(());
        };
        let (kind, args) = (*kind, args.clone());
        if !holds_string_slot(kind, &args) {
            return Ok(());
        }
        match kind {
            CollectionKind::Vec => self.emit_vec_string_release(header, &args),
            CollectionKind::HashMap | CollectionKind::BTreeMap => {
                self.emit_map_string_release(kind, header, &args)
            }
            // A `String`'s buffer is a byte run, so it has no slot that owns anything.
            CollectionKind::String => Ok(()),
        }
    }

    /// Call the instantiation's `Vec` element-release helper, building it on first use.
    fn emit_vec_string_release(
        &mut self,
        header: PointerValue<'ctx>,
        args: &[Type],
    ) -> CodegenResult<()> {
        let element_ty = collection_arg(args, 0)?;
        let name = element_release_helper_name(CollectionKind::Vec, args);
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = self.context.void_type().fn_type(&[ptr_ty.into()], false);

        let helper = self.get_or_build_helper(&name, fn_type, move |ctx, func| {
            let entry = ctx.context.append_basic_block(func, "entry");
            let cond_bb = ctx.context.append_basic_block(func, "cond");
            let body_bb = ctx.context.append_basic_block(func, "body");
            let exit_bb = ctx.context.append_basic_block(func, "exit");
            ctx.builder.position_at_end(entry);

            let header = func
                .get_nth_param(0)
                .ok_or_else(|| CodegenError::InternalError("element release arity".into()))?
                .into_pointer_value();
            let i64_ty = ctx.context.i64_type();
            let cursor = ctx.entry_alloca(i64_ty, "rel.i")?;
            ctx.builder
                .build_store(cursor, i64_ty.const_zero())
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            // The live prefix is what the collection owns; a slot past `len` was given
            // up by a `pop` that took its buffer with it.
            let len = ctx.load_header_field(header, FIELD_LEN, "rel.len")?;
            ctx.builder
                .build_unconditional_branch(cond_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            ctx.builder.position_at_end(cond_bb);
            let index = ctx
                .builder
                .build_load(i64_ty, cursor, "rel.iv")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_int_value();
            let more = ctx
                .builder
                .build_int_compare(IntPredicate::ULT, index, len, "rel.more")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.builder
                .build_conditional_branch(more, body_bb, exit_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            ctx.builder.position_at_end(body_bb);
            let slot = ctx.vec_slot_ptr(header, &element_ty, index)?;
            ctx.release_slot_string(slot)?;
            let next = ctx
                .builder
                .build_int_add(index, i64_ty.const_int(1, false), "rel.next")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.builder
                .build_store(cursor, next)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.builder
                .build_unconditional_branch(cond_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            ctx.builder.position_at_end(exit_bb);
            ctx.builder
                .build_return(None)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            Ok(())
        })?;
        self.builder
            .build_call(helper, &[header.into()], "")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }
}

/// Whether any slot of this instantiation holds a `string` the collection owns.
pub(super) fn holds_string_slot(kind: CollectionKind, args: &[Type]) -> bool {
    match kind {
        CollectionKind::String => false,
        _ => args.iter().any(|arg| matches!(arg, Type::String)),
    }
}

/// The name of the per-instantiation element-release helper.
pub(in crate::codegen::collections) fn element_release_helper_name(
    kind: CollectionKind,
    args: &[Type],
) -> String {
    let mangled: Vec<String> = args.iter().map(Type::mangle).collect();
    format!("__neuro_{}_drop_elems_{}", kind.tag(), mangled.join("_"))
}
