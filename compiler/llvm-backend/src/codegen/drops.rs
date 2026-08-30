// Deterministic destruction (`Drop`): scope-exit destructor insertion.
//
// A binding of a `Drop` type runs its `{struct}__drop(&mut self)` destructor when
// its lexical scope ends on a *normal* exit — fall-through, `return`, `break`, or
// `continue` (a panic aborts without running destructors). Each owned
// binding carries an `i1` drop flag, set `false` when the value is moved out, so a
// moved value is not dropped twice. Every helper here is inert when the
// program declares no `Drop` types: the scope stack stays empty and nothing is
// emitted.

use ast_types::BinaryOp;
use inkwell::values::{BasicValueEnum, PointerValue};
use neuro_hir::{HirExpr, HirExprKind, HirType};

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

use super::context::{CodegenContext, DropEntry, DropTarget};

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

    /// Resolve how a binding of `binding_ty` is destroyed at scope exit, or `None`
    /// when it owns nothing that needs releasing. The HIR carries the binding's
    /// resolved type, so this reads it directly.
    pub(crate) fn drop_target(&self, binding_ty: &HirType) -> Option<DropTarget> {
        match Type::from_hir(binding_ty) {
            // A collection always owns a heap buffer, independently of whether the
            // program declares any user `Drop` type.
            Type::Collection { .. } => Some(DropTarget::Collection),
            Type::Struct(name) if self.drop_types.contains(&name) => {
                Some(DropTarget::UserDrop(name))
            }
            _ => None,
        }
    }

    /// Whether evaluating `expr` always yields a freshly `malloc`'d string buffer
    /// that nothing else aliases, making its consumer responsible for releasing it.
    ///
    /// Deliberately conservative: it answers `true` only for the two producers that
    /// allocate unconditionally. A `.rodata` literal, a variable, a `slice` borrowing
    /// its source, and a value returned by a function — which may have returned either
    /// a literal or a heap buffer, indistinguishably — all answer `false` and are never
    /// freed. The asymmetry is the point: a missed `true` leaks a buffer, while a wrong
    /// `true` frees `.rodata` or double-frees, so only provable ownership counts.
    pub(crate) fn produces_owned_string(expr: &HirExpr) -> bool {
        match &expr.kind {
            // `codegen_interp_string` concatenates every piece into one fresh buffer,
            // and does so unconditionally — even a hole-free interpolation allocates.
            HirExprKind::InterpString { .. } => true,
            // `+` yielding a `string` is `codegen_string_concat`, which always allocates
            // a `len1 + len2` buffer. No numeric addition produces a `string`, so the
            // result type alone identifies the concatenation.
            HirExprKind::Binary {
                op: BinaryOp::Add, ..
            } => matches!(Type::from_hir(&expr.ty), Type::String),
            _ => false,
        }
    }

    /// Release the heap buffer behind a `string` fat pointer held in `storage_ptr`.
    ///
    /// Emitted only for a binding registered as [`DropTarget::HeapString`], whose
    /// initializer [`produces_owned_string`] proved allocates.
    fn emit_heap_string_free(&mut self, storage_ptr: PointerValue<'ctx>) -> CodegenResult<()> {
        let fat_ptr_ty = self.string_fat_ptr_type();
        let buffer = self
            .builder
            .build_struct_gep(fat_ptr_ty, storage_ptr, 0, "str.drop.buf.addr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let buffer = self
            .builder
            .build_load(
                self.context.ptr_type(inkwell::AddressSpace::default()),
                buffer,
                "str.drop.buf",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let free_fn = self.get_or_declare_free();
        self.builder
            .build_call(free_fn, &[buffer.into()], "")
            .map_err(|e| CodegenError::LlvmError(format!("failed to free string: {}", e)))?;
        Ok(())
    }

    /// Record an owned `Drop`-typed binding for destruction at scope exit.
    ///
    /// Allocates the binding's `i1` drop flag (initialized `true`) and pushes a
    /// [`DropEntry`] onto the innermost scope. The caller must have verified the
    /// binding's type needs one via [`drop_target`].
    pub(crate) fn register_local_drop(
        &mut self,
        name: &str,
        storage_ptr: PointerValue<'ctx>,
        target: DropTarget,
    ) -> CodegenResult<()> {
        let bool_ty = self.context.bool_type();
        let flag_ptr = self.entry_alloca(bool_ty, "drop.flag")?;
        self.builder
            .build_store(flag_ptr, bool_ty.const_int(1, false))
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        if let Some(scope) = self.drop_scopes.last_mut() {
            scope.push(DropEntry {
                name: name.to_string(),
                storage_ptr,
                flag_ptr,
                target,
            });
        }
        Ok(())
    }

    /// Clear the drop flag of the place named by `expr` if it is a tracked `Drop`
    /// binding being moved out of. A non-identifier, or a binding that is not
    /// Drop-tracked, is a no-op. Mirrors the move sites the type checker validates.
    pub(crate) fn mark_moved_for_drop(&mut self, expr: &HirExpr) {
        if self.drop_scopes.is_empty() {
            return;
        }
        let HirExprKind::Variable(name) = &expr.kind else {
            return;
        };

        let flag_ptr = self
            .drop_scopes
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .find(|entry| &entry.name == name)
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
        let mut pending: Vec<(PointerValue<'ctx>, PointerValue<'ctx>, DropTarget)> = Vec::new();
        for scope in self.drop_scopes[min_index..].iter().rev() {
            for entry in scope.iter().rev() {
                pending.push((entry.storage_ptr, entry.flag_ptr, entry.target.clone()));
            }
        }
        for (storage_ptr, flag_ptr, target) in pending {
            self.emit_one_drop(storage_ptr, flag_ptr, &target)?;
        }
        Ok(())
    }

    /// Emit a single flag-guarded destructor:
    /// `if drop_flag { destroy(&storage); drop_flag = false }`.
    fn emit_one_drop(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
        flag_ptr: PointerValue<'ctx>,
        target: &DropTarget,
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
        match target {
            DropTarget::UserDrop(struct_name) => {
                let mangled = format!("{}__drop", struct_name);
                let drop_fn = *self
                    .functions
                    .get(&mangled)
                    .ok_or_else(|| CodegenError::UndefinedFunction(mangled.clone()))?;
                let receiver: BasicValueEnum<'ctx> = storage_ptr.into();
                self.builder
                    .build_call(drop_fn, &[receiver.into()], "")
                    .map_err(|e| {
                        CodegenError::LlvmError(format!("failed to build drop call: {}", e))
                    })?;
            }
            DropTarget::Collection => self.emit_collection_free(storage_ptr)?,
            DropTarget::HeapString => self.emit_heap_string_free(storage_ptr)?,
        }
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
