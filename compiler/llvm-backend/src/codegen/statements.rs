use crate::codegen::context::LoopTargets;
use ast_types::*;
use inkwell::IntPredicate;

use crate::errors::{CodegenError, CodegenResult};
use crate::type_mapping::TypeMapper;
use crate::types::Type;

use super::context::CodegenContext;

impl<'ctx> CodegenContext<'ctx> {
    /// Generate code for a variable declaration statement.
    ///
    /// When `declared_ty` is `Some`, the alloca is created at that width and the
    /// initializer value is widened/truncated to match (e.g. `val x: i64 = 42`
    /// emits an i64 alloca with the literal sign-extended from i32).
    pub(crate) fn codegen_var_decl(
        &mut self,
        name: &str,
        declared_ty: Option<&ast_types::Type>,
        init: Option<&Expr>,
    ) -> CodegenResult<()> {
        let init_val = if let Some(expr) = init {
            Some(self.codegen_expr(expr)?)
        } else {
            None
        };

        if let Some(val) = init_val {
            let (alloca_ty, final_val) = if let Some(ast_ty) = declared_ty {
                let target_sem = crate::types::Type::from_ast(ast_ty);
                let llvm_target = self.type_mapper.map_type(&target_sem)?;
                let coerced = self.coerce_if_needed(val, llvm_target, &target_sem)?;
                (llvm_target, coerced)
            } else {
                (val.get_type(), val)
            };

            let alloca = self.builder.build_alloca(alloca_ty, name).map_err(|e| {
                CodegenError::LlvmError(format!("failed to allocate variable: {}", e))
            })?;
            self.builder.build_store(alloca, final_val).map_err(|e| {
                CodegenError::LlvmError(format!("failed to store initial value: {}", e))
            })?;
            self.variables.insert(name.to_string(), alloca);
            self.variable_types.insert(name.to_string(), alloca_ty);
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
                if matches!(target_sem, crate::types::Type::F64) {
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
            _ => Ok(val),
        }
    }

    /// Generate code for an assignment statement
    pub(crate) fn codegen_assignment(&self, name: &str, value: &Expr) -> CodegenResult<()> {
        // Generate code for the value expression
        let val = self.codegen_expr(value)?;

        // Lookup the variable pointer (must already exist)
        let var_ptr = self
            .variables
            .get(name)
            .ok_or_else(|| CodegenError::UndefinedVariable(name.to_string()))?;

        // Store the new value into the variable
        self.builder.build_store(*var_ptr, val).map_err(|e| {
            CodegenError::LlvmError(format!("failed to store value in assignment: {}", e))
        })?;

        Ok(())
    }

    /// Generate code for a return statement
    pub(crate) fn codegen_return(&self, value: Option<&Expr>) -> CodegenResult<()> {
        if let Some(expr) = value {
            let ret_val = self.codegen_expr(expr)?;
            self.builder
                .build_return(Some(&ret_val))
                .map_err(|e| CodegenError::LlvmError(format!("failed to build return: {}", e)))?;
        } else {
            self.builder.build_return(None).map_err(|e| {
                CodegenError::LlvmError(format!("failed to build void return: {}", e))
            })?;
        }
        Ok(())
    }

    /// Generate code for an if/else statement
    pub(crate) fn codegen_if(
        &mut self,
        condition: &Expr,
        then_block: &[Stmt],
        else_if_blocks: &[(Expr, Vec<Stmt>)],
        else_block: &Option<Vec<Stmt>>,
    ) -> CodegenResult<()> {
        let cond_val = self.codegen_expr(condition)?;

        let parent_fn = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("if statement outside function".to_string())
        })?;

        let then_bb = self.context.append_basic_block(parent_fn, "then");
        let else_bb = self.context.append_basic_block(parent_fn, "else");
        let merge_bb = self.context.append_basic_block(parent_fn, "ifcont");

        // Build conditional branch
        self.builder
            .build_conditional_branch(cond_val.into_int_value(), then_bb, else_bb)
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to build conditional branch: {}", e))
            })?;

        // Generate then block
        self.builder.position_at_end(then_bb);
        for stmt in then_block {
            self.codegen_stmt(stmt)?;
        }
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
            for stmt in else_stmts {
                self.codegen_stmt(stmt)?;
            }
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

    /// Generate code for a while statement
    pub(crate) fn codegen_while(&mut self, condition: &Expr, body: &[Stmt]) -> CodegenResult<()> {
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
        self.loop_targets.push(LoopTargets {
            continue_bb: cond_bb,
            break_bb: exit_bb,
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

    /// Generate code for a for-range statement (`for i in start..end { ... }`).
    pub(crate) fn codegen_for_range(
        &mut self,
        iterator: &shared_types::Identifier,
        start: &Expr,
        end: &Expr,
        inclusive: bool,
        body: &[Stmt],
    ) -> CodegenResult<()> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;

        let start_val = self.codegen_expr(start)?;
        let end_val = self.codegen_expr(end)?;
        let iter_name = iterator.name.clone();

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

        let iter_sem_ty = self
            .expr_types
            .get(&start.span().start)
            .cloned()
            .unwrap_or(Type::I32);
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
        self.loop_targets.push(LoopTargets {
            continue_bb: step_bb,
            break_bb: exit_bb,
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
    pub(crate) fn codegen_stmt(&mut self, stmt: &Stmt) -> CodegenResult<()> {
        match stmt {
            Stmt::VarDecl { name, ty, init, .. } => {
                self.codegen_var_decl(&name.name, ty.as_ref(), init.as_ref())
            }
            Stmt::Assignment { target, value, .. } => self.codegen_assignment(&target.name, value),
            Stmt::Return { value, .. } => self.codegen_return(value.as_ref()),
            Stmt::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => self.codegen_if(condition, then_block, else_if_blocks, else_block),
            Stmt::While {
                condition, body, ..
            } => self.codegen_while(condition, body),
            Stmt::ForRange {
                iterator,
                start,
                end,
                inclusive,
                body,
                ..
            } => self.codegen_for_range(iterator, start, end, *inclusive, body),
            Stmt::Break { .. } => {
                let targets = self.loop_targets.last().ok_or_else(|| {
                    CodegenError::InternalError(
                        "break used outside loop during codegen".to_string(),
                    )
                })?;

                if let Some(current_bb) = self.builder.get_insert_block() {
                    if current_bb.get_terminator().is_none() {
                        self.builder
                            .build_unconditional_branch(targets.break_bb)
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
            Stmt::Continue { .. } => {
                let targets = self.loop_targets.last().ok_or_else(|| {
                    CodegenError::InternalError(
                        "continue used outside loop during codegen".to_string(),
                    )
                })?;

                if let Some(current_bb) = self.builder.get_insert_block() {
                    if current_bb.get_terminator().is_none() {
                        self.builder
                            .build_unconditional_branch(targets.continue_bb)
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
            Stmt::FieldAssignment {
                object,
                field,
                value,
                ..
            } => self.codegen_field_assignment(&object.name, &field.name, value),

            Stmt::Const { name, ty, value, .. } => {
                let declared_sem = crate::types::Type::from_ast(ty);
                let val = self.codegen_const_expr_typed(value, &declared_sem)?;
                self.const_values.insert(name.name.clone(), val);
                Ok(())
            }

            Stmt::Expr(expr) => {
                self.codegen_expr(expr)?;
                Ok(())
            }
        }
    }

    /// Emit a module-level constant as an LLVM global constant and cache its value.
    pub(crate) fn codegen_global_const(&mut self, def: &ast_types::ConstDef) -> CodegenResult<()> {
        let declared_sem = crate::types::Type::from_ast(&def.ty);
        let val = self.codegen_const_expr_typed(&def.value, &declared_sem)?;
        let llvm_ty = val.get_type();
        let global = self.module.add_global(llvm_ty, None, &def.name.name);
        global.set_initializer(&val);
        global.set_constant(true);
        global.set_linkage(inkwell::module::Linkage::Internal);

        // Cache the value directly so identifier resolution returns the constant without
        // emitting a load — consts are values, not memory locations.
        self.const_values.insert(def.name.name.clone(), val);
        Ok(())
    }
}
