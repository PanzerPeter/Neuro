// Codegen for `match` expressions.
//
// The scrutinee is evaluated once into an alloca. Each arm becomes a test block that
// ORs its alternatives (variant tag / scalar equality / range), branching to the arm
// body on a hit and to the next arm otherwise. An arm body first materializes its
// bindings (the whole scrutinee, or decoded enum payload slots), then — if the arm has
// a guard — branches on the guard before evaluating the body into the shared result
// slot. The frontend guarantees exhaustiveness, so the final fall-through is
// `unreachable`.

use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use inkwell::IntPredicate;
use neuro_hir::{
    HirBindingSource, HirExpr, HirExprKind, HirMatchArm, HirMatchBinding, HirMatchTest,
};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::type_mapping::TypeMapper;
use crate::types::Type;

/// A binding's saved prior state in the three name maps, restored when the arm's
/// blocks are done so bindings do not leak to sibling arms or shadow the outer scope.
struct SavedBinding<'ctx> {
    name: String,
    ptr: Option<PointerValue<'ctx>>,
    ty: Option<BasicTypeEnum<'ctx>>,
    sem: Option<Type>,
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
        let scrut_llvm = scrut_val.get_type();
        let scrut_alloca = self
            .builder
            .build_alloca(scrut_llvm, "match.scrut")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(scrut_alloca, scrut_val)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let is_void = matches!(result_ty, Type::Void);
        let result_slot = if is_void {
            None
        } else {
            let llvm_ty = self.get_any_llvm_type(result_ty)?;
            Some(
                self.builder
                    .build_alloca(llvm_ty, "match.result")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
            )
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

        self.builder
            .build_unconditional_branch(test_bbs[0])
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        for (i, arm) in arms.iter().enumerate() {
            let body_bb = self
                .context
                .append_basic_block(parent_fn, &format!("match.arm{}", i));
            let next_bb = test_bbs[i + 1];

            self.builder.position_at_end(test_bbs[i]);
            let matched =
                self.codegen_arm_tests(&arm.tests, scrut_alloca, scrut_llvm, &scrut_sem)?;
            self.builder
                .build_conditional_branch(matched, body_bb, next_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            self.builder.position_at_end(body_bb);
            self.codegen_arm_body(
                arm,
                scrut_alloca,
                scrut_llvm,
                &scrut_sem,
                result_slot,
                merge_bb,
                next_bb,
            )?;
        }

        // Exhaustiveness is guaranteed by the type checker, so no value reaches here.
        self.builder.position_at_end(fail_bb);
        self.builder
            .build_unreachable()
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(merge_bb);
        match result_slot {
            Some(slot) => {
                let llvm_ty = self.get_any_llvm_type(result_ty)?;
                self.builder
                    .build_load(llvm_ty, slot, "match.val")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))
            }
            None => Ok(self.context.i32_type().const_int(0, false).into()),
        }
    }

    /// OR together an arm's alternative tests into a single `i1` match predicate.
    fn codegen_arm_tests(
        &mut self,
        tests: &[HirMatchTest],
        scrut_alloca: PointerValue<'ctx>,
        scrut_llvm: BasicTypeEnum<'ctx>,
        scrut_sem: &Type,
    ) -> CodegenResult<IntValue<'ctx>> {
        let mut acc: Option<IntValue<'ctx>> = None;
        for test in tests {
            let one = self.codegen_single_test(test, scrut_alloca, scrut_llvm, scrut_sem)?;
            acc = Some(match acc {
                None => one,
                Some(prev) => self
                    .builder
                    .build_or(prev, one, "match.or")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
            });
        }
        match acc {
            Some(v) => Ok(v),
            // An empty test list never matches; fall through.
            None => Ok(self.context.bool_type().const_int(0, false)),
        }
    }

    /// Lower one refutable test to an `i1`.
    fn codegen_single_test(
        &mut self,
        test: &HirMatchTest,
        scrut_alloca: PointerValue<'ctx>,
        scrut_llvm: BasicTypeEnum<'ctx>,
        scrut_sem: &Type,
    ) -> CodegenResult<IntValue<'ctx>> {
        match test {
            HirMatchTest::Wildcard => Ok(self.context.bool_type().const_int(1, false)),
            HirMatchTest::Tag { tag } => {
                let tag_val = self.load_enum_tag(scrut_alloca, scrut_llvm)?;
                let want = self.context.i32_type().const_int(*tag as u64, false);
                self.builder
                    .build_int_compare(IntPredicate::EQ, tag_val, want, "match.tag")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))
            }
            HirMatchTest::IntEq { value } => {
                let scalar = self.load_match_scalar(scrut_alloca, scrut_llvm)?;
                let want = scalar.get_type().const_int(*value as u64, false);
                self.builder
                    .build_int_compare(IntPredicate::EQ, scalar, want, "match.eq")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))
            }
            HirMatchTest::IntRange { lo, hi } => {
                let scalar = self.load_match_scalar(scrut_alloca, scrut_llvm)?;
                let unsigned =
                    TypeMapper::is_unsigned_int(scrut_sem) || matches!(scrut_sem, Type::Char);
                let (ge_pred, le_pred) = if unsigned {
                    (IntPredicate::UGE, IntPredicate::ULE)
                } else {
                    (IntPredicate::SGE, IntPredicate::SLE)
                };
                let lo_c = scalar.get_type().const_int(*lo as u64, false);
                let hi_c = scalar.get_type().const_int(*hi as u64, false);
                let ge = self
                    .builder
                    .build_int_compare(ge_pred, scalar, lo_c, "match.ge")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                let le = self
                    .builder
                    .build_int_compare(le_pred, scalar, hi_c, "match.le")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                self.builder
                    .build_and(ge, le, "match.range")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))
            }
        }
    }

    /// Materialize the arm's bindings, evaluate its guard (if any), then its body into
    /// the result slot, branching to `merge_bb` on success or `next_bb` when a guard
    /// fails. Bindings are removed from the name maps before returning.
    #[allow(clippy::too_many_arguments)]
    fn codegen_arm_body(
        &mut self,
        arm: &HirMatchArm,
        scrut_alloca: PointerValue<'ctx>,
        scrut_llvm: BasicTypeEnum<'ctx>,
        scrut_sem: &Type,
        result_slot: Option<PointerValue<'ctx>>,
        merge_bb: inkwell::basic_block::BasicBlock<'ctx>,
        next_bb: inkwell::basic_block::BasicBlock<'ctx>,
    ) -> CodegenResult<()> {
        let saved = self.bind_arm(&arm.bindings, scrut_alloca, scrut_llvm, scrut_sem)?;

        if let Some(guard) = &arm.guard {
            let parent_fn = self
                .current_function
                .ok_or_else(|| CodegenError::InternalError("guard outside function".to_string()))?;
            let guard_val = self.codegen_expr(guard)?.into_int_value();
            let eval_bb = self.context.append_basic_block(parent_fn, "match.guard.ok");
            self.builder
                .build_conditional_branch(guard_val, eval_bb, next_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.builder.position_at_end(eval_bb);
        }

        match result_slot {
            Some(slot) => {
                let val = self.codegen_expr(&arm.body)?;
                if !self.current_block_terminated() {
                    self.mark_moved_for_drop(&arm.body);
                    self.builder
                        .build_store(slot, val)
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
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
            self.builder
                .build_unconditional_branch(merge_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        }

        self.restore_bindings(saved);
        Ok(())
    }

    /// Create allocas for an arm's bindings and register them in the name maps,
    /// returning the prior entries so they can be restored afterwards.
    fn bind_arm(
        &mut self,
        bindings: &[HirMatchBinding],
        scrut_alloca: PointerValue<'ctx>,
        scrut_llvm: BasicTypeEnum<'ctx>,
        scrut_sem: &Type,
    ) -> CodegenResult<Vec<SavedBinding<'ctx>>> {
        let mut saved = Vec::with_capacity(bindings.len());
        for b in bindings {
            let (value, sem, llvm_ty) = match &b.source {
                HirBindingSource::Scrutinee => {
                    let val = self
                        .builder
                        .build_load(scrut_llvm, scrut_alloca, "match.bind")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                    (val, scrut_sem.clone(), scrut_llvm)
                }
                HirBindingSource::EnumPayload { slot } => {
                    let field_sem = Type::from_hir(&b.ty);
                    let field_llvm = self.get_any_llvm_type(&field_sem)?;
                    let raw = self.load_enum_payload_slot(scrut_alloca, scrut_llvm, *slot)?;
                    let decoded = self.decode_enum_payload_field(raw, &field_sem)?;
                    (decoded, field_sem, field_llvm)
                }
            };

            let alloca = self
                .builder
                .build_alloca(llvm_ty, &b.name)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.builder
                .build_store(alloca, value)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

            saved.push(SavedBinding {
                name: b.name.clone(),
                ptr: self.variables.insert(b.name.clone(), alloca),
                ty: self.variable_types.insert(b.name.clone(), llvm_ty),
                sem: self.type_env.insert(b.name.clone(), sem),
            });
        }
        Ok(saved)
    }

    /// Restore the name maps to their pre-arm state.
    fn restore_bindings(&mut self, saved: Vec<SavedBinding<'ctx>>) {
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
        }
    }

    /// Load the discriminant tag (field 0) of an enum scrutinee.
    fn load_enum_tag(
        &self,
        scrut_alloca: PointerValue<'ctx>,
        scrut_llvm: BasicTypeEnum<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        let agg = self
            .builder
            .build_load(scrut_llvm, scrut_alloca, "match.enum")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let tag = self
            .builder
            .build_extract_value(agg.into_struct_value(), 0, "match.tag")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(tag.into_int_value())
    }

    /// Load enum payload slot `slot` (a packed `i64`) of an enum scrutinee.
    fn load_enum_payload_slot(
        &self,
        scrut_alloca: PointerValue<'ctx>,
        scrut_llvm: BasicTypeEnum<'ctx>,
        slot: usize,
    ) -> CodegenResult<IntValue<'ctx>> {
        let agg = self
            .builder
            .build_load(scrut_llvm, scrut_alloca, "match.enum")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let payload = self
            .builder
            .build_extract_value(agg.into_struct_value(), 1, "match.payload")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let word = self
            .builder
            .build_extract_value(payload.into_array_value(), slot as u32, "match.slot")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(word.into_int_value())
    }

    /// Load a scalar scrutinee (integer / `char` / `bool`) as an integer value.
    fn load_match_scalar(
        &self,
        scrut_alloca: PointerValue<'ctx>,
        scrut_llvm: BasicTypeEnum<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        let val = self
            .builder
            .build_load(scrut_llvm, scrut_alloca, "match.scalar")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(val.into_int_value())
    }

    /// Decode one packed `i64` payload slot back to a field of `field_ty` — the inverse
    /// of the enum construction encoding: truncate to the field's width, then
    /// bitcast back to a float when the field is a float.
    fn decode_enum_payload_field(
        &self,
        raw: IntValue<'ctx>,
        field_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        if field_ty.is_float() || matches!(field_ty, Type::F16 | Type::BF16) {
            let int_width = match field_ty {
                Type::F16 | Type::BF16 => self.context.i16_type(),
                Type::F32 => self.context.i32_type(),
                Type::F64 => self.context.i64_type(),
                _ => unreachable!("guarded by the float check above"),
            };
            let narrowed = self
                .builder
                .build_int_truncate(raw, int_width, "match.fbits")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            let float_ty = self.type_mapper.map_type(field_ty)?;
            return self
                .builder
                .build_bit_cast(narrowed, float_ty, "match.fdecode")
                .map_err(|e| CodegenError::LlvmError(e.to_string()));
        }

        let target = self.type_mapper.map_type(field_ty)?.into_int_type();
        if target.get_bit_width() == 64 {
            return Ok(raw.into());
        }
        let narrowed = self
            .builder
            .build_int_truncate(raw, target, "match.idecode")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(narrowed.into())
    }
}
