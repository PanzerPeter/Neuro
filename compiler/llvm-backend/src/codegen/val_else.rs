// Codegen for the `val PATTERN = value else |binding| { ... }` statement.
//
// The scrutinee is evaluated once into an alloca and tested; a hit falls through into
// the success block with the pattern's bindings materialized there, a miss enters the
// else block with its own binding in scope. The frontend has verified the else block
// diverges, so its tail is `unreachable` and the success block is the only path that
// reaches the statements after this one — which is exactly why those statements may
// use the bindings unconditionally.

use neuro_hir::{HirExpr, HirMatchBinding, HirMatchTest, HirStmt};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

impl<'ctx> CodegenContext<'ctx> {
    /// Lower a `val-else` statement, leaving the builder positioned in its success
    /// block with `bindings` live.
    pub(crate) fn codegen_val_else(
        &mut self,
        scrutinee: &HirExpr,
        test: &HirMatchTest,
        bindings: &[HirMatchBinding],
        else_binding: Option<&HirMatchBinding>,
        else_block: &[HirStmt],
    ) -> CodegenResult<()> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("val-else outside function".to_string()))?;

        let scrut_sem = Type::from_hir(&scrutinee.ty);
        let scrut_val = self.codegen_expr(scrutinee)?;
        let scrut_llvm = scrut_val.get_type();
        let scrut_alloca = self
            .builder
            .build_alloca(scrut_llvm, "valelse.scrut")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(scrut_alloca, scrut_val)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let ok_bb = self.context.append_basic_block(parent_fn, "valelse.ok");
        let else_bb = self.context.append_basic_block(parent_fn, "valelse.else");

        let matched = self.codegen_single_test(test, scrut_alloca, scrut_llvm, &scrut_sem)?;
        self.builder
            .build_conditional_branch(matched, ok_bb, else_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(else_bb);
        self.codegen_else_branch(
            else_binding,
            else_block,
            scrut_alloca,
            scrut_llvm,
            &scrut_sem,
        )?;

        self.builder.position_at_end(ok_bb);
        // Deliberately not restored: these bindings belong to the enclosing block and
        // must stay visible to every statement after this one.
        let _ = self.bind_arm(bindings, scrut_alloca, scrut_llvm, &scrut_sem)?;
        Ok(())
    }

    /// Emit the else branch in its own drop scope, with `else_binding` visible only
    /// inside it. The branch is known to diverge, so anything that still falls out of
    /// it is unreachable.
    fn codegen_else_branch(
        &mut self,
        else_binding: Option<&HirMatchBinding>,
        else_block: &[HirStmt],
        scrut_alloca: inkwell::values::PointerValue<'ctx>,
        scrut_llvm: inkwell::types::BasicTypeEnum<'ctx>,
        scrut_sem: &Type,
    ) -> CodegenResult<()> {
        let bound = match else_binding {
            Some(binding) => std::slice::from_ref(binding),
            None => &[],
        };
        let saved = self.bind_arm(bound, scrut_alloca, scrut_llvm, scrut_sem)?;

        self.push_drop_scope();
        for stmt in else_block {
            if self.current_block_terminated() {
                break;
            }
            self.codegen_stmt(stmt)?;
        }
        self.pop_drop_scope();

        if !self.current_block_terminated() {
            self.builder
                .build_unreachable()
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        }

        self.restore_bindings(saved);
        Ok(())
    }
}
