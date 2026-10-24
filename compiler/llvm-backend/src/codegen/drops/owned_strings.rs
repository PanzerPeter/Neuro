//! Which expressions hand over a heap `string` buffer, and the releases that settle one
//! at the consumer that reads it.

use ast_types::BinaryOp;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use neuro_hir::{HirExpr, HirExprKind, HirType};

use crate::codegen::context::{CodegenContext, DropTarget};
use crate::errors::{CodegenError, CodegenResult};
use crate::types::{CollectionKind, Type};

use super::peel_newtype;
use crate::codegen::string_ownership::tail_exits;

/// Whether this expression shape allocates the tensor buffer it hands back, so that
/// nothing else can still own it.
///
/// Deliberately a whitelist rather than "not a place": an `if`, a `match` or a block
/// yields whatever its branch yields, which may be a buffer a binding still owns, and
/// releasing that would free it twice. A slice qualifies because it is a copy, while an
/// index that reads one element yields no buffer at all. A shape cast and a detach
/// consume their receiver, so the buffer they hand on has no other owner left.
pub(super) fn builds_its_own_buffer(expr: &HirExpr) -> bool {
    if let HirExprKind::TensorIndex { .. } = expr.kind {
        return matches!(expr.ty, HirType::Tensor { .. });
    }
    matches!(
        expr.kind,
        HirExprKind::Binary { .. }
            | HirExprKind::TensorShapeCast { .. }
            | HirExprKind::TensorDetach { .. }
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

/// The `StringBuilder` builder method that copies its bytes out into an owned `string`.
/// Matched by name here the way the builder type itself is matched by name.
pub(super) const TO_OWNED_METHOD: &str = "to_string";

/// `string.clone()`, which copies the bytes into a buffer of their own.
pub(super) const STRING_CLONE_METHOD: &str = "clone";

/// The collection methods that move a slot's value out inside an `Option`: `pop`, a map's
/// `remove`, and the value `insert` displaced. Matched by name against a collection
/// receiver, the way the builder's `to_string` is.
pub(super) const OPTION_TAKERS: [&str; 3] = ["pop", "remove", "insert"];

/// The `Vec` methods that move one element out and return it bare.
pub(super) const VEC_TAKERS: [&str; 2] = ["remove", "swap_remove"];

impl<'ctx> CodegenContext<'ctx> {
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
            // `codegen_interp_string` concatenates every piece into one fresh buffer,
            // and does so unconditionally: even a hole-free interpolation allocates.
            HirExprKind::InterpString { .. } => true,
            // A newtype is its inner value: wrapping a fresh buffer hands it on unchanged.
            HirExprKind::NewtypeConstruct { value, .. } => self.produces_owned_string(value),
            // A branch or a block yields one of its tails, so it hands over a buffer of
            // its own exactly when every tail does. A tail that is not an expression (an
            // arm that returns) leaves the value unnamed, and the answer is the safe one.
            HirExprKind::If {
                else_block: Some(_),
                ..
            }
            | HirExprKind::Match { .. }
            | HirExprKind::Block { .. } => {
                let mut tails = Vec::new();
                tail_exits(expr, &mut tails)
                    && tails.iter().all(|tail| self.produces_owned_string(tail))
            }
            // `+` yielding a `string` is `codegen_string_concat`, which always allocates
            // a `len1 + len2` buffer. No numeric addition produces a `string`, so the
            // result type alone identifies the concatenation.
            HirExprKind::Binary {
                op: BinaryOp::Add, ..
            } => matches!(Type::from_hir(&expr.ty), Type::String),
            // `StringBuilder::to_string` is `codegen_string_to_owned`, which copies the
            // builder's live bytes into a buffer of their own on every call. It reaches
            // here as the `FieldAccess` callee a method call lowers to; a program that
            // declares its own `StringBuilder` shadows the builder, and its receiver is then a
            // `Type::Struct` that this arm does not match.
            HirExprKind::Call { callee, args } => match &callee.kind {
                HirExprKind::Path { type_name, member } => self
                    .string_ownership
                    .returns_owned(&format!("{}__{}", type_name, member)),
                HirExprKind::FieldAccess { object, field } => {
                    let receiver = Type::from_hir(&object.ty);
                    // A user method is summarized under the name its call mangles.
                    if let Type::Struct(type_name) | Type::Enum(type_name) = receiver.referent() {
                        return self
                            .string_ownership
                            .returns_owned(&format!("{}__{}", type_name, field));
                    }
                    let builder_copy = field == TO_OWNED_METHOD
                        && matches!(
                            receiver.referent(),
                            Type::Collection {
                                kind: CollectionKind::StringBuilder,
                                ..
                            }
                        );
                    // `string.clone()` copies the bytes into a buffer of their own.
                    let string_clone =
                        field == STRING_CLONE_METHOD && matches!(receiver.referent(), Type::String);
                    // `v.remove(i)` / `v.swap_remove(i)` hand the slot's own buffer out with
                    // it, and a slot always owns its bytes.
                    let taken = VEC_TAKERS.contains(&field.as_str())
                        && matches!(Type::from_hir(&expr.ty), Type::String)
                        && matches!(
                            receiver.referent(),
                            Type::Collection {
                                kind: CollectionKind::Vec,
                                ..
                            }
                        );
                    taken || (args.is_empty() && (builder_copy || string_clone))
                }
                // A function whose every return path allocates hands the buffer to its
                // caller, which is the one storing position that no expression shape
                // can settle. The summary excludes a name a local binding shadows, so a
                // function value called through the indirect path is not mistaken for
                // it; a closure whose lifted body is known is asked under that body's name.
                HirExprKind::Variable(_) | HirExprKind::Closure { .. } => {
                    match (self.lifted_closure_name(callee), &callee.kind) {
                        (Some(lifted), _) => self.string_ownership.returns_owned(&lifted),
                        (None, HirExprKind::Variable(name)) => {
                            self.string_ownership.returns_owned(name)
                        }
                        (None, _) => false,
                    }
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Whether `expr` yields an `Option` whose `string` payload the binding that takes it
    /// out owns.
    ///
    /// The collection methods that move a slot's value out are the shapes that qualify: a
    /// slot owns its bytes, and the value leaves the collection with them, so nothing
    /// else reaches what the payload holds. `get` lends a borrow instead and is not one.
    pub(crate) fn produces_owned_option_payload(&self, expr: &HirExpr) -> bool {
        let HirExprKind::Call { callee, .. } = &expr.kind else {
            return false;
        };
        let HirExprKind::FieldAccess { object, field } = &callee.kind else {
            return false;
        };
        if !OPTION_TAKERS.contains(&field.as_str()) {
            return false;
        }
        matches!(
            Type::from_hir(&object.ty).referent(),
            Type::Collection { kind, .. } if !matches!(kind, CollectionKind::StringBuilder)
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

    /// The run-time owner bit of a read-only argument that is a call to a `string`
    /// forwarder, taken right after the argument is evaluated (the next argument's
    /// evaluation would overwrite it). `s(a + b)` hands back the buffer it was given,
    /// owned exactly when that argument was, and a read-only callee leaves it to the
    /// caller, which has no binding to release it through.
    pub(crate) fn forwarded_argument_owner(
        &mut self,
        callee: &str,
        index: usize,
        arg: &HirExpr,
    ) -> Option<IntValue<'ctx>> {
        if !self.string_ownership.param_is_read_only(callee, index) {
            return None;
        }
        self.take_forwarded_owner(arg)
    }

    /// Release, after the call, each forwarded argument whose owner bit is set.
    pub(crate) fn release_forwarded_arguments(
        &mut self,
        owners: &[(usize, IntValue<'ctx>)],
        values: &[BasicValueEnum<'ctx>],
    ) -> CodegenResult<()> {
        for (index, owner) in owners {
            let Some(BasicValueEnum::StructValue(fat_ptr)) = values.get(*index) else {
                continue;
            };
            let function = self
                .builder
                .get_insert_block()
                .and_then(|block| block.get_parent())
                .ok_or_else(|| {
                    CodegenError::InternalError("a call outside any function".to_string())
                })?;
            let release = self.context.append_basic_block(function, "fwd.release");
            let done = self.context.append_basic_block(function, "fwd.done");
            self.builder
                .build_conditional_branch(*owner, release, done)?;
            self.builder.position_at_end(release);
            let buffer = self.builder.build_extract_value(*fat_ptr, 0, "fwd.buf")?;
            let free_fn = self.release_fn()?;
            self.builder.build_call(free_fn, &[buffer.into()], "")?;
            self.builder.build_unconditional_branch(done)?;
            self.builder.position_at_end(done);
        }
        Ok(())
    }

    /// The drop flag of the binding `expr` names, when that binding may own a heap
    /// `string`, or of the held `string` position it names (`p.name`, `t.0`, `a[1]`),
    /// which a move out of it hands over just as a bare binding's does.
    pub(super) fn owned_string_flag_ptr(&self, expr: &HirExpr) -> Option<PointerValue<'ctx>> {
        let expr = peel_newtype(expr);
        if let HirExprKind::Variable(name) = &expr.kind {
            return self
                .live_drop_entry(name)
                .filter(|entry| matches!(entry.target, DropTarget::HeapString))
                .map(|entry| entry.flag_ptr);
        }
        if !matches!(Type::from_hir(&expr.ty), Type::String) {
            return None;
        }
        let (name, Some(path)) = Self::moved_place(expr)? else {
            return None;
        };
        self.live_drop_entry(name)?
            .held
            .iter()
            .find(|held| held.path == path && matches!(held.target, DropTarget::HeapString))
            .map(|held| held.flag_ptr)
    }

    /// Whether the `string` argument `arg` is owned at this point of the run: `true` for
    /// one that allocates in place, the flag of the binding it names, what a forwarder
    /// it calls handed back, and `false` for anything else. Read before the call's move
    /// clears the flag.
    pub(crate) fn argument_string_owner(&mut self, arg: &HirExpr) -> CodegenResult<IntValue<'ctx>> {
        let bool_ty = self.context.bool_type();
        if self.produces_owned_string(arg) {
            return Ok(bool_ty.const_int(1, false));
        }
        if let Some(owner) = self.take_forwarded_owner(arg) {
            return Ok(owner);
        }
        Ok(self
            .load_owned_string_flag(arg)?
            .unwrap_or_else(|| bool_ty.const_zero()))
    }

    /// Read the owner bit of argument `index` for each position of a holder forwarder's
    /// result it fills (`held`), before the call's move clears the argument's flag.
    pub(crate) fn read_held_owner(
        &mut self,
        held: &[(Vec<String>, usize)],
        index: usize,
        arg: &HirExpr,
        out: &mut Vec<(Vec<String>, IntValue<'ctx>)>,
    ) -> CodegenResult<()> {
        let Some((path, _)) = held.iter().find(|(_, filled)| *filled == index) else {
            return Ok(());
        };
        let owner = self.argument_string_owner(arg)?;
        out.push((path.clone(), owner));
        Ok(())
    }

    /// The name a call to a `string` forwarder is summarized under, when `expr` is one.
    /// Keyed the way [`produces_owned_string`](CodegenContext::produces_owned_string)
    /// keys a producer, which is the name the call path lowers the call under.
    pub(crate) fn forwarding_callee(&self, expr: &HirExpr) -> Option<String> {
        let HirExprKind::Call { callee, .. } = &peel_newtype(expr).kind else {
            return None;
        };
        let key = self.summary_key(callee)?;
        self.string_ownership.forwarded_param(&key).map(|_| key)
    }

    /// The name the ownership summary keys a call to `callee` under: a function, a
    /// closure's lifted body, or an associated function or method of a user type.
    pub(super) fn summary_key(&self, callee: &HirExpr) -> Option<String> {
        Some(match &callee.kind {
            HirExprKind::Variable(name) => self
                .lifted_closure_name(callee)
                .unwrap_or_else(|| name.clone()),
            HirExprKind::Closure { .. } => self.lifted_closure_name(callee)?,
            HirExprKind::Path { type_name, member } => format!("{}__{}", type_name, member),
            HirExprKind::FieldAccess { object, field } => {
                match Type::from_hir(&object.ty).referent() {
                    Type::Struct(type_name) | Type::Enum(type_name) => {
                        format!("{}__{}", type_name, field)
                    }
                    _ => return None,
                }
            }
            _ => return None,
        })
    }

    /// Whether the buffer the forwarder call `expr` just yielded is owned, when `expr` is
    /// one and the call path recorded its answer.
    pub(crate) fn take_forwarded_owner(&mut self, expr: &HirExpr) -> Option<IntValue<'ctx>> {
        let key = self.forwarding_callee(expr)?;
        match self.forwarded_string_owner.take() {
            Some((callee, owner)) if callee == key => Some(owner),
            _ => None,
        }
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
        if !builds_its_own_buffer(receiver) {
            return Ok(());
        }
        self.build_dlpack_release(handle)
    }
}
