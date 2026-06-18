// Neuro Programming Language - LLVM Backend
// Deterministic destruction (`Drop`, §2.1): scope-exit destructor insertion.
//
// A binding of a `Drop` type runs its `{struct}__drop(&mut self)` destructor when
// its lexical scope ends on a *normal* exit — fall-through, `return`, `break`, or
// `continue` (§1.2: a panic aborts without running destructors). Each owned
// binding carries an `i1` drop flag, set `false` when the value is moved out, so a
// moved value is not dropped twice (§2.2). Every helper here is inert when the
// program declares no `Drop` types: the scope stack stays empty and nothing is
// emitted.

use ast_types::Expr;
use inkwell::values::{BasicValueEnum, PointerValue};

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

use super::context::{CodegenContext, DropEntry};

impl<'ctx> CodegenContext<'ctx> {
    /// Open a new lexical drop scope. Paired with [`pop_drop_scope`].
    pub(crate) fn push_drop_scope(&mut self) {
        self.drop_scopes.push(Vec::new());
    }

    /// Close the innermost drop scope without emitting drops. Drops for a scope are
    /// emitted explicitly (see [`emit_top_scope_drops`] / [`emit_drops_through`])
    /// before the scope is popped, so this only discards the bookkeeping.
    pub(crate) fn pop_drop_scope(&mut self) {
        let _ = self.drop_scopes.pop();
    }

    /// Resolve the `Drop`-type struct name a binding holds, or `None` if the binding
    /// is not a Drop type. Prefers the declared annotation; otherwise reads the
    /// initializer's recorded type. Only struct names present in `drop_types` match,
    /// so this returns `None` for every program without Drop types.
    pub(crate) fn drop_struct_name(
        &self,
        declared_ty: Option<&ast_types::Type>,
        init: Option<&Expr>,
    ) -> Option<String> {
        if self.drop_types.is_empty() {
            return None;
        }
        let resolved = if let Some(decl) = declared_ty {
            Some(Type::from_ast(decl))
        } else {
            init.and_then(|e| self.expr_types.get(&e.span().start).cloned())
        };
        match resolved {
            Some(Type::Struct(name)) if self.drop_types.contains(&name) => Some(name),
            _ => None,
        }
    }

    /// Record an owned `Drop`-typed binding for destruction at scope exit (§2.1).
    ///
    /// Allocates the binding's `i1` drop flag (initialized `true`) and pushes a
    /// [`DropEntry`] onto the innermost scope. The caller must have verified the
    /// binding's type is a Drop type via [`drop_struct_name`].
    pub(crate) fn register_local_drop(
        &mut self,
        name: &str,
        storage_ptr: PointerValue<'ctx>,
        struct_name: String,
    ) -> CodegenResult<()> {
        let bool_ty = self.context.bool_type();
        let flag_ptr = self
            .builder
            .build_alloca(bool_ty, "drop.flag")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(flag_ptr, bool_ty.const_int(1, false))
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        if let Some(scope) = self.drop_scopes.last_mut() {
            scope.push(DropEntry {
                name: name.to_string(),
                storage_ptr,
                flag_ptr,
                struct_name,
            });
        }
        Ok(())
    }

    /// Clear the drop flag of the place named by `expr` if it is a tracked `Drop`
    /// binding being moved out of (§2.2). A non-identifier, or a binding that is not
    /// Drop-tracked, is a no-op. Mirrors the move sites the type checker validates.
    pub(crate) fn mark_moved_for_drop(&mut self, expr: &Expr) {
        if self.drop_scopes.is_empty() {
            return;
        }
        let mut place = expr;
        while let Expr::Paren(inner, _) = place {
            place = inner;
        }
        let Expr::Identifier(ident) = place else {
            return;
        };

        let flag_ptr = self
            .drop_scopes
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .find(|entry| entry.name == ident.name)
            .map(|entry| entry.flag_ptr);

        if let Some(flag_ptr) = flag_ptr {
            let _ = self
                .builder
                .build_store(flag_ptr, self.context.bool_type().const_zero());
        }
    }

    /// Emit the destructor calls for the innermost scope, in reverse declaration
    /// order, then leave the scope in place (the caller pops it). Used at the normal
    /// fall-through end of a lexical block.
    pub(crate) fn emit_top_scope_drops(&mut self) -> CodegenResult<()> {
        let depth = self.drop_scopes.len();
        if depth == 0 {
            return Ok(());
        }
        self.emit_drops_through(depth - 1)
    }

    /// Emit destructor calls for every open scope from the innermost down to and
    /// including `min_index`, in LIFO order, without popping any scope. Used at
    /// `return` (`min_index = 0`) and at `break`/`continue` (the loop's body scope).
    pub(crate) fn emit_drops_through(&mut self, min_index: usize) -> CodegenResult<()> {
        if min_index >= self.drop_scopes.len() {
            return Ok(());
        }
        // Snapshot the entries first so the destructor calls below can borrow `self`
        // mutably without aliasing the scope stack. Innermost scope first, reverse
        // declaration order within each scope.
        let mut pending: Vec<(PointerValue<'ctx>, PointerValue<'ctx>, String)> = Vec::new();
        for scope in self.drop_scopes[min_index..].iter().rev() {
            for entry in scope.iter().rev() {
                pending.push((entry.storage_ptr, entry.flag_ptr, entry.struct_name.clone()));
            }
        }
        for (storage_ptr, flag_ptr, struct_name) in pending {
            self.emit_one_drop(storage_ptr, flag_ptr, &struct_name)?;
        }
        Ok(())
    }

    /// Emit a single flag-guarded destructor call:
    /// `if drop_flag { struct__drop(&storage); drop_flag = false }`.
    fn emit_one_drop(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
        flag_ptr: PointerValue<'ctx>,
        struct_name: &str,
    ) -> CodegenResult<()> {
        if self.current_block_terminated() {
            return Ok(());
        }
        let parent_fn = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("drop emitted outside function".to_string())
        })?;

        let bool_ty = self.context.bool_type();
        let flag = self
            .builder
            .build_load(bool_ty, flag_ptr, "drop.flag.load")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();

        let run_bb = self.context.append_basic_block(parent_fn, "drop.run");
        let cont_bb = self.context.append_basic_block(parent_fn, "drop.cont");
        self.builder
            .build_conditional_branch(flag, run_bb, cont_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(run_bb);
        let mangled = format!("{}__drop", struct_name);
        let drop_fn = *self
            .functions
            .get(&mangled)
            .ok_or_else(|| CodegenError::UndefinedFunction(mangled.clone()))?;
        let receiver: BasicValueEnum<'ctx> = storage_ptr.into();
        self.builder
            .build_call(drop_fn, &[receiver.into()], "")
            .map_err(|e| CodegenError::LlvmError(format!("failed to build drop call: {}", e)))?;
        // Clear the flag so a re-reachable drop site cannot run the destructor twice.
        self.builder
            .build_store(flag_ptr, bool_ty.const_zero())
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_unconditional_branch(cont_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(cont_bb);
        Ok(())
    }
}
