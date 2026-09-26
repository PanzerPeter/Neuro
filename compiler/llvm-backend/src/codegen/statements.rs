use crate::codegen::context::{DropTarget, LoopTargets};
use inkwell::values::BasicValueEnum;
use inkwell::IntPredicate;
use neuro_hir::{HirConst, HirExpr, HirExprKind, HirPlace, HirStmt, HirType};
use shared_types::Span;

use crate::errors::{CodegenError, CodegenResult};
use crate::type_mapping::TypeMapper;
use crate::types::Type;

use super::context::CodegenContext;

/// Everything a counted range loop needs but its body.
///
/// `index` names the position binding of `(start..end).enumerate()`: it counts
/// iterations from zero rather than reusing the induction variable, which carries
/// the range's own bounds and element type.
pub(crate) struct ForRangeHead<'a> {
    pub(crate) label: Option<&'a str>,
    pub(crate) index: Option<&'a str>,
    pub(crate) iterator: &'a str,
    pub(crate) start: &'a HirExpr,
    pub(crate) end: &'a HirExpr,
    pub(crate) inclusive: bool,
    /// `.rev()`: the same bounds walked from the last value down to `start`.
    pub(crate) reversed: bool,
}

impl<'ctx> CodegenContext<'ctx> {
    /// Generate code for a variable declaration statement.
    ///
    /// The HIR carries the binding's resolved type (`ty`), declared or inferred, so
    /// the alloca is created at that type and the initializer value is coerced to it
    /// (e.g. `val x: i64 = 42` emits an i64 alloca with the literal sign-extended).
    pub(crate) fn codegen_var_decl(
        &mut self,
        name: &str,
        ty: &HirType,
        init: Option<&HirExpr>,
        mutable: bool,
    ) -> CodegenResult<()> {
        // Resolve whether this binding owns a `Drop` value before the initializer is
        // consumed, so its destructor can be scheduled for scope exit.
        //
        // A `string` owns nothing by type. The same fat pointer describes a `.rodata`
        // literal and a heap buffer, so ownership comes from the initializer instead,
        // through a runtime flag: armed by a producer that always allocates, carried over
        // from the binding a move takes the value from, and clear otherwise. A binding
        // that can never own a buffer (an immutable one holding neither) carries none; a
        // `mut` one always does, because a later reassignment may hand it a buffer
        // whatever it started out holding.
        let owns_initial_string = init.is_some_and(|expr| self.produces_owned_string(expr));
        let may_own_string = owns_initial_string
            || mutable
            || init.is_some_and(|expr| self.names_an_owned_string(expr));
        let drop_target = self.drop_target(ty).or_else(|| {
            (may_own_string && matches!(Type::from_hir(ty), Type::String))
                .then_some(DropTarget::HeapString)
        });

        // A holder built from a literal collects the ownership of each `string` a binding
        // moves into it, to hand to its positions once registered below.
        let collecting = init
            .is_some_and(Self::is_aggregate_literal)
            .then(Default::default);
        let outer = std::mem::replace(&mut self.literal_string_moves, collecting);
        let init_val = init.map(|expr| self.codegen_expr(expr)).transpose();
        let literal_moves = std::mem::replace(&mut self.literal_string_moves, outer)
            .map(|moves| moves.flags)
            .unwrap_or_default();
        let init_val = init_val?;

        if let Some(val) = init_val {
            let target_sem = Type::from_hir(ty);
            // `get_any_llvm_type` resolves struct types (which `map_type` rejects); the
            // coercion is a no-op for non-scalar/array values whose type already matches.
            let alloca_ty = self.get_any_llvm_type(&target_sem)?;
            let final_val = self.coerce_if_needed(val, alloca_ty, &target_sem)?;

            let alloca = self.entry_alloca(alloca_ty, name)?;
            self.builder.build_store(alloca, final_val).map_err(|e| {
                CodegenError::LlvmError(format!("failed to store initial value: {}", e))
            })?;
            // `bind_name` records the binding's nominal type for later place statements
            // (field / index assignment) that must recover a struct or array name, and
            // hands back whatever the name meant before. That goes into the enclosing
            // scope's frame so leaving the block puts the outer binding back.
            let pool_registered = self.pool_registered_type(&target_sem);
            let moves_a_registration =
                init.is_some_and(|expr| self.moves_a_pool_registration(expr));
            let shadowed = self.bind_name(name, alloca, alloca_ty, target_sem.clone());
            if let Some(scope) = self.name_scopes.last_mut() {
                scope.push(shadowed);
            }
            // A binding the block declares dies with the arena, so a later store into it
            // may keep the bump path. Everything else a store can reach outlives the
            // block.
            self.note_pool_local(name);

            // Read before the move below disarms it: whether the binding a `string` is
            // moved out of owned the buffer is a runtime fact, and it moves with the value.
            let moved_string_flag = match init {
                Some(expr) if !owns_initial_string => self.load_owned_string_flag(expr)?,
                _ => None,
            };
            // The same for every `string` position of a holder moved whole (`val q = p`).
            let moved_position_flags = match init {
                Some(expr) => self.load_held_string_flags(expr)?,
                None => Vec::new(),
            };
            // Binding a place into a new owner moves it (`val b = a`): clear the source's
            // drop flag so it is not also dropped. Then register the new binding.
            if let Some(expr) = init {
                self.mark_moved_for_drop(expr);
            }
            // A `PoolAware` binding inside a `pool` is released by the arena's sweep at
            // the block's brace instead of by its own scope, so it takes one path or the
            // other and never both.
            if let Some(struct_name) = pool_registered {
                if moves_a_registration {
                    self.transfer_pool_registration(name, &struct_name, alloca)?;
                } else {
                    self.register_pool_aware(name, &struct_name, alloca)?;
                }
            } else if matches!(drop_target, Some(DropTarget::HeapString)) {
                // The one target no type proves: its flag comes from the initializer.
                let flag_ptr = self.register_local_drop(name, alloca, DropTarget::HeapString)?;
                if !owns_initial_string {
                    let owns =
                        moved_string_flag.unwrap_or_else(|| self.context.bool_type().const_zero());
                    self.builder.build_store(flag_ptr, owns)?;
                }
            } else {
                self.register_owned_binding(name, alloca, &target_sem)?;
                // A `string` position the holder just took a fresh buffer into is armed
                // from the initializer for the same reason: its type proves nothing.
                if let Some(expr) = init {
                    self.arm_stored_string_positions(name, &[], expr)?;
                }
                self.store_held_string_flags(name, &moved_position_flags)?;
                self.store_held_string_flags(name, &literal_moves)?;
            }
        }

        Ok(())
    }

    /// Widen, truncate, or extend `val` to match `target_llvm` when the LLVM types
    /// differ (e.g. i32 literal into an i64 alloca).  A no-op when already equal.
    pub(crate) fn coerce_if_needed(
        &self,
        val: inkwell::values::BasicValueEnum<'ctx>,
        target_llvm: inkwell::types::BasicTypeEnum<'ctx>,
        target_sem: &crate::types::Type,
    ) -> CodegenResult<inkwell::values::BasicValueEnum<'ctx>> {
        use inkwell::types::BasicTypeEnum;
        use inkwell::values::BasicValueEnum;

        if val.get_type() == target_llvm {
            return Ok(val);
        }

        match (val, target_llvm) {
            (BasicValueEnum::IntValue(iv), BasicTypeEnum::IntType(it)) => {
                let from_w = iv.get_type().get_bit_width();
                let to_w = it.get_bit_width();
                if to_w > from_w {
                    if TypeMapper::is_unsigned_int(target_sem) {
                        Ok(self.builder.build_int_z_extend(iv, it, "coerce")?.into())
                    } else {
                        Ok(self.builder.build_int_s_extend(iv, it, "coerce")?.into())
                    }
                } else {
                    Ok(self.builder.build_int_truncate(iv, it, "coerce")?.into())
                }
            }
            (BasicValueEnum::FloatValue(fv), BasicTypeEnum::FloatType(ft)) => {
                // Choose ext vs trunc by bit width so half-precision targets coerce
                // correctly; equal-width never reaches here (guarded above).
                if ft.get_bit_width() > fv.get_type().get_bit_width() {
                    Ok(self.builder.build_float_ext(fv, ft, "coerce")?.into())
                } else {
                    Ok(self.builder.build_float_trunc(fv, ft, "coerce")?.into())
                }
            }
            // Element-wise array coercion: rebuild an `[N x T]` aggregate at the
            // target element width so an untyped `[1, 2, 3]` literal (default i32) fits a
            // declared `[i64; N]`. Mirrors the scalar arms, applied per element.
            (BasicValueEnum::ArrayValue(av), BasicTypeEnum::ArrayType(at)) => {
                let crate::types::Type::Array { element, size } = target_sem else {
                    return Ok(av.into());
                };
                let elem_llvm = self.type_mapper.map_type(element)?;
                let mut agg = at.get_undef();
                for i in 0..*size as u32 {
                    let e = self.builder.build_extract_value(av, i, "arr.coerce.get")?;
                    let ce = self.coerce_if_needed(e, elem_llvm, element)?;
                    agg = self
                        .builder
                        .build_insert_value(agg, ce, i, "arr.coerce.set")?
                        .into_array_value();
                }
                Ok(agg.into())
            }
            _ => Ok(val),
        }
    }

    /// Generate code for an assignment statement.
    ///
    /// Order is load-bearing: the new value is evaluated first, because a reassignment
    /// may read the binding it overwrites; only then does the prior value lose its owner
    /// and get released.
    pub(crate) fn codegen_assignment(&mut self, name: &str, value: &HirExpr) -> CodegenResult<()> {
        let collecting = Self::is_aggregate_literal(value).then(Default::default);
        let outer = std::mem::replace(&mut self.literal_string_moves, collecting);
        let val = self.codegen_expr(value);
        let literal_moves = std::mem::replace(&mut self.literal_string_moves, outer)
            .map(|moves| moves.flags)
            .unwrap_or_default();
        let val = val?;

        // `p = p` changes no owner: the storage keeps the value it already held. Both
        // the release below and the move-marking would disown a value that is still
        // there, leaving the binding pointing at freed memory.
        let assigns_from_itself =
            matches!(&value.kind, HirExprKind::Variable(source) if source == name);
        let rearm = if assigns_from_itself {
            None
        } else {
            self.drop_reassigned_value(name)?
        };

        // The variable's alloca must already exist from its declaration.
        let var_ptr = *self
            .variables
            .get(name)
            .ok_or_else(|| CodegenError::UndefinedVariable(name.to_string()))?;

        self.builder.build_store(var_ptr, val).map_err(|e| {
            CodegenError::LlvmError(format!("failed to store value in assignment: {}", e))
        })?;

        // As at a declaration, a `string` moved out of an owning binding brings that
        // binding's flag with it; read it before the move clears it.
        let moved_string_flag = match &rearm {
            Some((_, DropTarget::HeapString)) if !self.produces_owned_string(value) => {
                self.load_owned_string_flag(value)?
            }
            _ => None,
        };
        let moved_position_flags = if rearm.is_some() && !assigns_from_itself {
            self.load_held_string_flags(value)?
        } else {
            Vec::new()
        };
        // Assigning a place moves it into the target, so the source stops being an owner
        // and the target starts being one.
        if !assigns_from_itself {
            self.mark_moved_for_drop(value);
        }
        if let Some((flag_ptr, target)) = rearm {
            self.rearm_drop_flag(flag_ptr, &target, value)?;
            if let Some(owns) = moved_string_flag {
                self.builder.build_store(flag_ptr, owns)?;
            }
            self.rearm_held_drop_flags(name)?;
            self.arm_stored_string_positions(name, &[], value)?;
            self.store_held_string_flags(name, &moved_position_flags)?;
            self.store_held_string_flags(name, &literal_moves)?;
        }

        Ok(())
    }

    /// Store `value` into the storage `place` names.
    ///
    /// One arm per place form rather than one address computation, because the
    /// indexable types do not share one: a `Vec` slot is behind a header, a slice slot
    /// behind a fat pointer, and a tensor slot behind a DLPack handle.
    pub(crate) fn codegen_place_store(
        &mut self,
        place: &HirPlace,
        value: &HirExpr,
        span: Span,
    ) -> CodegenResult<()> {
        match place {
            HirPlace::Var { name, .. } => self.codegen_assignment(name, value),
            HirPlace::Field { object, field, .. } => {
                self.codegen_field_assignment(object, field, value)
            }
            HirPlace::Deref { pointer, .. } => self.codegen_deref_assignment(pointer, value),
            HirPlace::TensorIndex { object, axes, .. } => {
                self.codegen_tensor_index_assignment(object, axes, value, span.start)
            }
            HirPlace::Index { object, index, .. } => {
                let obj_ty = Type::from_hir(&object.ty);
                match obj_ty.referent() {
                    Type::Collection { .. } => {
                        self.codegen_vec_index_assignment(object, &obj_ty, index, value)
                    }
                    Type::Slice(_) => {
                        self.codegen_slice_index_assignment(object, &obj_ty, index, value)
                    }
                    _ => self.codegen_index_assignment(object, index, value),
                }
            }
        }
    }

    /// Generate code for `*pointer = value`, a store through a mutable reference.
    /// `pointer` evaluates to the referent's address; the value is stored there.
    pub(crate) fn codegen_deref_assignment(
        &mut self,
        pointer: &HirExpr,
        value: &HirExpr,
    ) -> CodegenResult<()> {
        let ptr_val = self.codegen_expr(pointer)?;
        let ptr = ptr_val.into_pointer_value();
        let val = self.codegen_expr(value)?;
        self.builder.build_store(ptr, val).map_err(|e| {
            CodegenError::LlvmError(format!("failed to store through reference: {}", e))
        })?;
        self.mark_moved_for_drop(value);
        Ok(())
    }

    /// Generate code for a return statement
    pub(crate) fn codegen_return(&mut self, value: Option<&HirExpr>) -> CodegenResult<()> {
        if let Some(expr) = value {
            let ret_val = self.codegen_expr(expr)?;
            // `return panic("x")`: evaluating the operand already terminated the block with
            // `unreachable`, so there is no value to return and no terminator slot left.
            if self.current_block_terminated() {
                return Ok(());
            }
            // Returning a place moves it out, so it must not be dropped; every other
            // live `Drop` binding in the function is destroyed before control leaves.
            self.mark_moved_for_drop(expr);
            self.emit_drops_through(0)?;
            self.builder
                .build_return(Some(&ret_val))
                .map_err(|e| CodegenError::LlvmError(format!("failed to build return: {}", e)))?;
        } else {
            self.emit_drops_through(0)?;
            self.builder.build_return(None).map_err(|e| {
                CodegenError::LlvmError(format!("failed to build void return: {}", e))
            })?;
        }
        Ok(())
    }

    /// Generate code for an if/else statement
    pub(crate) fn codegen_if(
        &mut self,
        condition: &HirExpr,
        then_block: &[HirStmt],
        else_if_blocks: &[(HirExpr, Vec<HirStmt>)],
        else_block: &Option<Vec<HirStmt>>,
    ) -> CodegenResult<()> {
        let cond_val = self.codegen_expr(condition)?;

        let parent_fn = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("if statement outside function".to_string())
        })?;

        let then_bb = self.context.append_basic_block(parent_fn, "then");
        let else_bb = self.context.append_basic_block(parent_fn, "else");
        let merge_bb = self.context.append_basic_block(parent_fn, "ifcont");

        self.builder
            .build_conditional_branch(cond_val.into_int_value(), then_bb, else_bb)
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to build conditional branch: {}", e))
            })?;

        // Generate then block in its own drop scope so locals declared in the branch
        // are destroyed at the branch's end.
        self.builder.position_at_end(then_bb);
        self.push_drop_scope();
        for stmt in then_block {
            if self.current_block_terminated() {
                break;
            }
            self.codegen_stmt(stmt)?;
        }
        if !self.current_block_terminated() {
            self.emit_top_scope_drops()?;
        }
        self.pop_drop_scope();
        // After nested control flow the builder may be positioned at a block that is NOT
        // then_bb (e.g. the merge block of an inner if).  Checking then_bb would miss that
        // case, so we check whichever block the builder currently occupies.
        if let Some(current_bb) = self.builder.get_insert_block() {
            if current_bb.get_terminator().is_none() {
                self.builder
                    .build_unconditional_branch(merge_bb)
                    .map_err(|e| {
                        CodegenError::LlvmError(format!("failed to build branch: {}", e))
                    })?;
            }
        }

        // Generate else-if and else blocks.
        // Each else-if arm is the condition of the next level: the remaining arms and
        // the final else become the recursive else_if/else_block so they remain mutually
        // exclusive with the current arm.
        self.builder.position_at_end(else_bb);
        if let Some(((elif_cond, elif_stmts), rest)) = else_if_blocks.split_first() {
            self.codegen_if(elif_cond, elif_stmts, rest, else_block)?;
        } else if let Some(else_stmts) = else_block {
            self.push_drop_scope();
            for stmt in else_stmts {
                if self.current_block_terminated() {
                    break;
                }
                self.codegen_stmt(stmt)?;
            }
            if !self.current_block_terminated() {
                self.emit_top_scope_drops()?;
            }
            self.pop_drop_scope();
        }
        // Same: check current insert block, not the fixed else_bb, for the same reason.
        if let Some(current_bb) = self.builder.get_insert_block() {
            if current_bb.get_terminator().is_none() {
                self.builder
                    .build_unconditional_branch(merge_bb)
                    .map_err(|e| {
                        CodegenError::LlvmError(format!("failed to build branch: {}", e))
                    })?;
            }
        }

        // Continue at merge block
        self.builder.position_at_end(merge_bb);

        Ok(())
    }

    /// Resolve the [`LoopTargets`] a `break`/`continue` refers to: the innermost
    /// loop, or the nearest enclosing one carrying `label`. Label resolution is
    /// validated in semantic analysis, so a miss here is an internal error.
    fn lookup_loop_target(&self, label: Option<&str>) -> CodegenResult<&LoopTargets<'ctx>> {
        match label {
            Some(label) => self
                .loop_targets
                .iter()
                .rev()
                .find(|t| t.label.as_deref() == Some(label))
                .ok_or_else(|| {
                    CodegenError::InternalError(format!(
                        "undefined loop label '{}' during codegen",
                        label
                    ))
                }),
            None => self.loop_targets.last().ok_or_else(|| {
                CodegenError::InternalError(
                    "break/continue used outside loop during codegen".to_string(),
                )
            }),
        }
    }

    pub(crate) fn codegen_while(
        &mut self,
        label: Option<&str>,
        condition: &HirExpr,
        body: &[HirStmt],
    ) -> CodegenResult<()> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;

        let cond_bb = self.context.append_basic_block(parent_fn, "while.cond");
        let body_bb = self.context.append_basic_block(parent_fn, "while.body");
        let exit_bb = self.context.append_basic_block(parent_fn, "while.exit");

        let current_bb = self.builder.get_insert_block().ok_or_else(|| {
            CodegenError::InternalError("no insert block before while".to_string())
        })?;

        if current_bb.get_terminator().is_none() {
            self.builder
                .build_unconditional_branch(cond_bb)
                .map_err(|e| CodegenError::LlvmError(format!("failed to build branch: {}", e)))?;
        }

        self.builder.position_at_end(cond_bb);
        let cond_val = self.codegen_expr(condition)?;
        self.builder
            .build_conditional_branch(cond_val.into_int_value(), body_bb, exit_bb)
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to build conditional branch: {}", e))
            })?;

        self.builder.position_at_end(body_bb);
        let body_scope_index = self.drop_scopes.len();
        self.push_drop_scope();
        self.loop_targets.push(LoopTargets {
            label: label.map(str::to_string),
            continue_bb: cond_bb,
            break_bb: exit_bb,
            break_slot: None,
            drop_scope_depth: body_scope_index,
        });
        for stmt in body {
            if let Some(current_bb) = self.builder.get_insert_block() {
                if current_bb.get_terminator().is_some() {
                    break;
                }
            }
            self.codegen_stmt(stmt)?;
        }
        let _ = self.loop_targets.pop();
        if !self.current_block_terminated() {
            self.emit_top_scope_drops()?;
        }
        self.pop_drop_scope();

        if let Some(tail_bb) = self.builder.get_insert_block() {
            if tail_bb.get_terminator().is_none() {
                self.builder
                    .build_unconditional_branch(cond_bb)
                    .map_err(|e| {
                        CodegenError::LlvmError(format!("failed to build branch: {}", e))
                    })?;
            }
        }

        self.builder.position_at_end(exit_bb);
        Ok(())
    }

    /// Generate code for an infinite `loop { ... }`, returning the loop's
    /// value when it is used as an expression and yields one via `break v`.
    ///
    /// Unlike `while`, there is no condition block: control branches
    /// unconditionally into the body and back to its top, so the only way out is a
    /// `break`. `continue` re-enters the body from the top. `result_ty` is the loop
    /// expression's type: when it is not `Void`, a result slot is allocated,
    /// value-carrying `break`s store into it, and the loaded value is returned.
    /// A statement-position loop reaches here as a `HirStmt::Expr` whose loop node
    /// carries type `void`, so no slot is allocated and no value is produced.
    pub(crate) fn codegen_loop(
        &mut self,
        label: Option<&str>,
        body: &[HirStmt],
        result_ty: &Type,
    ) -> CodegenResult<Option<BasicValueEnum<'ctx>>> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;

        let result_ty = result_ty.clone();
        let result_slot = if matches!(result_ty, Type::Void) {
            None
        } else {
            let llvm_ty = self.get_any_llvm_type(&result_ty)?;
            Some(self.entry_alloca(llvm_ty, "loopexpr.result")?)
        };

        let body_bb = self.context.append_basic_block(parent_fn, "loop.body");
        let exit_bb = self.context.append_basic_block(parent_fn, "loop.exit");

        let current_bb = self.builder.get_insert_block().ok_or_else(|| {
            CodegenError::InternalError("no insert block before loop".to_string())
        })?;

        if current_bb.get_terminator().is_none() {
            self.builder
                .build_unconditional_branch(body_bb)
                .map_err(|e| CodegenError::LlvmError(format!("failed to build branch: {}", e)))?;
        }

        self.builder.position_at_end(body_bb);
        let body_scope_index = self.drop_scopes.len();
        self.push_drop_scope();
        self.loop_targets.push(LoopTargets {
            label: label.map(str::to_string),
            continue_bb: body_bb,
            break_bb: exit_bb,
            break_slot: result_slot,
            drop_scope_depth: body_scope_index,
        });
        for stmt in body {
            if let Some(current_bb) = self.builder.get_insert_block() {
                if current_bb.get_terminator().is_some() {
                    break;
                }
            }
            self.codegen_stmt(stmt)?;
        }
        let _ = self.loop_targets.pop();
        if !self.current_block_terminated() {
            self.emit_top_scope_drops()?;
        }
        self.pop_drop_scope();

        if let Some(tail_bb) = self.builder.get_insert_block() {
            if tail_bb.get_terminator().is_none() {
                self.builder
                    .build_unconditional_branch(body_bb)
                    .map_err(|e| {
                        CodegenError::LlvmError(format!("failed to build branch: {}", e))
                    })?;
            }
        }

        self.builder.position_at_end(exit_bb);

        match result_slot {
            Some(slot) => {
                let llvm_ty = self.get_any_llvm_type(&result_ty)?;
                let val = self.builder.build_load(llvm_ty, slot, "loopexpr.val")?;
                Ok(Some(val))
            }
            None => Ok(None),
        }
    }

    /// The value a reversed range's binding is mirrored around: `start + last`, where
    /// `last` is the greatest value the range names.
    ///
    /// Subtracting the ascending induction variable from it yields `last` on the first
    /// iteration and `start` on the final one. Both the sum and the subtraction wrap, and
    /// wrapping is exactly right: every value the loop yields lies inside the element
    /// type, so the arithmetic is correct modulo its width even when `start + last` is
    /// not representable.
    fn reversed_range_origin(
        &self,
        start: BasicValueEnum<'ctx>,
        end: BasicValueEnum<'ctx>,
        inclusive: bool,
    ) -> CodegenResult<inkwell::values::IntValue<'ctx>> {
        let end = end.into_int_value();
        let last = match inclusive {
            true => end,
            false => self.builder.build_int_sub(
                end,
                end.get_type().const_int(1, false),
                "for.rev.last",
            )?,
        };
        self.builder
            .build_int_add(start.into_int_value(), last, "for.rev.origin")
            .map_err(CodegenError::from)
    }

    /// Generate code for a for-range statement (`for i in start..end { ... }`).
    pub(crate) fn codegen_for_range(
        &mut self,
        head: ForRangeHead<'_>,
        body: &[HirStmt],
    ) -> CodegenResult<()> {
        let ForRangeHead {
            label,
            index,
            iterator,
            start,
            end,
            inclusive,
            reversed,
        } = head;
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;

        let iter_sem_ty = Type::from_hir(&start.ty);
        let start_val = self.codegen_expr(start)?;
        let end_val = self.codegen_expr(end)?;
        let iter_name = iterator.to_string();
        // Record the iterator's type so a body place statement can recover it.
        self.type_env.insert(iter_name.clone(), iter_sem_ty.clone());

        let iter_alloca = self.entry_alloca(start_val.get_type(), &iter_name)?;
        self.builder
            .build_store(iter_alloca, start_val)
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to initialize iterator: {}", e))
            })?;

        // A reversed range keeps the ASCENDING induction variable and mirrors it onto the
        // binding at the top of the body. Counting down in the binding itself would step
        // past `start` to terminate, and on an unsigned range starting at zero that step
        // wraps instead: `for i in (0..n).rev()` would never leave the loop.
        let induction_alloca = match reversed {
            true => {
                let slot = self.entry_alloca(start_val.get_type(), "for.rev.k")?;
                self.builder.build_store(slot, start_val).map_err(|e| {
                    CodegenError::LlvmError(format!("failed to initialize iterator: {}", e))
                })?;
                slot
            }
            false => iter_alloca,
        };
        let mirror_origin = reversed
            .then(|| self.reversed_range_origin(start_val, end_val, inclusive))
            .transpose()?;

        let previous_var = self.variables.insert(iter_name.clone(), iter_alloca);
        let previous_var_type = self
            .variable_types
            .insert(iter_name.clone(), start_val.get_type());

        let i64_ty = self.context.i64_type();
        let position_alloca = match index {
            Some(_) => {
                let slot = self.entry_alloca(i64_ty, "for.pos")?;
                self.builder.build_store(slot, i64_ty.const_zero())?;
                Some(slot)
            }
            None => None,
        };
        let index_binding = self.bind_loop_index(index)?;

        let cond_bb = self.context.append_basic_block(parent_fn, "for.cond");
        let body_bb = self.context.append_basic_block(parent_fn, "for.body");
        let step_bb = self.context.append_basic_block(parent_fn, "for.step");
        let exit_bb = self.context.append_basic_block(parent_fn, "for.exit");

        let current_bb = self.builder.get_insert_block().ok_or_else(|| {
            CodegenError::InternalError("no insert block before for-range".to_string())
        })?;

        if current_bb.get_terminator().is_none() {
            self.builder
                .build_unconditional_branch(cond_bb)
                .map_err(|e| CodegenError::LlvmError(format!("failed to build branch: {}", e)))?;
        }

        self.builder.position_at_end(cond_bb);
        let iter_int = self
            .builder
            .build_load(start_val.get_type(), induction_alloca, "for.cur")?
            .into_int_value();
        let end_int = end_val.into_int_value();

        let cmp_predicate = match (TypeMapper::is_unsigned_int(&iter_sem_ty), inclusive) {
            (true, true) => IntPredicate::ULE,
            (true, false) => IntPredicate::ULT,
            (false, true) => IntPredicate::SLE,
            (false, false) => IntPredicate::SLT,
        };

        let cond_val = self
            .builder
            .build_int_compare(cmp_predicate, iter_int, end_int, "for.cond")
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to build for condition compare: {}", e))
            })?;
        self.builder
            .build_conditional_branch(cond_val, body_bb, exit_bb)
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to build conditional branch: {}", e))
            })?;

        self.builder.position_at_end(body_bb);
        if let Some(origin) = mirror_origin {
            let mirrored = self
                .builder
                .build_int_sub(origin, iter_int, "for.rev.cur")?;
            self.builder.build_store(iter_alloca, mirrored)?;
        }
        if let Some(slot) = position_alloca {
            let position = self
                .builder
                .build_load(i64_ty, slot, "for.pos.cur")?
                .into_int_value();
            self.store_loop_index(&index_binding, position)?;
        }
        let body_scope_index = self.drop_scopes.len();
        self.push_drop_scope();
        self.loop_targets.push(LoopTargets {
            label: label.map(str::to_string),
            continue_bb: step_bb,
            break_bb: exit_bb,
            break_slot: None,
            drop_scope_depth: body_scope_index,
        });
        for stmt in body {
            if let Some(current_bb) = self.builder.get_insert_block() {
                if current_bb.get_terminator().is_some() {
                    break;
                }
            }
            self.codegen_stmt(stmt)?;
        }
        let _ = self.loop_targets.pop();
        if !self.current_block_terminated() {
            self.emit_top_scope_drops()?;
        }
        self.pop_drop_scope();

        if let Some(tail_bb) = self.builder.get_insert_block() {
            if tail_bb.get_terminator().is_none() {
                self.builder
                    .build_unconditional_branch(step_bb)
                    .map_err(|e| {
                        CodegenError::LlvmError(format!("failed to build branch: {}", e))
                    })?;
            }
        }

        self.builder.position_at_end(step_bb);
        let current_iter = self
            .builder
            .build_load(start_val.get_type(), induction_alloca, "for.cur")?
            .into_int_value();
        let one = current_iter.get_type().const_int(1, false);
        let next_iter = self
            .builder
            .build_int_add(current_iter, one, "for.next")
            .map_err(|e| CodegenError::LlvmError(format!("failed to increment iterator: {}", e)))?;
        self.builder
            .build_store(induction_alloca, next_iter)
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to store incremented iterator: {}", e))
            })?;
        if let Some(slot) = position_alloca {
            let current = self
                .builder
                .build_load(i64_ty, slot, "for.pos.cur")?
                .into_int_value();
            let next =
                self.builder
                    .build_int_add(current, i64_ty.const_int(1, false), "for.pos.next")?;
            self.builder.build_store(slot, next)?;
        }
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(format!("failed to build branch: {}", e)))?;

        self.builder.position_at_end(exit_bb);
        self.unbind_loop_index(index_binding);

        if let Some(previous) = previous_var {
            self.variables.insert(iter_name.clone(), previous);
        } else {
            self.variables.remove(&iter_name);
        }

        if let Some(previous_ty) = previous_var_type {
            self.variable_types.insert(iter_name.clone(), previous_ty);
        } else {
            self.variable_types.remove(&iter_name);
        }

        Ok(())
    }

    /// Generate code for a statement
    pub(crate) fn codegen_stmt(&mut self, stmt: &HirStmt) -> CodegenResult<()> {
        // Statements following a divergent statement (a `panic`/`unreachable` builtin, or
        // a `return`/`break`/`continue`) are dead code: the current block already has a
        // terminator. Emitting into it would append instructions after a terminator and
        // fail LLVM verification, so skip them. LLVM drops the now-unreferenced code.
        if self.current_block_terminated() {
            return Ok(());
        }

        match stmt {
            HirStmt::VarDecl {
                name,
                ty,
                init,
                mutable,
                ..
            } => self.codegen_var_decl(name, ty, init.as_ref(), *mutable),
            HirStmt::Assign { place, value, span } => {
                self.store_outside_pool(place, |ctx| ctx.codegen_place_store(place, value, *span))
            }
            HirStmt::TensorCompoundAssign {
                place,
                op,
                value,
                ty,
                span,
            } => {
                let receiver = place.to_expr(*span);
                self.codegen_tensor_compound_assign(&receiver, *op, value, ty, span.start)
            }
            HirStmt::Return { value, .. } => self.codegen_return(value.as_ref()),
            HirStmt::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => self.codegen_if(condition, then_block, else_if_blocks, else_block),
            HirStmt::While {
                label,
                condition,
                body,
                ..
            } => self.codegen_while(label.as_deref(), condition, body),
            HirStmt::ForRange {
                label,
                index,
                iterator,
                start,
                end,
                inclusive,
                reversed,
                body,
                ..
            } => self.codegen_for_range(
                ForRangeHead {
                    label: label.as_deref(),
                    index: index.as_deref(),
                    iterator,
                    start,
                    end,
                    inclusive: *inclusive,
                    reversed: *reversed,
                },
                body,
            ),
            HirStmt::ForEach {
                label,
                index,
                iterator,
                iterable,
                body,
                ..
            } => {
                let obj_ty = Type::from_hir(&iterable.ty);
                if matches!(obj_ty.referent(), Type::Collection { .. }) {
                    return self.codegen_vec_for_each(
                        label.as_deref(),
                        index.as_deref(),
                        iterator,
                        iterable,
                        &obj_ty,
                        body,
                    );
                }
                if matches!(obj_ty.referent(), Type::Slice(_)) {
                    return self.codegen_slice_for_each(
                        label.as_deref(),
                        index.as_deref(),
                        iterator,
                        iterable,
                        &obj_ty,
                        body,
                    );
                }
                self.codegen_for_each(label.as_deref(), index.as_deref(), iterator, iterable, body)
            }
            HirStmt::Break { label, value, .. } => {
                let target = self.lookup_loop_target(label.as_deref())?;
                let break_bb = target.break_bb;
                let break_slot = target.break_slot;
                let drop_depth = target.drop_scope_depth;

                // A value-carrying `break v` stores `v` into the loop's result
                // slot before exiting; semantic analysis guarantees the slot exists.
                if let Some(value_expr) = value {
                    let val = self.codegen_expr(value_expr)?;
                    if let Some(slot) = break_slot {
                        if !self.current_block_terminated() {
                            self.builder.build_store(slot, val)?;
                        }
                    }
                    // A broken-out place is moved out of the loop and must not be dropped.
                    self.mark_moved_for_drop(value_expr);
                }

                // Destroy every binding from here down to the loop's body scope before
                // leaving the loop.
                self.emit_drops_through(drop_depth)?;

                if let Some(current_bb) = self.builder.get_insert_block() {
                    if current_bb.get_terminator().is_none() {
                        self.builder
                            .build_unconditional_branch(break_bb)
                            .map_err(|e| {
                                CodegenError::LlvmError(format!(
                                    "failed to build break branch: {}",
                                    e
                                ))
                            })?;
                    }
                }

                Ok(())
            }
            HirStmt::Continue { label, .. } => {
                let target = self.lookup_loop_target(label.as_deref())?;
                let continue_bb = target.continue_bb;
                let drop_depth = target.drop_scope_depth;

                // Re-entering the loop ends this iteration's body scope, so its bindings
                // are destroyed before the back-edge.
                self.emit_drops_through(drop_depth)?;

                if let Some(current_bb) = self.builder.get_insert_block() {
                    if current_bb.get_terminator().is_none() {
                        self.builder
                            .build_unconditional_branch(continue_bb)
                            .map_err(|e| {
                                CodegenError::LlvmError(format!(
                                    "failed to build continue branch: {}",
                                    e
                                ))
                            })?;
                    }
                }

                Ok(())
            }
            HirStmt::ValElse {
                scrutinee,
                test,
                bindings,
                else_binding,
                else_block,
                ..
            } => {
                self.codegen_val_else(scrutinee, test, bindings, else_binding.as_ref(), else_block)
            }

            HirStmt::Const {
                name, ty, value, ..
            } => {
                let declared_sem = Type::from_hir(ty);
                let val = self.codegen_const_expr_typed(value, &declared_sem)?;
                self.const_values.insert(name.clone(), val);
                self.type_env.insert(name.clone(), declared_sem);
                Ok(())
            }

            HirStmt::Expr(expr) => {
                // A call in statement position may return unit `()`; dispatch directly so
                // a void result is discarded rather than treated as a missing value.
                //
                // Nothing reads a statement's value, so an owned `string` it produced has
                // no consumer at all and is released here rather than at one.
                let value = if let HirExprKind::Call { callee, args } = &expr.kind {
                    self.codegen_call_dispatch(callee, args, &expr.span)?
                } else {
                    Some(self.codegen_expr(expr)?)
                };
                if let Some(value) = value {
                    self.release_string_temporary(expr, value)?;
                    self.drop_unbound_temporary(expr, value)?;
                }
                Ok(())
            }
        }
    }

    /// Emit a module-level constant as an LLVM global constant and cache its value.
    pub(crate) fn codegen_global_const(&mut self, def: &HirConst) -> CodegenResult<()> {
        let declared_sem = Type::from_hir(&def.ty);
        let val = self.codegen_const_expr_typed(&def.value, &declared_sem)?;
        let llvm_ty = val.get_type();
        let global = self.module.add_global(llvm_ty, None, &def.name);
        global.set_initializer(&val);
        global.set_constant(true);
        global.set_linkage(inkwell::module::Linkage::Internal);

        // Cache the value directly so identifier resolution returns the constant without
        // emitting a load: consts are values, not memory locations.
        self.const_values.insert(def.name.clone(), val);
        Ok(())
    }
}
