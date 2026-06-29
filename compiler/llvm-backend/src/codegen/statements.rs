use crate::codegen::context::LoopTargets;
use inkwell::values::BasicValueEnum;
use inkwell::IntPredicate;
use neuro_hir::{HirConst, HirExpr, HirExprKind, HirStmt, HirType};

use crate::errors::{CodegenError, CodegenResult};
use crate::type_mapping::TypeMapper;
use crate::types::Type;

use super::context::CodegenContext;

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
    ) -> CodegenResult<()> {
        // Resolve whether this binding owns a `Drop` value before the initializer is
        // consumed, so its destructor can be scheduled for scope exit (§2.1).
        let drop_name = self.drop_struct_name(ty);

        let init_val = if let Some(expr) = init {
            Some(self.codegen_expr(expr)?)
        } else {
            None
        };

        if let Some(val) = init_val {
            let target_sem = Type::from_hir(ty);
            // `get_any_llvm_type` resolves struct types (which `map_type` rejects); the
            // coercion is a no-op for non-scalar/array values whose type already matches.
            let alloca_ty = self.get_any_llvm_type(&target_sem)?;
            let final_val = self.coerce_if_needed(val, alloca_ty, &target_sem)?;

            let alloca = self.builder.build_alloca(alloca_ty, name).map_err(|e| {
                CodegenError::LlvmError(format!("failed to allocate variable: {}", e))
            })?;
            self.builder.build_store(alloca, final_val).map_err(|e| {
                CodegenError::LlvmError(format!("failed to store initial value: {}", e))
            })?;
            self.variables.insert(name.to_string(), alloca);
            self.variable_types.insert(name.to_string(), alloca_ty);
            // Record the binding's nominal type for later place statements (field /
            // index assignment) that must recover a struct or array name.
            self.type_env.insert(name.to_string(), target_sem);

            // Binding a place into a new owner moves it (`val b = a`): clear the source's
            // drop flag so it is not also dropped (§2.2). Then register the new binding.
            if let Some(expr) = init {
                self.mark_moved_for_drop(expr);
            }
            if let Some(struct_name) = drop_name {
                self.register_local_drop(name, alloca, struct_name)?;
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
                        Ok(self
                            .builder
                            .build_int_z_extend(iv, it, "coerce")
                            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                            .into())
                    } else {
                        Ok(self
                            .builder
                            .build_int_s_extend(iv, it, "coerce")
                            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                            .into())
                    }
                } else {
                    Ok(self
                        .builder
                        .build_int_truncate(iv, it, "coerce")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            (BasicValueEnum::FloatValue(fv), BasicTypeEnum::FloatType(ft)) => {
                // Choose ext vs trunc by bit width so half-precision targets coerce
                // correctly (§1.2); equal-width never reaches here (guarded above).
                if ft.get_bit_width() > fv.get_type().get_bit_width() {
                    Ok(self
                        .builder
                        .build_float_ext(fv, ft, "coerce")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_float_trunc(fv, ft, "coerce")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            // Element-wise array coercion (§3.1): rebuild an `[N x T]` aggregate at the
            // target element width so an untyped `[1, 2, 3]` literal (default i32) fits a
            // declared `[i64; N]`. Mirrors the scalar arms, applied per element.
            (BasicValueEnum::ArrayValue(av), BasicTypeEnum::ArrayType(at)) => {
                let crate::types::Type::Array { element, size } = target_sem else {
                    return Ok(av.into());
                };
                let elem_llvm = self.type_mapper.map_type(element)?;
                let mut agg = at.get_undef();
                for i in 0..*size as u32 {
                    let e = self
                        .builder
                        .build_extract_value(av, i, "arr.coerce.get")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                    let ce = self.coerce_if_needed(e, elem_llvm, element)?;
                    agg = self
                        .builder
                        .build_insert_value(agg, ce, i, "arr.coerce.set")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into_array_value();
                }
                Ok(agg.into())
            }
            _ => Ok(val),
        }
    }

    /// Generate code for an assignment statement
    pub(crate) fn codegen_assignment(&mut self, name: &str, value: &HirExpr) -> CodegenResult<()> {
        let val = self.codegen_expr(value)?;

        // The variable's alloca must already exist from its declaration.
        let var_ptr = self
            .variables
            .get(name)
            .ok_or_else(|| CodegenError::UndefinedVariable(name.to_string()))?;

        self.builder.build_store(*var_ptr, val).map_err(|e| {
            CodegenError::LlvmError(format!("failed to store value in assignment: {}", e))
        })?;

        // Assigning a place moves it into the target (§2.2). The prior value held by a
        // reassigned `Drop` binding is not dropped here (a known limitation); the target
        // is still dropped once at scope exit.
        self.mark_moved_for_drop(value);

        Ok(())
    }

    /// Generate code for `*pointer = value` — a store through a mutable reference (§2.5).
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
            // live `Drop` binding in the function is destroyed before control leaves (§2.1).
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
        // are destroyed at the branch's end (§2.1).
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

    /// Generate code for an infinite `loop { ... }` (§3.7), returning the loop's
    /// value when it is used as an expression and yields one via `break v`.
    ///
    /// Unlike `while`, there is no condition block: control branches
    /// unconditionally into the body and back to its top, so the only way out is a
    /// `break`. `continue` re-enters the body from the top. `result_ty` is the loop
    /// expression's type (§3.7): when it is not `Void`, a result slot is allocated,
    /// value-carrying `break`s store into it, and the loaded value is returned.
    /// `Stmt::Loop` passes `Void` (the value is discarded); `Expr::Loop` passes its
    /// resolved type.
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
            Some(
                self.builder
                    .build_alloca(llvm_ty, "loopexpr.result")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
            )
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
                let val = self
                    .builder
                    .build_load(llvm_ty, slot, "loopexpr.val")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                Ok(Some(val))
            }
            None => Ok(None),
        }
    }

    /// Generate code for a for-range statement (`for i in start..end { ... }`).
    pub(crate) fn codegen_for_range(
        &mut self,
        label: Option<&str>,
        iterator: &str,
        start: &HirExpr,
        end: &HirExpr,
        inclusive: bool,
        body: &[HirStmt],
    ) -> CodegenResult<()> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;

        let iter_sem_ty = Type::from_hir(&start.ty);
        let start_val = self.codegen_expr(start)?;
        let end_val = self.codegen_expr(end)?;
        let iter_name = iterator.to_string();
        // Record the iterator's type so a body place statement can recover it.
        self.type_env.insert(iter_name.clone(), iter_sem_ty.clone());

        let iter_alloca = self
            .builder
            .build_alloca(start_val.get_type(), &iter_name)
            .map_err(|e| CodegenError::LlvmError(format!("failed to allocate iterator: {}", e)))?;
        self.builder
            .build_store(iter_alloca, start_val)
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to initialize iterator: {}", e))
            })?;

        let previous_var = self.variables.insert(iter_name.clone(), iter_alloca);
        let previous_var_type = self
            .variable_types
            .insert(iter_name.clone(), start_val.get_type());

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
        let iter_val = self.codegen_identifier(&iter_name)?;
        let iter_int = iter_val.into_int_value();
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
        let current_iter = self.codegen_identifier(&iter_name)?.into_int_value();
        let one = current_iter.get_type().const_int(1, false);
        let next_iter = self
            .builder
            .build_int_add(current_iter, one, "for.next")
            .map_err(|e| CodegenError::LlvmError(format!("failed to increment iterator: {}", e)))?;
        self.builder
            .build_store(iter_alloca, next_iter)
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to store incremented iterator: {}", e))
            })?;
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(format!("failed to build branch: {}", e)))?;

        self.builder.position_at_end(exit_bb);

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
            HirStmt::VarDecl { name, ty, init, .. } => {
                self.codegen_var_decl(name, ty, init.as_ref())
            }
            HirStmt::Assignment { target, value, .. } => self.codegen_assignment(target, value),
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
            HirStmt::Loop { label, body, .. } => {
                // Statement position: the loop value (if any) is discarded.
                self.codegen_loop(label.as_deref(), body, &Type::Void)?;
                Ok(())
            }
            HirStmt::ForRange {
                label,
                iterator,
                start,
                end,
                inclusive,
                body,
                ..
            } => self.codegen_for_range(label.as_deref(), iterator, start, end, *inclusive, body),
            HirStmt::ForEach {
                label,
                iterator,
                iterable,
                body,
                ..
            } => self.codegen_for_each(label.as_deref(), iterator, iterable, body),
            HirStmt::IndexAssignment {
                target,
                index,
                value,
                ..
            } => self.codegen_index_assignment(target, index, value),
            HirStmt::Break { label, value, .. } => {
                let target = self.lookup_loop_target(label.as_deref())?;
                let break_bb = target.break_bb;
                let break_slot = target.break_slot;
                let drop_depth = target.drop_scope_depth;

                // A value-carrying `break v` (§3.7) stores `v` into the loop's result
                // slot before exiting; semantic analysis guarantees the slot exists.
                if let Some(value_expr) = value {
                    let val = self.codegen_expr(value_expr)?;
                    if let Some(slot) = break_slot {
                        if !self.current_block_terminated() {
                            self.builder
                                .build_store(slot, val)
                                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                        }
                    }
                    // A broken-out place is moved out of the loop and must not be dropped.
                    self.mark_moved_for_drop(value_expr);
                }

                // Destroy every binding from here down to the loop's body scope before
                // leaving the loop (§2.1).
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
                // are destroyed before the back-edge (§2.1).
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
            HirStmt::FieldAssignment {
                object,
                field,
                value,
                ..
            } => self.codegen_field_assignment(object, field, value),

            HirStmt::DerefAssignment { pointer, value, .. } => {
                self.codegen_deref_assignment(pointer, value)
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
                if let HirExprKind::Call { callee, args } = &expr.kind {
                    self.codegen_call_dispatch(callee, args, &expr.span)?;
                } else {
                    self.codegen_expr(expr)?;
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
        // emitting a load — consts are values, not memory locations.
        self.const_values.insert(def.name.clone(), val);
        Ok(())
    }
}
