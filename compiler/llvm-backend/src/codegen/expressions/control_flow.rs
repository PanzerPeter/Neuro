// Codegen for expressions: Expression-position control flow (if-expressions, block expressions).

use inkwell::basic_block::BasicBlock;
use inkwell::values::*;
use neuro_hir::{HirExpr, HirStmt};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

impl<'ctx> CodegenContext<'ctx> {
    /// Codegen a value-producing if-expression using an alloca result slot.
    ///
    /// `result_ty` is the if-expression's resolved type — `expr.ty` for a value
    /// position `if`, or the function return type when a tail `if` is the implicit
    /// return. `Void` selects the statement form (no result slot).
    pub(crate) fn codegen_if_expr(
        &mut self,
        condition: &HirExpr,
        then_block: &[HirStmt],
        else_if_blocks: &[(HirExpr, Vec<HirStmt>)],
        else_block: &Option<Vec<HirStmt>>,
        result_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let result_ty = result_ty.clone();

        if matches!(result_ty, Type::Void) {
            self.codegen_if(condition, then_block, else_if_blocks, else_block)?;
            return Ok(self.context.i32_type().const_int(0, false).into());
        }

        let parent_fn = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("if-expression outside function".to_string())
        })?;

        let llvm_result_ty = self.get_any_llvm_type(&result_ty)?;
        let result_alloca = self
            .builder
            .build_alloca(llvm_result_ty, "ifexpr.result")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let cond_val = self.codegen_expr(condition)?.into_int_value();
        let then_bb = self.context.append_basic_block(parent_fn, "ifexpr.then");
        let else_bb = self.context.append_basic_block(parent_fn, "ifexpr.else");
        let merge_bb = self.context.append_basic_block(parent_fn, "ifexpr.merge");

        self.builder
            .build_conditional_branch(cond_val, then_bb, else_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(then_bb);
        self.codegen_arm_into_alloca(then_block, result_alloca)?;
        if !self.current_block_terminated() {
            self.builder
                .build_unconditional_branch(merge_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        }

        self.builder.position_at_end(else_bb);
        self.codegen_if_expr_else_arm(else_if_blocks, else_block, result_alloca, merge_bb)?;

        self.builder.position_at_end(merge_bb);
        self.builder
            .build_load(llvm_result_ty, result_alloca, "ifexpr.val")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// Recursively emit the else/elif arm of an if-expression, storing the result into `alloca`.
    fn codegen_if_expr_else_arm(
        &mut self,
        else_if_blocks: &[(HirExpr, Vec<HirStmt>)],
        else_block: &Option<Vec<HirStmt>>,
        alloca: PointerValue<'ctx>,
        merge_bb: BasicBlock<'ctx>,
    ) -> CodegenResult<()> {
        if let Some(((elif_cond, elif_stmts), rest)) = else_if_blocks.split_first() {
            let parent_fn = self
                .current_function
                .ok_or_else(|| CodegenError::InternalError("elif outside function".to_string()))?;
            let elif_cond_val = self.codegen_expr(elif_cond)?.into_int_value();
            let elif_then_bb = self
                .context
                .append_basic_block(parent_fn, "ifexpr.elif.then");
            let elif_else_bb = self
                .context
                .append_basic_block(parent_fn, "ifexpr.elif.else");

            self.builder
                .build_conditional_branch(elif_cond_val, elif_then_bb, elif_else_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            self.builder.position_at_end(elif_then_bb);
            self.codegen_arm_into_alloca(elif_stmts, alloca)?;
            if !self.current_block_terminated() {
                self.builder
                    .build_unconditional_branch(merge_bb)
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            }

            self.builder.position_at_end(elif_else_bb);
            self.codegen_if_expr_else_arm(rest, else_block, alloca, merge_bb)?;
        } else if let Some(else_stmts) = else_block {
            self.codegen_arm_into_alloca(else_stmts, alloca)?;
            if !self.current_block_terminated() {
                self.builder
                    .build_unconditional_branch(merge_bb)
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            }
        } else {
            self.builder
                .build_unconditional_branch(merge_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        }
        Ok(())
    }

    /// Codegen all stmts in an arm; store the last `Stmt::Expr`'s value into `alloca`.
    ///
    /// The arm is its own drop scope: locals declared in it are destroyed before
    /// control reaches the merge block, while a yielded place escapes (its drop flag
    /// is cleared as a move).
    fn codegen_arm_into_alloca(
        &mut self,
        stmts: &[HirStmt],
        alloca: PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        self.push_drop_scope();
        let Some((last, init)) = stmts.split_last() else {
            self.pop_drop_scope();
            return Ok(());
        };
        for stmt in init {
            if self.current_block_terminated() {
                break;
            }
            self.codegen_stmt(stmt)?;
        }
        if !self.current_block_terminated() {
            if let HirStmt::Expr(expr) = last {
                let val = self.codegen_expr(expr)?;
                // A diverging arm value (e.g. `else { panic("x") }`) terminates the block
                // with `unreachable`; there is no result to store and the caller skips merge.
                if !self.current_block_terminated() {
                    self.mark_moved_for_drop(expr);
                    self.builder
                        .build_store(alloca, val)
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                }
            } else {
                self.codegen_stmt(last)?;
            }
        }
        if !self.current_block_terminated() {
            self.emit_top_scope_drops()?;
        }
        self.pop_drop_scope();
        Ok(())
    }

    /// Codegen a block expression: run stmts, return the last `Stmt::Expr`'s value.
    pub(crate) fn codegen_block_expr(
        &mut self,
        stmts: &[HirStmt],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        self.push_drop_scope();
        let Some((last, init)) = stmts.split_last() else {
            self.pop_drop_scope();
            return Ok(self.context.i32_type().const_int(0, false).into());
        };
        for stmt in init {
            if self.current_block_terminated() {
                break;
            }
            self.codegen_stmt(stmt)?;
        }
        let result = if let HirStmt::Expr(expr) = last {
            let val = self.codegen_expr(expr)?;
            // The yielded place escapes the block, so it is moved out, not dropped here.
            self.mark_moved_for_drop(expr);
            val
        } else {
            if !self.current_block_terminated() {
                self.codegen_stmt(last)?;
            }
            self.context.i32_type().const_int(0, false).into()
        };
        if !self.current_block_terminated() {
            self.emit_top_scope_drops()?;
        }
        self.pop_drop_scope();
        Ok(result)
    }
}
