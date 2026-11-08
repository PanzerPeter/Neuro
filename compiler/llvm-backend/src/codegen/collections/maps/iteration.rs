// `for` over a `HashMap` / `BTreeMap`: a walk of the live slots in slot order, which for
// the ordered map is ascending key order.
//
// One of the map-codegen modules under `maps`; each adds methods to the same
// `impl CodegenContext` block.

use inkwell::IntPredicate;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use neuro_hir::{HirExpr, HirStmt, MapView};

use super::{STATE_FULL, SlotField};
use crate::codegen::collections::{FIELD_CAP, FIELD_LEN, FIELD_USED, collection_arg};
use crate::codegen::context::{CodegenContext, DropTarget, LoopTargets};
use crate::errors::{CodegenError, CodegenResult};
use crate::types::{CollectionKind, Type};

/// The drop-scope name of the table a consuming loop took over. It contains `__`, which
/// no declared name may, so no move can clear its flag.
const CONSUMED_MAP: &str = "__consumed_map";

impl<'ctx> CodegenContext<'ctx> {
    /// `for e in m` / `&m` / `&mut m`, and the `m.keys()` / `m.values()` views.
    ///
    /// A borrowed head binds `(&K, &V)` or `(&K, &mut V)` at each slot's address, a view
    /// one half of that, and a by-value head the entry `(K, V)`; lowering types the
    /// binding by the same rule. A by-value head over entries that own something consumes
    /// them: the loop takes the table from its owner, each pass's binding owns the entry
    /// it holds, and whatever no pass reached is released with the buffer when the loop
    /// is left, by `break` and `return` as much as by running out.
    ///
    /// `index` counts the entries yielded, not the slots walked, which a hashed table
    /// leaves gaps between.
    pub(crate) fn codegen_map_for_each(
        &mut self,
        label: Option<&str>,
        index: Option<&str>,
        iterator: &str,
        iterable: &HirExpr,
        view: Option<MapView>,
        body: &[HirStmt],
    ) -> CodegenResult<()> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;
        let obj_ty = Type::from_hir(&iterable.ty);
        let Type::Collection { kind, args } = obj_ty.referent().clone() else {
            return Err(CodegenError::InternalError(format!(
                "map loop over a non-map type {:?}",
                obj_ty
            )));
        };
        let key_ty = collection_arg(&args, 0)?;
        let value_ty = collection_arg(&args, 1)?;
        let lent = match obj_ty {
            Type::Reference { mutable, .. } => Some(mutable),
            _ => None,
        };
        let owner_header = self.collection_place_ptr(iterable, &obj_ty)?;
        let consumes = view.is_none()
            && lent.is_none()
            && (self.slot_needs_release(&key_ty) || self.slot_needs_release(&value_ty));
        let header = match consumes {
            true => self.take_map_for_loop(owner_header, obj_ty.referent())?,
            false => owner_header,
        };
        let binding_ty = map_binding_type(&key_ty, &value_ty, view, lent);
        let binding_llvm = self.get_any_llvm_type(&binding_ty)?;
        let i64_ty = self.context.i64_type();

        let slot_alloca = self.entry_alloca(i64_ty, "meach.slot")?;
        self.builder.build_store(slot_alloca, i64_ty.const_zero())?;
        let count_alloca = self.entry_alloca(i64_ty, "meach.count")?;
        self.builder
            .build_store(count_alloca, i64_ty.const_zero())?;
        let elem_alloca = self.entry_alloca(binding_llvm, iterator)?;

        self.type_env
            .insert(iterator.to_string(), binding_ty.clone());
        let iter_name = iterator.to_string();
        let previous_var = self.variables.insert(iter_name.clone(), elem_alloca);
        let previous_ty = self.variable_types.insert(iter_name.clone(), binding_llvm);
        let index_binding = self.bind_loop_index(index)?;

        let cond_bb = self.context.append_basic_block(parent_fn, "meach.cond");
        let live_bb = self.context.append_basic_block(parent_fn, "meach.live");
        let body_bb = self.context.append_basic_block(parent_fn, "meach.body");
        let step_bb = self.context.append_basic_block(parent_fn, "meach.step");
        let exit_bb = self.context.append_basic_block(parent_fn, "meach.exit");

        if !self.current_block_terminated() {
            self.builder.build_unconditional_branch(cond_bb)?;
        }

        // A hashed table scatters its live slots across the whole buffer; the ordered one
        // keeps them in a dense prefix.
        self.builder.position_at_end(cond_bb);
        let slot = self
            .builder
            .build_load(i64_ty, slot_alloca, "meach.sv")?
            .into_int_value();
        let limit = match kind {
            CollectionKind::HashMap => self.load_header_field(header, FIELD_CAP, "meach.cap")?,
            _ => self.load_header_field(header, FIELD_LEN, "meach.len")?,
        };
        let more = self
            .builder
            .build_int_compare(IntPredicate::ULT, slot, limit, "meach.more")?;
        self.builder
            .build_conditional_branch(more, live_bb, exit_bb)?;

        self.builder.position_at_end(live_bb);
        let live = match kind {
            CollectionKind::HashMap => {
                let state = self.load_slot_state(header, &key_ty, &value_ty, slot)?;
                self.builder.build_int_compare(
                    IntPredicate::EQ,
                    state,
                    self.context.i8_type().const_int(STATE_FULL, false),
                    "meach.full",
                )?
            }
            _ => self.context.bool_type().const_int(1, false),
        };
        self.builder
            .build_conditional_branch(live, body_bb, step_bb)?;

        self.builder.position_at_end(body_bb);
        let entry = self.map_loop_entry(
            (kind, header, slot),
            (&key_ty, &value_ty),
            view,
            lent,
            binding_llvm,
        )?;
        self.builder.build_store(elem_alloca, entry)?;
        if consumes {
            // The entry now belongs to this pass's binding, so the remainder the loop
            // still owns starts past its slot.
            let taken =
                self.builder
                    .build_int_add(slot, i64_ty.const_int(1, false), "meach.taken")?;
            self.store_header_field(header, FIELD_USED, taken)?;
        }
        let count = self
            .builder
            .build_load(i64_ty, count_alloca, "meach.cv")?
            .into_int_value();
        self.store_loop_index(&index_binding, count)?;
        let counted =
            self.builder
                .build_int_add(count, i64_ty.const_int(1, false), "meach.counted")?;
        self.builder.build_store(count_alloca, counted)?;

        let body_scope_index = self.drop_scopes.len();
        self.push_drop_scope();
        if consumes {
            self.register_slot_owner(iterator, elem_alloca, &binding_ty)?;
        }
        self.loop_targets.push(LoopTargets {
            label: label.map(str::to_string),
            continue_bb: step_bb,
            break_bb: exit_bb,
            break_slot: None,
            drop_scope_depth: body_scope_index,
        });
        for stmt in body {
            if self.current_block_terminated() {
                break;
            }
            self.codegen_stmt(stmt)?;
        }
        let _ = self.loop_targets.pop();
        if !self.current_block_terminated() {
            self.emit_top_scope_drops()?;
        }
        self.pop_drop_scope();

        if let Some(tail_bb) = self.builder.get_insert_block()
            && tail_bb.get_terminator().is_none()
        {
            self.builder.build_unconditional_branch(step_bb)?;
        }

        self.builder.position_at_end(step_bb);
        let current = self
            .builder
            .build_load(i64_ty, slot_alloca, "meach.sv")?
            .into_int_value();
        let next = self
            .builder
            .build_int_add(current, i64_ty.const_int(1, false), "meach.next")?;
        self.builder.build_store(slot_alloca, next)?;
        self.builder.build_unconditional_branch(cond_bb)?;

        self.builder.position_at_end(exit_bb);
        self.unbind_loop_index(index_binding);
        if consumes {
            self.emit_top_scope_drops()?;
            self.pop_drop_scope();
        }

        match previous_var {
            Some(p) => {
                self.variables.insert(iter_name.clone(), p);
            }
            None => {
                self.variables.remove(&iter_name);
            }
        }
        match previous_ty {
            Some(p) => {
                self.variable_types.insert(iter_name, p);
            }
            None => {
                self.variable_types.remove(&iter_name);
            }
        }
        Ok(())
    }

    /// The value one pass binds for the live slot `slot`: a borrow of the key or value
    /// for a view, the pair of borrows for a borrowed head, and the entry's bits for a
    /// by-value one.
    fn map_loop_entry(
        &mut self,
        (kind, header, slot): (CollectionKind, PointerValue<'ctx>, IntValue<'ctx>),
        (key_ty, value_ty): (&Type, &Type),
        view: Option<MapView>,
        lent: Option<bool>,
        binding_llvm: inkwell::types::BasicTypeEnum<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let key_ptr =
            self.map_slot_field_ptr(kind, header, key_ty, value_ty, slot, SlotField::Key)?;
        let value_ptr =
            self.map_slot_field_ptr(kind, header, key_ty, value_ty, slot, SlotField::Value)?;
        let (key, value) = match (view, lent) {
            (Some(MapView::Keys), _) => return self.slot_borrow(key_ptr, key_ty, false),
            (Some(MapView::Values), _) => return self.slot_borrow(value_ptr, value_ty, false),
            (None, Some(mutable)) => (
                self.slot_borrow(key_ptr, key_ty, false)?,
                self.slot_borrow(value_ptr, value_ty, mutable)?,
            ),
            (None, None) => {
                let key_llvm = self.collection_value_type(key_ty)?;
                let value_llvm = self.collection_value_type(value_ty)?;
                (
                    self.builder.build_load(key_llvm, key_ptr, "meach.key")?,
                    self.builder
                        .build_load(value_llvm, value_ptr, "meach.val")?,
                )
            }
        };
        let pair = binding_llvm.into_struct_type().get_undef();
        let pair = self
            .builder
            .build_insert_value(pair, key, 0, "meach.pair.k")?;
        let pair = self
            .builder
            .build_insert_value(pair, value, 1, "meach.pair.v")?;
        Ok(pair.into_struct_value().into())
    }

    /// Move the map at `owner_header` into a header the consuming loop about to run
    /// owns, leaving its owner an empty one to release, and open the scope that releases
    /// whatever the loop does not take. The `used` field, which only insertion reads,
    /// marks the first slot not yet taken.
    fn take_map_for_loop(
        &mut self,
        owner_header: PointerValue<'ctx>,
        map_ty: &Type,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let header_ty = self.collection_header_type();
        let taken = self.entry_alloca(header_ty, "meach.src")?;
        let value = self
            .builder
            .build_load(header_ty, owner_header, "meach.src.val")?;
        self.builder.build_store(taken, value)?;
        self.builder
            .build_store(owner_header, header_ty.const_zero())?;
        self.store_header_field(taken, FIELD_USED, self.context.i64_type().const_zero())?;
        self.push_drop_scope();
        self.register_local_drop(
            CONSUMED_MAP,
            taken,
            DropTarget::MapRemainder(map_ty.clone()),
        )?;
        Ok(taken)
    }
}

/// What a map loop binds per entry, by the rule lowering applies: a shared borrow of the
/// key or value for a view, `(&K, &V)` / `(&K, &mut V)` through a borrow, and `(K, V)`.
fn map_binding_type(
    key_ty: &Type,
    value_ty: &Type,
    view: Option<MapView>,
    lent: Option<bool>,
) -> Type {
    let borrow = |inner: &Type, mutable: bool| Type::Reference {
        inner: Box::new(inner.clone()),
        mutable,
    };
    match (view, lent) {
        (Some(MapView::Keys), _) => borrow(key_ty, false),
        (Some(MapView::Values), _) => borrow(value_ty, false),
        (None, Some(mutable)) => {
            Type::Tuple(vec![borrow(key_ty, false), borrow(value_ty, mutable)])
        }
        (None, None) => Type::Tuple(vec![key_ty.clone(), value_ty.clone()]),
    }
}
