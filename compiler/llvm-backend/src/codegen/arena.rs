// The linear arena behind a `pool { }` block.
//
// A pool block allocates by moving a bump pointer and frees by moving it back, so
// its exit costs one store no matter how many objects the body built. Two module
// globals hold the whole allocator: the chunk's base address, reserved on the first
// `pool` a run reaches, and the offset of the first free byte in it. Entering a block
// reads the offset (its *mark*); leaving writes it back, which is the bulk free.
// Nesting needs nothing more, because a nested block's mark is simply a larger offset.
//
// Every release in the program goes through a wrapper that returns without calling
// libc when the pointer lies inside the chunk: arena memory is reclaimed by the mark
// restore, and handing it to `free` would corrupt the heap. The wrapper is what lets
// the ordinary drop path stay exactly as it is inside a pool. In a program with no
// `pool` the base global is never written, so the check folds away.
//
// What the arena does NOT capture is as important as what it does. An allocation made by
// a callee belongs to whoever the callee gives it to, which this phase cannot prove, so
// it stays an ordinary heap allocation — slower, and never unsafe. Nor does the arena
// capture a store whose place outlives the block: the language routes that allocation to the
// heap, because the value is still reachable after the mark goes back. `route_store_off_arena`
// is where the block's own bindings are told apart from the ones it inherited.

use inkwell::module::Linkage;
use inkwell::values::{FunctionValue, GlobalValue, IntValue, PointerValue};
use inkwell::{AddressSpace, IntPredicate};
use neuro_hir::HirPlace;

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

use super::context::{CodegenContext, DropTarget};

/// Bytes reserved for the arena the first time a run enters a `pool`.
///
/// One chunk serves the whole program: it is reserved lazily and never released, so
/// the cost of overshooting is untouched address space rather than resident memory.
/// The figure covers a handful of the 1024x1024 `f32` tensors a training step allocates;
/// a block that outgrows it keeps working, one heap allocation at a time.
const ARENA_CAPACITY: u64 = 64 * 1024 * 1024;

/// Alignment given to a plain (non-over-aligned) arena allocation. Matches the
/// guarantee libc `malloc` makes, so a buffer that moves between the two paths is
/// aligned the same either way.
const ARENA_MIN_ALIGN: u64 = 16;

const ARENA_BASE_GLOBAL: &str = "__neuro_arena_base";
const ARENA_OFFSET_GLOBAL: &str = "__neuro_arena_offset";
const ARENA_MARK_FN: &str = "__neuro_arena_mark";
const ARENA_RELEASE_FN: &str = "__neuro_arena_release";
const ARENA_ALLOC_FN: &str = "__neuro_arena_alloc";
const ARENA_ALIGNED_ALLOC_FN: &str = "__neuro_arena_aligned_alloc";
const RELEASE_FN: &str = "__neuro_release";
const ALIGNED_RELEASE_FN: &str = "__neuro_aligned_release";
const POOL_HEAD_GLOBAL: &str = "__neuro_pool_head";
const POOL_REGISTER_FN: &str = "__neuro_pool_register";
const POOL_SWEEP_FN: &str = "__neuro_pool_sweep";

/// Fields of a registration cell: the previously registered cell, the instance, its
/// `bulk_release` thunk, and the instance's drop flag. Pushing onto the head is what
/// makes the list reverse-registration order the moment it is walked forward.
const CELL_NEXT: u32 = 0;
const CELL_INSTANCE: u32 = 1;
const CELL_RELEASE: u32 = 2;
const CELL_FLAG: u32 = 3;
const CELL_FIELDS: u64 = 4;

/// The prelude struct `register_with_pool` names. Matched here by name, the way the
/// trait itself is: a program that shadows it withdraws the opt-in with it.
const POOL_HANDLE_STRUCT: &str = "PoolHandle";

impl<'ctx> CodegenContext<'ctx> {
    /// Record `name` as a binding of the innermost open `pool`, so a later store into it
    /// keeps the bump path. Inert outside a pool.
    pub(crate) fn note_pool_local(&mut self, name: &str) {
        if let Some(locals) = self.pool_locals.last_mut() {
            locals.insert(name.to_string());
        }
    }

    /// Emit `store` with the arena switched off when `place` reaches storage that
    /// outlives the innermost open `pool`.
    ///
    /// The language rule: an allocation reachable from a value that outlives the block does not come
    /// from the arena, because the mark restore at the closing brace would leave it
    /// dangling. The whole store is emitted at depth zero rather than just its
    /// right-hand side: an address computation that allocates is reachable from the same
    /// place, and a `PoolAware` value stored this way must take its own `Drop` rather
    /// than the block's sweep. Inert outside a pool, and inert for a store into one of
    /// the block's own bindings, which the arena still holds.
    pub(crate) fn store_outside_pool<T>(
        &mut self,
        place: &HirPlace,
        store: impl FnOnce(&mut Self) -> CodegenResult<T>,
    ) -> CodegenResult<T> {
        if !self.route_store_off_arena(place) {
            return store(self);
        }
        let depth = std::mem::replace(&mut self.pool_depth, 0);
        let result = store(self);
        self.pool_depth = depth;
        result
    }

    /// Whether a store into `place` must bypass the arena. True for every place not
    /// rooted in a binding the innermost open `pool` declared itself, which is the
    /// conservative half of the routing rule: an owner the analysis cannot place inside the
    /// block is treated as outliving it.
    fn route_store_off_arena(&self, place: &HirPlace) -> bool {
        let Some(locals) = self.pool_locals.last() else {
            return false;
        };
        !Self::place_root(place).is_some_and(|root| locals.contains(root))
    }

    /// The allocator the code being emitted right now must call for a plain buffer:
    /// the arena inside a `pool` body, libc `malloc` outside one. Same signature
    /// either way, so a call site needs no branch of its own.
    pub(crate) fn alloc_fn(&self) -> CodegenResult<FunctionValue<'ctx>> {
        if self.pool_depth == 0 {
            return Ok(self.get_or_declare_malloc());
        }
        self.get_or_build_arena_alloc()
    }

    /// The over-aligned counterpart of [`alloc_fn`](CodegenContext::alloc_fn). Both
    /// take the host libc's argument order, which `codegen/dlpack.rs` builds.
    pub(crate) fn aligned_alloc_fn(&self) -> CodegenResult<FunctionValue<'ctx>> {
        if self.pool_depth == 0 {
            return Ok(self.get_or_declare_aligned_alloc());
        }
        self.get_or_build_arena_aligned_alloc()
    }

    /// The release paired with [`alloc_fn`](CodegenContext::alloc_fn). Unlike the
    /// allocator it does not depend on where it is emitted: a buffer allocated inside a
    /// pool can be released from anywhere, and only the pointer says which allocator
    /// owns it.
    pub(crate) fn release_fn(&self) -> CodegenResult<FunctionValue<'ctx>> {
        self.get_or_build_release(RELEASE_FN, self.get_or_declare_free())
    }

    /// The release paired with
    /// [`aligned_alloc_fn`](CodegenContext::aligned_alloc_fn). Kept distinct from
    /// [`release_fn`](CodegenContext::release_fn) because the two libc allocators do
    /// not share a release on Windows.
    pub(crate) fn aligned_release_fn(&self) -> CodegenResult<FunctionValue<'ctx>> {
        self.get_or_build_release(ALIGNED_RELEASE_FN, self.get_or_declare_aligned_free())
    }

    /// Enter a pool region: reserve the chunk if this is the program's first `pool`,
    /// and read the mark that [`emit_arena_release`](CodegenContext::emit_arena_release)
    /// restores.
    pub(crate) fn emit_arena_mark(&self) -> CodegenResult<IntValue<'ctx>> {
        let mark = self.get_or_build_arena_mark()?;
        let call = self
            .builder
            .build_call(mark, &[], "pool.mark")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        call.try_as_basic_value()
            .basic()
            .map(|v| v.into_int_value())
            .ok_or_else(|| CodegenError::InternalError("arena mark returned void".into()))
    }

    /// Leave a pool region, releasing everything the block allocated in one store.
    pub(crate) fn emit_arena_release(&self, mark: IntValue<'ctx>) -> CodegenResult<()> {
        let release = self.get_or_build_arena_release()?;
        self.builder
            .build_call(release, &[mark.into()], "")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Read the registration list's head, which a `pool` body saves on entry and hands
    /// back to [`emit_pool_sweep`](CodegenContext::emit_pool_sweep) as the point the
    /// sweep must stop at. A nested block therefore releases its own registrations and
    /// leaves the enclosing block's alone.
    pub(crate) fn emit_pool_head(&self) -> CodegenResult<PointerValue<'ctx>> {
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        Ok(self
            .builder
            .build_load(ptr_type, self.pool_head().as_pointer_value(), "pool.head")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value())
    }

    /// Push `instance` onto the registration list of the innermost open `pool`.
    ///
    /// `release` is the instance's `bulk_release` thunk and `flag` its drop flag, so a
    /// value moved out of the binding before the block ends is swept once, from
    /// whichever binding still owns it.
    pub(crate) fn emit_pool_register(
        &self,
        instance: PointerValue<'ctx>,
        release: FunctionValue<'ctx>,
        flag: PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        let register = self.get_or_build_pool_register()?;
        self.builder
            .build_call(
                register,
                &[
                    instance.into(),
                    release.as_global_value().as_pointer_value().into(),
                    flag.into(),
                ],
                "",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Run `bulk_release` over everything registered since `stop`, newest first, and
    /// unlink each cell as it goes. Emitted immediately before the mark restore: the
    /// cells themselves live in the arena, so the restore reclaims them too.
    pub(crate) fn emit_pool_sweep(&self, stop: PointerValue<'ctx>) -> CodegenResult<()> {
        let sweep = self.get_or_build_pool_sweep()?;
        self.builder
            .build_call(sweep, &[stop.into()], "")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// The struct a `pool` body registers its `PoolAware` bindings through, when one is
    /// open and `struct_name` opts in. `None` leaves the binding on the ordinary drop
    /// path, which is what every type outside a pool takes.
    pub(crate) fn pool_registered_type(&self, ty: &Type) -> Option<String> {
        if self.pool_depth == 0 {
            return None;
        }
        let Type::Struct(name) = ty else {
            return None;
        };
        self.pool_aware_types.contains(name).then(|| name.clone())
    }

    /// Bind `name` to the arena instead of to its own scope exit: call the type's
    /// `register_with_pool` with the active arena's handle, then push the instance onto
    /// the registration list the block's closing brace sweeps in reverse.
    ///
    /// For a value the block constructs. A value moved into `name` from a binding the
    /// block already registered takes
    /// [`transfer_pool_registration`](CodegenContext::transfer_pool_registration).
    pub(crate) fn register_pool_aware(
        &mut self,
        name: &str,
        struct_name: &str,
        storage_ptr: PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        let registrar = self.pool_aware_method(struct_name, "register_with_pool")?;
        let handle = self.emit_pool_handle()?;
        // `register_with_pool` takes `&self`, which this backend passes by value; only
        // the `&mut self` of `bulk_release` arrives as a pointer.
        let receiver = self
            .builder
            .build_load(
                self.get_struct_llvm_type(struct_name)?,
                storage_ptr,
                "pool.receiver",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_call(registrar, &[receiver.into(), handle.into()], "")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.transfer_pool_registration(name, struct_name, storage_ptr)
    }

    /// Put `name` on the registration list without announcing a new resource.
    ///
    /// A move hands the same external resource to a new binding, and the language has
    /// `register_with_pool` called by the *constructor*, so running it again here would
    /// register one resource twice. The list entry is still needed: the source binding's
    /// flag was cleared by the move, so this is the binding the sweep must release, and
    /// its position in the list is where ownership actually ended up.
    pub(crate) fn transfer_pool_registration(
        &mut self,
        name: &str,
        struct_name: &str,
        storage_ptr: PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        let release = self.pool_aware_method(struct_name, "bulk_release")?;
        let flag = self.register_local_drop(name, storage_ptr, DropTarget::PoolRegistered)?;
        self.emit_pool_register(storage_ptr, release, flag)
    }

    fn pool_aware_method(
        &self,
        struct_name: &str,
        method: &str,
    ) -> CodegenResult<FunctionValue<'ctx>> {
        let mangled = format!("{struct_name}__{method}");
        self.functions
            .get(&mangled)
            .copied()
            .ok_or(CodegenError::UndefinedFunction(mangled))
    }

    /// A `PoolHandle` holding the innermost open pool's arena mark, which is what
    /// distinguishes one live arena region from another. Stack-allocated per
    /// registration: `register_with_pool` takes it by reference and may not keep it.
    fn emit_pool_handle(&self) -> CodegenResult<PointerValue<'ctx>> {
        let mark = *self.pool_marks.last().ok_or_else(|| {
            CodegenError::InternalError("pool registration outside a pool region".to_string())
        })?;
        let handle_type = self.get_any_llvm_type(&Type::Struct(POOL_HANDLE_STRUCT.to_string()))?;
        let handle = self.entry_alloca(handle_type, "pool.handle")?;
        let id = self
            .builder
            .build_struct_gep(handle_type, handle, 0, "pool.handle.id")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(id, mark)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(handle)
    }

    /// The `{ next, instance, release, flag }` cell one registration occupies.
    fn pool_cell_type(&self) -> inkwell::types::StructType<'ctx> {
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        self.context
            .struct_type(&[ptr_type.into(); CELL_FIELDS as usize], false)
    }

    fn pool_head(&self) -> GlobalValue<'ctx> {
        if let Some(existing) = self.module.get_global(POOL_HEAD_GLOBAL) {
            return existing;
        }
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let global = self.module.add_global(ptr_type, None, POOL_HEAD_GLOBAL);
        global.set_linkage(Linkage::Internal);
        global.set_initializer(&ptr_type.const_null());
        global
    }

    fn get_or_build_pool_register(&self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(POOL_REGISTER_FN) {
            return Ok(existing);
        }
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let function = self.module.add_function(
            POOL_REGISTER_FN,
            self.context
                .void_type()
                .fn_type(&[ptr_type.into(), ptr_type.into(), ptr_type.into()], false),
            Some(Linkage::Internal),
        );
        // Resolved before the detached body opens: building a helper's body inside
        // another's would leave the builder parked in the wrong block on the way out.
        let alloc = self.get_or_build_arena_alloc()?;
        self.detached(|| {
            let entry = self.context.append_basic_block(function, "entry");
            let link = self.context.append_basic_block(function, "link");
            let done = self.context.append_basic_block(function, "done");
            let cell_type = self.pool_cell_type();

            self.builder.position_at_end(entry);
            let size = cell_type.size_of().ok_or_else(|| {
                CodegenError::InternalError("registration cell has no size".into())
            })?;
            let cell = self
                .builder
                .build_call(alloc, &[size.into()], "pool.cell")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| CodegenError::InternalError("arena alloc returned void".into()))?
                .into_pointer_value();
            // An allocator that failed leaves the instance unregistered rather than
            // storing through null; its own `Drop` is all that is owed then.
            let missing = self
                .builder
                .build_is_null(cell, "pool.cell.missing")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.builder
                .build_conditional_branch(missing, done, link)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            self.builder.position_at_end(link);
            let head_global = self.pool_head();
            let head = self
                .builder
                .build_load(ptr_type, head_global.as_pointer_value(), "pool.head")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            let fields = [
                (CELL_NEXT, head),
                (
                    CELL_INSTANCE,
                    function.get_nth_param(0).ok_or_else(|| {
                        CodegenError::InternalError("register lost its instance".into())
                    })?,
                ),
                (
                    CELL_RELEASE,
                    function.get_nth_param(1).ok_or_else(|| {
                        CodegenError::InternalError("register lost its thunk".into())
                    })?,
                ),
                (
                    CELL_FLAG,
                    function.get_nth_param(2).ok_or_else(|| {
                        CodegenError::InternalError("register lost its flag".into())
                    })?,
                ),
            ];
            for (index, value) in fields {
                let slot = self
                    .builder
                    .build_struct_gep(cell_type, cell, index, "pool.cell.field")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                self.builder
                    .build_store(slot, value)
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            }
            self.builder
                .build_store(head_global.as_pointer_value(), cell)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.builder
                .build_unconditional_branch(done)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            self.builder.position_at_end(done);
            self.builder
                .build_return(None)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            Ok(())
        })?;
        Ok(function)
    }

    fn get_or_build_pool_sweep(&self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(POOL_SWEEP_FN) {
            return Ok(existing);
        }
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let bool_type = self.context.bool_type();
        let function = self.module.add_function(
            POOL_SWEEP_FN,
            self.context.void_type().fn_type(&[ptr_type.into()], false),
            Some(Linkage::Internal),
        );
        let release = self.release_fn()?;
        self.detached(|| {
            let entry = self.context.append_basic_block(function, "entry");
            let walk = self.context.append_basic_block(function, "walk");
            let body = self.context.append_basic_block(function, "body");
            let call = self.context.append_basic_block(function, "call");
            let unlink = self.context.append_basic_block(function, "unlink");
            let done = self.context.append_basic_block(function, "done");
            let cell_type = self.pool_cell_type();
            let head_global = self.pool_head();
            let stop = function
                .get_first_param()
                .ok_or_else(|| CodegenError::InternalError("sweep lost its stop cell".into()))?
                .into_pointer_value();

            self.builder.position_at_end(entry);
            self.builder
                .build_unconditional_branch(walk)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            self.builder.position_at_end(walk);
            let cell = self
                .builder
                .build_load(ptr_type, head_global.as_pointer_value(), "pool.cell")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_pointer_value();
            let reached_stop = self
                .builder
                .build_int_compare(IntPredicate::EQ, cell, stop, "pool.at.stop")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.builder
                .build_conditional_branch(reached_stop, done, body)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            self.builder.position_at_end(body);
            let mut loaded = Vec::with_capacity(CELL_FIELDS as usize);
            for index in [CELL_NEXT, CELL_INSTANCE, CELL_RELEASE, CELL_FLAG] {
                let slot = self
                    .builder
                    .build_struct_gep(cell_type, cell, index, "pool.cell.field")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                loaded.push(
                    self.builder
                        .build_load(ptr_type, slot, "pool.cell.value")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into_pointer_value(),
                );
            }
            let (next, instance, thunk, flag) = (loaded[0], loaded[1], loaded[2], loaded[3]);
            // Unlinked before the call so a `bulk_release` that itself enters a pool
            // cannot walk a cell this sweep has already claimed.
            self.builder
                .build_store(head_global.as_pointer_value(), next)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            let live = self
                .builder
                .build_load(bool_type, flag, "pool.cell.live")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_int_value();
            self.builder
                .build_conditional_branch(live, call, unlink)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            self.builder.position_at_end(call);
            let thunk_type = self.context.void_type().fn_type(&[ptr_type.into()], false);
            self.builder
                .build_indirect_call(thunk_type, thunk, &[instance.into()], "")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.builder
                .build_store(flag, bool_type.const_zero())
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.builder
                .build_unconditional_branch(unlink)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            self.builder.position_at_end(unlink);
            // A no-op for the cells the arena holds; it matters only for one the
            // allocator spilled to the heap because the chunk was full.
            self.builder
                .build_call(release, &[cell.into()], "")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.builder
                .build_unconditional_branch(walk)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            self.builder.position_at_end(done);
            self.builder
                .build_return(None)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            Ok(())
        })?;
        Ok(function)
    }

    fn arena_base(&self) -> GlobalValue<'ctx> {
        if let Some(existing) = self.module.get_global(ARENA_BASE_GLOBAL) {
            return existing;
        }
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let global = self.module.add_global(ptr_type, None, ARENA_BASE_GLOBAL);
        global.set_linkage(Linkage::Internal);
        global.set_initializer(&ptr_type.const_null());
        global
    }

    fn arena_offset(&self) -> GlobalValue<'ctx> {
        if let Some(existing) = self.module.get_global(ARENA_OFFSET_GLOBAL) {
            return existing;
        }
        let i64_type = self.context.i64_type();
        let global = self.module.add_global(i64_type, None, ARENA_OFFSET_GLOBAL);
        global.set_linkage(Linkage::Internal);
        global.set_initializer(&i64_type.const_zero());
        global
    }

    /// Emit a helper's body without disturbing the function the caller is in the
    /// middle of. Every arena helper is created lazily: at the module's first `pool`
    /// block, or its first release.
    fn detached(&self, emit: impl FnOnce() -> CodegenResult<()>) -> CodegenResult<()> {
        let resume_at = self.builder.get_insert_block();
        let result = emit();
        if let Some(block) = resume_at {
            self.builder.position_at_end(block);
        }
        result
    }

    fn get_or_build_arena_mark(&self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(ARENA_MARK_FN) {
            return Ok(existing);
        }
        let i64_type = self.context.i64_type();
        let function = self.module.add_function(
            ARENA_MARK_FN,
            i64_type.fn_type(&[], false),
            Some(Linkage::Internal),
        );
        self.detached(|| {
            let entry = self.context.append_basic_block(function, "entry");
            let reserve = self.context.append_basic_block(function, "reserve");
            let done = self.context.append_basic_block(function, "done");
            let ptr_type = self.context.ptr_type(AddressSpace::default());
            let base = self.arena_base();

            self.builder.position_at_end(entry);
            let current = self
                .builder
                .build_load(ptr_type, base.as_pointer_value(), "arena.base")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_pointer_value();
            let missing = self
                .builder
                .build_is_null(current, "arena.missing")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.builder
                .build_conditional_branch(missing, reserve, done)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            // A failed reservation leaves the base null, which routes every allocation
            // in the block to the heap instead of aborting the program.
            self.builder.position_at_end(reserve);
            let malloc = self.get_or_declare_malloc();
            let chunk = self
                .builder
                .build_call(
                    malloc,
                    &[i64_type.const_int(ARENA_CAPACITY, false).into()],
                    "arena.chunk",
                )
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| CodegenError::InternalError("malloc returned void".into()))?;
            self.builder
                .build_store(base.as_pointer_value(), chunk)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.builder
                .build_unconditional_branch(done)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            self.builder.position_at_end(done);
            let offset = self.arena_offset();
            let mark = self
                .builder
                .build_load(i64_type, offset.as_pointer_value(), "arena.mark")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.builder
                .build_return(Some(&mark))
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            Ok(())
        })?;
        Ok(function)
    }

    fn get_or_build_arena_release(&self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(ARENA_RELEASE_FN) {
            return Ok(existing);
        }
        let i64_type = self.context.i64_type();
        let function = self.module.add_function(
            ARENA_RELEASE_FN,
            self.context.void_type().fn_type(&[i64_type.into()], false),
            Some(Linkage::Internal),
        );
        self.detached(|| {
            let entry = self.context.append_basic_block(function, "entry");
            self.builder.position_at_end(entry);
            let mark = function
                .get_first_param()
                .ok_or_else(|| CodegenError::InternalError("arena release lost its mark".into()))?;
            let offset = self.arena_offset();
            self.builder
                .build_store(offset.as_pointer_value(), mark)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.builder
                .build_return(None)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            Ok(())
        })?;
        Ok(function)
    }

    fn get_or_build_arena_alloc(&self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(ARENA_ALLOC_FN) {
            return Ok(existing);
        }
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let function = self.module.add_function(
            ARENA_ALLOC_FN,
            ptr_type.fn_type(&[i64_type.into()], false),
            Some(Linkage::Internal),
        );
        self.detached(|| {
            let size = function
                .get_first_param()
                .ok_or_else(|| CodegenError::InternalError("arena alloc lost its size".into()))?
                .into_int_value();
            let align = i64_type.const_int(ARENA_MIN_ALIGN, false);
            let fallback = self.get_or_declare_malloc();
            self.build_bump_body(function, size, align, fallback, &[size.into()])
        })?;
        Ok(function)
    }

    fn get_or_build_arena_aligned_alloc(&self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(ARENA_ALIGNED_ALLOC_FN) {
            return Ok(existing);
        }
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let function = self.module.add_function(
            ARENA_ALIGNED_ALLOC_FN,
            ptr_type.fn_type(&[i64_type.into(), i64_type.into()], false),
            Some(Linkage::Internal),
        );
        self.detached(|| {
            let first = function
                .get_nth_param(0)
                .ok_or_else(|| {
                    CodegenError::InternalError("aligned alloc lost an argument".into())
                })?
                .into_int_value();
            let second = function
                .get_nth_param(1)
                .ok_or_else(|| {
                    CodegenError::InternalError("aligned alloc lost an argument".into())
                })?
                .into_int_value();
            // The parameters arrive in the host libc's order, because the call site
            // builds one argument list for both paths. See `ALIGNED_ALLOC_FN`.
            let (align, size) = if cfg!(target_os = "windows") {
                (second, first)
            } else {
                (first, second)
            };
            let fallback = self.get_or_declare_aligned_alloc();
            self.build_bump_body(
                function,
                size,
                align,
                fallback,
                &[first.into(), second.into()],
            )
        })?;
        Ok(function)
    }

    /// Fill an allocator's body: align the bump pointer up to `align`, take `size`
    /// bytes when they fit in the chunk, and otherwise hand the request to `fallback`.
    ///
    /// The absolute address is what gets aligned, not the offset: the chunk's own base
    /// carries only libc's guarantee, which is weaker than the alignment a tensor
    /// buffer asks for.
    fn build_bump_body(
        &self,
        function: FunctionValue<'ctx>,
        size: IntValue<'ctx>,
        align: IntValue<'ctx>,
        fallback: FunctionValue<'ctx>,
        fallback_args: &[inkwell::values::BasicMetadataValueEnum<'ctx>],
    ) -> CodegenResult<()> {
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let entry = self.context.append_basic_block(function, "entry");
        let bump = self.context.append_basic_block(function, "bump");
        let take = self.context.append_basic_block(function, "take");
        let heap = self.context.append_basic_block(function, "heap");

        self.builder.position_at_end(entry);
        let base_global = self.arena_base();
        let base = self
            .builder
            .build_load(ptr_type, base_global.as_pointer_value(), "arena.base")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let missing = self
            .builder
            .build_is_null(base, "arena.missing")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(missing, heap, bump)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(bump);
        let base_int = self
            .builder
            .build_ptr_to_int(base, i64_type, "arena.base.int")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let offset_global = self.arena_offset();
        let offset = self
            .builder
            .build_load(i64_type, offset_global.as_pointer_value(), "arena.offset")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();
        let cursor = self
            .builder
            .build_int_add(base_int, offset, "arena.cursor")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let bias = self
            .builder
            .build_int_sub(align, i64_type.const_int(1, false), "align.bias")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let raised = self
            .builder
            .build_int_add(cursor, bias, "align.raised")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        // `-align` is `~(align - 1)` for the powers of two an alignment may be, which
        // is what clears the low bits without a second constant.
        let mask = self
            .builder
            .build_int_neg(align, "align.mask")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let aligned = self
            .builder
            .build_and(raised, mask, "align.addr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let used = self
            .builder
            .build_int_sub(aligned, base_int, "arena.used")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let end = self
            .builder
            .build_int_add(used, size, "arena.end")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let fits = self
            .builder
            .build_int_compare(
                IntPredicate::ULE,
                end,
                i64_type.const_int(ARENA_CAPACITY, false),
                "arena.fits",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(fits, take, heap)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(take);
        self.builder
            .build_store(offset_global.as_pointer_value(), end)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let allocated = self
            .builder
            .build_int_to_ptr(aligned, ptr_type, "arena.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_return(Some(&allocated))
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(heap);
        let spilled = self
            .builder
            .build_call(fallback, fallback_args, "arena.spill")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("allocator returned void".into()))?;
        self.builder
            .build_return(Some(&spilled))
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Build a release wrapper named `name` that forwards to `libc_release` for
    /// anything the arena does not own.
    fn get_or_build_release(
        &self,
        name: &str,
        libc_release: FunctionValue<'ctx>,
    ) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(name) {
            return Ok(existing);
        }
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let function = self.module.add_function(
            name,
            self.context.void_type().fn_type(&[ptr_type.into()], false),
            Some(Linkage::Internal),
        );
        self.detached(|| {
            let entry = self.context.append_basic_block(function, "entry");
            let heap = self.context.append_basic_block(function, "heap");
            let done = self.context.append_basic_block(function, "done");

            self.builder.position_at_end(entry);
            let target = function
                .get_first_param()
                .ok_or_else(|| CodegenError::InternalError("release lost its pointer".into()))?
                .into_pointer_value();
            let owned = self.build_arena_owns(target)?;
            self.builder
                .build_conditional_branch(owned, done, heap)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            self.builder.position_at_end(heap);
            self.builder
                .build_call(libc_release, &[target.into()], "")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.builder
                .build_unconditional_branch(done)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            self.builder.position_at_end(done);
            self.builder
                .build_return(None)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            Ok(())
        })?;
        Ok(function)
    }

    /// Whether `target` points into the arena chunk. Compares the whole reserved
    /// range rather than the live prefix: a pointer above the mark is memory the
    /// arena has already reclaimed, and it must not reach libc either.
    fn build_arena_owns(&self, target: PointerValue<'ctx>) -> CodegenResult<IntValue<'ctx>> {
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let base = self
            .builder
            .build_load(ptr_type, self.arena_base().as_pointer_value(), "arena.base")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let base_int = self
            .builder
            .build_ptr_to_int(base, i64_type, "arena.base.int")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let target_int = self
            .builder
            .build_ptr_to_int(target, i64_type, "arena.target.int")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let limit = self
            .builder
            .build_int_add(
                base_int,
                i64_type.const_int(ARENA_CAPACITY, false),
                "arena.limit",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let above = self
            .builder
            .build_int_compare(IntPredicate::UGE, target_int, base_int, "arena.above")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let below = self
            .builder
            .build_int_compare(IntPredicate::ULT, target_int, limit, "arena.below")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let inside = self
            .builder
            .build_and(above, below, "arena.inside")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        // An unreserved arena is a null base, against which every heap pointer would
        // compare "above".
        let reserved = self
            .builder
            .build_is_not_null(base, "arena.reserved")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_and(inside, reserved, "arena.owns")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }
}
