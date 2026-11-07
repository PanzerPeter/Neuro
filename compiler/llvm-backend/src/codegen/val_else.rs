// Codegen for the `val PATTERN = value else |binding| { ... }` statement.
//
// The scrutinee is evaluated once into an alloca and tested; a hit falls through into
// the success block with the pattern's bindings materialized there, a miss enters the
// else block with its own binding in scope. The frontend has verified the else block
// diverges, so its tail is `unreachable` and the success block is the only path that
// reaches the statements after this one, which is exactly why those statements may
// use the bindings unconditionally.

use neuro_hir::{HirExpr, HirMatchBinding, HirMatchTest, HirStmt};

use crate::codegen::context::CodegenContext;
use crate::codegen::drops::{UNBOUND_TEMPORARY, is_pure_place};
use crate::codegen::expressions::matches::ArmOwnership;
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
        let scrut_alloca = self.entry_alloca(scrut_val.get_type(), "valelse.scrut")?;
        self.builder.build_store(scrut_alloca, scrut_val)?;

        let else_bb = self.context.append_basic_block(parent_fn, "valelse.else");

        self.codegen_pattern_test(test, scrut_alloca, &scrut_sem, else_bb)?;
        let ok_bb = self.builder.get_insert_block().ok_or_else(|| {
            CodegenError::InternalError("val-else test left no block".to_string())
        })?;

        self.builder.position_at_end(else_bb);
        self.codegen_else_branch(
            else_binding,
            else_block,
            scrutinee,
            scrut_alloca,
            &scrut_sem,
        )?;

        self.builder.position_at_end(ok_bb);
        // A temporary has no binding left to own what the pattern takes out of it, so the
        // bindings do, exactly as a `match` arm's would. A place keeps its payload.
        let ownership = if is_pure_place(scrutinee) {
            ArmOwnership::EnclosingString { scrutinee }
        } else {
            ArmOwnership::Arm {
                scrutinee,
                shares: false,
            }
        };
        // Deliberately not restored: these bindings belong to the enclosing block and
        // must stay visible to every statement after this one.
        let _ = self.bind_arm(bindings, scrut_alloca, &scrut_sem, ownership)?;
        Ok(())
    }

    /// Emit the else branch in its own drop scope, with `else_binding` visible only
    /// inside it. The branch is known to diverge, so anything that still falls out of
    /// it is unreachable. A temporary scrutinee reaches this branch with no owner: its
    /// else binding takes the payload, and with none the scope destroys the whole value.
    fn codegen_else_branch(
        &mut self,
        else_binding: Option<&HirMatchBinding>,
        else_block: &[HirStmt],
        scrutinee: &HirExpr,
        scrut_alloca: inkwell::values::PointerValue<'ctx>,
        scrut_sem: &Type,
    ) -> CodegenResult<()> {
        let bound = match else_binding {
            Some(binding) => std::slice::from_ref(binding),
            None => &[],
        };
        let temporary = !is_pure_place(scrutinee);
        let ownership = if temporary {
            ArmOwnership::Arm {
                scrutinee,
                shares: false,
            }
        } else {
            ArmOwnership::Enclosing
        };

        self.push_drop_scope();
        let saved = self.bind_arm(bound, scrut_alloca, scrut_sem, ownership)?;
        if temporary && else_binding.is_none() {
            self.register_owned_binding(UNBOUND_TEMPORARY, scrut_alloca, scrut_sem)?;
        }
        for stmt in else_block {
            if self.current_block_terminated() {
                break;
            }
            self.codegen_stmt(stmt)?;
        }
        self.pop_drop_scope();

        if !self.current_block_terminated() {
            self.builder.build_unreachable()?;
        }

        self.restore_bindings(saved);
        Ok(())
    }
}
