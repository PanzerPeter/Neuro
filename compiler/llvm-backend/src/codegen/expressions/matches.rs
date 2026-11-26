// Codegen for `match` expressions.
//
// The scrutinee is evaluated once into an alloca, and every test and binding walks to its
// part of that place. An arm tries its alternatives in turn: each test branches away at
// its first failure, since a later test may read a part that only exists when an earlier
// one held, and an alternative that holds stores its bindings into the arm's slots and
// jumps to the one shared body. There a guard decides, and a failing one resumes at the
// alternative after the one that matched. The frontend guarantees exhaustiveness, so the
// final fall-through is `unreachable`.

use crate::codegen::drops::is_fresh_value;
use inkwell::IntPredicate;
use inkwell::basic_block::BasicBlock;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use neuro_hir::{HirExpr, HirExprKind, HirMatchArm, HirMatchBinding, HirMatchTest, HirPatternStep};

use crate::codegen::context::CodegenContext;
use crate::codegen::drops::is_pure_place;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// A binding's saved prior state in the name maps, restored when the arm's blocks are
/// done so bindings do not leak to sibling arms or shadow the outer scope. `constant` is
/// what the name meant in `const_values`, which a body-level `const` shadows.
pub(crate) struct SavedBinding<'ctx> {
    name: String,
    ptr: Option<PointerValue<'ctx>>,
    ty: Option<BasicTypeEnum<'ctx>>,
    sem: Option<Type>,
    constant: Option<BasicValueEnum<'ctx>>,
}

/// What a taken arm's bindings left of the matched value, which the arm releases: the
/// scrutinee's slot and type, whether the match owns it, and which alternative matched
/// when the arm has several.
pub(crate) struct ArmRemainder<'ctx> {
    scrut_alloca: PointerValue<'ctx>,
    scrut_sem: Type,
    owned: IntValue<'ctx>,
    matched: Option<PointerValue<'ctx>>,
}

/// The tag `test` requires of the value it tests, if it requires one at this level.
fn matched_tag(test: &HirMatchTest) -> Option<u32> {
    match test {
        HirMatchTest::Tag { tag } => Some(*tag),
        HirMatchTest::All(tests) => tests.iter().find_map(matched_tag),
        _ => None,
    }
}

/// The test `test` makes of the part one step reaches, the step chosen by `is_step`.
fn part_test(
    test: &HirMatchTest,
    is_step: impl Fn(&HirPatternStep) -> bool + Copy,
) -> Option<&HirMatchTest> {
    match test {
        HirMatchTest::Part { step, test } if is_step(step) => Some(test),
        HirMatchTest::All(tests) => tests.iter().find_map(|t| part_test(t, is_step)),
        _ => None,
    }
}

/// The alloca one binding lives in, shared by the alternatives of its arm.
struct BindingSlot<'ctx> {
    alloca: PointerValue<'ctx>,
    llvm_ty: BasicTypeEnum<'ctx>,
    sem: Type,
}

/// Who releases what an arm's bindings take out of the scrutinee.
#[derive(Clone, Copy)]
pub(crate) enum ArmOwnership {
    /// A `match` arm: the caller has opened a scope for these bindings, and the payload
    /// they take is theirs for the arm's duration. `shares` says the scrutinee keeps its
    /// function values, so a binding of one takes a count of its own.
    Arm { shares: bool },
}

impl<'ctx> CodegenContext<'ctx> {
    /// Lower a `match` expression, returning its value (a placeholder for a
    /// `Void` match used in statement position).
    pub(crate) fn codegen_match(
        &mut self,
        scrutinee: &HirExpr,
        arms: &[HirMatchArm],
        result_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("match outside function".to_string()))?;

        let scrut_sem = Type::from_hir(&scrutinee.ty);
        let scrut_val = self.codegen_expr(scrutinee)?;
        let scrut_alloca = self.entry_alloca(scrut_val.get_type(), "match.scrut")?;
        self.builder.build_store(scrut_alloca, scrut_val)?;

        // A binding an arm takes by value carries whatever the scrutinee held out of it.
        // Which position leaves depends on the tag, which is not knowable here, so every
        // owner the scrutinee holds is disowned at once: a part of the variant the taken
        // arm did not bind leaks rather than being released twice. An arm that binds
        // nothing takes nothing, so it hands the scrutinee its flags back.
        //
        // A scrutinee whose only owners are function values is copied, not moved, since
        // the language lets it be read again: it keeps its counts, and a binding takes
        // its own. So does a binding out of a place nothing here disowned, one reached
        // through a borrow.
        let copies = self.shares_on_copy(scrut_sem.referent());
        let mut disowned = Vec::new();
        if !copies && arms.iter().any(|arm| !arm.bindings.is_empty()) {
            disowned = self.mark_held_moved_for_drop(scrutinee)?;
        }
        let shares = is_pure_place(scrutinee) && disowned.is_empty();
        // What an arm that binds leaves unbound is the arm's to release, when the match
        // took the scrutinee over: a place it disowned, which owned what the flags held,
        // or a temporary no one else owns.
        let remainder_owned = if !disowned.is_empty() {
            let mut all = self.context.bool_type().const_int(1, false);
            for (_, owned) in &disowned {
                all = self.builder.build_and(all, *owned, "match.owned.all")?;
            }
            Some(all)
        } else if !copies && is_fresh_value(scrutinee) && self.pool_depth == 0 {
            Some(self.context.bool_type().const_int(1, false))
        } else {
            None
        };

        let is_void = matches!(result_ty, Type::Void);
        let result_slot = if is_void {
            None
        } else {
            let llvm_ty = self.get_any_llvm_type(result_ty)?;
            Some(self.entry_alloca(llvm_ty, "match.result")?)
        };

        let merge_bb = self.context.append_basic_block(parent_fn, "match.merge");

        // Pre-create one test block per arm plus a fall-through block.
        let mut test_bbs = Vec::with_capacity(arms.len() + 1);
        for i in 0..arms.len() {
            test_bbs.push(
                self.context
                    .append_basic_block(parent_fn, &format!("match.test{}", i)),
            );
        }
        let fail_bb = self.context.append_basic_block(parent_fn, "match.fail");
        test_bbs.push(fail_bb);

        self.builder.build_unconditional_branch(test_bbs[0])?;

        for (i, arm) in arms.iter().enumerate() {
            let body_bb = self
                .context
                .append_basic_block(parent_fn, &format!("match.arm{}", i));
            let next_bb = test_bbs[i + 1];

            self.builder.position_at_end(test_bbs[i]);
            let slots_of_arm = self.binding_slots(&arm.bindings)?;
            let (guard_failed, matched) = self.codegen_alternatives(
                arm,
                &slots_of_arm,
                scrut_alloca,
                &scrut_sem,
                body_bb,
                next_bb,
            )?;
            let remainder = remainder_owned.map(|owned| ArmRemainder {
                scrut_alloca,
                scrut_sem: scrut_sem.clone(),
                owned,
                matched,
            });

            self.builder.position_at_end(body_bb);
            if arm.bindings.is_empty() {
                for (flag_ptr, owned) in &disowned {
                    self.builder.build_store(*flag_ptr, *owned)?;
                }
                // A temporary scrutinee no arm binding takes from has no other owner, and
                // nothing in this arm can name it.
                self.drop_unbound_temporary(scrutinee, scrut_val)?;
            }
            self.codegen_arm_body(
                arm,
                &slots_of_arm,
                ArmOwnership::Arm { shares },
                (result_slot, remainder),
                merge_bb,
                guard_failed,
            )?;
        }

        // Exhaustiveness is guaranteed by the type checker, so no value reaches here.
        self.builder.position_at_end(fail_bb);
        self.builder.build_unreachable()?;

        self.builder.position_at_end(merge_bb);
        match result_slot {
            Some(slot) => {
                let llvm_ty = self.get_any_llvm_type(result_ty)?;
                self.builder
                    .build_load(llvm_ty, slot, "match.val")
                    .map_err(CodegenError::from)
            }
            None => Ok(self.context.i32_type().const_int(0, false).into()),
        }
    }

    /// Emit an arm's alternatives from the current block: each that holds stores its
    /// bindings and jumps to `body_bb`, and the last that fails reaches `next_bb`. Returns
    /// where a failed guard goes, which is the alternative after the one that matched, and
    /// for an arm of several alternatives the slot recording which one did.
    fn codegen_alternatives(
        &mut self,
        arm: &HirMatchArm,
        binding_slots: &[BindingSlot<'ctx>],
        scrut_alloca: PointerValue<'ctx>,
        scrut_sem: &Type,
        body_bb: BasicBlock<'ctx>,
        next_bb: BasicBlock<'ctx>,
    ) -> CodegenResult<(BasicBlock<'ctx>, Option<PointerValue<'ctx>>)> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("match outside function".to_string()))?;
        let i32_ty = self.context.i32_type();
        let matched = if arm.tests.len() > 1 {
            Some(self.entry_alloca(i32_ty, "match.alt.index")?)
        } else {
            None
        };

        let mut resume = Vec::new();
        for (alternative, test) in arm.tests.iter().enumerate() {
            let miss_bb = if alternative + 1 == arm.tests.len() {
                next_bb
            } else {
                let miss_bb = self.context.append_basic_block(parent_fn, "match.alt");
                resume.push((i32_ty.const_int(alternative as u64, false), miss_bb));
                miss_bb
            };
            self.codegen_pattern_test(test, scrut_alloca, scrut_sem, miss_bb)?;
            self.store_bindings(
                &arm.bindings,
                binding_slots,
                alternative,
                scrut_alloca,
                scrut_sem,
            )?;
            if let Some(matched) = matched {
                self.builder
                    .build_store(matched, i32_ty.const_int(alternative as u64, false))?;
            }
            self.builder.build_unconditional_branch(body_bb)?;
            self.builder.position_at_end(miss_bb);
        }

        let Some(matched) = matched.filter(|_| arm.guard.is_some()) else {
            return Ok((next_bb, matched));
        };
        let guard_failed = self
            .context
            .append_basic_block(parent_fn, "match.guard.retry");
        self.builder.position_at_end(guard_failed);
        let index = self
            .builder
            .build_load(i32_ty, matched, "match.alt.matched")?
            .into_int_value();
        self.builder.build_switch(index, next_bb, &resume)?;
        Ok((guard_failed, Some(matched)))
    }

    /// Lower `test` of the value at `place`, branching to `fail_bb` at the first failure
    /// and leaving the builder where every test held.
    pub(crate) fn codegen_pattern_test(
        &mut self,
        test: &HirMatchTest,
        place: PointerValue<'ctx>,
        sem: &Type,
        fail_bb: BasicBlock<'ctx>,
    ) -> CodegenResult<()> {
        let holds = match test {
            HirMatchTest::Wildcard => return Ok(()),
            HirMatchTest::Never => self.context.bool_type().const_zero(),
            HirMatchTest::All(tests) => {
                for test in tests {
                    self.codegen_pattern_test(test, place, sem, fail_bb)?;
                }
                return Ok(());
            }
            HirMatchTest::Part { step, test } => {
                let (part, part_sem) = self.pattern_part(place, sem, step)?;
                return self.codegen_pattern_test(test, part, &part_sem, fail_bb);
            }
            HirMatchTest::Tag { tag } => {
                let tag_val = self.load_enum_tag(place, sem)?;
                let want = self.context.i32_type().const_int(*tag as u64, false);
                self.builder
                    .build_int_compare(IntPredicate::EQ, tag_val, want, "match.tag")?
            }
            HirMatchTest::IntEq { value } => {
                let scalar = self.load_match_scalar(place, sem)?;
                let want = scalar.get_type().const_int(*value as u64, false);
                self.builder
                    .build_int_compare(IntPredicate::EQ, scalar, want, "match.eq")?
            }
            HirMatchTest::IntRange { lo, hi } => {
                let scalar = self.load_match_scalar(place, sem)?;
                let unsigned = sem.is_unsigned_int() || matches!(sem, Type::Char);
                let (ge_pred, le_pred) = if unsigned {
                    (IntPredicate::UGE, IntPredicate::ULE)
                } else {
                    (IntPredicate::SGE, IntPredicate::SLE)
                };
                let lo_c = scalar.get_type().const_int(*lo as u64, false);
                let hi_c = scalar.get_type().const_int(*hi as u64, false);
                let ge = self
                    .builder
                    .build_int_compare(ge_pred, scalar, lo_c, "match.ge")?;
                let le = self
                    .builder
                    .build_int_compare(le_pred, scalar, hi_c, "match.le")?;
                self.builder.build_and(ge, le, "match.range")?
            }
            HirMatchTest::SliceLen { len, exact } => {
                let (_, actual) = self.load_slice_parts(place, sem)?;
                let want = actual.get_type().const_int(*len, false);
                let predicate = if *exact {
                    IntPredicate::EQ
                } else {
                    IntPredicate::UGE
                };
                self.builder
                    .build_int_compare(predicate, actual, want, "match.len")?
            }
        };
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("match outside function".to_string()))?;
        let pass_bb = self.context.append_basic_block(parent_fn, "match.pass");
        self.builder
            .build_conditional_branch(holds, pass_bb, fail_bb)?;
        self.builder.position_at_end(pass_bb);
        Ok(())
    }

    /// The place `step` reaches from the value of type `sem` at `place`, and its type.
    /// The address of payload slot `slot` of the enum value at `place`.
    fn payload_slot(
        &mut self,
        place: PointerValue<'ctx>,
        sem: &Type,
        slot: usize,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let i64_ty = self.context.i64_type();
        let enum_llvm = self.get_any_llvm_type(sem)?.into_struct_type();
        let payload = self
            .builder
            .build_struct_gep(enum_llvm, place, 1, "match.payload")?;
        let payload_llvm = enum_llvm.get_field_type_at_index(1).ok_or_else(|| {
            CodegenError::InternalError("an enum with no payload field".to_string())
        })?;
        let indices = [i64_ty.const_zero(), i64_ty.const_int(slot as u64, false)];
        // SAFETY: the checker proved `slot` a field of the variant whose tag test guards
        // this read, and the payload array has a slot per field.
        Ok(unsafe {
            self.builder
                .build_in_bounds_gep(payload_llvm, payload, &indices, "match.slot")?
        })
    }

    /// Release what the taken arm's bindings left of the matched value: under the
    /// remainder's ownership, the alternative that matched (read from its slot when the
    /// arm has several) walks its own test and binding paths.
    fn release_arm_remainder(
        &mut self,
        arm: &HirMatchArm,
        remainder: &ArmRemainder<'ctx>,
    ) -> CodegenResult<()> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("match outside function".to_string()))?;
        let run_bb = self
            .context
            .append_basic_block(parent_fn, "match.rest.release");
        let join_bb = self
            .context
            .append_basic_block(parent_fn, "match.rest.done");
        self.builder
            .build_conditional_branch(remainder.owned, run_bb, join_bb)?;
        self.builder.position_at_end(run_bb);
        let paths = |alternative: usize| -> Vec<&[HirPatternStep]> {
            arm.bindings
                .iter()
                .filter_map(|binding| binding.paths.get(alternative).map(Vec::as_slice))
                .collect()
        };
        match remainder.matched {
            None => {
                let test = arm.tests.first();
                self.release_unbound_parts(
                    remainder.scrut_alloca,
                    &remainder.scrut_sem,
                    test,
                    &paths(0),
                )?;
            }
            Some(matched) => {
                let i32_ty = self.context.i32_type();
                let index = self
                    .builder
                    .build_load(i32_ty, matched, "match.alt.taken")?
                    .into_int_value();
                let cases: Vec<_> = (0..arm.tests.len())
                    .map(|alternative| {
                        (
                            i32_ty.const_int(alternative as u64, false),
                            self.context.append_basic_block(parent_fn, "match.rest.alt"),
                        )
                    })
                    .collect();
                self.builder.build_switch(index, join_bb, &cases)?;
                for (alternative, (_, case_bb)) in cases.iter().enumerate() {
                    self.builder.position_at_end(*case_bb);
                    self.release_unbound_parts(
                        remainder.scrut_alloca,
                        &remainder.scrut_sem,
                        arm.tests.get(alternative),
                        &paths(alternative),
                    )?;
                    self.builder.build_unconditional_branch(join_bb)?;
                }
                self.builder.position_at_end(join_bb);
                return Ok(());
            }
        }
        self.builder.build_unconditional_branch(join_bb)?;
        self.builder.position_at_end(join_bb);
        Ok(())
    }

    /// Release every owning part of the value at `place`, of type `sem`, that `test`
    /// matched and no binding path in `bound` (each relative to `place`) takes. A path
    /// ending here takes the whole part; a part no path enters is released whole. Only
    /// an enum's payload slots and an array's elements are walked into: any other shape
    /// a path enters is left as it is.
    fn release_unbound_parts(
        &mut self,
        place: PointerValue<'ctx>,
        sem: &Type,
        test: Option<&HirMatchTest>,
        bound: &[&[HirPatternStep]],
    ) -> CodegenResult<()> {
        if bound.iter().any(|path| path.is_empty()) {
            return Ok(());
        }
        if bound.is_empty() {
            if self.holds_owner(sem) {
                self.emit_captured_drop(place, sem)?;
            }
            return Ok(());
        }
        match sem {
            Type::Enum(name) => {
                let Some(tag) = test.and_then(matched_tag) else {
                    return Ok(());
                };
                let fields = self
                    .type_mapper
                    .enum_payload_types(name)
                    .and_then(|variants| variants.get(tag as usize))
                    .cloned()
                    .unwrap_or_default();
                for (slot, field_ty) in fields.iter().enumerate() {
                    let below: Vec<&[HirPatternStep]> = bound
                        .iter()
                        .filter_map(|path| match path.split_first() {
                            Some((HirPatternStep::EnumPayload { slot: s, .. }, rest))
                                if *s == slot =>
                            {
                                Some(rest)
                            }
                            _ => None,
                        })
                        .collect();
                    let part_test = test.and_then(|t| {
                        part_test(t, |step| {
                            matches!(step, HirPatternStep::EnumPayload { slot: s, .. } if *s == slot)
                        })
                    });
                    let slot_ptr = self.payload_slot(place, sem, slot)?;
                    self.release_unbound_parts(slot_ptr, field_ty, part_test, &below)?;
                }
            }
            Type::Array { element, size } => {
                for index in 0..*size as u64 {
                    let rest_covers = bound.iter().any(|path| {
                        matches!(path.first(), Some(HirPatternStep::Rest { start, .. }) if *start <= index)
                    });
                    if rest_covers {
                        continue;
                    }
                    let below: Vec<&[HirPatternStep]> = bound
                        .iter()
                        .filter_map(|path| match path.split_first() {
                            Some((HirPatternStep::Element { index: i, .. }, rest))
                                if *i == index =>
                            {
                                Some(rest)
                            }
                            _ => None,
                        })
                        .collect();
                    let part_test = test.and_then(|t| {
                        part_test(t, |step| {
                            matches!(step, HirPatternStep::Element { index: i, .. } if *i == index)
                        })
                    });
                    let element_ptr = self.sequence_part(place, sem, index)?;
                    self.release_unbound_parts(element_ptr, element, part_test, &below)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn pattern_part(
        &mut self,
        place: PointerValue<'ctx>,
        sem: &Type,
        step: &HirPatternStep,
    ) -> CodegenResult<(PointerValue<'ctx>, Type)> {
        let part = match step {
            HirPatternStep::EnumPayload { slot, .. } => self.payload_slot(place, sem, *slot)?,
            HirPatternStep::Element { index, .. } => self.sequence_part(place, sem, *index)?,
            HirPatternStep::Rest { start, .. } => {
                let tail = self.sequence_part(place, sem, *start)?;
                if matches!(sem, Type::Array { .. }) {
                    tail
                } else {
                    let (_, len) = self.load_slice_parts(place, sem)?;
                    let start = len.get_type().const_int(*start, false);
                    let rest_len = self.builder.build_int_sub(len, start, "match.rest.len")?;
                    let fat_llvm = self.get_any_llvm_type(sem)?;
                    let fat = self.entry_alloca(fat_llvm, "match.rest")?;
                    let value = fat_llvm.into_struct_type().get_undef();
                    let value = self
                        .builder
                        .build_insert_value(value, tail, 0, "match.rest")?;
                    let value =
                        self.builder
                            .build_insert_value(value, rest_len, 1, "match.rest")?;
                    self.builder.build_store(fat, value.into_struct_value())?;
                    fat
                }
            }
            HirPatternStep::Deref { .. } => {
                let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
                self.builder
                    .build_load(ptr_ty, place, "match.deref")?
                    .into_pointer_value()
            }
        };
        let ty = match step {
            HirPatternStep::EnumPayload { ty, .. }
            | HirPatternStep::Element { ty, .. }
            | HirPatternStep::Rest { ty, .. }
            | HirPatternStep::Deref { ty } => ty,
        };
        Ok((part, Type::from_hir(ty)))
    }

    /// The address of element `index` of the array or slice at `place`.
    fn sequence_part(
        &mut self,
        place: PointerValue<'ctx>,
        sem: &Type,
        index: u64,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let i64_ty = self.context.i64_type();
        let index = i64_ty.const_int(index, false);
        if matches!(sem, Type::Array { .. }) {
            let array_llvm = self.get_any_llvm_type(sem)?;
            // SAFETY: the checker bounds a pattern's elements by the array's length.
            return unsafe {
                self.builder.build_in_bounds_gep(
                    array_llvm,
                    place,
                    &[i64_ty.const_zero(), index],
                    "match.elem",
                )
            }
            .map_err(CodegenError::from);
        }
        let Type::Slice(element) = sem.referent() else {
            return Err(CodegenError::InternalError(
                "an array pattern needs an array or a slice".to_string(),
            ));
        };
        let element_llvm = self.get_any_llvm_type(element)?;
        let (data, _) = self.load_slice_parts(place, sem)?;
        // SAFETY: a `SliceLen` test ahead of every element read proved the index in bounds.
        unsafe {
            self.builder
                .build_in_bounds_gep(element_llvm, data, &[index], "match.elem")
        }
        .map_err(CodegenError::from)
    }

    /// The data pointer and length of the slice borrow at `place`.
    fn load_slice_parts(
        &self,
        place: PointerValue<'ctx>,
        sem: &Type,
    ) -> CodegenResult<(PointerValue<'ctx>, IntValue<'ctx>)> {
        let fat = self
            .builder
            .build_load(self.get_any_llvm_type(sem)?, place, "match.slice")?
            .into_struct_value();
        let data = self.builder.build_extract_value(fat, 0, "match.data")?;
        let len = self
            .builder
            .build_extract_value(fat, 1, "match.slice.len")?;
        Ok((data.into_pointer_value(), len.into_int_value()))
    }

    /// Bring the arm's stored bindings into scope, evaluate its guard (if any), then its
    /// body into the result slot, branching to `merge_bb` on success or `guard_failed`
    /// when the guard fails. Bindings are removed from the name maps before returning.
    fn codegen_arm_body(
        &mut self,
        arm: &HirMatchArm,
        binding_slots: &[BindingSlot<'ctx>],
        ownership: ArmOwnership,
        (result_slot, remainder): (Option<PointerValue<'ctx>>, Option<ArmRemainder<'ctx>>),
        merge_bb: BasicBlock<'ctx>,
        guard_failed: BasicBlock<'ctx>,
    ) -> CodegenResult<()> {
        // An arm binding takes its payload BY VALUE out of a scrutinee the match already
        // disowned, so for the arm's duration the binding is the only owner. Its own drop
        // scope keeps the release inside the arm, where the value is live.
        self.push_drop_scope();
        let saved = self.register_bindings(&arm.bindings, binding_slots, ownership)?;

        if let Some(guard) = &arm.guard {
            let parent_fn = self
                .current_function
                .ok_or_else(|| CodegenError::InternalError("guard outside function".to_string()))?;
            let guard_val = self.codegen_expr(guard)?.into_int_value();
            let eval_bb = self.context.append_basic_block(parent_fn, "match.guard.ok");
            self.builder
                .build_conditional_branch(guard_val, eval_bb, guard_failed)?;
            self.builder.position_at_end(eval_bb);
        }
        // Taken: what the arm's bindings left of the value is released here, after the
        // guard, since a failing one hands the whole value to the next arm.
        if let Some(remainder) = remainder.filter(|_| !arm.bindings.is_empty()) {
            self.release_arm_remainder(arm, &remainder)?;
        }

        match result_slot {
            Some(slot) => {
                let val = self.codegen_expr(&arm.body)?;
                if !self.current_block_terminated() {
                    self.mark_moved_for_drop(&arm.body)?;
                    self.builder.build_store(slot, val)?;
                }
            }
            None => {
                // Void match: the body is evaluated for its effect. A unit-returning
                // call must route through the call dispatch so its `void` result is
                // discarded rather than treated as a missing value.
                if let HirExprKind::Call { callee, args } = &arm.body.kind {
                    self.codegen_call_dispatch(callee, args, &arm.body.span)?;
                } else {
                    self.codegen_expr(&arm.body)?;
                }
            }
        }

        if !self.current_block_terminated() {
            self.emit_top_scope_drops()?;
            self.builder.build_unconditional_branch(merge_bb)?;
        }

        self.pop_drop_scope();
        self.restore_bindings(saved);
        Ok(())
    }

    /// Create, fill and register a single alternative's bindings, returning the prior
    /// entries in the name maps so they can be restored afterwards.
    pub(crate) fn bind_arm(
        &mut self,
        bindings: &[HirMatchBinding],
        scrut_alloca: PointerValue<'ctx>,
        scrut_sem: &Type,
        ownership: ArmOwnership,
    ) -> CodegenResult<Vec<SavedBinding<'ctx>>> {
        let binding_slots = self.binding_slots(bindings)?;
        self.store_bindings(bindings, &binding_slots, 0, scrut_alloca, scrut_sem)?;
        self.register_bindings(bindings, &binding_slots, ownership)
    }

    /// One alloca per binding, shared by the alternatives of its arm.
    fn binding_slots(
        &mut self,
        bindings: &[HirMatchBinding],
    ) -> CodegenResult<Vec<BindingSlot<'ctx>>> {
        bindings
            .iter()
            .map(|binding| {
                let sem = Type::from_hir(&binding.ty);
                let llvm_ty = self.get_any_llvm_type(&sem)?;
                let alloca = self.entry_alloca(llvm_ty, &binding.name)?;
                Ok(BindingSlot {
                    alloca,
                    llvm_ty,
                    sem,
                })
            })
            .collect()
    }

    /// Read each binding out of the scrutinee along `alternative`'s path.
    fn store_bindings(
        &mut self,
        bindings: &[HirMatchBinding],
        binding_slots: &[BindingSlot<'ctx>],
        alternative: usize,
        scrut_alloca: PointerValue<'ctx>,
        scrut_sem: &Type,
    ) -> CodegenResult<()> {
        for (binding, slot) in bindings.iter().zip(binding_slots) {
            let path = binding.paths.get(alternative).ok_or_else(|| {
                CodegenError::InternalError(format!("'{}' has no path here", binding.name))
            })?;
            let mut place = scrut_alloca;
            let mut sem = scrut_sem.clone();
            for step in path {
                (place, sem) = self.pattern_part(place, &sem, step)?;
            }
            let value = self.builder.build_load(slot.llvm_ty, place, "match.bind")?;
            self.builder.build_store(slot.alloca, value)?;
        }
        Ok(())
    }

    /// Put stored bindings in the name maps and hand what they took to `ownership`,
    /// returning what each name meant before.
    fn register_bindings(
        &mut self,
        bindings: &[HirMatchBinding],
        binding_slots: &[BindingSlot<'ctx>],
        ownership: ArmOwnership,
    ) -> CodegenResult<Vec<SavedBinding<'ctx>>> {
        let mut saved = Vec::with_capacity(bindings.len());
        for (binding, slot) in bindings.iter().zip(binding_slots) {
            let BindingSlot {
                alloca,
                llvm_ty,
                sem,
            } = slot;
            let payload = binding.paths.iter().any(|path| !path.is_empty());
            if let (ArmOwnership::Arm { shares }, true) = (ownership, payload) {
                if shares && self.holds_function(sem) {
                    self.retain_held_functions(*alloca, sem)?;
                }
                self.register_owned_binding(&binding.name, *alloca, sem)?;
            }
            saved.push(self.bind_name(&binding.name, *alloca, *llvm_ty, sem.clone()));
        }
        Ok(saved)
    }

    /// Register `name` in the three name maps, returning whatever the name meant
    /// before so an enclosing scope can put it back.
    ///
    /// Every binding form goes through here, which is what keeps the three maps in
    /// step: a name registered in one and missed in another resolves to a slot of the
    /// wrong type later.
    pub(crate) fn bind_name(
        &mut self,
        name: &str,
        alloca: PointerValue<'ctx>,
        llvm_ty: BasicTypeEnum<'ctx>,
        sem: Type,
    ) -> SavedBinding<'ctx> {
        SavedBinding {
            name: name.to_string(),
            ptr: self.variables.insert(name.to_string(), alloca),
            ty: self.variable_types.insert(name.to_string(), llvm_ty),
            sem: self.type_env.insert(name.to_string(), sem),
            constant: self.const_values.get(name).copied(),
        }
    }

    /// Bind a body-level `const`, handing back what the name meant before. A local of
    /// the same name is unbound for the constant's scope: identifiers resolve locals
    /// first, so leaving it in place would let the outer local win over the newer const.
    pub(crate) fn bind_const(
        &mut self,
        name: &str,
        value: BasicValueEnum<'ctx>,
        sem: Type,
    ) -> SavedBinding<'ctx> {
        SavedBinding {
            name: name.to_string(),
            ptr: self.variables.remove(name),
            ty: self.variable_types.remove(name),
            sem: self.type_env.insert(name.to_string(), sem),
            constant: self.const_values.insert(name.to_string(), value),
        }
    }

    /// Restore the name maps to their pre-arm state.
    pub(crate) fn restore_bindings(&mut self, saved: Vec<SavedBinding<'ctx>>) {
        for s in saved.into_iter().rev() {
            match s.ptr {
                Some(prev) => self.variables.insert(s.name.clone(), prev),
                None => self.variables.remove(&s.name),
            };
            match s.ty {
                Some(prev) => self.variable_types.insert(s.name.clone(), prev),
                None => self.variable_types.remove(&s.name),
            };
            match s.sem {
                Some(prev) => self.type_env.insert(s.name.clone(), prev),
                None => self.type_env.remove(&s.name),
            };
            match s.constant {
                Some(prev) => self.const_values.insert(s.name.clone(), prev),
                None => self.const_values.remove(&s.name),
            };
        }
    }

    /// Load the discriminant tag (field 0) of the enum at `place`.
    fn load_enum_tag(
        &self,
        place: PointerValue<'ctx>,
        sem: &Type,
    ) -> CodegenResult<IntValue<'ctx>> {
        let enum_llvm = self.get_any_llvm_type(sem)?.into_struct_type();
        let tag_ptr = self
            .builder
            .build_struct_gep(enum_llvm, place, 0, "match.tag.ptr")?;
        let tag = self
            .builder
            .build_load(self.context.i32_type(), tag_ptr, "match.tag")?;
        Ok(tag.into_int_value())
    }

    /// Load the scalar (integer / `char` / `bool`) at `place`.
    fn load_match_scalar(
        &self,
        place: PointerValue<'ctx>,
        sem: &Type,
    ) -> CodegenResult<IntValue<'ctx>> {
        let val = self
            .builder
            .build_load(self.get_any_llvm_type(sem)?, place, "match.scalar")?;
        Ok(val.into_int_value())
    }
}
