// NEURO Programming Language - LLVM Backend
// Codegen for expressions: Expression-position control flow (if-expressions, block expressions).

use ast_types::*;
use inkwell::basic_block::BasicBlock;
use inkwell::values::*;

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

impl<'ctx> CodegenContext<'ctx> {
    /// Codegen a value-producing if-expression using an alloca result slot.
    pub(crate) fn codegen_if_expr(
        &mut self,
        condition: &Expr,
        then_block: &[Stmt],
        else_if_blocks: &[(Expr, Vec<Stmt>)],
        else_block: &Option<Vec<Stmt>>,
        span: &shared_types::Span,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let result_ty = self
            .expr_types
            .get(&span.start)
            .cloned()
            .unwrap_or(Type::Void);

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
        else_if_blocks: &[(Expr, Vec<Stmt>)],
        else_block: &Option<Vec<Stmt>>,
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
    fn codegen_arm_into_alloca(
        &mut self,
        stmts: &[Stmt],
        alloca: PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        let Some((last, init)) = stmts.split_last() else {
            return Ok(());
        };
        for stmt in init {
            self.codegen_stmt(stmt)?;
        }
        if let Stmt::Expr(expr) = last {
            let val = self.codegen_expr(expr)?;
            // A diverging arm value (e.g. `else { panic("x") }`) terminates the block with
            // `unreachable`; there is no result to store and the caller skips the merge branch.
            if !self.current_block_terminated() {
                self.builder
                    .build_store(alloca, val)
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            }
        } else {
            self.codegen_stmt(last)?;
        }
        Ok(())
    }

    /// Codegen a block expression: run stmts, return the last `Stmt::Expr`'s value.
    pub(crate) fn codegen_block_expr(
        &mut self,
        stmts: &[Stmt],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let Some((last, init)) = stmts.split_last() else {
            return Ok(self.context.i32_type().const_int(0, false).into());
        };
        for stmt in init {
            self.codegen_stmt(stmt)?;
        }
        if let Stmt::Expr(expr) = last {
            self.codegen_expr(expr)
        } else {
            self.codegen_stmt(last)?;
            Ok(self.context.i32_type().const_int(0, false).into())
        }
    }
}
