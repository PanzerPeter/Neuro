//! Loops: `while`, `loop` and the counted `for` over a range, with its `.rev()` and
//! `.step(n)` walks and the branch targets `break` / `continue` resolve to.

use inkwell::IntPredicate;
use inkwell::values::BasicValueEnum;
use neuro_hir::{HirExpr, HirStmt};

use crate::codegen::context::{CodegenContext, LoopTargets};
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// The panic a `.step(n)` whose run-time stride is zero or negative aborts with.
pub(super) const RANGE_STEP_NOT_POSITIVE: &str = "range step must be positive";

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
    /// `.step(n)`: the stride of that walk, of the range's own type.
    pub(crate) step: Option<&'a HirExpr>,
}

impl<'ctx> CodegenContext<'ctx> {
    /// Resolve the [`LoopTargets`] a `break`/`continue` refers to: the innermost
    /// loop, or the nearest enclosing one carrying `label`. Label resolution is
    /// validated in semantic analysis, so a miss here is an internal error.
    pub(super) fn lookup_loop_target(
        &self,
        label: Option<&str>,
    ) -> CodegenResult<&LoopTargets<'ctx>> {
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
            if let Some(current_bb) = self.builder.get_insert_block()
                && current_bb.get_terminator().is_some()
            {
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
            self.builder
                .build_unconditional_branch(cond_bb)
                .map_err(|e| CodegenError::LlvmError(format!("failed to build branch: {}", e)))?;
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
            if let Some(current_bb) = self.builder.get_insert_block()
                && current_bb.get_terminator().is_some()
            {
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
            self.builder
                .build_unconditional_branch(body_bb)
                .map_err(|e| CodegenError::LlvmError(format!("failed to build branch: {}", e)))?;
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
    pub(super) fn reversed_range_origin(
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

    /// Evaluate a `.step(n)` stride once, before the loop, and abort unless it is
    /// positive: a zero stride would never leave the loop, and the language defines no
    /// meaning for a negative one (`.rev()` is how a range walks down).
    pub(super) fn codegen_range_stride(
        &mut self,
        step: &HirExpr,
        element: &Type,
    ) -> CodegenResult<inkwell::values::IntValue<'ctx>> {
        let stride = self.codegen_expr(step)?.into_int_value();
        let zero = stride.get_type().const_zero();
        let predicate = match element.is_unsigned_int() {
            true => IntPredicate::NE,
            false => IntPredicate::SGT,
        };
        let positive = self
            .builder
            .build_int_compare(predicate, stride, zero, "for.step.ok")?;
        self.codegen_guard_or_panic(positive, RANGE_STEP_NOT_POSITIVE, step.span.start)?;
        Ok(stride)
    }

    /// Leave a stepped or inclusive loop when no further value fits in the range.
    ///
    /// Adding the stride first and comparing after is wrong near the top of the type:
    /// `(0u8..255).step(10)` would wrap from 250 back to 4 and never exit. The distance
    /// still to go is compared instead. `end - current` is taken as an unsigned
    /// difference, which is exact even for a signed range whose width exceeds its
    /// type's maximum, because the loop only reaches here with `current` inside the range.
    pub(super) fn exit_when_stride_overshoots(
        &mut self,
        current: inkwell::values::IntValue<'ctx>,
        end: inkwell::values::IntValue<'ctx>,
        stride: inkwell::values::IntValue<'ctx>,
        inclusive: bool,
        exit_bb: inkwell::basic_block::BasicBlock<'ctx>,
    ) -> CodegenResult<()> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;
        let remaining = self.builder.build_int_sub(end, current, "for.step.left")?;
        // An inclusive range may still yield `end` itself, so a stride landing exactly on
        // it continues; an exclusive one must land strictly short of `end`.
        let overshoots = match inclusive {
            true => IntPredicate::ULT,
            false => IntPredicate::ULE,
        };
        let done =
            self.builder
                .build_int_compare(overshoots, remaining, stride, "for.step.done")?;
        let advance_bb = self.context.append_basic_block(parent_fn, "for.advance");
        self.builder
            .build_conditional_branch(done, exit_bb, advance_bb)?;
        self.builder.position_at_end(advance_bb);
        Ok(())
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
            step,
        } = head;
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;

        let iter_sem_ty = Type::from_hir(&start.ty);
        let start_val = self.codegen_expr(start)?;
        let end_val = self.codegen_expr(end)?;
        let stride = step
            .map(|step| self.codegen_range_stride(step, &iter_sem_ty))
            .transpose()?;
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

        let cmp_predicate = match (iter_sem_ty.is_unsigned_int(), inclusive) {
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
            if let Some(current_bb) = self.builder.get_insert_block()
                && current_bb.get_terminator().is_some()
            {
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
            self.builder
                .build_unconditional_branch(step_bb)
                .map_err(|e| CodegenError::LlvmError(format!("failed to build branch: {}", e)))?;
        }

        self.builder.position_at_end(step_bb);
        let current_iter = self
            .builder
            .build_load(start_val.get_type(), induction_alloca, "for.cur")?
            .into_int_value();
        let increment = stride.unwrap_or_else(|| current_iter.get_type().const_int(1, false));
        // An inclusive range may end at its type's maximum, which has no successor:
        // `i + 1` wraps to the minimum, `i <= end` admits it, and the loop never leaves.
        // A unit stride is checked the same way as a stepped one, so `end` itself exits.
        if stride.is_some() || inclusive {
            self.exit_when_stride_overshoots(current_iter, end_int, increment, inclusive, exit_bb)?;
        }
        let next_iter = self
            .builder
            .build_int_add(current_iter, increment, "for.next")
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
}
