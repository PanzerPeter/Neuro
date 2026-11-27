// The linear arena behind a `pool { }` block.
//
// A pool block allocates by moving a bump pointer and frees by moving it back, so
// its exit costs one store no matter how many objects the body built, plus one sweep
// step per `PoolAware` value it registered. The allocator is a chain of chunks held in
// module globals: a table of chunk bases and sizes, how many are reserved, the chunk the
// bump pointer is in and the offset of the first free byte there. The first chunk is
// reserved on the first `pool` a run reaches; a request the current chunk cannot hold
// moves on to the next, reserving it (at least twice the size) when there is none yet.
// Entering a block reads the position (its *mark*, the chunk index above the offset);
// leaving writes it back, which is the bulk free, and the chunks past it stay reserved
// for the next block to reuse. Nesting needs nothing more, because a nested block's mark
// is simply a later position.
//
// Every release in the program goes through a wrapper that returns without calling
// libc when the pointer lies inside the chunk: arena memory is reclaimed by the mark
// restore, and handing it to `free` would corrupt the heap. The wrapper is what lets
// the ordinary drop path stay exactly as it is inside a pool. In a program with no
// `pool` the base global is never written, so the optimizer folds the check away.
//
// The globals are per process, which holds only while a program has one thread: the
// language gives each thread its own arena, so `spawn` has to make them thread-local.
//
// What the arena does NOT capture is as important as what it does. An allocation made by
// a callee belongs to whoever the callee gives it to, which this phase cannot prove, so
// it stays an ordinary heap allocation, slower and never unsafe. Nor does the arena
// capture a store whose place outlives the block: the language routes that allocation to the
// heap, because the value is still reachable after the mark goes back. `route_store_off_arena`
// is where the block's own bindings are told apart from the ones it inherited.

use inkwell::module::Linkage;
use inkwell::values::{BasicValueEnum, FunctionValue, GlobalValue, IntValue, PointerValue};
use inkwell::{AddressSpace, IntPredicate};
use neuro_hir::HirPlace;

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

use super::context::{CodegenContext, DropEntry, DropTarget};

/// Bytes of the first chunk, reserved the first time a run enters a `pool`.
///
/// Chunks are reserved lazily and never released, so the cost of overshooting is untouched
/// address space rather than resident memory. The figure covers a handful of the
/// 1024x1024 `f32` tensors a training step allocates; a block that outgrows it moves on to
/// a chunk twice the size.
const ARENA_FIRST_CHUNK: u64 = 64 * 1024 * 1024;

/// How many chunks the chain may hold. Each is at least twice the one before, so the
/// last is far past any memory a machine has; a request past it goes to the heap.
const ARENA_MAX_CHUNKS: u32 = 24;

/// A mark is the chunk index above this many bits of offset.
const MARK_OFFSET_BITS: u64 = 48;

/// Alignment given to a plain (non-over-aligned) arena allocation. Matches the
/// guarantee libc `malloc` makes, so a buffer that moves between the two paths is
/// aligned the same either way.
const ARENA_MIN_ALIGN: u64 = 16;

const ARENA_CHUNKS_GLOBAL: &str = "__neuro_arena_chunks";
const ARENA_SIZES_GLOBAL: &str = "__neuro_arena_sizes";
const ARENA_COUNT_GLOBAL: &str = "__neuro_arena_count";
const ARENA_CHUNK_GLOBAL: &str = "__neuro_arena_chunk";
const ARENA_OFFSET_GLOBAL: &str = "__neuro_arena_offset";
const ARENA_OWNS_FN: &str = "__neuro_arena_owns";
const ARENA_REALLOC_FN: &str = "__neuro_arena_realloc";
const HEAP_REALLOC_FN: &str = "__neuro_realloc";
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
/// Fields past the header in a binding's cell (`pool_registration_slot`): the drop flag
/// and the instance itself.
const SLOT_FLAG: u32 = 4;
const SLOT_INSTANCE: u32 = 5;

/// The prelude struct `register_with_pool` names. Matched here by name, the way the
/// trait itself is: a program that shadows it withdraws the opt-in with it.
const POOL_HANDLE_STRUCT: &str = "PoolHandle";

impl<'ctx> CodegenContext<'ctx> {
    /// Record `storage` as a binding of the innermost open `pool`, so a later store into
    /// it keeps the bump path. Inert outside a pool.
    pub(crate) fn note_pool_local(&mut self, storage: PointerValue<'ctx>) {
        if let Some(locals) = self.pool_locals.last_mut() {
            locals.insert(storage);
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
    /// block is treated as outliving it. The root is compared by storage, not by name: a
    /// block may declare a name in an inner scope that an outer binding also carries, and
    /// once that scope ends the name means the outer binding again.
    fn route_store_off_arena(&self, place: &HirPlace) -> bool {
        let Some(locals) = self.pool_locals.last() else {
            return false;
        };
        !Self::place_root(place)
            .and_then(|root| self.variables.get(root))
            .is_some_and(|storage| locals.contains(storage))
    }

    /// The function the builder is emitting into right now.
    pub(crate) fn emitting_function(&self) -> Option<FunctionValue<'ctx>> {
        self.builder.get_insert_block()?.get_parent()
    }

    /// Whether an allocation emitted right now is one the innermost open `pool` body
    /// writes. A helper built lazily in the middle of a body shares its `pool_depth`, but
    /// every caller in the program runs that helper, so its allocations are a callee's.
    pub(crate) fn allocating_in_pool_body(&self) -> bool {
        self.pool_depth > 0
            && self.pool_exits.last().and_then(|exit| exit.function) == self.emitting_function()
    }

    /// The allocator the code being emitted right now must call for a plain buffer:
    /// the arena inside a `pool` body, libc `malloc` outside one. Same signature
    /// either way, so a call site needs no branch of its own.
    pub(crate) fn alloc_fn(&self) -> CodegenResult<FunctionValue<'ctx>> {
        if !self.allocating_in_pool_body() {
            return Ok(self.get_or_declare_malloc());
        }
        self.get_or_build_arena_alloc()
    }

    /// The over-aligned counterpart of [`alloc_fn`](CodegenContext::alloc_fn). Both
    /// take the host libc's argument order, which `codegen/dlpack.rs` builds.
    pub(crate) fn aligned_alloc_fn(&self) -> CodegenResult<FunctionValue<'ctx>> {
        if !self.allocating_in_pool_body() {
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

    /// Whether a collection whose header is stored at `header` grows on the arena: the
    /// growth is emitted in a `pool` body and the header is one of that block's own
    /// bindings, which dies with it. Any other collection grows on the heap.
    pub(crate) fn grows_in_arena(&self, header: PointerValue<'ctx>) -> bool {
        self.allocating_in_pool_body()
            && self
                .pool_locals
                .last()
                .is_some_and(|locals| locals.contains(&header))
    }

    /// `realloc(ptr, old_bytes, new_bytes) -> ptr` for a growable buffer: on the arena when
    /// `in_arena`, else on the heap. Either takes a buffer of either kind, since a buffer
    /// moves with its value: the heap one copies an arena buffer out rather than handing it
    /// to libc, and the arena one leaves a heap buffer to libc's `realloc`.
    pub(crate) fn growth_realloc_fn(&self, in_arena: bool) -> CodegenResult<FunctionValue<'ctx>> {
        match in_arena {
            true => self.get_or_build_growth_realloc(ARENA_REALLOC_FN, true),
            false => self.get_or_build_growth_realloc(HEAP_REALLOC_FN, false),
        }
    }

    /// Build `name(ptr, old_bytes, new_bytes)`. A null or heap `ptr` goes to the arena's
    /// allocator or libc's `realloc` by `in_arena`; an arena `ptr` grows in place when it is
    /// the last allocation and the chunk has room, and is otherwise copied into a fresh
    /// block. The block it leaves is the arena's to reclaim at the mark.
    fn get_or_build_growth_realloc(
        &self,
        name: &str,
        in_arena: bool,
    ) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(name) {
            return Ok(existing);
        }
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let function = self.module.add_function(
            name,
            ptr_type.fn_type(&[ptr_type.into(), i64_type.into(), i64_type.into()], false),
            Some(Linkage::Internal),
        );
        let alloc = match in_arena {
            true => self.get_or_build_arena_alloc()?,
            false => self.get_or_declare_malloc(),
        };
        let owns = self.get_or_build_arena_owns()?;
        self.detached(|| {
            let entry = self.context.append_basic_block(function, "entry");
            let probe = self.context.append_basic_block(function, "probe");
            let libc = self.context.append_basic_block(function, "libc");
            let arena = self.context.append_basic_block(function, "arena");
            let extend = self.context.append_basic_block(function, "extend");
            let copy = self.context.append_basic_block(function, "copy");
            let filled = self.context.append_basic_block(function, "filled");
            let bare = self.context.append_basic_block(function, "bare");

            self.builder.position_at_end(entry);
            let ptr = self.param(function, 0)?.into_pointer_value();
            let old = self.param(function, 1)?.into_int_value();
            let new = self.param(function, 2)?.into_int_value();
            let fresh = self.builder.build_is_null(ptr, "grow.fresh")?;
            // A fresh buffer comes from this variant's own allocator.
            let first = match in_arena {
                true => copy,
                false => libc,
            };
            self.builder.build_conditional_branch(fresh, first, probe)?;

            self.builder.position_at_end(probe);
            let owned = self
                .builder
                .build_call(owns, &[ptr.into()], "grow.owned")?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| CodegenError::InternalError("arena owns returned void".into()))?
                .into_int_value();
            let owned_next = match in_arena {
                true => arena,
                false => copy,
            };
            self.builder
                .build_conditional_branch(owned, owned_next, libc)?;

            self.builder.position_at_end(libc);
            let realloc = self.get_or_declare_realloc();
            let moved = self
                .builder
                .build_call(realloc, &[ptr.into(), new.into()], "grow.libc")?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| CodegenError::InternalError("realloc returned void".into()))?;
            self.builder.build_return(Some(&moved))?;

            // In place: `ptr` ends at the bump pointer of the current chunk, so nothing was
            // allocated after it, and the chunk holds the larger size.
            self.builder.position_at_end(arena);
            let current = self
                .builder
                .build_load(
                    i64_type,
                    self.arena_counter_global(ARENA_CHUNK_GLOBAL)
                        .as_pointer_value(),
                    "arena.current",
                )?
                .into_int_value();
            let offset_global = self.arena_offset_global();
            let offset = self
                .builder
                .build_load(i64_type, offset_global.as_pointer_value(), "arena.offset")?
                .into_int_value();
            let (base, capacity) = self.load_chunk(current)?;
            let base_int = self
                .builder
                .build_ptr_to_int(base, i64_type, "arena.base.int")?;
            let ptr_int = self.builder.build_ptr_to_int(ptr, i64_type, "grow.ptr")?;
            let start = self
                .builder
                .build_int_sub(ptr_int, base_int, "grow.start")?;
            let in_current = self.builder.build_int_compare(
                IntPredicate::UGE,
                ptr_int,
                base_int,
                "grow.in.chunk",
            )?;
            let tail = self.builder.build_int_add(start, old, "grow.tail")?;
            let last =
                self.builder
                    .build_int_compare(IntPredicate::EQ, tail, offset, "grow.last")?;
            let end = self.builder.build_int_add(start, new, "grow.end")?;
            let room =
                self.builder
                    .build_int_compare(IntPredicate::ULT, end, capacity, "grow.room")?;
            let unwrapped =
                self.builder
                    .build_int_compare(IntPredicate::UGE, end, start, "grow.unwrapped")?;
            let in_place = self.builder.build_and(in_current, last, "grow.place")?;
            let in_place = self.builder.build_and(in_place, room, "grow.place")?;
            let in_place = self.builder.build_and(in_place, unwrapped, "grow.place")?;
            self.builder
                .build_conditional_branch(in_place, extend, copy)?;

            self.builder.position_at_end(extend);
            self.builder
                .build_store(offset_global.as_pointer_value(), end)?;
            self.builder.build_return(Some(&ptr))?;

            self.builder.position_at_end(copy);
            let block = self
                .builder
                .build_call(alloc, &[new.into()], "grow.block")?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| CodegenError::InternalError("allocator returned void".into()))?
                .into_pointer_value();
            let lost = self.builder.build_is_null(block, "grow.lost")?;
            let nothing = self.builder.build_or(lost, fresh, "grow.nothing")?;
            self.builder
                .build_conditional_branch(nothing, bare, filled)?;

            self.builder.position_at_end(filled);
            let shorter =
                self.builder
                    .build_int_compare(IntPredicate::ULT, new, old, "grow.shorter")?;
            let bytes = self.builder.build_select(shorter, new, old, "grow.bytes")?;
            let memcpy = self.get_or_declare_memcpy();
            self.builder
                .build_call(memcpy, &[block.into(), ptr.into(), bytes.into()], "")?;
            self.builder.build_return(Some(&block))?;

            // A fresh buffer has nothing to copy, and a failed one nothing to copy into.
            self.builder.position_at_end(bare);
            self.builder.build_return(Some(&block))?;
            Ok(())
        })?;
        Ok(function)
    }

    /// Enter a pool region: reserve the chunk if this is the program's first `pool`,
    /// and read the mark that [`emit_arena_release`](CodegenContext::emit_arena_release)
    /// restores.
    pub(crate) fn emit_arena_mark(&self) -> CodegenResult<IntValue<'ctx>> {
        let mark = self.get_or_build_arena_mark()?;
        let call = self.builder.build_call(mark, &[], "pool.mark")?;
        call.try_as_basic_value()
            .basic()
            .map(|v| v.into_int_value())
            .ok_or_else(|| CodegenError::InternalError("arena mark returned void".into()))
    }

    /// Leave a pool region, releasing everything the block allocated in one store.
    pub(crate) fn emit_arena_release(&self, mark: IntValue<'ctx>) -> CodegenResult<()> {
        let release = self.get_or_build_arena_release()?;
        self.builder.build_call(release, &[mark.into()], "")?;
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
            .build_load(ptr_type, self.pool_head().as_pointer_value(), "pool.head")?
            .into_pointer_value())
    }

    /// Run `bulk_release` over everything registered since `stop`, newest first, and
    /// unlink each cell as it goes. Emitted immediately before the mark restore: the
    /// cells themselves live in the arena, so the restore reclaims them too.
    pub(crate) fn emit_pool_sweep(&self, stop: PointerValue<'ctx>) -> CodegenResult<()> {
        let sweep = self.get_or_build_pool_sweep()?;
        self.builder.build_call(sweep, &[stop.into()], "")?;
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

    /// Storage for a `PoolAware` binding of LLVM type `llvm_ty` that the innermost open
    /// `pool` releases, and its drop flag: both inside the binding's own registration
    /// cell, pushed onto the list the block's closing brace sweeps in reverse. Returns
    /// `(storage, flag)`; the flag is armed.
    ///
    /// A cell per evaluation is what lets a loop register one value per iteration: a
    /// stack slot and flag would be shared by every iteration, so the sweep would see the
    /// last value through every cell and release it once. The cell lives in the arena,
    /// so the mark restore reclaims it after the sweep, and a mutation made after
    /// registration is what `bulk_release` reads. When the allocator fails the binding
    /// falls back to a stack slot and goes unregistered, and so unreleased: out of
    /// memory, nothing better is left.
    pub(crate) fn pool_registration_slot(
        &mut self,
        struct_name: &str,
        llvm_ty: inkwell::types::BasicTypeEnum<'ctx>,
    ) -> CodegenResult<(PointerValue<'ctx>, PointerValue<'ctx>)> {
        let release = self.pool_aware_method(struct_name, "bulk_release")?;
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let bool_type = self.context.bool_type();
        // The cell's header, then the flag and the instance it points at.
        let slot_type = self.context.struct_type(
            &[
                ptr_type.into(),
                ptr_type.into(),
                ptr_type.into(),
                ptr_type.into(),
                bool_type.into(),
                llvm_ty,
            ],
            false,
        );
        let size = slot_type
            .size_of()
            .ok_or_else(|| CodegenError::InternalError("registration cell has no size".into()))?;
        let register = self.get_or_build_pool_register()?;
        let cell = self
            .builder
            .build_call(
                register,
                &[
                    release.as_global_value().as_pointer_value().into(),
                    size.into(),
                ],
                "pool.cell",
            )?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("pool register returned void".into()))?
            .into_pointer_value();
        let fallback = self.entry_alloca(slot_type, "pool.cell.fallback")?;
        let missing = self.builder.build_is_null(cell, "pool.cell.missing")?;
        let base = self
            .builder
            .build_select(missing, fallback, cell, "pool.cell.base")?
            .into_pointer_value();
        let instance =
            self.builder
                .build_struct_gep(slot_type, base, SLOT_INSTANCE, "pool.instance")?;
        let flag = self
            .builder
            .build_struct_gep(slot_type, base, SLOT_FLAG, "pool.flag")?;
        for (index, value) in [(CELL_INSTANCE, instance), (CELL_FLAG, flag)] {
            let field = self
                .builder
                .build_struct_gep(slot_type, base, index, "pool.cell.field")?;
            self.builder.build_store(field, value)?;
        }
        self.builder
            .build_store(flag, bool_type.const_int(1, false))?;
        Ok((instance, flag))
    }

    /// Bind `name`, at `storage` with drop flag `flag` from
    /// [`pool_registration_slot`](CodegenContext::pool_registration_slot), to the arena
    /// instead of to its own scope exit.
    ///
    /// `announce` calls the type's `register_with_pool` with the active arena's handle,
    /// for a value the block constructs. A value moved in from a binding the block
    /// already registered is not announced: the language has `register_with_pool` called
    /// by the *constructor*, so running it again would register one resource twice. Its
    /// cell is still needed, because the move cleared the source's flag, so this is the
    /// binding the sweep must release.
    pub(crate) fn bind_pool_registered(
        &mut self,
        name: &str,
        struct_name: &str,
        storage: PointerValue<'ctx>,
        flag: PointerValue<'ctx>,
        announce: bool,
    ) -> CodegenResult<()> {
        if announce {
            let registrar = self.pool_aware_method(struct_name, "register_with_pool")?;
            let handle = self.emit_pool_handle()?;
            self.builder
                .build_call(registrar, &[storage.into(), handle.into()], "")?;
        }
        if let Some(scope) = self.drop_scopes.last_mut() {
            scope.push(DropEntry {
                name: name.to_string(),
                storage_ptr: storage,
                flag_ptr: flag,
                target: DropTarget::PoolRegistered,
                held: Vec::new(),
            });
        }
        Ok(())
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

    /// A `PoolHandle` holding the innermost open pool's arena mark. The mark is not a
    /// unique id, since two regions opened with nothing allocated between them share it,
    /// and the field is private, so no program can read it. Stack-allocated per
    /// registration: `register_with_pool` takes it by reference and may not keep it.
    fn emit_pool_handle(&self) -> CodegenResult<PointerValue<'ctx>> {
        let mark = *self.pool_marks.last().ok_or_else(|| {
            CodegenError::InternalError("pool registration outside a pool region".to_string())
        })?;
        let handle_type = self.get_any_llvm_type(&Type::Struct(POOL_HANDLE_STRUCT.to_string()))?;
        let handle = self.entry_alloca(handle_type, "pool.handle")?;
        let id = self
            .builder
            .build_struct_gep(handle_type, handle, 0, "pool.handle.id")?;
        self.builder.build_store(id, mark)?;
        Ok(handle)
    }

    /// The `{ next, instance, release, flag }` cell one registration occupies.
    fn pool_cell_type(&self) -> inkwell::types::StructType<'ctx> {
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        self.context
            .struct_type(&[ptr_type.into(); CELL_FIELDS as usize], false)
    }

    /// The internal global `name`, created holding `initial` on first use.
    fn internal_global(&self, name: &str, initial: BasicValueEnum<'ctx>) -> GlobalValue<'ctx> {
        if let Some(existing) = self.module.get_global(name) {
            return existing;
        }
        let global = self.module.add_global(initial.get_type(), None, name);
        global.set_linkage(Linkage::Internal);
        global.set_initializer(&initial);
        global
    }

    fn pool_head(&self) -> GlobalValue<'ctx> {
        let null = self.context.ptr_type(AddressSpace::default()).const_null();
        self.internal_global(POOL_HEAD_GLOBAL, null.into())
    }

    /// `__neuro_pool_register(release, size) -> cell`: allocate a `size`-byte cell from
    /// the arena, fill in its link and `bulk_release` thunk and push it onto the list.
    /// The caller fills in the instance and flag, which live inside the cell. Null when
    /// the allocator failed, and then nothing is linked.
    fn get_or_build_pool_register(&self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(POOL_REGISTER_FN) {
            return Ok(existing);
        }
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let i64_type = self.context.i64_type();
        let function = self.module.add_function(
            POOL_REGISTER_FN,
            ptr_type.fn_type(&[ptr_type.into(), i64_type.into()], false),
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
            let size = self.param(function, 1)?;
            let cell = self
                .builder
                .build_call(alloc, &[size.into()], "pool.cell")?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| CodegenError::InternalError("arena alloc returned void".into()))?
                .into_pointer_value();
            let missing = self.builder.build_is_null(cell, "pool.cell.missing")?;
            self.builder.build_conditional_branch(missing, done, link)?;

            self.builder.position_at_end(link);
            let head_global = self.pool_head();
            let head =
                self.builder
                    .build_load(ptr_type, head_global.as_pointer_value(), "pool.head")?;
            for (index, value) in [(CELL_NEXT, head), (CELL_RELEASE, self.param(function, 0)?)] {
                let slot =
                    self.builder
                        .build_struct_gep(cell_type, cell, index, "pool.cell.field")?;
                self.builder.build_store(slot, value)?;
            }
            self.builder
                .build_store(head_global.as_pointer_value(), cell)?;
            self.builder.build_unconditional_branch(done)?;

            self.builder.position_at_end(done);
            self.builder.build_return(Some(&cell))?;
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
            let stop = self.param(function, 0)?.into_pointer_value();

            self.builder.position_at_end(entry);
            self.builder.build_unconditional_branch(walk)?;

            self.builder.position_at_end(walk);
            let cell = self
                .builder
                .build_load(ptr_type, head_global.as_pointer_value(), "pool.cell")?
                .into_pointer_value();
            let reached_stop =
                self.builder
                    .build_int_compare(IntPredicate::EQ, cell, stop, "pool.at.stop")?;
            self.builder
                .build_conditional_branch(reached_stop, done, body)?;

            self.builder.position_at_end(body);
            let load_field = |index: u32| -> CodegenResult<PointerValue<'ctx>> {
                let slot =
                    self.builder
                        .build_struct_gep(cell_type, cell, index, "pool.cell.field")?;
                Ok(self
                    .builder
                    .build_load(ptr_type, slot, "pool.cell.value")?
                    .into_pointer_value())
            };
            let next = load_field(CELL_NEXT)?;
            let instance = load_field(CELL_INSTANCE)?;
            let thunk = load_field(CELL_RELEASE)?;
            let flag = load_field(CELL_FLAG)?;
            // Unlinked before the call so a `bulk_release` that itself enters a pool
            // cannot walk a cell this sweep has already claimed.
            self.builder
                .build_store(head_global.as_pointer_value(), next)?;
            let live = self
                .builder
                .build_load(bool_type, flag, "pool.cell.live")?
                .into_int_value();
            self.builder.build_conditional_branch(live, call, unlink)?;

            self.builder.position_at_end(call);
            let thunk_type = self.context.void_type().fn_type(&[ptr_type.into()], false);
            self.builder
                .build_indirect_call(thunk_type, thunk, &[instance.into()], "")?;
            self.builder.build_store(flag, bool_type.const_zero())?;
            self.builder.build_unconditional_branch(unlink)?;

            self.builder.position_at_end(unlink);
            // A no-op for the cells the arena holds; it matters only for one the
            // allocator spilled to the heap because the chunk was full.
            self.builder.build_call(release, &[cell.into()], "")?;
            self.builder.build_unconditional_branch(walk)?;

            self.builder.position_at_end(done);
            self.builder.build_return(None)?;
            Ok(())
        })?;
        Ok(function)
    }

    /// The internal `[ARENA_MAX_CHUNKS x ptr]` table of chunk bases, null past the count.
    fn arena_chunks_global(&self) -> GlobalValue<'ctx> {
        let table = self
            .context
            .ptr_type(AddressSpace::default())
            .array_type(ARENA_MAX_CHUNKS);
        self.internal_global(ARENA_CHUNKS_GLOBAL, table.const_zero().into())
    }

    /// The internal `[ARENA_MAX_CHUNKS x i64]` table of chunk sizes in bytes.
    fn arena_sizes_global(&self) -> GlobalValue<'ctx> {
        let table = self.context.i64_type().array_type(ARENA_MAX_CHUNKS);
        self.internal_global(ARENA_SIZES_GLOBAL, table.const_zero().into())
    }

    /// An internal `i64` global starting at zero: the chunk count, the bump pointer's
    /// chunk, or its offset in that chunk.
    fn arena_counter_global(&self, name: &str) -> GlobalValue<'ctx> {
        self.internal_global(name, self.context.i64_type().const_zero().into())
    }

    fn arena_offset_global(&self) -> GlobalValue<'ctx> {
        self.arena_counter_global(ARENA_OFFSET_GLOBAL)
    }

    /// Load `index`'s entry of the chunk base table and of the size table.
    fn load_chunk(
        &self,
        index: IntValue<'ctx>,
    ) -> CodegenResult<(PointerValue<'ctx>, IntValue<'ctx>)> {
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let i64_type = self.context.i64_type();
        let base_slot = self.chunk_slot(self.arena_chunks_global(), ptr_type.into(), index)?;
        let size_slot = self.chunk_slot(self.arena_sizes_global(), i64_type.into(), index)?;
        let base = self
            .builder
            .build_load(ptr_type, base_slot, "chunk.base")?
            .into_pointer_value();
        let size = self
            .builder
            .build_load(i64_type, size_slot, "chunk.size")?
            .into_int_value();
        Ok((base, size))
    }

    /// Record `base` and `size` as chunk `index`.
    fn store_chunk(
        &self,
        index: IntValue<'ctx>,
        base: PointerValue<'ctx>,
        size: IntValue<'ctx>,
    ) -> CodegenResult<()> {
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let i64_type = self.context.i64_type();
        let base_slot = self.chunk_slot(self.arena_chunks_global(), ptr_type.into(), index)?;
        let size_slot = self.chunk_slot(self.arena_sizes_global(), i64_type.into(), index)?;
        self.builder.build_store(base_slot, base)?;
        self.builder.build_store(size_slot, size)?;
        Ok(())
    }

    fn chunk_slot(
        &self,
        table: GlobalValue<'ctx>,
        element: inkwell::types::BasicTypeEnum<'ctx>,
        index: IntValue<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let zero = self.context.i64_type().const_zero();
        // SAFETY: every index reaching here is below `ARENA_MAX_CHUNKS`: the count and the
        // current chunk never pass it, and the advance checks before it steps.
        let slot = unsafe {
            self.builder.build_in_bounds_gep(
                inkwell::types::BasicType::array_type(&element, ARENA_MAX_CHUNKS),
                table.as_pointer_value(),
                &[zero, index],
                "chunk.slot",
            )?
        };
        Ok(slot)
    }

    /// Emit a helper's body without disturbing the function the caller is in the
    /// middle of. Every arena helper is created lazily: at the module's first `pool`
    /// block, or its first release.
    pub(crate) fn detached(&self, emit: impl FnOnce() -> CodegenResult<()>) -> CodegenResult<()> {
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
            let record = self.context.append_basic_block(function, "record");
            let done = self.context.append_basic_block(function, "done");
            let count_global = self.arena_counter_global(ARENA_COUNT_GLOBAL);

            self.builder.position_at_end(entry);
            let count = self
                .builder
                .build_load(i64_type, count_global.as_pointer_value(), "arena.count")?
                .into_int_value();
            let missing = self.builder.build_int_compare(
                IntPredicate::EQ,
                count,
                i64_type.const_zero(),
                "arena.missing",
            )?;
            self.builder
                .build_conditional_branch(missing, reserve, done)?;

            // A failed reservation leaves the count at zero, which routes every allocation
            // in the block to the heap instead of aborting the program.
            self.builder.position_at_end(reserve);
            let first = i64_type.const_int(ARENA_FIRST_CHUNK, false);
            let chunk = self.call_malloc(first, "arena.chunk")?;
            let failed = self.builder.build_is_null(chunk, "arena.failed")?;
            self.builder
                .build_conditional_branch(failed, done, record)?;

            self.builder.position_at_end(record);
            self.store_chunk(i64_type.const_zero(), chunk, first)?;
            self.builder.build_store(
                count_global.as_pointer_value(),
                i64_type.const_int(1, false),
            )?;
            self.builder.build_unconditional_branch(done)?;

            self.builder.position_at_end(done);
            let current = self.builder.build_load(
                i64_type,
                self.arena_counter_global(ARENA_CHUNK_GLOBAL)
                    .as_pointer_value(),
                "arena.current",
            )?;
            let offset = self.builder.build_load(
                i64_type,
                self.arena_offset_global().as_pointer_value(),
                "arena.offset",
            )?;
            let high = self.builder.build_left_shift(
                current.into_int_value(),
                i64_type.const_int(MARK_OFFSET_BITS, false),
                "mark.chunk",
            )?;
            let mark = self
                .builder
                .build_or(high, offset.into_int_value(), "arena.mark")?;
            self.builder.build_return(Some(&mark))?;
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
            let mark = self.param(function, 0)?.into_int_value();
            let chunk = self.builder.build_right_shift(
                mark,
                i64_type.const_int(MARK_OFFSET_BITS, false),
                false,
                "mark.chunk",
            )?;
            let offset = self.builder.build_and(
                mark,
                i64_type.const_int((1 << MARK_OFFSET_BITS) - 1, false),
                "mark.offset",
            )?;
            self.builder.build_store(
                self.arena_counter_global(ARENA_CHUNK_GLOBAL)
                    .as_pointer_value(),
                chunk,
            )?;
            self.builder
                .build_store(self.arena_offset_global().as_pointer_value(), offset)?;
            self.builder.build_return(None)?;
            Ok(())
        })?;
        Ok(function)
    }

    /// `malloc(size)`, as a pointer.
    fn call_malloc(&self, size: IntValue<'ctx>, name: &str) -> CodegenResult<PointerValue<'ctx>> {
        let malloc = self.get_or_declare_malloc();
        Ok(self
            .builder
            .build_call(malloc, &[size.into()], name)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("malloc returned void".into()))?
            .into_pointer_value())
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
            let size = self.param(function, 0)?.into_int_value();
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
            let first = self.param(function, 0)?.into_int_value();
            let second = self.param(function, 1)?.into_int_value();
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

    /// Fill an allocator's body: align the bump pointer up to `align`, take `size` bytes
    /// when they fit in the current chunk, move on to the next chunk (reserving it, at least
    /// twice the size and big enough for the request, when the chain ends there) when they
    /// do not, and hand the request to `fallback` when no chunk can be had.
    ///
    /// The absolute address is what gets aligned, not the offset: a chunk's own base
    /// carries only its allocator's guarantee, which can be weaker than the alignment
    /// a tensor buffer asks for.
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
        let start = self.context.append_basic_block(function, "start");
        let bump = self.context.append_basic_block(function, "bump");
        let take = self.context.append_basic_block(function, "take");
        let next = self.context.append_basic_block(function, "next");
        let step = self.context.append_basic_block(function, "step");
        let grow = self.context.append_basic_block(function, "grow");
        let record = self.context.append_basic_block(function, "record");
        let heap = self.context.append_basic_block(function, "heap");
        let count_global = self.arena_counter_global(ARENA_COUNT_GLOBAL);
        let chunk_global = self.arena_counter_global(ARENA_CHUNK_GLOBAL);
        let offset_global = self.arena_offset_global();
        let one = i64_type.const_int(1, false);

        self.builder.position_at_end(entry);
        let count = self
            .builder
            .build_load(i64_type, count_global.as_pointer_value(), "arena.count")?
            .into_int_value();
        let missing = self.builder.build_int_compare(
            IntPredicate::EQ,
            count,
            i64_type.const_zero(),
            "arena.missing",
        )?;
        // What a fresh chunk must hold for the request whatever its base's alignment. A
        // `size` so large the sum wraps is one libc refuses too.
        let need = self.builder.build_int_add(size, align, "arena.need")?;
        let wrapped =
            self.builder
                .build_int_compare(IntPredicate::ULT, need, size, "arena.wrapped")?;
        let refuse = self.builder.build_or(missing, wrapped, "arena.refuse")?;
        self.builder.build_conditional_branch(refuse, heap, start)?;

        self.builder.position_at_end(start);
        let current = self
            .builder
            .build_load(i64_type, chunk_global.as_pointer_value(), "arena.current")?
            .into_int_value();
        let offset = self
            .builder
            .build_load(i64_type, offset_global.as_pointer_value(), "arena.offset")?
            .into_int_value();
        self.builder.build_unconditional_branch(bump)?;

        // One chunk per pass: `index` is the chunk tried, `from` the offset free in it.
        self.builder.position_at_end(bump);
        let index_phi = self.builder.build_phi(i64_type, "chunk.index")?;
        let from_phi = self.builder.build_phi(i64_type, "chunk.from")?;
        let index = index_phi.as_basic_value().into_int_value();
        let from = from_phi.as_basic_value().into_int_value();
        let (base, capacity) = self.load_chunk(index)?;
        let base_int = self
            .builder
            .build_ptr_to_int(base, i64_type, "arena.base.int")?;
        let cursor = self.builder.build_int_add(base_int, from, "arena.cursor")?;
        let bias = self.builder.build_int_sub(align, one, "align.bias")?;
        let raised = self.builder.build_int_add(cursor, bias, "align.raised")?;
        // `-align` is `~(align - 1)` for the powers of two an alignment may be, which
        // is what clears the low bits without a second constant.
        let mask = self.builder.build_int_neg(align, "align.mask")?;
        let aligned = self.builder.build_and(raised, mask, "align.addr")?;
        let used = self
            .builder
            .build_int_sub(aligned, base_int, "arena.used")?;
        let end = self.builder.build_int_add(used, size, "arena.end")?;
        // Strictly below the capacity: a zero-size request at the very end would otherwise
        // get the limit itself, which `build_arena_owns` disowns and `free` then rejects.
        let in_chunk =
            self.builder
                .build_int_compare(IntPredicate::ULT, end, capacity, "arena.in.chunk")?;
        let unwrapped =
            self.builder
                .build_int_compare(IntPredicate::UGE, end, used, "arena.unwrapped")?;
        let fits = self.builder.build_and(in_chunk, unwrapped, "arena.fits")?;
        self.builder.build_conditional_branch(fits, take, next)?;

        self.builder.position_at_end(take);
        self.builder
            .build_store(chunk_global.as_pointer_value(), index)?;
        self.builder
            .build_store(offset_global.as_pointer_value(), end)?;
        let allocated = self
            .builder
            .build_int_to_ptr(aligned, ptr_type, "arena.ptr")?;
        self.builder.build_return(Some(&allocated))?;

        self.builder.position_at_end(next);
        let following = self.builder.build_int_add(index, one, "chunk.next")?;
        let at_end = self.builder.build_int_compare(
            IntPredicate::UGE,
            following,
            i64_type.const_int(u64::from(ARENA_MAX_CHUNKS), false),
            "chunk.at.end",
        )?;
        self.builder.build_conditional_branch(at_end, heap, step)?;

        // A chunk already reserved is reused from its start; past the last, one is reserved.
        self.builder.position_at_end(step);
        let reserved = self
            .builder
            .build_load(i64_type, count_global.as_pointer_value(), "arena.count")?
            .into_int_value();
        let have =
            self.builder
                .build_int_compare(IntPredicate::ULT, following, reserved, "chunk.have")?;
        self.builder.build_conditional_branch(have, bump, grow)?;

        self.builder.position_at_end(grow);
        let doubled = self
            .builder
            .build_int_add(capacity, capacity, "chunk.doubled")?;
        let short =
            self.builder
                .build_int_compare(IntPredicate::ULT, doubled, need, "chunk.short")?;
        let new_size = self
            .builder
            .build_select(short, need, doubled, "chunk.size.new")?
            .into_int_value();
        let chunk = self.call_malloc(new_size, "arena.chunk")?;
        let failed = self.builder.build_is_null(chunk, "arena.failed")?;
        self.builder
            .build_conditional_branch(failed, heap, record)?;

        self.builder.position_at_end(record);
        self.store_chunk(following, chunk, new_size)?;
        let grown = self
            .builder
            .build_int_add(following, one, "arena.count.new")?;
        self.builder
            .build_store(count_global.as_pointer_value(), grown)?;
        self.builder.build_unconditional_branch(bump)?;

        let zero = i64_type.const_zero();
        index_phi.add_incoming(&[(&current, start), (&following, step), (&following, record)]);
        from_phi.add_incoming(&[(&offset, start), (&zero, step), (&zero, record)]);

        self.builder.position_at_end(heap);
        let spilled = self
            .builder
            .build_call(fallback, fallback_args, "arena.spill")?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("allocator returned void".into()))?;
        self.builder.build_return(Some(&spilled))?;
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
            let target = self.param(function, 0)?.into_pointer_value();
            let owned = self.build_arena_owns(target)?;
            self.builder.build_conditional_branch(owned, done, heap)?;

            self.builder.position_at_end(heap);
            self.builder
                .build_call(libc_release, &[target.into()], "")?;
            self.builder.build_unconditional_branch(done)?;

            self.builder.position_at_end(done);
            self.builder.build_return(None)?;
            Ok(())
        })?;
        Ok(function)
    }

    /// Whether `target` points into a reserved chunk. Compares each whole reserved range
    /// rather than its live prefix: a pointer above the mark is memory the arena has
    /// already reclaimed, and it must not reach the allocator either.
    fn build_arena_owns(&self, target: PointerValue<'ctx>) -> CodegenResult<IntValue<'ctx>> {
        let owns = self.get_or_build_arena_owns()?;
        Ok(self
            .builder
            .build_call(owns, &[target.into()], "arena.owns")?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("arena owns returned void".into()))?
            .into_int_value())
    }

    /// `__neuro_arena_owns(ptr) -> i1`: the chunk walk behind [`Self::build_arena_owns`].
    /// A program that never reached a `pool` has no chunk, so the walk ends at once.
    fn get_or_build_arena_owns(&self) -> CodegenResult<FunctionValue<'ctx>> {
        if let Some(existing) = self.module.get_function(ARENA_OWNS_FN) {
            return Ok(existing);
        }
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let function = self.module.add_function(
            ARENA_OWNS_FN,
            self.context.bool_type().fn_type(&[ptr_type.into()], false),
            Some(Linkage::Internal),
        );
        self.detached(|| {
            let entry = self.context.append_basic_block(function, "entry");
            let walk = self.context.append_basic_block(function, "walk");
            let next = self.context.append_basic_block(function, "next");
            let yes = self.context.append_basic_block(function, "yes");
            let no = self.context.append_basic_block(function, "no");

            self.builder.position_at_end(entry);
            let target = self.param(function, 0)?.into_pointer_value();
            let target_int = self
                .builder
                .build_ptr_to_int(target, i64_type, "arena.target.int")?;
            let count = self
                .builder
                .build_load(
                    i64_type,
                    self.arena_counter_global(ARENA_COUNT_GLOBAL)
                        .as_pointer_value(),
                    "arena.count",
                )?
                .into_int_value();
            let none = self.builder.build_int_compare(
                IntPredicate::EQ,
                count,
                i64_type.const_zero(),
                "arena.none",
            )?;
            self.builder.build_conditional_branch(none, no, walk)?;

            self.builder.position_at_end(walk);
            let index_phi = self.builder.build_phi(i64_type, "chunk.index")?;
            let index = index_phi.as_basic_value().into_int_value();
            let (base, size) = self.load_chunk(index)?;
            let base_int = self
                .builder
                .build_ptr_to_int(base, i64_type, "arena.base.int")?;
            let limit = self.builder.build_int_add(base_int, size, "arena.limit")?;
            let above = self.builder.build_int_compare(
                IntPredicate::UGE,
                target_int,
                base_int,
                "arena.above",
            )?;
            let below = self.builder.build_int_compare(
                IntPredicate::ULT,
                target_int,
                limit,
                "arena.below",
            )?;
            let inside = self.builder.build_and(above, below, "arena.inside")?;
            self.builder.build_conditional_branch(inside, yes, next)?;

            self.builder.position_at_end(next);
            let following =
                self.builder
                    .build_int_add(index, i64_type.const_int(1, false), "chunk.next")?;
            let more = self.builder.build_int_compare(
                IntPredicate::ULT,
                following,
                count,
                "chunk.more",
            )?;
            self.builder.build_conditional_branch(more, walk, no)?;
            index_phi.add_incoming(&[(&i64_type.const_zero(), entry), (&following, next)]);

            self.builder.position_at_end(yes);
            self.builder
                .build_return(Some(&self.context.bool_type().const_int(1, false)))?;
            self.builder.position_at_end(no);
            self.builder
                .build_return(Some(&self.context.bool_type().const_zero()))?;
            Ok(())
        })?;
        Ok(function)
    }
}
