// Ownership of the values a collection's slots hold.
//
// A slot owns what it holds, and every `string` position inside it owns its bytes: that is
// the invariant the release walk relies on, since a slot has no flag of its own to say
// otherwise. An insertion moves its operand in, and the store is where the invariant is
// made: a `string` position the operand owned is adopted, and one it did not (a `.rodata`
// literal, a borrowed view) is copied, exactly as a store through a borrow does. A read
// never copies: indexing names the slot in place, and the only reads that hand a value
// out (`pop`, `remove`) give the slot up with it.

use inkwell::IntPredicate;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use neuro_hir::{HirExpr, HirExprKind};

use super::{FIELD_CAP, FIELD_LEN, collection_arg};
use crate::codegen::context::{CodegenContext, DropTarget};
use crate::errors::{CodegenError, CodegenResult};
use crate::types::{CollectionKind, Type};

impl<'ctx> CodegenContext<'ctx> {
    /// The bytes of a `string` value in a buffer of their own.
    ///
    /// `malloc(0)` may hand back null, which would make an empty copy indistinguishable
    /// from a failed allocation, so an empty string still takes a byte, the same spare
    /// byte `StringBuilder::to_string` reserves.
    pub(crate) fn copy_string_bytes(
        &mut self,
        value: BasicValueEnum<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let i64_ty = self.context.i64_type();
        let fat_ptr = value.into_struct_value();
        let source = self
            .builder
            .build_extract_value(fat_ptr, 0, "str.src")?
            .into_pointer_value();
        let len = self
            .builder
            .build_extract_value(fat_ptr, 1, "str.src.len")?
            .into_int_value();

        let empty = self.builder.build_int_compare(
            IntPredicate::EQ,
            len,
            i64_ty.const_zero(),
            "str.dup.empty",
        )?;
        let bytes = self
            .builder
            .build_select(empty, i64_ty.const_int(1, false), len, "str.dup.size")?
            .into_int_value();
        let copy = self.build_malloc(bytes, "str.dup")?;
        self.build_memcpy_call(copy, source, len)?;
        self.build_string_value(copy, len)
    }

    /// The value to write into a slot of type `slot_ty`, given `value`, the evaluated
    /// `expr`, and move it out of whatever `expr` names.
    ///
    /// Every `string` position of the result owns its bytes: the ones `expr` owned are
    /// handed over, the rest copied. The owners are read before the move clears them.
    pub(crate) fn value_for_collection_slot(
        &mut self,
        expr: &HirExpr,
        value: BasicValueEnum<'ctx>,
        slot_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        if !self.slot_holds_string(slot_ty) {
            self.mark_moved_for_drop(expr)?;
            return Ok(value);
        }
        let owners = self.stored_string_owners(expr, slot_ty)?;
        let staged = self.entry_alloca(value.get_type(), "slot.value")?;
        self.builder.build_store(staged, value)?;
        self.own_strings_stored_through_borrow(staged, slot_ty, &owners)?;
        self.mark_moved_for_drop(expr)?;
        self.builder
            .build_load(value.get_type(), staged, "slot.owned")
            .map_err(CodegenError::from)
    }

    /// A borrow of the slot at `slot_ptr` holding a `slot_ty`, as `&T` / `&mut T`
    /// values are represented: the slot's address, except for a shared `&string`, which
    /// is the fat pointer itself.
    pub(crate) fn slot_borrow(
        &mut self,
        slot_ptr: PointerValue<'ctx>,
        slot_ty: &Type,
        mutable: bool,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        if !mutable && matches!(slot_ty, Type::String) {
            let string_llvm = self.collection_value_type(slot_ty)?;
            return self
                .builder
                .build_load(string_llvm, slot_ptr, "slot.str.view")
                .map_err(CodegenError::from);
        }
        Ok(slot_ptr.into())
    }

    /// Whether a slot of `ty` owns anything its release must give back: a typed owner, or
    /// a `string` position, which a slot always owns.
    pub(crate) fn slot_needs_release(&self, ty: &Type) -> bool {
        self.holds_owner(ty) || self.slot_holds_string(ty)
    }

    /// Whether a value of `ty` is a `string` or has one at a fixed position inside it.
    fn slot_holds_string(&self, ty: &Type) -> bool {
        matches!(ty, Type::String) || !self.string_position_paths(ty).is_empty()
    }

    /// Destroy the value a slot holds at `slot_ptr`: every typed owner inside it, and
    /// every `string` position, which the slot invariant says owns its bytes. An enum
    /// payload's `string` is not reached, as no position of an enum is.
    pub(crate) fn drop_slot_value(
        &mut self,
        slot_ptr: PointerValue<'ctx>,
        ty: &Type,
    ) -> CodegenResult<()> {
        if self.holds_owner(ty) {
            self.emit_value_destructor(slot_ptr, ty)?;
        }
        for position_ptr in self.string_positions_of(slot_ptr, ty)? {
            self.release_slot_string(position_ptr)?;
        }
        Ok(())
    }

    /// Register `name`, whose storage `ptr` holds a `ty` value just taken out of a slot,
    /// as that value's owner in the innermost scope. The slot owned every `string`
    /// position, so the binding does too, which a binding's own planning cannot know.
    pub(crate) fn register_slot_owner(
        &mut self,
        name: &str,
        ptr: PointerValue<'ctx>,
        ty: &Type,
    ) -> CodegenResult<()> {
        if matches!(ty, Type::String) {
            self.register_local_drop(name, ptr, DropTarget::HeapString)?;
            return Ok(());
        }
        self.register_owned_binding(name, ptr, ty)?;
        let flags: Vec<PointerValue<'ctx>> = self
            .drop_scopes
            .last()
            .and_then(|scope| scope.last())
            .filter(|entry| entry.name == name)
            .map(|entry| {
                entry
                    .held
                    .iter()
                    .filter(|held| matches!(held.target, DropTarget::HeapString))
                    .map(|held| held.flag_ptr)
                    .collect()
            })
            .unwrap_or_default();
        let armed = self.context.bool_type().const_int(1, false);
        for flag_ptr in flags {
            self.builder.build_store(flag_ptr, armed)?;
        }
        Ok(())
    }

    /// Destroy `value`, the result of the statement `expr`, when it is a value a
    /// collection gave up (`pop`, `remove`, `swap_remove`, the value `insert` displaced)
    /// and nothing binds it. A bare `string` is the consumer's: the statement already
    /// released it as the owned string it is. A value the unbound-temporary path destroys
    /// (a user destructor, a function value) has only its `string` positions left here.
    pub(crate) fn drop_discarded_slot_value(
        &mut self,
        expr: &HirExpr,
        value: BasicValueEnum<'ctx>,
    ) -> CodegenResult<()> {
        let Some(element_ty) = taken_slot_type(expr) else {
            return Ok(());
        };
        if self.pool_depth > 0 || !self.slot_needs_release(&element_ty) {
            return Ok(());
        }
        let result_ty = Type::from_hir(&expr.ty);
        if result_ty == element_ty && matches!(element_ty, Type::String) {
            return Ok(());
        }
        let strings_only = self.temporary_drops_its_owners(&result_ty);
        let staged = self.entry_alloca(value.get_type(), "taken.discarded")?;
        self.builder.build_store(staged, value)?;
        let option = match &result_ty {
            Type::Enum(option) if result_ty != element_ty => option.clone(),
            _ => return self.release_taken_value(staged, &element_ty, strings_only),
        };
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;
        let some_tag = self.enum_variant_tag(&option, "Some")?;
        let enum_llvm = self.type_mapper.enum_struct_type(&option)?;
        let tag_ptr = self
            .builder
            .build_struct_gep(enum_llvm, staged, 0, "taken.tag.ptr")?;
        let tag = self
            .builder
            .build_load(self.context.i32_type(), tag_ptr, "taken.tag")?
            .into_int_value();
        let is_some = self.builder.build_int_compare(
            IntPredicate::EQ,
            tag,
            self.context
                .i32_type()
                .const_int(u64::from(some_tag), false),
            "taken.some",
        )?;
        let drop_bb = self.context.append_basic_block(parent_fn, "taken.drop");
        let done_bb = self.context.append_basic_block(parent_fn, "taken.done");
        self.builder
            .build_conditional_branch(is_some, drop_bb, done_bb)?;
        self.builder.position_at_end(drop_bb);
        let payload_array_ty = enum_llvm
            .get_field_type_at_index(1)
            .ok_or_else(|| {
                CodegenError::InternalError(format!("enum '{}' has no payload field", option))
            })?
            .into_array_type();
        let payload_ptr = self
            .builder
            .build_struct_gep(enum_llvm, staged, 1, "taken.payload")?;
        let slot_ptr = self.aggregate_position_ptr(
            payload_array_ty.into(),
            payload_ptr,
            0,
            "taken.payload.slot",
        )?;
        self.release_taken_value(slot_ptr, &element_ty, strings_only)?;
        self.builder.build_unconditional_branch(done_bb)?;
        self.builder.position_at_end(done_bb);
        Ok(())
    }

    /// Destroy a value taken out of a slot at `ptr`, or with `strings_only` just release
    /// its `string` positions, when another path destroys its typed owners.
    fn release_taken_value(
        &mut self,
        ptr: PointerValue<'ctx>,
        ty: &Type,
        strings_only: bool,
    ) -> CodegenResult<()> {
        if !strings_only {
            return self.drop_slot_value(ptr, ty);
        }
        for position_ptr in self.string_positions_of(ptr, ty)? {
            self.release_slot_string(position_ptr)?;
        }
        Ok(())
    }

    /// Release the buffer behind the `string` fat pointer stored at `slot_ptr`.
    pub(super) fn release_slot_string(
        &mut self,
        slot_ptr: PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        let fat_ptr_ty = self.string_fat_ptr_type();
        let buffer_ptr = self
            .builder
            .build_struct_gep(fat_ptr_ty, slot_ptr, 0, "slot.str.addr")?;
        let buffer = self.builder.build_load(
            self.context.ptr_type(inkwell::AddressSpace::default()),
            buffer_ptr,
            "slot.str.buf",
        )?;
        let release = self.release_fn()?;
        self.builder.build_call(release, &[buffer.into()], "")?;
        Ok(())
    }

    /// Release what every live slot of a collection of type `ty` holds. Emitted before
    /// the buffer itself is freed, and again by `clear()`, which gives up the same slots
    /// while keeping the buffer.
    ///
    /// A collection whose slots own nothing emits nothing at all, so a `Vec<i32>` costs
    /// exactly what it did before.
    pub(crate) fn emit_collection_slot_release(
        &mut self,
        header: PointerValue<'ctx>,
        ty: &Type,
    ) -> CodegenResult<()> {
        let Type::Collection { kind, args } = ty.referent() else {
            return Ok(());
        };
        let (kind, args) = (*kind, args.clone());
        // A `StringBuilder`'s buffer is a byte run, so it has no slot that owns anything.
        if kind == CollectionKind::StringBuilder
            || !args.iter().any(|arg| self.slot_needs_release(arg))
        {
            return Ok(());
        }
        match kind {
            CollectionKind::Vec => {
                let start = self.context.i64_type().const_zero();
                self.emit_vec_slot_release(header, &args, start)
            }
            _ => self.emit_map_slot_release(kind, header, &args),
        }
    }

    /// Release the slots of a `Vec` from `start` to its length, through the
    /// instantiation's release helper, building it on first use.
    pub(super) fn emit_vec_slot_release(
        &mut self,
        header: PointerValue<'ctx>,
        args: &[Type],
        start: IntValue<'ctx>,
    ) -> CodegenResult<()> {
        let element_ty = collection_arg(args, 0)?;
        let name = element_release_helper_name(CollectionKind::Vec, args);
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        let i64_ty = self.context.i64_type();
        let fn_type = self
            .context
            .void_type()
            .fn_type(&[ptr_ty.into(), i64_ty.into()], false);

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
            let start = func
                .get_nth_param(1)
                .ok_or_else(|| CodegenError::InternalError("element release arity".into()))?
                .into_int_value();
            let cursor = ctx.entry_alloca(i64_ty, "rel.i")?;
            ctx.builder.build_store(cursor, start)?;
            // The live prefix is what the collection owns; a slot past `len` was given
            // up by a `pop` or a `remove` that took its value with it.
            let len = ctx.load_header_field(header, FIELD_LEN, "rel.len")?;
            ctx.builder.build_unconditional_branch(cond_bb)?;

            ctx.builder.position_at_end(cond_bb);
            let index = ctx
                .builder
                .build_load(i64_ty, cursor, "rel.iv")?
                .into_int_value();
            let more = ctx
                .builder
                .build_int_compare(IntPredicate::ULT, index, len, "rel.more")?;
            ctx.builder
                .build_conditional_branch(more, body_bb, exit_bb)?;

            ctx.builder.position_at_end(body_bb);
            let slot = ctx.vec_slot_ptr(header, &element_ty, index)?;
            ctx.drop_slot_value(slot, &element_ty)?;
            let next = ctx
                .builder
                .build_int_add(index, i64_ty.const_int(1, false), "rel.next")?;
            ctx.builder.build_store(cursor, next)?;
            ctx.builder.build_unconditional_branch(cond_bb)?;

            ctx.builder.position_at_end(exit_bb);
            ctx.builder.build_return(None)?;
            Ok(())
        })?;
        self.builder
            .build_call(helper, &[header.into(), start.into()], "")?;
        Ok(())
    }

    /// Build a `Vec` header over a buffer allocated for `capacity` elements of
    /// `element_ty`, for `Vec::with_capacity(n)`.
    pub(super) fn vec_header_with_capacity(
        &mut self,
        element_ty: &Type,
        capacity: IntValue<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let elem_llvm = self.collection_value_type(element_ty)?;
        let stride = self.size_of_type(elem_llvm)?;
        let bytes = self
            .builder
            .build_int_mul(capacity, stride, "vec.cap.bytes")?;
        // The buffer grows through libc `realloc`, so it starts out as libc's too, never
        // an arena's. Whatever a zero-byte request returns, null or not, `free` accepts.
        let realloc = self.get_or_declare_realloc();
        let null = self
            .context
            .ptr_type(inkwell::AddressSpace::default())
            .const_null();
        let buffer = self
            .builder
            .build_call(realloc, &[null.into(), bytes.into()], "vec.cap.buf")?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("realloc returned void".into()))?
            .into_pointer_value();
        let header_ty = self.collection_header_type();
        let mut header = header_ty.const_zero();
        header = self
            .builder
            .build_insert_value(header, buffer, super::FIELD_BUFFER, "vec.cap.hdr")?
            .into_struct_value();
        header = self
            .builder
            .build_insert_value(header, capacity, FIELD_CAP, "vec.cap.hdr")?
            .into_struct_value();
        Ok(header.into())
    }
}

/// The element or value type of the collection `expr` takes a value out of, when `expr` is
/// a call to one of the methods that give a slot's value up to their caller.
fn taken_slot_type(expr: &HirExpr) -> Option<Type> {
    let HirExprKind::Call { callee, .. } = &expr.kind else {
        return None;
    };
    let HirExprKind::FieldAccess { object, field } = &callee.kind else {
        return None;
    };
    if !SLOT_TAKERS.contains(&field.as_str()) {
        return None;
    }
    match Type::from_hir(&object.ty).referent() {
        Type::Collection { kind, args } if *kind != CollectionKind::StringBuilder => {
            args.last().cloned()
        }
        _ => None,
    }
}

/// The collection methods that hand a slot's value to their caller, moving it out.
pub(crate) const SLOT_TAKERS: [&str; 4] = ["pop", "remove", "swap_remove", "insert"];

/// The name of the per-instantiation slot-release helper.
pub(in crate::codegen::collections) fn element_release_helper_name(
    kind: CollectionKind,
    args: &[Type],
) -> String {
    let mangled: Vec<String> = args.iter().map(Type::mangle).collect();
    format!("__neuro_{}_drop_elems_{}", kind.tag(), mangled.join("_"))
}
