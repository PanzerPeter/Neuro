// Deterministic destruction (`Drop`): scope-exit destructor insertion.
//
// A binding of a `Drop` type runs its `{struct}__drop(&mut self)` destructor when
// its lexical scope ends on a *normal* exit: fall-through, `return`, `break`, or
// `continue` (a panic aborts without running destructors). Each owned
// binding carries an `i1` drop flag, set `false` when the value is moved out, so a
// moved value is not dropped twice. Every helper here is inert when the
// program declares no `Drop` types: the scope stack stays empty and nothing is
// emitted.

use ast_types::BinaryOp;
use inkwell::basic_block::BasicBlock;
use inkwell::types::{ArrayType, BasicTypeEnum};
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use inkwell::IntPredicate;
use neuro_hir::{HirExpr, HirExprKind, HirPlace, HirType};
use shared_types::Literal;

use crate::errors::{CodegenError, CodegenResult};
use crate::types::{CollectionKind, Type};

use super::context::{CodegenContext, DropEntry, DropTarget, HeldDrop};

/// The name an unbound temporary's drop entry is registered under. `__` is rejected in
/// every declared name, so no move site can reach the entry.
const UNBOUND_TEMPORARY: &str = "__unbound_temporary";

/// The `String` builder method that copies its bytes out into an owned `string`.
/// Matched by name here the way the builder type itself is matched by name.
const TO_OWNED_METHOD: &str = "to_string";
/// `string.clone()`, which copies the bytes into a buffer of their own.
const STRING_CLONE_METHOD: &str = "clone";

/// The collection readers that hand their result out inside an `Option`. Matched by
/// name against a collection receiver, the way the builder's `to_string` is.
const FALLIBLE_READERS: [&str; 2] = ["get", "pop"];

/// The drop flag of a `string` position inside a holder. It is the one flag that
/// starts `false`, so its name is what tells an armed position from a planned one.
pub(crate) const STRING_POSITION_FLAG: &str = "str.owned.flag";

impl<'ctx> CodegenContext<'ctx> {
    /// Open a new lexical scope. Paired with [`pop_drop_scope`].
    ///
    /// The name scope rides along with the drop scope because they delimit the same
    /// thing: every push here is a `{ }` a binding can be declared in, and a binding
    /// whose owner is released on the way out must stop being resolvable on the way
    /// out too.
    pub(crate) fn push_drop_scope(&mut self) {
        self.drop_scopes.push(Vec::new());
        self.name_scopes.push(Vec::new());
    }

    /// Close the innermost scope, restoring the names its bindings shadowed. Drops for
    /// a scope are emitted explicitly (see [`emit_top_scope_drops`] /
    /// [`emit_drops_through`]) before the scope is popped, so the destructors have
    /// already run against the bindings being unbound here.
    pub(crate) fn pop_drop_scope(&mut self) {
        let _ = self.drop_scopes.pop();
        if let Some(shadowed) = self.name_scopes.pop() {
            self.restore_bindings(shadowed);
        }
    }

    /// Resolve how a binding of `binding_ty` is destroyed at scope exit, or `None`
    /// when it owns nothing that needs releasing. The HIR carries the binding's
    /// resolved type, so this reads it directly.
    pub(crate) fn drop_target(&self, binding_ty: &HirType) -> Option<DropTarget> {
        self.drop_target_of(&Type::from_hir(binding_ty))
    }

    /// The same resolution over an already-lowered type, for the call sites that reach
    /// codegen with a [`Type`] rather than the HIR it came from.
    ///
    /// A newtype needs no arm: `Type::from_hir` erases it to its inner type, so a
    /// newtype wrapping an owner resolves to whatever the inner type resolves to.
    pub(crate) fn drop_target_of(&self, binding_ty: &Type) -> Option<DropTarget> {
        match binding_ty {
            // A collection always owns a heap buffer, independently of whether the
            // program declares any user `Drop` type.
            Type::Collection { .. } => Some(DropTarget::Collection(binding_ty.clone())),
            // So does a tensor: every construction allocates its buffer, and the type
            // says so, and there is no borrowed value of tensor type to confuse it with.
            Type::Tensor { .. } => Some(DropTarget::TensorBuffer),
            Type::Struct(name) if self.drop_types.contains(name) => {
                Some(DropTarget::UserDrop(name.clone()))
            }
            Type::Enum(name) if self.enum_holds_owner(name) => {
                Some(DropTarget::EnumPayload(name.clone()))
            }
            // Owns nothing itself, but something inside it does: the work is in the
            // held entries the registration plans alongside this target. A `string`
            // position counts here even though a `string` binding does not, because a
            // position has storage of its own that the store into it can arm.
            other if self.holds_owner(other) || self.holds_string_position(other) => {
                Some(DropTarget::Aggregate)
            }
            _ => None,
        }
    }

    /// Whether a value of `ty` owns anything that must be released, directly or through
    /// a position inside it.
    ///
    /// A `string` answers `false` on purpose: a string binding owns its buffer only when
    /// its initializer allocated one ([`produces_owned_string`]), which is a property of
    /// the expression and not of the type, so no position reached through a type can be
    /// proven to own one.
    pub(crate) fn holds_owner(&self, ty: &Type) -> bool {
        match ty {
            Type::Collection { .. } | Type::Tensor { .. } => true,
            Type::Struct(name) => {
                self.drop_types.contains(name)
                    || self
                        .struct_defs
                        .get(name)
                        .is_some_and(|fields| fields.iter().any(|(_, f)| self.holds_owner(f)))
            }
            Type::Enum(name) => self.enum_holds_owner(name),
            Type::Array { element, size } => *size > 0 && self.holds_owner(element),
            Type::Tuple(elements) => elements.iter().any(|e| self.holds_owner(e)),
            _ => false,
        }
    }

    /// Whether a value of `ty` has a `string` position inside it: a struct field, an
    /// array or tuple element, or one reached through those.
    ///
    /// Separate from [`holds_owner`] because the two answer different questions. A
    /// typed owner is destroyed wherever the type appears, an enum payload slot
    /// included, where no flag exists to guard it. A `string` position is destroyed only
    /// when the store into it provably allocated, which needs the flag, so it is planned
    /// where a flag can be planned and nowhere else.
    fn holds_string_position(&self, ty: &Type) -> bool {
        self.held_positions(ty).iter().any(|(_, position_ty)| {
            matches!(position_ty, Type::String) || self.holds_string_position(position_ty)
        })
    }

    /// Whether any variant of the named enum carries a payload field that owns something.
    fn enum_holds_owner(&self, name: &str) -> bool {
        self.type_mapper
            .enum_payload_types(name)
            .is_some_and(|variants| {
                variants
                    .iter()
                    .flatten()
                    .any(|field| self.holds_owner(field))
            })
    }

    /// Whether a value of `ty` runs a user destructor somewhere inside it.
    fn holds_user_drop(&self, ty: &Type) -> bool {
        match ty {
            Type::Struct(name) => {
                self.drop_types.contains(name)
                    || self
                        .struct_defs
                        .get(name)
                        .is_some_and(|fields| fields.iter().any(|(_, t)| self.holds_user_drop(t)))
            }
            Type::Enum(name) => self
                .type_mapper
                .enum_payload_types(name)
                .is_some_and(|variants| variants.iter().flatten().any(|t| self.holds_user_drop(t))),
            Type::Tuple(elements) => elements.iter().any(|t| self.holds_user_drop(t)),
            Type::Array { element, .. } => self.holds_user_drop(element),
            _ => false,
        }
    }

    /// Destroy `value`, the result of `expr`, when it is a fresh value no binding owns
    /// and it runs a user destructor: a statement nothing reads, or a temporary only read
    /// from (`make().id`). No scope exit reaches such a value, so without this its
    /// destructor never ran (BUG-047).
    ///
    /// Only a call, a struct literal or an enum construction is fresh; anything else may
    /// be a read of a value some binding still owns. Inside a `pool` it does nothing: the
    /// arena, not a scope, owns what the block allocates.
    pub(crate) fn drop_unbound_temporary(
        &mut self,
        expr: &HirExpr,
        value: BasicValueEnum<'ctx>,
    ) -> CodegenResult<()> {
        let fresh = matches!(
            expr.kind,
            HirExprKind::Call { .. }
                | HirExprKind::StructLiteral { .. }
                | HirExprKind::EnumConstruct { .. }
        );
        let ty = Type::from_hir(&expr.ty);
        if !fresh || self.pool_depth > 0 || !self.holds_user_drop(&ty) {
            return Ok(());
        }
        if self.current_block_terminated() {
            return Ok(());
        }
        let slot = self.entry_alloca(value.get_type(), "unbound.tmp")?;
        self.builder.build_store(slot, value)?;
        let depth = self.drop_scopes.len();
        self.push_drop_scope();
        self.register_owned_binding(UNBOUND_TEMPORARY, slot, &ty)?;
        self.emit_drops_through(depth)?;
        self.pop_drop_scope();
        Ok(())
    }

    /// Plan the drops for every owner held inside a binding of `ty`, flattening the
    /// field and element paths under `storage_ptr` into one entry each.
    ///
    /// The address of each position is GEP'd here, at the binding site, rather than at
    /// the drop sites: the offsets are constant, and the binding site dominates every
    /// scope exit, reassignment and move that can later reach the value.
    ///
    /// The recursion terminates because a position's type is strictly smaller than its
    /// holder's; a type containing itself by value has no finite layout and is rejected
    /// long before codegen.
    fn plan_held_drops(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
        ty: &Type,
        path: &mut Vec<String>,
        out: &mut Vec<HeldDrop<'ctx>>,
    ) -> CodegenResult<()> {
        let positions = self.held_positions(ty);
        if positions.is_empty() {
            return Ok(());
        }

        let holder_llvm = self.get_any_llvm_type(ty)?;
        for (index, (segment, position_ty)) in positions.into_iter().enumerate() {
            let is_string = matches!(position_ty, Type::String);
            if !is_string
                && !self.holds_owner(&position_ty)
                && !self.holds_string_position(&position_ty)
            {
                continue;
            }
            let position_ptr = self.aggregate_position_ptr(
                holder_llvm,
                storage_ptr,
                index as u32,
                &format!("held.{}.ptr", segment),
            )?;
            path.push(segment);
            // A `string` position starts DISARMED: the type proves nothing, so the
            // position owns a buffer only once a store that provably allocated has
            // armed it. Every other target's type is the proof, so it starts armed.
            if is_string {
                let flag_ptr = self.disarmed_drop_flag()?;
                out.push(HeldDrop {
                    path: path.clone(),
                    storage_ptr: position_ptr,
                    flag_ptr,
                    target: DropTarget::HeapString,
                });
                let _ = path.pop();
                continue;
            }
            if let Some(target) = self.drop_target_of(&position_ty) {
                if !matches!(target, DropTarget::Aggregate) {
                    let flag_ptr = self.arm_drop_flag()?;
                    out.push(HeldDrop {
                        path: path.clone(),
                        storage_ptr: position_ptr,
                        flag_ptr,
                        target,
                    });
                }
            }
            self.plan_held_drops(position_ptr, &position_ty, path, out)?;
            let _ = path.pop();
        }
        Ok(())
    }

    /// Register a binding that owns something, or holds something that does, for
    /// destruction at scope exit. A binding of `ty` that owns nothing registers nothing.
    pub(crate) fn register_owned_binding(
        &mut self,
        name: &str,
        storage_ptr: PointerValue<'ctx>,
        ty: &Type,
    ) -> CodegenResult<()> {
        let Some(target) = self.drop_target_of(ty) else {
            return Ok(());
        };
        let mut held = Vec::new();
        self.plan_held_drops(storage_ptr, ty, &mut Vec::new(), &mut held)?;
        let flag_ptr = self.arm_drop_flag()?;
        if let Some(scope) = self.drop_scopes.last_mut() {
            scope.push(DropEntry {
                name: name.to_string(),
                storage_ptr,
                flag_ptr,
                target,
                held,
            });
        }
        Ok(())
    }

    /// A fresh `i1` drop-flag slot in the entry block, initialized to `true`.
    fn arm_drop_flag(&mut self) -> CodegenResult<PointerValue<'ctx>> {
        let bool_ty = self.context.bool_type();
        let flag_ptr = self.entry_alloca(bool_ty, "drop.flag")?;
        self.builder
            .build_store(flag_ptr, bool_ty.const_int(1, false))?;
        Ok(flag_ptr)
    }

    /// The same slot, initialized to `false`, for a position that owns nothing until a
    /// store proves it does. The initializing store is in the entry block so a position
    /// a conditional never writes still reads a definite `false` at scope exit.
    ///
    /// Named apart from [`arm_drop_flag`]'s slot because the two carry different claims:
    /// this one says "not yet", and a `store i1 true` against it is the whole record of
    /// which `string` positions a program actually owns.
    fn disarmed_drop_flag(&mut self) -> CodegenResult<PointerValue<'ctx>> {
        let bool_ty = self.context.bool_type();
        let flag_ptr = self.entry_alloca(bool_ty, STRING_POSITION_FLAG)?;
        self.builder.build_store(flag_ptr, bool_ty.const_zero())?;
        Ok(flag_ptr)
    }

    /// Whether evaluating `expr` always yields a freshly `malloc`'d string buffer
    /// that nothing else aliases, making its consumer responsible for releasing it.
    ///
    /// Deliberately conservative: it answers `true` only for the producers that
    /// allocate unconditionally. A `.rodata` literal, a variable and a `slice` borrowing
    /// its source all answer `false` and are never freed. The asymmetry is the point: a
    /// missed `true` leaks a buffer, while a wrong `true` frees `.rodata` or
    /// double-frees, so only provable ownership counts.
    ///
    /// A call to a user function is the one answer that is not read off the expression:
    /// the body decides, and [`crate::codegen::string_ownership`] has already read every
    /// body in the program to say which functions allocate on every return path.
    pub(crate) fn produces_owned_string(&self, expr: &HirExpr) -> bool {
        match &expr.kind {
            // A `string` read out of a collection slot is copied out of it
            // ([`value_from_collection_slot`](CodegenContext::value_from_collection_slot)),
            // so the reader owns the copy and the collection keeps its own. An ARRAY index
            // is deliberately not this shape: an array's `string` position is owned by the
            // holder's own drop entry and a read of it aliases that buffer.
            HirExprKind::Index { object, .. } => {
                matches!(Type::from_hir(&expr.ty), Type::String)
                    && matches!(
                        Type::from_hir(&object.ty).referent(),
                        Type::Collection { .. }
                    )
            }
            // `codegen_interp_string` concatenates every piece into one fresh buffer,
            // and does so unconditionally: even a hole-free interpolation allocates.
            HirExprKind::InterpString { .. } => true,
            // `+` yielding a `string` is `codegen_string_concat`, which always allocates
            // a `len1 + len2` buffer. No numeric addition produces a `string`, so the
            // result type alone identifies the concatenation.
            HirExprKind::Binary {
                op: BinaryOp::Add, ..
            } => matches!(Type::from_hir(&expr.ty), Type::String),
            // `String::to_string` is `codegen_string_to_owned`, which copies the
            // builder's live bytes into a buffer of their own on every call. It reaches
            // here as the `FieldAccess` callee a method call lowers to; a program that
            // declares its own `String` shadows the builder, and its receiver is then a
            // `Type::Struct` that this arm does not match.
            HirExprKind::Call { callee, args } => match &callee.kind {
                HirExprKind::Path { type_name, member } => self
                    .string_ownership
                    .returns_owned(&format!("{}__{}", type_name, member)),
                HirExprKind::FieldAccess { object, field } => {
                    let receiver = Type::from_hir(&object.ty);
                    // A user method is summarized under the name its call mangles.
                    if let Type::Struct(type_name) = receiver.referent() {
                        return self
                            .string_ownership
                            .returns_owned(&format!("{}__{}", type_name, field));
                    }
                    let builder_copy = field == TO_OWNED_METHOD
                        && matches!(
                            receiver.referent(),
                            Type::Collection {
                                kind: CollectionKind::String,
                                ..
                            }
                        );
                    // `string.clone()` copies the bytes into a buffer of their own.
                    let string_clone =
                        field == STRING_CLONE_METHOD && matches!(receiver.referent(), Type::String);
                    args.is_empty() && (builder_copy || string_clone)
                }
                // A function whose every return path allocates hands the buffer to its
                // caller, which is the one storing position that no expression shape
                // can settle. The summary excludes a name a local binding shadows, so a
                // closure called through the indirect path is not mistaken for it.
                HirExprKind::Variable(name) => self.string_ownership.returns_owned(name),
                _ => false,
            },
            _ => false,
        }
    }

    /// Whether `expr` yields an `Option` whose `string` payload the binding that takes it
    /// out owns.
    ///
    /// A collection's fallible readers are the shapes that qualify, and for the two
    /// reasons the boundary rule gives: `get` copies the slot's bytes into a buffer of
    /// their own, and `pop` takes the slot's buffer with it by shrinking past the slot.
    /// Either way the collection no longer reaches what the payload holds.
    pub(crate) fn produces_owned_option_payload(&self, expr: &HirExpr) -> bool {
        let HirExprKind::Call { callee, .. } = &expr.kind else {
            return false;
        };
        let HirExprKind::FieldAccess { object, field } = &callee.kind else {
            return false;
        };
        if !FALLIBLE_READERS.contains(&field.as_str()) {
            return false;
        }
        matches!(
            Type::from_hir(&object.ty).referent(),
            Type::Collection { kind, .. } if !matches!(kind, CollectionKind::String)
        )
    }

    /// Release the buffer behind an owned `string` that a consumer has finished reading
    /// and that no binding will ever name.
    ///
    /// Emitted at the consumers that provably copy the bytes out and retain none of
    /// them: a `+` operand, an `==` operand, a `.len()` or `.clone()` receiver, a
    /// `push_str` argument, and a statement whose value is discarded. A consumer that may STORE the fat
    /// pointer instead (a by-value call argument, a collection element, a struct field)
    /// is deliberately not one of them: the buffer outlives the expression there, so
    /// releasing it would hand out a dangling pointer. It leaks instead, which is the
    /// direction [`produces_owned_string`] already errs in.
    pub(crate) fn release_string_temporary(
        &self,
        expr: &HirExpr,
        fat_ptr: BasicValueEnum<'ctx>,
    ) -> CodegenResult<()> {
        if !self.produces_owned_string(expr) {
            return Ok(());
        }
        let BasicValueEnum::StructValue(fat_ptr) = fat_ptr else {
            return Err(CodegenError::InternalError(
                "an owned string temporary reached its release as something other than a fat pointer"
                    .to_string(),
            ));
        };
        let buffer = self
            .builder
            .build_extract_value(fat_ptr, 0, "str.tmp.buf")?;
        let free_fn = self.release_fn()?;
        self.builder
            .build_call(free_fn, &[buffer.into()], "")
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to free a string temporary: {}", e))
            })?;
        Ok(())
    }

    /// Release the buffer an owned `string` argument handed to `callee` once the call
    /// has returned.
    ///
    /// The callee's frame dies before this point, so the only way the buffer can still
    /// be reachable is if the callee put it somewhere that outlives the call: its return
    /// value, or a place behind one of its reference parameters.
    /// [`crate::codegen::string_ownership`] has read the body to rule both out, and
    /// answers `false` for every shape it does not recognise, so an unanalysable callee
    /// leaks the buffer rather than being handed a dangling one.
    pub(crate) fn release_owned_arguments(
        &mut self,
        callee: &str,
        args: &[HirExpr],
        values: &[BasicValueEnum<'ctx>],
    ) -> CodegenResult<()> {
        for (index, arg) in args.iter().enumerate() {
            if !self.string_ownership.param_is_read_only(callee, index) {
                continue;
            }
            if !self.produces_owned_string(arg) {
                continue;
            }
            let Some(value) = values.get(index) else {
                continue;
            };
            self.release_string_temporary(arg, *value)?;
        }
        Ok(())
    }

    /// Release the heap buffer behind a `string` fat pointer held in `storage_ptr`.
    ///
    /// Emitted only for a binding registered as [`DropTarget::HeapString`], whose
    /// initializer [`produces_owned_string`] proved allocates.
    fn emit_heap_string_free(&mut self, storage_ptr: PointerValue<'ctx>) -> CodegenResult<()> {
        let fat_ptr_ty = self.string_fat_ptr_type();
        let buffer =
            self.builder
                .build_struct_gep(fat_ptr_ty, storage_ptr, 0, "str.drop.buf.addr")?;
        let buffer = self.builder.build_load(
            self.context.ptr_type(inkwell::AddressSpace::default()),
            buffer,
            "str.drop.buf",
        )?;
        let free_fn = self.release_fn()?;
        self.builder
            .build_call(free_fn, &[buffer.into()], "")
            .map_err(|e| CodegenError::LlvmError(format!("failed to free string: {}", e)))?;
        Ok(())
    }

    /// Release the tensor a binding in `storage_ptr` owns, through the DLPack `deleter`
    /// its own handle carries.
    ///
    /// The binding's storage holds the handle itself, so it is one load away. Dispatching
    /// through the field rather than calling `free` here is what makes the release a
    /// scope exit performs the same one a foreign owner of the handle performs. Emitted
    /// only for a binding registered as [`DropTarget::TensorBuffer`], whose flag a move
    /// at any of the move sites has already cleared.
    fn emit_tensor_buffer_free(&mut self, storage_ptr: PointerValue<'ctx>) -> CodegenResult<()> {
        let handle = self
            .builder
            .build_load(
                self.context.ptr_type(inkwell::AddressSpace::default()),
                storage_ptr,
                "tensor.drop.handle",
            )?
            .into_pointer_value();
        self.build_dlpack_release(handle)
    }

    /// Record an owned `Drop`-typed binding for destruction at scope exit.
    ///
    /// Allocates the binding's `i1` drop flag (initialized `true`) and pushes a
    /// [`DropEntry`] onto the innermost scope, handing the flag back. The caller must
    /// have verified the binding's type needs one via [`drop_target`].
    pub(crate) fn register_local_drop(
        &mut self,
        name: &str,
        storage_ptr: PointerValue<'ctx>,
        target: DropTarget,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let flag_ptr = self.arm_drop_flag()?;
        if let Some(scope) = self.drop_scopes.last_mut() {
            scope.push(DropEntry {
                name: name.to_string(),
                storage_ptr,
                flag_ptr,
                target,
                held: Vec::new(),
            });
        }
        Ok(flag_ptr)
    }

    /// The drop entry of the binding `name` resolves to at this point, if it owns one.
    ///
    /// Matched on the binding's storage as well as its name: a binding that owns
    /// nothing (a borrow, a scalar) registers no entry, so a lookup by name alone would
    /// fall through it to an outer owner it shadows, and a store or a move through the
    /// inner name would then release or disown the outer value.
    fn live_drop_entry(&self, name: &str) -> Option<&DropEntry<'ctx>> {
        let storage = self.variables.get(name)?;
        self.drop_scopes
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .find(|entry| entry.name == name)
            .filter(|entry| entry.storage_ptr == *storage)
    }

    /// The drop flag of the binding `expr` names, when that binding may own a heap
    /// `string`.
    fn owned_string_flag_ptr(&self, expr: &HirExpr) -> Option<PointerValue<'ctx>> {
        let HirExprKind::Variable(name) = &expr.kind else {
            return None;
        };
        self.live_drop_entry(name)
            .filter(|entry| matches!(entry.target, DropTarget::HeapString))
            .map(|entry| entry.flag_ptr)
    }

    /// Whether `expr` names a binding that may own a heap `string`, so a move out of it
    /// may carry a buffer.
    pub(crate) fn names_an_owned_string(&self, expr: &HirExpr) -> bool {
        self.owned_string_flag_ptr(expr).is_some()
    }

    /// The current value of the drop flag of the binding `expr` names, when that binding
    /// may own a heap `string`: whether it owns the buffer at this point of the run.
    /// `None` for anything else, which owns nothing a move could carry.
    pub(crate) fn load_owned_string_flag(
        &mut self,
        expr: &HirExpr,
    ) -> CodegenResult<Option<IntValue<'ctx>>> {
        let Some(flag_ptr) = self.owned_string_flag_ptr(expr) else {
            return Ok(None);
        };
        let owns = self
            .builder
            .build_load(self.context.bool_type(), flag_ptr, "str.owns")?
            .into_int_value();
        Ok(Some(owns))
    }

    /// Whether `expr` names a binding the enclosing `pool` already registered, making
    /// this initializer a transfer of a live registration rather than a construction.
    pub(crate) fn moves_a_pool_registration(&self, expr: &HirExpr) -> bool {
        let HirExprKind::Variable(name) = &expr.kind else {
            return false;
        };
        self.live_drop_entry(name)
            .is_some_and(|entry| matches!(entry.target, DropTarget::PoolRegistered))
    }

    /// The binding a store's place is rooted in, or `None` where the place reaches
    /// storage no binding here names — a `*p` write, whose referent belongs to whoever
    /// handed the reference over.
    pub(crate) fn place_root(place: &HirPlace) -> Option<&str> {
        match place {
            HirPlace::Var { name, .. } => Some(name),
            HirPlace::Field { object, .. }
            | HirPlace::Index { object, .. }
            | HirPlace::TensorIndex { object, .. } => {
                Self::moved_place(object).map(|(root, _)| root)
            }
            HirPlace::Deref { .. } => None,
        }
    }

    /// Resolve the place `expr` names as a binding plus the field and element path
    /// taken through it, or `None` when it is not rooted at a plain binding.
    ///
    /// A non-constant index yields `None` rather than a path: `a[i]` names no
    /// particular element, so the language makes a move through it a move of the whole
    /// binding, which is what a `None` path spells at the one caller below.
    fn moved_place(expr: &HirExpr) -> Option<(&str, Option<Vec<String>>)> {
        match &expr.kind {
            HirExprKind::Variable(name) => Some((name, Some(Vec::new()))),
            HirExprKind::FieldAccess { object, field } => Self::extend_place(object, field),
            HirExprKind::TupleIndex { object, index } => {
                Self::extend_place(object, &index.to_string())
            }
            HirExprKind::Index { object, index } => match &index.kind {
                HirExprKind::Literal(Literal::Integer(value, _)) if *value >= 0 => {
                    Self::extend_place(object, &value.to_string())
                }
                _ => {
                    let (root, _) = Self::moved_place(object)?;
                    Some((root, None))
                }
            },
            _ => None,
        }
    }

    /// Append one accessor segment to the place `object` names, keeping the collapse to
    /// the whole binding that an unevaluable index already forced.
    fn extend_place<'e>(
        object: &'e HirExpr,
        segment: &str,
    ) -> Option<(&'e str, Option<Vec<String>>)> {
        let (root, path) = Self::moved_place(object)?;
        Some((
            root,
            path.map(|mut path| {
                path.push(segment.to_string());
                path
            }),
        ))
    }

    /// Clear the drop flag of the place `expr` names, if it is a tracked owner being
    /// moved out of. Mirrors the move sites the type checker validates.
    ///
    /// A path into a holder disowns that position and everything beneath it, leaving the
    /// holder's siblings armed — that is what makes destructuring a sequence of ordinary
    /// moves. Naming the binding itself, or naming a place through an index the compiler
    /// cannot evaluate, disowns the whole binding.
    pub(crate) fn mark_moved_for_drop(&mut self, expr: &HirExpr) {
        if self.drop_scopes.is_empty() {
            return;
        }
        let Some((name, path)) = Self::moved_place(expr) else {
            return;
        };
        // The whole-binding collapse stands in for "some element left"; a read that moves
        // nothing out must not disown the holder, or none of its owners is ever released.
        if path.is_none() && self.read_moves_nothing(expr) {
            return;
        }

        let mut flags: Vec<PointerValue<'ctx>> = Vec::new();
        let entry = self.live_drop_entry(name);
        let Some(entry) = entry else {
            return;
        };
        let prefix = path.unwrap_or_default();
        if prefix.is_empty() {
            flags.push(entry.flag_ptr);
        }
        flags.extend(
            entry
                .held
                .iter()
                .filter(|held| held.path.starts_with(&prefix))
                .map(|held| held.flag_ptr),
        );

        let zero = self.context.bool_type().const_zero();
        for flag_ptr in flags {
            let _ = self.builder.build_store(flag_ptr, zero);
        }
    }

    /// Whether reading `expr` leaves its holder owning everything it owned: a `Copy`
    /// value is copied out, and a collection's `string` element is copied into a buffer
    /// of the reader's own.
    fn read_moves_nothing(&self, expr: &HirExpr) -> bool {
        let ty = Type::from_hir(&expr.ty);
        if !matches!(ty, Type::String) {
            return self.drop_target_of(&ty).is_none();
        }
        matches!(
            &expr.kind,
            HirExprKind::Index { object, .. }
                if matches!(Type::from_hir(&object.ty).referent(), Type::Collection { .. })
        )
    }

    /// Whether `expr` reads a value out of a position another binding already owns, so
    /// that the holder's own drop is what releases it.
    ///
    /// The distinction matters where an owned value is copied into a temporary to be
    /// read: the copy aliases the holder's buffer, so registering it as an owner in its
    /// own right would release that buffer twice.
    pub(crate) fn reads_a_held_place(&self, expr: &HirExpr) -> bool {
        let Some((name, path)) = Self::moved_place(expr) else {
            return false;
        };
        // A bare binding is not a held place: its own entry is what releases it.
        if path.as_ref().is_some_and(|segments| segments.is_empty()) {
            return false;
        }
        let prefix = path.unwrap_or_default();
        self.live_drop_entry(name)
            .is_some_and(|entry| entry.held.iter().any(|held| held.path.starts_with(&prefix)))
    }

    /// Release the buffer of a tensor receiver that no binding owns.
    ///
    /// A reduction or a sort READS its receiver and builds a fresh result, so a receiver
    /// that was constructed for the call — an operator result, a value a call returned, a
    /// constructor, another reduction — has no drop entry to release it at scope exit and
    /// would otherwise leak once per evaluation. Only the shapes that provably allocate a
    /// buffer of their own qualify: a place expression names a binding whose own entry
    /// covers it, and every other shape may hand back a buffer something else still owns.
    pub(crate) fn release_receiver_temporary(
        &mut self,
        receiver: &HirExpr,
        handle: PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        if matches!(Type::from_hir(&receiver.ty), Type::Reference { .. }) {
            return Ok(());
        }
        if !builds_its_own_buffer(&receiver.kind) {
            return Ok(());
        }
        self.build_dlpack_release(handle)
    }

    /// Disown every owner a binding holds, without touching the binding's own flag, and
    /// hand back each cleared flag with the value it held.
    ///
    /// A `match` that binds a payload by value is the one move site with no place
    /// expression to name: which position left depends on the tag. Clearing all of them
    /// is the conservative answer for an arm that binds — a part of the variant it did
    /// not bind leaks rather than being released twice. An arm that binds nothing takes
    /// nothing, and stores the returned values back.
    pub(crate) fn mark_held_moved_for_drop(
        &mut self,
        name: &str,
    ) -> CodegenResult<Vec<(PointerValue<'ctx>, IntValue<'ctx>)>> {
        let flags: Vec<PointerValue<'ctx>> = self
            .live_drop_entry(name)
            .map(|entry| {
                let mut flags: Vec<PointerValue<'ctx>> =
                    entry.held.iter().map(|held| held.flag_ptr).collect();
                if matches!(entry.target, DropTarget::EnumPayload(_)) {
                    flags.push(entry.flag_ptr);
                }
                flags
            })
            .unwrap_or_default();

        let bool_ty = self.context.bool_type();
        let mut saved = Vec::with_capacity(flags.len());
        for flag_ptr in flags {
            let owned = self
                .builder
                .build_load(bool_ty, flag_ptr, "match.owned")?
                .into_int_value();
            saved.push((flag_ptr, owned));
        }
        for (flag_ptr, _) in &saved {
            self.builder.build_store(*flag_ptr, bool_ty.const_zero())?;
        }
        Ok(saved)
    }

    /// Release the value a reassigned binding is about to lose, and hand back its drop
    /// flag and target so the caller can re-arm them for the incoming value.
    ///
    /// The caller must have evaluated the new value BEFORE calling this: a reassignment
    /// is allowed to read the binding it overwrites (`s = s + "!"` concatenates out of
    /// the very buffer this then frees), so releasing first would hand the concatenation
    /// freed memory.
    ///
    /// Two bindings answer `None` and keep their prior value. One the drop pass never
    /// tracked owns nothing to release. A pool-registered one is released by the arena's
    /// LIFO sweep at the block's closing brace and by nothing else, so a per-assignment
    /// release would free a pointer the arena still holds.
    pub(crate) fn drop_reassigned_value(
        &mut self,
        name: &str,
    ) -> CodegenResult<Option<(PointerValue<'ctx>, DropTarget)>> {
        let entry = self.live_drop_entry(name).map(Self::snapshot_entry);

        let Some(pending) = entry else {
            return Ok(None);
        };
        let (_, flag_ptr, target) = pending[0].clone();
        if matches!(target, DropTarget::PoolRegistered) {
            return Ok(None);
        }

        for (storage_ptr, flag_ptr, target) in pending {
            self.emit_one_drop(storage_ptr, flag_ptr, &target)?;
        }
        Ok(Some((flag_ptr, target)))
    }

    /// Release what `name`'s held position at `path` is about to lose to a field
    /// assignment, then re-arm that position for the value replacing it.
    ///
    /// Ordered like [`drop_reassigned_value`] and for the same reason: the caller has
    /// already evaluated the new value, so the field may be read on the way to
    /// replacing itself. The storage the position addresses does not move, so re-arming
    /// is a flag store and needs no second plan.
    ///
    /// A `string` position is released here but left disarmed: `p.name = "lit"`
    /// displaces an owner and takes on none, so what re-arms it is the incoming value's
    /// own shape, through [`arm_stored_string_positions`].
    pub(crate) fn drop_displaced_held_value(
        &mut self,
        name: &str,
        path: &[String],
    ) -> CodegenResult<()> {
        let pending: Vec<(PointerValue<'ctx>, PointerValue<'ctx>, DropTarget)> = self
            .live_drop_entry(name)
            .map(|entry| {
                entry
                    .held
                    .iter()
                    .filter(|held| held.path.starts_with(path))
                    .map(|held| (held.storage_ptr, held.flag_ptr, held.target.clone()))
                    .collect()
            })
            .unwrap_or_default();

        let armed = self.context.bool_type().const_int(1, false);
        for (storage_ptr, flag_ptr, target) in pending {
            self.emit_one_drop(storage_ptr, flag_ptr, &target)?;
            if matches!(target, DropTarget::HeapString) {
                continue;
            }
            self.builder.build_store(flag_ptr, armed)?;
        }
        Ok(())
    }

    /// Release what the position `object.segment` loses to a store, then arm it for
    /// `value`. A segment is a field name or a literal array position, at any depth.
    ///
    /// Only a position inside a binding this frame owns has flags to consult; a place
    /// reached through a borrow belongs to whoever lent it and is left alone here.
    pub(crate) fn displace_held_position(
        &mut self,
        object: &HirExpr,
        segment: &str,
        value: &HirExpr,
    ) -> CodegenResult<()> {
        let Some((root, Some(path))) = Self::extend_place(object, segment) else {
            return Ok(());
        };
        let root = root.to_string();
        self.drop_displaced_held_value(&root, &path)?;
        // A `string` position the store hands a fresh buffer takes ownership of it here:
        // the release above left every such position disarmed, because the type says
        // nothing about what the incoming value owns.
        self.arm_stored_string_positions(&root, &path, value)
    }

    /// [`displace_held_position`] for an array element whose position is known only at
    /// run time: the address being written is compared against each element the owner
    /// tracks, and the one it matches is released and re-armed.
    pub(crate) fn displace_array_element(
        &mut self,
        object: &HirExpr,
        array_llvm: ArrayType<'ctx>,
        array_ptr: PointerValue<'ctx>,
        element_ptr: PointerValue<'ctx>,
        value: &HirExpr,
    ) -> CodegenResult<()> {
        let Some((root, Some(prefix))) = Self::moved_place(object) else {
            return Ok(());
        };
        let root = root.to_string();
        let Some(entry) = self.live_drop_entry(&root) else {
            return Ok(());
        };
        let tracked: Vec<Vec<String>> = (0..array_llvm.len())
            .map(|k| {
                let mut path = prefix.clone();
                path.push(k.to_string());
                path
            })
            .filter(|path| entry.held.iter().any(|held| held.path.starts_with(path)))
            .collect();
        if tracked.is_empty() {
            return Ok(());
        }

        let parent_fn = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("element store emitted outside a function".to_string())
        })?;
        let i64t = self.context.i64_type();
        let written = self
            .builder
            .build_ptr_to_int(element_ptr, i64t, "elem.addr")?;
        // ponytail: one compare per tracked element; switch on the index instead if
        // arrays of owners grow large enough for the chain to matter.
        for path in tracked {
            let position: u64 = path
                .last()
                .and_then(|segment| segment.parse().ok())
                .ok_or_else(|| {
                    CodegenError::InternalError("array position is not a number".to_string())
                })?;
            // SAFETY: `position` is below the array's length, so the GEP stays inside it.
            let position_ptr = unsafe {
                self.builder.build_in_bounds_gep(
                    array_llvm,
                    array_ptr,
                    &[i64t.const_zero(), i64t.const_int(position, false)],
                    "held.elem.ptr",
                )?
            };
            let position_addr =
                self.builder
                    .build_ptr_to_int(position_ptr, i64t, "held.elem.addr")?;
            let hit = self.builder.build_int_compare(
                IntPredicate::EQ,
                written,
                position_addr,
                "held.elem.hit",
            )?;
            let release_bb = self.context.append_basic_block(parent_fn, "displace.elem");
            let next_bb = self.context.append_basic_block(parent_fn, "displace.next");
            self.builder
                .build_conditional_branch(hit, release_bb, next_bb)?;
            self.builder.position_at_end(release_bb);
            self.drop_displaced_held_value(&root, &path)?;
            self.arm_stored_string_positions(&root, &path, value)?;
            self.builder.build_unconditional_branch(next_bb)?;
            self.builder.position_at_end(next_bb);
        }
        Ok(())
    }

    /// The drop work one entry stands for, innermost-holder first: the binding's own
    /// release, then each owner it holds in declaration order.
    fn snapshot_entry(
        entry: &DropEntry<'ctx>,
    ) -> Vec<(PointerValue<'ctx>, PointerValue<'ctx>, DropTarget)> {
        let mut pending = vec![(entry.storage_ptr, entry.flag_ptr, entry.target.clone())];
        pending.extend(
            entry
                .held
                .iter()
                .map(|held| (held.storage_ptr, held.flag_ptr, held.target.clone())),
        );
        pending
    }

    /// Arm `flag_ptr` for the value a reassignment has just stored, so scope exit
    /// releases the incoming value rather than the one already released.
    ///
    /// Ownership is re-derived from the assigned expression, not assumed: a `string`
    /// binding owns a buffer only when the expression that produced it allocated one, so
    /// reassigning a heap string from a `.rodata` literal must leave the flag clear.
    /// Every other target's type proves the ownership on its own.
    pub(crate) fn rearm_drop_flag(
        &mut self,
        flag_ptr: PointerValue<'ctx>,
        target: &DropTarget,
        value: &HirExpr,
    ) -> CodegenResult<()> {
        let owns_new_value = match target {
            DropTarget::HeapString => self.produces_owned_string(value),
            _ => true,
        };
        let bool_ty = self.context.bool_type();
        self.builder
            .build_store(flag_ptr, bool_ty.const_int(owns_new_value as u64, false))?;
        Ok(())
    }

    /// Re-arm every held position of a reassigned binding, so scope exit releases what
    /// the incoming holder holds rather than what the released one held.
    ///
    /// Unconditional for a position whose own type proves it owns something, which no
    /// assignment can change. A `string` position is skipped and left disarmed: its
    /// ownership comes from the expression stored into it, so
    /// [`arm_stored_string_positions`] arms it from the incoming value instead.
    pub(crate) fn rearm_held_drop_flags(&mut self, name: &str) -> CodegenResult<()> {
        let flags: Vec<PointerValue<'ctx>> = self
            .live_drop_entry(name)
            .map(|entry| {
                entry
                    .held
                    .iter()
                    .filter(|held| !matches!(held.target, DropTarget::HeapString))
                    .map(|held| held.flag_ptr)
                    .collect()
            })
            .unwrap_or_default();

        let armed = self.context.bool_type().const_int(1, false);
        for flag_ptr in flags {
            self.builder.build_store(flag_ptr, armed)?;
        }
        Ok(())
    }

    /// Arm the `string` positions of `name` that `value` stored a freshly allocated
    /// buffer into, so the holder's destruction releases them.
    ///
    /// Reads the holder's literal shape rather than its type: `P { name: a + b }` owns
    /// the concatenation and `P { name: "lit" }` owns nothing, and only the expression
    /// tells the two apart. A holder built any other way — returned by a call, read out
    /// of another value, selected by an `if` — arms nothing, because the buffer behind
    /// its positions may be one something else still owns.
    pub(crate) fn arm_stored_string_positions(
        &mut self,
        name: &str,
        prefix: &[String],
        value: &HirExpr,
    ) -> CodegenResult<()> {
        let mut armed: Vec<Vec<String>> = Vec::new();
        self.collect_armed_positions(value, &mut prefix.to_vec(), &mut armed);
        if armed.is_empty() {
            return Ok(());
        }

        let flags: Vec<PointerValue<'ctx>> = self
            .live_drop_entry(name)
            .map(|entry| {
                entry
                    .held
                    .iter()
                    .filter(|held| matches!(held.target, DropTarget::HeapString))
                    .filter(|held| armed.iter().any(|path| path == &held.path))
                    .map(|held| held.flag_ptr)
                    .collect()
            })
            .unwrap_or_default();

        let armed_flag = self.context.bool_type().const_int(1, false);
        for flag_ptr in flags {
            self.builder.build_store(flag_ptr, armed_flag)?;
        }
        Ok(())
    }

    /// The current flags of the `string` positions the holder `expr` names, keyed by
    /// path: which of them own their buffer at this point of the run. Empty unless `expr`
    /// is a bare binding, the one holder whose positions a move hands over whole.
    pub(crate) fn load_held_string_flags(
        &mut self,
        expr: &HirExpr,
    ) -> CodegenResult<Vec<(Vec<String>, IntValue<'ctx>)>> {
        let HirExprKind::Variable(name) = &expr.kind else {
            return Ok(Vec::new());
        };
        let positions: Vec<(Vec<String>, PointerValue<'ctx>)> = self
            .live_drop_entry(name)
            .map(|entry| {
                entry
                    .held
                    .iter()
                    .filter(|held| matches!(held.target, DropTarget::HeapString))
                    .map(|held| (held.path.clone(), held.flag_ptr))
                    .collect()
            })
            .unwrap_or_default();
        let bool_ty = self.context.bool_type();
        let mut flags = Vec::with_capacity(positions.len());
        for (path, flag_ptr) in positions {
            let owns = self
                .builder
                .build_load(bool_ty, flag_ptr, "held.str.owns")?
                .into_int_value();
            flags.push((path, owns));
        }
        Ok(flags)
    }

    /// Store flags [`load_held_string_flags`] read off a moved holder into the same
    /// positions of `name`, the holder that took the value.
    pub(crate) fn store_held_string_flags(
        &mut self,
        name: &str,
        flags: &[(Vec<String>, IntValue<'ctx>)],
    ) -> CodegenResult<()> {
        let targets: Vec<(PointerValue<'ctx>, IntValue<'ctx>)> = self
            .live_drop_entry(name)
            .map(|entry| {
                flags
                    .iter()
                    .filter_map(|(path, owns)| {
                        entry
                            .held
                            .iter()
                            .find(|held| {
                                matches!(held.target, DropTarget::HeapString) && &held.path == path
                            })
                            .map(|held| (held.flag_ptr, *owns))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (flag_ptr, owns) in targets {
            self.builder.build_store(flag_ptr, owns)?;
        }
        Ok(())
    }

    /// Evaluate `value` into position `segment` of the aggregate literal being built, and
    /// move it in: the place it names stops owning what it held.
    ///
    /// While a holder is being built ([`CodegenContext::literal_string_moves`]), a `string`
    /// binding moved in hands its runtime flag to the position, read before the move clears
    /// it. A nested literal extends the path; anything else is evaluated with the collection
    /// suspended, because a literal inside it (a call's argument, a branch) builds a value
    /// that is not this holder's.
    pub(crate) fn codegen_literal_position(
        &mut self,
        segment: String,
        value: &HirExpr,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let nested = matches!(
            value.kind,
            HirExprKind::StructLiteral { .. }
                | HirExprKind::TupleLiteral { .. }
                | HirExprKind::ArrayLiteral { .. }
        );
        let suspended = if nested {
            if let Some(moves) = &mut self.literal_string_moves {
                moves.path.push(segment.clone());
            }
            None
        } else {
            self.literal_string_moves.take()
        };
        let val = self.codegen_expr(value);
        if nested {
            if let Some(moves) = &mut self.literal_string_moves {
                let _ = moves.path.pop();
            }
        } else {
            self.literal_string_moves = suspended;
        }
        let val = val?;
        if self.literal_string_moves.is_some() && matches!(Type::from_hir(&value.ty), Type::String)
        {
            if let Some(owns) = self.load_owned_string_flag(value)? {
                if let Some(moves) = &mut self.literal_string_moves {
                    let mut path = moves.path.clone();
                    path.push(segment);
                    moves.flags.push((path, owns));
                }
            }
        }
        self.mark_moved_for_drop(value);
        Ok(val)
    }

    /// Whether `value` is an aggregate literal, the one holder shape whose positions'
    /// moved-in ownership [`codegen_literal_position`] can collect.
    pub(crate) fn is_aggregate_literal(value: &HirExpr) -> bool {
        matches!(
            value.kind,
            HirExprKind::StructLiteral { .. }
                | HirExprKind::TupleLiteral { .. }
                | HirExprKind::ArrayLiteral { .. }
        )
    }

    /// The paths under `value` that a provable allocation was stored into, walking the
    /// aggregate literals in step with the path segments [`plan_held_drops`] assigned.
    ///
    /// A newtype contributes no segment: it is erased to its inner type at every other
    /// point in the drop pass, so its positions are its inner type's.
    fn collect_armed_positions(
        &self,
        value: &HirExpr,
        path: &mut Vec<String>,
        out: &mut Vec<Vec<String>>,
    ) {
        if matches!(Type::from_hir(&value.ty), Type::String) {
            if self.produces_owned_string(value) {
                out.push(path.clone());
            }
            return;
        }
        match &value.kind {
            HirExprKind::StructLiteral { fields, .. } => {
                for field in fields {
                    path.push(field.name.clone());
                    self.collect_armed_positions(&field.value, path, out);
                    let _ = path.pop();
                }
            }
            HirExprKind::ArrayLiteral { elements } | HirExprKind::TupleLiteral { elements } => {
                for (index, element) in elements.iter().enumerate() {
                    path.push(index.to_string());
                    self.collect_armed_positions(element, path, out);
                    let _ = path.pop();
                }
            }
            HirExprKind::NewtypeConstruct { value: inner, .. } => {
                self.collect_armed_positions(inner, path, out)
            }
            _ => {}
        }
    }

    /// Address of an aggregate's `index`-th position, for a struct, tuple, or array
    /// holder alike.
    ///
    /// `build_struct_gep` refuses an array pointee, so an array indexes through a
    /// two-step GEP instead. Every index reaching here is a constant read off the
    /// holder's own layout.
    fn aggregate_position_ptr(
        &self,
        holder_llvm: BasicTypeEnum<'ctx>,
        base_ptr: PointerValue<'ctx>,
        index: u32,
        name: &str,
    ) -> CodegenResult<PointerValue<'ctx>> {
        if !holder_llvm.is_array_type() {
            return self
                .builder
                .build_struct_gep(holder_llvm, base_ptr, index, name)
                .map_err(CodegenError::from);
        }
        let i64_ty = self.context.i64_type();
        // SAFETY: `index` is a position of `holder_llvm`'s own layout, so it is within
        // the array's length by construction and the GEP stays inside the object.
        unsafe {
            self.builder
                .build_in_bounds_gep(
                    holder_llvm,
                    base_ptr,
                    &[i64_ty.const_zero(), i64_ty.const_int(index as u64, false)],
                    name,
                )
                .map_err(CodegenError::from)
        }
    }

    /// The positions a value of `ty` holds, in declaration order, as `(accessor
    /// segment, type)`. The position of a pair in the list is its LLVM aggregate index,
    /// so it names both the path segment and the GEP.
    ///
    /// Empty for every type with no statically indexable inside, an enum included: which
    /// of an enum's payload slots is live depends on its tag, so it is walked by
    /// [`emit_enum_payload_drop`] under a switch instead of by a static path.
    fn held_positions(&self, ty: &Type) -> Vec<(String, Type)> {
        match ty {
            Type::Struct(name) => self.struct_defs.get(name).cloned().unwrap_or_default(),
            Type::Tuple(elements) => elements
                .iter()
                .enumerate()
                .map(|(i, e)| (i.to_string(), e.clone()))
                .collect(),
            Type::Array { element, size } => (0..*size)
                .map(|i| (i.to_string(), (**element).clone()))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Release whatever the active variant of an enum value holds, by switching on its
    /// tag and destroying that variant's owning payload slots.
    ///
    /// A payload field was written into its slot through memory, bit-exactly, so the
    /// slot's address is the field's address and each destructor reads it in place. The
    /// builder is left on the join block, which is what lets the flag-guarded caller
    /// finish its own branch around this.
    fn emit_enum_payload_drop(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
        enum_name: &str,
    ) -> CodegenResult<()> {
        let variants = self
            .type_mapper
            .enum_payload_types(enum_name)
            .cloned()
            .unwrap_or_default();
        let parent_fn = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("enum payload drop emitted outside function".to_string())
        })?;

        let enum_llvm = self.type_mapper.enum_struct_type(enum_name)?;
        let payload_array_ty = enum_llvm
            .get_field_type_at_index(1)
            .ok_or_else(|| {
                CodegenError::InternalError(format!("enum '{}' has no payload field", enum_name))
            })?
            .into_array_type();
        let tag_ptr =
            self.builder
                .build_struct_gep(enum_llvm, storage_ptr, 0, "enum.drop.tag.ptr")?;
        let tag = self
            .builder
            .build_load(self.context.i32_type(), tag_ptr, "enum.drop.tag")?
            .into_int_value();
        let payload_ptr =
            self.builder
                .build_struct_gep(enum_llvm, storage_ptr, 1, "enum.drop.payload")?;

        let join_bb = self.context.append_basic_block(parent_fn, "enum.drop.cont");
        let owning: Vec<usize> = variants
            .iter()
            .enumerate()
            .filter(|(_, fields)| fields.iter().any(|field| self.holds_owner(field)))
            .map(|(tag_value, _)| tag_value)
            .collect();
        let cases: Vec<(IntValue<'ctx>, BasicBlock<'ctx>)> = owning
            .iter()
            .map(|tag_value| {
                (
                    self.context.i32_type().const_int(*tag_value as u64, false),
                    self.context
                        .append_basic_block(parent_fn, &format!("enum.drop.v{}", tag_value)),
                )
            })
            .collect();
        self.builder.build_switch(tag, join_bb, &cases)?;

        for (tag_value, (_, case_bb)) in owning.iter().zip(cases.iter()) {
            self.builder.position_at_end(*case_bb);
            for (slot, field_ty) in variants[*tag_value].iter().enumerate() {
                if !self.holds_owner(field_ty) {
                    continue;
                }
                let slot_ptr = self.aggregate_position_ptr(
                    payload_array_ty.into(),
                    payload_ptr,
                    slot as u32,
                    "enum.drop.slot",
                )?;
                self.emit_value_destructor(slot_ptr, field_ty)?;
            }
            self.builder.build_unconditional_branch(join_bb)?;
        }

        self.builder.position_at_end(join_bb);
        Ok(())
    }

    /// Destroy the value at `storage_ptr`, and everything it holds, unconditionally.
    ///
    /// The flagged path is the one bindings take; this is for a position no flag can
    /// guard, namely an enum payload slot, where the tag the caller switched on has
    /// already established that the value is live.
    fn emit_value_destructor(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
        ty: &Type,
    ) -> CodegenResult<()> {
        match self.drop_target_of(ty) {
            Some(DropTarget::UserDrop(struct_name)) => {
                self.emit_user_drop_call(storage_ptr, &struct_name)?
            }
            Some(DropTarget::Collection(collection_ty)) => {
                self.emit_collection_free(storage_ptr, &collection_ty)?
            }
            Some(DropTarget::TensorBuffer) => self.emit_tensor_buffer_free(storage_ptr)?,
            Some(DropTarget::EnumPayload(enum_name)) => {
                self.emit_enum_payload_drop(storage_ptr, &enum_name)?
            }
            _ => {}
        }

        let positions = self.held_positions(ty);
        if positions.is_empty() {
            return Ok(());
        }
        let holder_llvm = self.get_any_llvm_type(ty)?;
        for (index, (segment, position_ty)) in positions.into_iter().enumerate() {
            if !self.holds_owner(&position_ty) {
                continue;
            }
            let position_ptr = self.aggregate_position_ptr(
                holder_llvm,
                storage_ptr,
                index as u32,
                &format!("held.{}.ptr", segment),
            )?;
            self.emit_value_destructor(position_ptr, &position_ty)?;
        }
        Ok(())
    }

    /// Call a type's `impl Drop` destructor against the value at `storage_ptr`, which
    /// is the `&mut self` receiver this backend passes by address.
    fn emit_user_drop_call(
        &mut self,
        storage_ptr: PointerValue<'ctx>,
        struct_name: &str,
    ) -> CodegenResult<()> {
        let mangled = format!("{}__drop", struct_name);
        let drop_fn = *self
            .functions
            .get(&mangled)
            .ok_or_else(|| CodegenError::UndefinedFunction(mangled.clone()))?;
        let receiver: BasicValueEnum<'ctx> = storage_ptr.into();
        self.builder
            .build_call(drop_fn, &[receiver.into()], "")
            .map_err(|e| CodegenError::LlvmError(format!("failed to build drop call: {}", e)))?;
        Ok(())
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
                // A pool-registered value outlives its own scope on purpose: its
                // release is the arena's LIFO sweep at the pool's closing brace, so every
                // such binding in the block is released in one ordered pass.
                if matches!(entry.target, DropTarget::PoolRegistered) {
                    continue;
                }
                pending.extend(Self::snapshot_entry(entry));
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
        // A holder that only holds has no release of its own; its held entries carry
        // the work and are emitted beside this call, not through it.
        if matches!(target, DropTarget::Aggregate) {
            return Ok(());
        }
        let parent_fn = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("drop emitted outside function".to_string())
        })?;

        let bool_ty = self.context.bool_type();
        let flag = self
            .builder
            .build_load(bool_ty, flag_ptr, "drop.flag.load")?
            .into_int_value();

        let run_bb = self.context.append_basic_block(parent_fn, "drop.run");
        let cont_bb = self.context.append_basic_block(parent_fn, "drop.cont");
        self.builder
            .build_conditional_branch(flag, run_bb, cont_bb)?;

        self.builder.position_at_end(run_bb);
        match target {
            DropTarget::UserDrop(struct_name) => {
                let struct_name = struct_name.clone();
                self.emit_user_drop_call(storage_ptr, &struct_name)?
            }
            DropTarget::Collection(collection_ty) => {
                let collection_ty = collection_ty.clone();
                self.emit_collection_free(storage_ptr, &collection_ty)?
            }
            DropTarget::HeapString => self.emit_heap_string_free(storage_ptr)?,
            DropTarget::TensorBuffer => self.emit_tensor_buffer_free(storage_ptr)?,
            DropTarget::EnumPayload(enum_name) => {
                let enum_name = enum_name.clone();
                self.emit_enum_payload_drop(storage_ptr, &enum_name)?
            }
            DropTarget::Aggregate => {
                return Err(CodegenError::InternalError(
                    "a holder with no release of its own reached the destructor path".to_string(),
                ))
            }
            DropTarget::PoolRegistered => {
                return Err(CodegenError::InternalError(
                    "a pool-registered value reached the per-scope drop path".to_string(),
                ))
            }
        }
        // Clear the flag so a re-reachable drop site cannot run the destructor twice.
        self.builder.build_store(flag_ptr, bool_ty.const_zero())?;
        self.builder.build_unconditional_branch(cont_bb)?;

        self.builder.position_at_end(cont_bb);
        Ok(())
    }
}

/// Whether this expression shape allocates the tensor buffer it hands back, so that
/// nothing else can still own it.
///
/// Deliberately a whitelist rather than "not a place": an `if`, a `match` or a block
/// yields whatever its branch yields, which may be a buffer a binding still owns, and
/// releasing that would free it twice. `TensorIndex` is out for the same reason — a
/// sliced view reads the receiver's storage.
fn builds_its_own_buffer(kind: &HirExprKind) -> bool {
    matches!(
        kind,
        HirExprKind::Binary { .. }
            | HirExprKind::Call { .. }
            | HirExprKind::TensorLiteral { .. }
            | HirExprKind::TensorFill { .. }
            | HirExprKind::TensorIdentity
            | HirExprKind::TensorRandomNormal { .. }
            | HirExprKind::TensorReduce { .. }
            | HirExprKind::TensorEinsum { .. }
            | HirExprKind::TensorApply { .. }
            | HirExprKind::TensorSort { .. }
            | HirExprKind::Math { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::CodegenContext;
    use neuro_hir::{HirExpr, HirExprKind, HirType};
    use shared_types::{Literal, Span};

    fn expr(kind: HirExprKind) -> HirExpr {
        HirExpr::new(kind, HirType::I32, Span::new(0, 0))
    }

    fn variable(name: &str) -> HirExpr {
        expr(HirExprKind::Variable(name.to_string()))
    }

    fn integer(value: i128) -> HirExpr {
        expr(HirExprKind::Literal(Literal::Integer(value, None)))
    }

    #[test]
    fn a_bare_binding_is_the_whole_place() {
        let place = variable("h");
        assert_eq!(
            CodegenContext::moved_place(&place),
            Some(("h", Some(Vec::new())))
        );
    }

    #[test]
    fn field_tuple_and_constant_index_segments_build_a_path() {
        let place = expr(HirExprKind::TupleIndex {
            object: Box::new(expr(HirExprKind::Index {
                object: Box::new(expr(HirExprKind::FieldAccess {
                    object: Box::new(variable("h")),
                    field: "inner".to_string(),
                })),
                index: Box::new(integer(2)),
            })),
            index: 1,
        });
        assert_eq!(
            CodegenContext::moved_place(&place),
            Some((
                "h",
                Some(vec!["inner".to_string(), "2".to_string(), "1".to_string()])
            ))
        );
    }

    /// `a[i]` names no particular element, so a move through it takes the whole
    /// binding. A `None` path is how that is spelled, and it survives further segments.
    #[test]
    fn a_runtime_index_collapses_to_the_whole_binding() {
        let element = expr(HirExprKind::Index {
            object: Box::new(variable("a")),
            index: Box::new(variable("i")),
        });
        assert_eq!(CodegenContext::moved_place(&element), Some(("a", None)));

        let field_of_element = expr(HirExprKind::FieldAccess {
            object: Box::new(element),
            field: "w".to_string(),
        });
        assert_eq!(
            CodegenContext::moved_place(&field_of_element),
            Some(("a", None))
        );
    }

    #[test]
    fn an_expression_that_is_not_a_place_names_nothing() {
        let call = expr(HirExprKind::FieldAccess {
            object: Box::new(integer(3)),
            field: "a".to_string(),
        });
        assert_eq!(CodegenContext::moved_place(&call), None);
    }
}
