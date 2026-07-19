//! Expression lowering with resolved-type re-derivation.

use ast_types::{BinaryOp, Expr, FieldInit, UnaryOp};
use neuro_hir::{HirExpr, HirExprKind, HirFieldInit, HirStmt, HirType};
use shared_types::Literal;

use crate::types::{float_suffix_type, int_suffix_type};
use crate::{is_full_float, is_integer, peels_to_string, LoopCtx, Lowerer, LoweringError};

/// The divergent panic-family builtins. Each aborts and never returns, so a
/// call takes on whatever type its context demands.
const PANIC_BUILTINS: &[&str] = &["panic", "assert", "unreachable"];

/// The deep-copy method shared by `string` and `Clone`-deriving structs.
const CLONE_METHOD: &str = "clone";

/// An enum variant's ordered payload fields: each `(optional field name, type)`.
/// `Some` name marks a struct-variant field; `None` a tuple-variant element.
type PayloadFields = Vec<(Option<String>, HirType)>;

impl Lowerer {
    /// Lower an expression to a typed [`HirExpr`], deriving its resolved type from
    /// the surrounding `expected` type where the language's contextual inference
    /// rules require it (literals, array elements, …).
    pub(crate) fn lower_expr(
        &mut self,
        expr: &Expr,
        expected: Option<&HirType>,
    ) -> Result<HirExpr, LoweringError> {
        let lowered = self.lower_expr_uncoerced(expr, expected)?;
        Ok(apply_dyn_coercion(lowered, expected))
    }

    /// Lower an expression without applying the trait-object coercion. Every contextual
    /// typing rule lives here; [`Lowerer::lower_expr`] wraps the result so the single
    /// `&T` → `&dyn Trait` unsizing site is applied uniformly wherever an
    /// expected type is supplied — call arguments, returns, and annotated bindings.
    fn lower_expr_uncoerced(
        &mut self,
        expr: &Expr,
        expected: Option<&HirType>,
    ) -> Result<HirExpr, LoweringError> {
        match expr {
            // Grouping is encoded by tree structure in the HIR; drop the node.
            Expr::Paren(inner, _) => self.lower_expr(inner, expected),

            Expr::Literal(lit, span) => {
                let ty = literal_type(lit, expected);
                Ok(HirExpr::new(HirExprKind::Literal(lit.clone()), ty, *span))
            }

            // A const generic parameter used as a value inside a monomorphized body
            // lowers to its concrete integer literal, typed by its declared int type.
            Expr::Identifier(ident) if self.const_subst.contains_key(&ident.name) => {
                let value = self.const_subst[&ident.name];
                let ty = self
                    .const_types
                    .get(&ident.name)
                    .cloned()
                    .unwrap_or(HirType::U64);
                Ok(HirExpr::new(
                    HirExprKind::Literal(Literal::Integer(value as i64, None)),
                    ty,
                    ident.span,
                ))
            }
            Expr::Identifier(ident) => match self.lookup(&ident.name) {
                Some(ty) => Ok(HirExpr::new(
                    HirExprKind::Variable(ident.name.clone()),
                    ty,
                    ident.span,
                )),
                None => Err(LoweringError::UnresolvedBinding {
                    name: ident.name.clone(),
                }),
            },

            Expr::Binary {
                left,
                op,
                right,
                span,
            } => {
                let left = self.lower_expr(left, None)?;
                // Operator-trait dispatch on a user type: desugar `a OP b` into the
                // impl method call `a.op(b)`. The checker validated the impl, so a lookup
                // hit means the call resolves.
                if let HirType::Struct(name) = left.ty.referent() {
                    if let Some(dispatch) = self.operator_binary_impls.get(&(name.clone(), *op)) {
                        let dispatch = crate::OpDispatch {
                            method: dispatch.method.clone(),
                            rhs_param: dispatch.rhs_param.clone(),
                            result: dispatch.result.clone(),
                        };
                        let right = self.lower_expr(right, None)?;
                        return self.build_operator_call(left, right, dispatch, *span);
                    }
                }
                let right = self.lower_expr(right, Some(&left.ty))?;
                let ty = binary_result_type(*op, &left.ty, &right.ty)?;
                Ok(HirExpr::new(
                    HirExprKind::Binary {
                        op: *op,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    ty,
                    *span,
                ))
            }

            Expr::Unary { op, operand, span } => {
                let operand_expected = match op {
                    UnaryOp::Negate => expected.filter(|t| is_numeric(t)),
                    UnaryOp::Not => None,
                    UnaryOp::BitNot => expected.filter(|t| is_integer(t)),
                };
                let operand = self.lower_expr(operand, operand_expected)?;
                // Operator-trait dispatch: `-a` → `a.neg()`, `~a` → `a.not()`.
                if let HirType::Struct(name) = operand.ty.referent() {
                    if let Some((method, result)) =
                        self.operator_unary_impls.get(&(name.clone(), *op))
                    {
                        let (method, result) = (method.clone(), result.clone());
                        return Ok(self.build_unary_operator_call(operand, method, result, *span));
                    }
                }
                let ty = match op {
                    UnaryOp::Negate | UnaryOp::BitNot => operand.ty.clone(),
                    UnaryOp::Not => HirType::Bool,
                };
                Ok(HirExpr::new(
                    HirExprKind::Unary {
                        op: *op,
                        operand: Box::new(operand),
                    },
                    ty,
                    *span,
                ))
            }

            Expr::Cast {
                expr,
                target_type,
                span,
            } => {
                let value = self.lower_expr(expr, None)?;
                let ty = self.resolve_type(target_type)?;
                Ok(HirExpr::new(
                    HirExprKind::Cast {
                        value: Box::new(value),
                    },
                    ty,
                    *span,
                ))
            }

            Expr::Call {
                func,
                type_args,
                args,
                span,
            } => self.lower_call(func, type_args, args, expected, *span),

            // A bare path is a unit-variant enum construction `E::V` when the
            // type names an enum, else an associated-function reference.
            Expr::Path {
                type_name,
                member,
                span,
            } if self.enums.contains_key(&type_name.name) => {
                self.lower_enum_construct(&type_name.name, &member.name, Vec::new(), *span)
            }

            Expr::Path {
                type_name,
                member,
                span,
            } => {
                let (params, ret) = self.assoc_signature(&type_name.name, &member.name)?;
                let ty = HirType::Function {
                    params,
                    ret: Box::new(ret),
                };
                Ok(HirExpr::new(
                    HirExprKind::Path {
                        type_name: type_name.name.clone(),
                        member: member.name.clone(),
                    },
                    ty,
                    *span,
                ))
            }

            Expr::StructLiteral {
                name,
                fields,
                base,
                span,
            } => self.lower_struct_literal(name, fields, base, *span),

            Expr::EnumStructLiteral {
                enum_name,
                variant,
                fields,
                span,
            } => self.lower_enum_struct_literal(&enum_name.name, &variant.name, fields, *span),

            Expr::FieldAccess {
                object,
                field,
                span,
            } => {
                let object = self.lower_expr(object, None)?;
                let HirType::Struct(struct_name) = object.ty.referent().clone() else {
                    return Err(LoweringError::Malformed {
                        detail: format!("field access on non-struct type '{}'", object.ty),
                    });
                };
                let ty = self.struct_field_type(&struct_name, &field.name)?;
                Ok(HirExpr::new(
                    HirExprKind::FieldAccess {
                        object: Box::new(object),
                        field: field.name.clone(),
                    },
                    ty,
                    *span,
                ))
            }

            Expr::Reference {
                operand,
                mutable,
                span,
            } => {
                let operand = self.lower_expr(operand, None)?;
                let ty = HirType::Reference {
                    inner: Box::new(operand.ty.clone()),
                    mutable: *mutable,
                };
                Ok(HirExpr::new(
                    HirExprKind::Reference {
                        operand: Box::new(operand),
                        mutable: *mutable,
                    },
                    ty,
                    *span,
                ))
            }

            Expr::Deref { operand, span } => {
                let operand = self.lower_expr(operand, None)?;
                let HirType::Reference { inner, .. } = &operand.ty else {
                    return Err(LoweringError::Malformed {
                        detail: format!("dereference of non-reference type '{}'", operand.ty),
                    });
                };
                let ty = (**inner).clone();
                Ok(HirExpr::new(
                    HirExprKind::Deref {
                        operand: Box::new(operand),
                    },
                    ty,
                    *span,
                ))
            }

            Expr::Range {
                start,
                end,
                inclusive,
                span,
            } => self.lower_range(start, end, *inclusive, *span),

            Expr::ArrayLiteral { elements, span } => {
                self.lower_array_literal(elements, expected, *span)
            }

            Expr::TupleLiteral { elements, span } => {
                self.lower_tuple_literal(elements, expected, *span)
            }

            Expr::TupleIndex {
                object,
                index,
                span,
            } => {
                let object = self.lower_expr(object, None)?;
                match object.ty.referent().clone() {
                    HirType::Tuple(element_tys) => {
                        let element_ty = element_tys.get(*index).cloned().ok_or_else(|| {
                            LoweringError::Malformed {
                                detail: format!(
                                    "tuple index {} out of range for arity {}",
                                    index,
                                    element_tys.len()
                                ),
                            }
                        })?;
                        Ok(HirExpr::new(
                            HirExprKind::TupleIndex {
                                object: Box::new(object),
                                index: *index,
                            },
                            element_ty,
                            *span,
                        ))
                    }
                    // `.0` on a newtype reads its transparent inner value. The
                    // checker guarantees the index is 0.
                    HirType::Newtype { inner, .. } => Ok(HirExpr::new(
                        HirExprKind::NewtypeAccess {
                            object: Box::new(object),
                        },
                        *inner,
                        *span,
                    )),
                    other => Err(LoweringError::Malformed {
                        detail: format!("tuple index into non-tuple type '{}'", other),
                    }),
                }
            }

            Expr::Index {
                object,
                index,
                span,
            } => {
                let object = self.lower_expr(object, None)?;
                let index = self.lower_expr(index, None)?;
                let HirType::Array { element, .. } = object.ty.referent().clone() else {
                    return Err(LoweringError::Malformed {
                        detail: format!("index into non-array type '{}'", object.ty),
                    });
                };
                Ok(HirExpr::new(
                    HirExprKind::Index {
                        object: Box::new(object),
                        index: Box::new(index),
                    },
                    *element,
                    *span,
                ))
            }

            Expr::ArrayRest {
                array,
                start,
                exact,
                span,
            } => {
                let array = self.lower_expr(array, None)?;
                let HirType::Array { element, size } = array.ty.referent().clone() else {
                    return Err(LoweringError::Malformed {
                        detail: format!("array rest pattern on non-array type '{}'", array.ty),
                    });
                };
                // Arity is validated in semantic analysis; re-check here so a
                // malformed input surfaces as an error rather than a subtraction
                // underflow on `size - start`.
                if (*exact && size != *start) || (!*exact && *start > size) {
                    return Err(LoweringError::Malformed {
                        detail: format!(
                            "array destructuring binds {} element(s) but the array has {}",
                            start, size
                        ),
                    });
                }
                Ok(HirExpr::new(
                    HirExprKind::ArrayRest {
                        array: Box::new(array),
                        start: *start,
                    },
                    HirType::Array {
                        element,
                        size: size - *start,
                    },
                    *span,
                ))
            }

            Expr::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                span,
            } => {
                let condition = self.lower_expr(condition, Some(&HirType::Bool))?;
                let (then_stmts, then_ty) = self.lower_block_value(then_block)?;
                let mut elifs = Vec::with_capacity(else_if_blocks.len());
                for (cond, block) in else_if_blocks {
                    let cond = self.lower_expr(cond, Some(&HirType::Bool))?;
                    let (block, _) = self.lower_block_value(block)?;
                    elifs.push((cond, block));
                }
                // An `if` is a value only with an `else`; otherwise it yields unit.
                let (else_block, ty) = match else_block {
                    Some(block) => {
                        let (block, _) = self.lower_block_value(block)?;
                        (Some(block), then_ty)
                    }
                    None => (None, HirType::Void),
                };
                Ok(HirExpr::new(
                    HirExprKind::If {
                        condition: Box::new(condition),
                        then_block: then_stmts,
                        else_if_blocks: elifs,
                        else_block,
                    },
                    ty,
                    *span,
                ))
            }

            Expr::Block { stmts, span } => {
                let (stmts, ty) = self.lower_block_value(stmts)?;
                Ok(HirExpr::new(HirExprKind::Block { stmts }, ty, *span))
            }

            Expr::Unsafe { stmts, span } => {
                let (stmts, ty) = self.lower_block_value(stmts)?;
                Ok(HirExpr::new(HirExprKind::Unsafe { stmts }, ty, *span))
            }

            Expr::Loop { label, body, span } => {
                let label_name = label.as_ref().map(|l| l.name.clone());
                self.loop_stack.push(LoopCtx {
                    label: label_name.clone(),
                    is_value: true,
                    value_ty: None,
                });
                self.push_scope();
                let body = self.lower_stmt_list(body);
                self.pop_scope();
                let ctx = self.loop_stack.pop();
                let body = body?;
                let ty = ctx.and_then(|c| c.value_ty).unwrap_or(HirType::Void);
                Ok(HirExpr::new(
                    HirExprKind::Loop {
                        label: label_name,
                        body,
                    },
                    ty,
                    *span,
                ))
            }

            Expr::Match {
                scrutinee,
                arms,
                span,
            } => self.lower_match(scrutinee, arms, expected, *span),
        }
    }

    /// Lower a `match` expression into the fully-resolved HIR node: each arm's
    /// patterns become refutable tests, its bindings resolve to payload slots or the
    /// whole scrutinee, and the guard/body lower with the bindings in scope. The match
    /// type is the first arm's body type, mirroring the checker.
    fn lower_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[ast_types::MatchArm],
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let scrutinee = self.lower_expr(scrutinee, None)?;
        let scrut_ty = scrutinee.ty.clone();

        // Body-type hint, mirroring the checker: the expected type if any, else the
        // first arm's type, so a later `_ => 0` infers a sibling arm's integer width.
        let mut hint: Option<HirType> = expected.cloned();
        let mut hir_arms = Vec::with_capacity(arms.len());
        let mut result_ty = HirType::Void;
        for (i, arm) in arms.iter().enumerate() {
            let mut tests = Vec::with_capacity(arm.patterns.len());
            for pat in &arm.patterns {
                tests.push(self.pattern_test(pat)?);
            }
            // Only a single-pattern arm binds (or-patterns cannot).
            let bindings = if arm.patterns.len() == 1 {
                self.pattern_bindings(&arm.patterns[0], &scrut_ty)?
            } else {
                Vec::new()
            };

            self.push_scope();
            for b in &bindings {
                self.define(b.name.clone(), b.ty.clone());
            }
            let guard = match &arm.guard {
                Some(g) => Some(self.lower_expr(g, Some(&HirType::Bool))?),
                None => None,
            };
            let body = self.lower_expr(&arm.body, hint.as_ref())?;
            self.pop_scope();

            if hint.is_none() {
                hint = Some(body.ty.clone());
            }
            if i == 0 {
                result_ty = body.ty.clone();
            }
            hir_arms.push(neuro_hir::HirMatchArm {
                tests,
                bindings,
                guard,
                body,
            });
        }

        Ok(HirExpr::new(
            HirExprKind::Match {
                scrutinee: Box::new(scrutinee),
                arms: hir_arms,
            },
            result_ty,
            span,
        ))
    }

    /// Build the refutable [`HirMatchTest`] for one pattern.
    fn pattern_test(
        &self,
        pat: &ast_types::Pattern,
    ) -> Result<neuro_hir::HirMatchTest, LoweringError> {
        use neuro_hir::HirMatchTest;
        match pat {
            ast_types::Pattern::Wildcard(_) | ast_types::Pattern::Binding(_) => {
                Ok(HirMatchTest::Wildcard)
            }
            ast_types::Pattern::Literal(lit, _) => Ok(HirMatchTest::IntEq {
                value: literal_scalar(lit)?,
            }),
            ast_types::Pattern::Range {
                start,
                end,
                inclusive,
                ..
            } => {
                let lo = literal_scalar(start)?;
                let hi_raw = literal_scalar(end)?;
                // Normalize an exclusive `a..b` to the inclusive `a..=b-1` codegen uses.
                let hi = if *inclusive { hi_raw } else { hi_raw - 1 };
                Ok(HirMatchTest::IntRange { lo, hi })
            }
            ast_types::Pattern::Enum {
                enum_name, variant, ..
            } => {
                let (tag, _) = self.enum_variant(&enum_name.name, &variant.name)?;
                Ok(HirMatchTest::Tag { tag })
            }
        }
    }

    /// Resolve the bindings a single (non-or) pattern introduces: the whole
    /// scrutinee for a bare binding, or payload slot extractions for an enum pattern.
    fn pattern_bindings(
        &self,
        pat: &ast_types::Pattern,
        scrut_ty: &HirType,
    ) -> Result<Vec<neuro_hir::HirMatchBinding>, LoweringError> {
        use neuro_hir::{HirBindingSource, HirMatchBinding};
        match pat {
            ast_types::Pattern::Binding(ident) => Ok(vec![HirMatchBinding {
                name: ident.name.clone(),
                ty: scrut_ty.clone(),
                source: HirBindingSource::Scrutinee,
            }]),
            ast_types::Pattern::Enum {
                enum_name,
                variant,
                payload,
                ..
            } => {
                let (_, fields) = self.enum_variant(&enum_name.name, &variant.name)?;
                let mut bindings = Vec::new();
                match payload {
                    ast_types::EnumPatternPayload::Unit => {}
                    ast_types::EnumPatternPayload::Tuple(subs) => {
                        for (slot, sub) in subs.iter().enumerate() {
                            if let ast_types::Pattern::Binding(ident) = sub {
                                let ty = fields
                                    .get(slot)
                                    .map(|(_, t)| t.clone())
                                    .unwrap_or(HirType::Void);
                                bindings.push(HirMatchBinding {
                                    name: ident.name.clone(),
                                    ty,
                                    source: HirBindingSource::EnumPayload { slot },
                                });
                            }
                        }
                    }
                    ast_types::EnumPatternPayload::Struct(field_pats) => {
                        for fp in field_pats {
                            if let ast_types::Pattern::Binding(ident) = &fp.pattern {
                                let slot = fields
                                    .iter()
                                    .position(|(n, _)| n.as_deref() == Some(fp.field.name.as_str()))
                                    .ok_or_else(|| LoweringError::Malformed {
                                        detail: format!(
                                            "unknown field '{}' in variant '{}::{}' pattern",
                                            fp.field.name, enum_name.name, variant.name
                                        ),
                                    })?;
                                let ty = fields[slot].1.clone();
                                bindings.push(HirMatchBinding {
                                    name: ident.name.clone(),
                                    ty,
                                    source: HirBindingSource::EnumPayload { slot },
                                });
                            }
                        }
                    }
                }
                Ok(bindings)
            }
            _ => Ok(Vec::new()),
        }
    }

    /// Lower a call, dispatching on the callee shape: free/builtin function,
    /// instance method, or associated function.
    fn lower_call(
        &mut self,
        func: &Expr,
        type_args: &[ast_types::GenericArg],
        args: &[Expr],
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        match func {
            // Newtype construction `Name(value)` takes precedence over a
            // same-named free function in call position, matching the checker.
            Expr::Identifier(ident) if self.newtypes.contains_key(&ident.name) => {
                self.lower_newtype_construct(&ident.name, args, span)
            }
            Expr::Identifier(ident) => {
                self.lower_ident_call(&ident.name, type_args, args, expected, span)
            }
            Expr::FieldAccess { object, field, .. } => {
                self.lower_method_call(object, &field.name, args, span)
            }
            // `Enum::Variant(args)` is a tuple-variant construction when the
            // type names an enum; otherwise an associated-function call.
            Expr::Path {
                type_name, member, ..
            } if self.enums.contains_key(&type_name.name) => {
                self.lower_enum_tuple_call(&type_name.name, &member.name, args, span)
            }
            Expr::Path {
                type_name, member, ..
            } => self.lower_assoc_call(&type_name.name, &member.name, args, span),
            other => Err(LoweringError::Malformed {
                detail: format!(
                    "call of non-callable expression {:?}",
                    std::mem::discriminant(other)
                ),
            }),
        }
    }

    /// Lower a newtype construction `Name(value)` to a transparent
    /// [`HirExprKind::NewtypeConstruct`] wrapper. The checker guarantees exactly one
    /// argument that matches the inner type.
    fn lower_newtype_construct(
        &mut self,
        name: &str,
        args: &[Expr],
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let inner_ast = self.newtypes[name].clone();
        let inner_ty = self.resolve_type(&inner_ast)?;
        let [arg] = args else {
            return Err(LoweringError::Malformed {
                detail: format!(
                    "newtype '{}' construction expects one argument, found {}",
                    name,
                    args.len()
                ),
            });
        };
        let value = self.lower_expr(arg, Some(&inner_ty))?;
        let nt_ty = HirType::Newtype {
            name: name.to_string(),
            inner: Box::new(inner_ty),
        };
        Ok(HirExpr::new(
            HirExprKind::NewtypeConstruct {
                name: name.to_string(),
                value: Box::new(value),
            },
            nt_ty,
            span,
        ))
    }

    /// Lower a plain identifier call: a registered free function, or one of the
    /// divergent panic-family builtins (which take their context's type).
    fn lower_ident_call(
        &mut self,
        name: &str,
        type_args: &[ast_types::GenericArg],
        args: &[Expr],
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        // A call to a generic function: infer its type arguments, queue the
        // matching monomorphized instance, and emit a call to that instance's name.
        if self.generic_templates.contains_key(name) {
            return self.lower_generic_call(name, type_args, args, span);
        }

        if let Some((params, ret)) = self.functions.get(name).cloned() {
            let args = self.lower_args(args, &params)?;
            let callee = HirExpr::new(
                HirExprKind::Variable(name.to_string()),
                HirType::Function {
                    params,
                    ret: Box::new(ret.clone()),
                },
                span,
            );
            return Ok(HirExpr::new(
                HirExprKind::Call {
                    callee: Box::new(callee),
                    args,
                },
                ret,
                span,
            ));
        }

        if PANIC_BUILTINS.contains(&name) {
            let param = match name {
                "panic" => vec![HirType::String],
                "assert" => vec![HirType::Bool],
                _ => vec![],
            };
            let args = self.lower_args(args, &param)?;
            // Divergent: adopt the expected context type, or unit in statement position.
            let ret = expected.cloned().unwrap_or(HirType::Void);
            let callee = HirExpr::new(
                HirExprKind::Variable(name.to_string()),
                HirType::Function {
                    params: param,
                    ret: Box::new(HirType::Void),
                },
                span,
            );
            return Ok(HirExpr::new(
                HirExprKind::Call {
                    callee: Box::new(callee),
                    args,
                },
                ret,
                span,
            ));
        }

        Err(LoweringError::UnresolvedCall {
            target: name.to_string(),
        })
    }

    /// Lower a call to a generic function to a call to its monomorphized instance
    /// The concrete type arguments are inferred by unifying the template's
    /// parameter annotations against the lowered arguments' resolved types; the
    /// instance is queued for emission and the call refers to its mangled name.
    fn lower_generic_call(
        &mut self,
        name: &str,
        type_args: &[ast_types::GenericArg],
        args: &[Expr],
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let template = self.generic_templates[name].clone();
        let gnames: std::collections::HashSet<String> = template
            .generics
            .iter()
            .filter(|g| matches!(g.kind, ast_types::GenericParamKind::Type))
            .map(|g| g.name.name.clone())
            .collect();
        let cnames: std::collections::HashSet<String> = template
            .generics
            .iter()
            .filter(|g| matches!(g.kind, ast_types::GenericParamKind::Const(_)))
            .map(|g| g.name.name.clone())
            .collect();

        let mut subst: std::collections::HashMap<String, HirType> =
            std::collections::HashMap::new();
        let mut const_subst: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        // Seed explicit turbofish arguments before inference, positionally.
        for (gp, arg) in template.generics.iter().zip(type_args.iter()) {
            match arg {
                ast_types::GenericArg::Const { value, .. } => {
                    const_subst.insert(gp.name.name.clone(), *value as u64);
                }
                ast_types::GenericArg::Type(ty) => {
                    subst.insert(gp.name.name.clone(), self.resolve_type(ty)?);
                }
            }
        }

        // Arguments drive inference, so lower them with no expected type first.
        let mut lowered_args = Vec::with_capacity(args.len());
        for arg in args {
            lowered_args.push(self.lower_expr(arg, None)?);
        }
        for (param, larg) in template.params.iter().zip(lowered_args.iter()) {
            crate::unify_ast_hir(
                &param.ty,
                &larg.ty,
                &gnames,
                &cnames,
                &mut subst,
                &mut const_subst,
            );
        }

        // Resolve the concrete parameter and return types under the inferred bindings.
        let saved_ty = std::mem::replace(&mut self.type_subst, subst.clone());
        let saved_c = std::mem::replace(&mut self.const_subst, const_subst.clone());
        let mut param_tys = Vec::with_capacity(template.params.len());
        for param in &template.params {
            param_tys.push(self.resolve_type(&param.ty)?);
        }
        let ret = match &template.return_type {
            Some(t) => self.resolve_type(t)?,
            None => HirType::Void,
        };
        self.type_subst = saved_ty;
        self.const_subst = saved_c;

        let mangled = crate::mangle_instance(name, &template.generics, &subst, &const_subst);
        if !self.mono_seen.contains(&mangled) {
            self.mono_seen.insert(mangled.clone());
            self.mono_pending.push(crate::MonoInstance {
                mangled: mangled.clone(),
                fn_name: name.to_string(),
                subst,
                const_subst,
            });
        }

        let callee = HirExpr::new(
            HirExprKind::Variable(mangled),
            HirType::Function {
                params: param_tys,
                ret: Box::new(ret.clone()),
            },
            span,
        );
        Ok(HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(callee),
                args: lowered_args,
            },
            ret,
            span,
        ))
    }

    /// Build the method call an overloaded binary operator desugars to:
    /// `a OP b` → `a.op(b)`. When the method's right parameter is a reference
    /// (`rhs: &Rhs`, the comparison traits), the argument is borrowed.
    fn build_operator_call(
        &mut self,
        object: HirExpr,
        rhs: HirExpr,
        dispatch: crate::OpDispatch,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let arg = if let HirType::Reference { mutable, .. } = &dispatch.rhs_param {
            let mutable = *mutable;
            let ty = HirType::Reference {
                inner: Box::new(rhs.ty.clone()),
                mutable,
            };
            HirExpr::new(
                HirExprKind::Reference {
                    operand: Box::new(rhs),
                    mutable,
                },
                ty,
                span,
            )
        } else {
            rhs
        };
        let callee = HirExpr::new(
            HirExprKind::FieldAccess {
                object: Box::new(object),
                field: dispatch.method,
            },
            dispatch.result.clone(),
            span,
        );
        Ok(HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(callee),
                args: vec![arg],
            },
            dispatch.result,
            span,
        ))
    }

    /// Build the method call an overloaded unary operator desugars to:
    /// `-a` → `a.neg()`, `~a` → `a.not()`.
    fn build_unary_operator_call(
        &mut self,
        operand: HirExpr,
        method: String,
        result: HirType,
        span: shared_types::Span,
    ) -> HirExpr {
        let callee = HirExpr::new(
            HirExprKind::FieldAccess {
                object: Box::new(operand),
                field: method,
            },
            result.clone(),
            span,
        );
        HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(callee),
                args: Vec::new(),
            },
            result,
            span,
        )
    }

    /// Lower `instance.method(args)`: a struct method (or the `.clone()` builtin on a
    /// `Clone` struct), or a compiler-known intrinsic on a builtin receiver.
    fn lower_method_call(
        &mut self,
        object: &Expr,
        method: &str,
        args: &[Expr],
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let object = self.lower_expr(object, None)?;
        let recv = object.ty.clone();

        let (lowered_args, result_ty) = if let HirType::Struct(struct_name) = recv.referent() {
            let struct_name = struct_name.clone();
            if let Some(mangled) = self
                .impl_methods
                .get(&struct_name)
                .and_then(|m| m.get(method))
                .cloned()
            {
                let (params, ret) = self.functions.get(&mangled).cloned().ok_or_else(|| {
                    LoweringError::UnresolvedCall {
                        target: mangled.clone(),
                    }
                })?;
                // params[0] is the implicit `self`; callers pass only the rest.
                let visible = if params.is_empty() {
                    &params[..]
                } else {
                    &params[1..]
                };
                (self.lower_args(args, visible)?, ret)
            } else if method == CLONE_METHOD && self.clone_structs.contains(&struct_name) {
                (self.lower_args(args, &[])?, HirType::Struct(struct_name))
            } else {
                return Err(LoweringError::UnresolvedCall {
                    target: format!("{}.{}", struct_name, method),
                });
            }
        } else if let HirType::DynObject(trait_name) = recv.referent() {
            // Dynamic dispatch: the call is typed from the trait's declaration —
            // no implementor is named here, since the concrete method is selected at
            // runtime through the vtable. The backend keys off the receiver's type.
            let trait_name = trait_name.clone();
            let sig = self
                .traits
                .get(&trait_name)
                .and_then(|ms| ms.iter().find(|m| m.name == method))
                .ok_or_else(|| LoweringError::UnresolvedCall {
                    target: format!("dyn {}.{}", trait_name, method),
                })?;
            let (params, ret) = (sig.params.clone(), sig.ret.clone());
            (self.lower_args(args, &params)?, ret)
        } else {
            self.lower_builtin_method(&recv, method, args)?
        };

        // The method-name callee is a synthetic node (the language has no first-class
        // method value); it carries the call's result type as a convenience for
        // backends that key dispatch off the field name and receiver type.
        let callee = HirExpr::new(
            HirExprKind::FieldAccess {
                object: Box::new(object),
                field: method.to_string(),
            },
            result_ty.clone(),
            span,
        );
        Ok(HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(callee),
                args: lowered_args,
            },
            result_ty,
            span,
        ))
    }

    /// Resolve a compiler-known intrinsic on a builtin (non-struct) receiver,
    /// returning the lowered arguments and the result type.
    fn lower_builtin_method(
        &mut self,
        recv: &HirType,
        method: &str,
        args: &[Expr],
    ) -> Result<(Vec<HirExpr>, HirType), LoweringError> {
        // String intrinsics auto-deref through `&string`, so match on the referent.
        match (recv.referent(), method) {
            (HirType::String, "len") => Ok((self.lower_args(args, &[])?, HirType::U64)),
            (HirType::String, "clone") => Ok((self.lower_args(args, &[])?, HirType::String)),
            (HirType::String, "slice") => {
                let arg = args.first().ok_or_else(|| LoweringError::Malformed {
                    detail: "string.slice expects a range argument".to_string(),
                })?;
                let range = self.lower_expr(arg, None)?;
                let slice_ty = HirType::Reference {
                    inner: Box::new(HirType::String),
                    mutable: false,
                };
                Ok((vec![range], slice_ty))
            }
            (HirType::Array { .. }, "len") => Ok((self.lower_args(args, &[])?, HirType::U64)),
            (
                _,
                "wrapping_add" | "wrapping_sub" | "wrapping_mul" | "saturating_add"
                | "saturating_sub" | "saturating_mul" | "shr",
            ) if is_integer(recv) => {
                let args = self.lower_args(args, std::slice::from_ref(recv))?;
                Ok((args, recv.clone()))
            }
            _ => Err(LoweringError::UnresolvedCall {
                target: format!("{}.{}", recv, method),
            }),
        }
    }

    /// Look up an enum variant by name, returning its discriminant tag (declaration
    /// index) and a clone of its ordered payload fields. The clone frees the enum
    /// table's immutable borrow before the mutable argument lowering that follows.
    fn enum_variant(
        &self,
        enum_name: &str,
        variant: &str,
    ) -> Result<(u32, PayloadFields), LoweringError> {
        let variants = self
            .enums
            .get(enum_name)
            .ok_or_else(|| LoweringError::UnresolvedType {
                name: enum_name.to_string(),
            })?;
        variants
            .iter()
            .enumerate()
            .find(|(_, v)| v.name == variant)
            .map(|(i, v)| (i as u32, v.fields.clone()))
            .ok_or_else(|| LoweringError::UnresolvedCall {
                target: format!("{}::{}", enum_name, variant),
            })
    }

    /// Assemble the single [`HirExprKind::EnumConstruct`] node every surface form
    /// lowers to. `payload` is already in the variant's declared field order.
    fn build_enum_construct(
        &self,
        enum_name: &str,
        variant: &str,
        tag: u32,
        payload: Vec<HirExpr>,
        span: shared_types::Span,
    ) -> HirExpr {
        HirExpr::new(
            HirExprKind::EnumConstruct {
                enum_name: enum_name.to_string(),
                variant: variant.to_string(),
                tag,
                payload,
            },
            HirType::Enum(enum_name.to_string()),
            span,
        )
    }

    /// Lower a unit-variant construction `E::V` — an empty payload.
    fn lower_enum_construct(
        &mut self,
        enum_name: &str,
        variant: &str,
        payload: Vec<HirExpr>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let (tag, _) = self.enum_variant(enum_name, variant)?;
        Ok(self.build_enum_construct(enum_name, variant, tag, payload, span))
    }

    /// Lower a tuple-variant construction `E::V(args)`: arguments are positional, so
    /// they are the payload as-is, lowered against the declared field types.
    fn lower_enum_tuple_call(
        &mut self,
        enum_name: &str,
        variant: &str,
        args: &[Expr],
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let (tag, fields) = self.enum_variant(enum_name, variant)?;
        let field_tys: Vec<HirType> = fields.into_iter().map(|(_, t)| t).collect();
        let payload = self.lower_args(args, &field_tys)?;
        Ok(self.build_enum_construct(enum_name, variant, tag, payload, span))
    }

    /// Lower a struct-variant construction `E::V { field: expr, ... }`: the provided
    /// fields are reordered into the variant's declared field order before becoming
    /// the payload, so codegen sees a single positional layout.
    fn lower_enum_struct_literal(
        &mut self,
        enum_name: &str,
        variant: &str,
        fields: &[FieldInit],
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let (tag, declared) = self.enum_variant(enum_name, variant)?;
        let mut payload = Vec::with_capacity(declared.len());
        for (field_name, field_ty) in &declared {
            let Some(field_name) = field_name else {
                return Err(LoweringError::Malformed {
                    detail: format!(
                        "tuple variant '{}::{}' constructed with field names",
                        enum_name, variant
                    ),
                });
            };
            let provided = fields
                .iter()
                .find(|f| &f.name.name == field_name)
                .ok_or_else(|| LoweringError::Malformed {
                    detail: format!(
                        "missing field '{}' for enum variant '{}::{}'",
                        field_name, enum_name, variant
                    ),
                })?;
            payload.push(self.lower_expr(&provided.value, Some(field_ty))?);
        }
        Ok(self.build_enum_construct(enum_name, variant, tag, payload, span))
    }

    /// Lower an associated-function call `TypeName::func(args)`.
    fn lower_assoc_call(
        &mut self,
        type_name: &str,
        member: &str,
        args: &[Expr],
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let (params, ret) = self.assoc_signature(type_name, member)?;
        let args = self.lower_args(args, &params)?;
        let callee = HirExpr::new(
            HirExprKind::Path {
                type_name: type_name.to_string(),
                member: member.to_string(),
            },
            HirType::Function {
                params,
                ret: Box::new(ret.clone()),
            },
            span,
        );
        Ok(HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(callee),
                args,
            },
            ret,
            span,
        ))
    }

    /// Lower each argument against its corresponding parameter type (the contextual
    /// hint a callee imposes on its arguments). Extra arguments lower with no hint.
    fn lower_args(
        &mut self,
        args: &[Expr],
        params: &[HirType],
    ) -> Result<Vec<HirExpr>, LoweringError> {
        let mut out = Vec::with_capacity(args.len());
        for (i, arg) in args.iter().enumerate() {
            out.push(self.lower_expr(arg, params.get(i))?);
        }
        Ok(out)
    }

    fn lower_struct_literal(
        &mut self,
        name: &shared_types::Identifier,
        fields: &[FieldInit],
        base: &Option<Box<Expr>>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        // A generic struct literal infers its type arguments from the field values,
        // then monomorphizes into a concrete instance.
        if self.generic_structs.contains_key(&name.name) {
            return self.lower_generic_struct_literal(name, fields, base, span);
        }

        let def =
            self.structs
                .get(&name.name)
                .cloned()
                .ok_or_else(|| LoweringError::UnresolvedType {
                    name: name.name.clone(),
                })?;

        let mut lowered_fields = Vec::with_capacity(fields.len());
        for FieldInit {
            name: fname,
            value,
            span: fspan,
        } in fields
        {
            let expected = def
                .iter()
                .find(|(n, _)| n == &fname.name)
                .map(|(_, t)| t.clone());
            let value = self.lower_expr(value, expected.as_ref())?;
            lowered_fields.push(HirFieldInit {
                name: fname.name.clone(),
                value: Box::new(value),
                span: *fspan,
            });
        }

        let struct_ty = HirType::Struct(name.name.clone());
        let base = match base {
            Some(b) => Some(Box::new(self.lower_expr(b, Some(&struct_ty))?)),
            None => None,
        };

        Ok(HirExpr::new(
            HirExprKind::StructLiteral {
                name: name.name.clone(),
                fields: lowered_fields,
                base,
            },
            struct_ty,
            span,
        ))
    }

    /// Lower a generic struct literal: infer the type arguments by unifying the
    /// template's field annotations against the lowered field values, monomorphize the
    /// instance, and emit an ordinary struct literal referring to its mangled name.
    fn lower_generic_struct_literal(
        &mut self,
        name: &shared_types::Identifier,
        fields: &[FieldInit],
        base: &Option<Box<Expr>>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let template = self
            .generic_structs
            .get(&name.name)
            .cloned()
            .ok_or_else(|| LoweringError::UnresolvedType {
                name: name.name.clone(),
            })?;
        let gnames: std::collections::HashSet<String> = template
            .generics
            .iter()
            .filter(|g| matches!(g.kind, ast_types::GenericParamKind::Type))
            .map(|g| g.name.name.clone())
            .collect();
        let cnames: std::collections::HashSet<String> = template
            .generics
            .iter()
            .filter(|g| matches!(g.kind, ast_types::GenericParamKind::Const(_)))
            .map(|g| g.name.name.clone())
            .collect();

        let mut subst: std::collections::HashMap<String, HirType> =
            std::collections::HashMap::new();
        let mut const_subst: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        let mut lowered_fields = Vec::with_capacity(fields.len());
        for FieldInit {
            name: fname,
            value,
            span: fspan,
        } in fields
        {
            let field_ast_ty = template
                .fields
                .iter()
                .find(|f| f.name.name == fname.name)
                .map(|f| f.ty.clone());
            let lowered = self.lower_expr(value, None)?;
            if let Some(ft) = &field_ast_ty {
                crate::unify_ast_hir(
                    ft,
                    &lowered.ty,
                    &gnames,
                    &cnames,
                    &mut subst,
                    &mut const_subst,
                );
            }
            lowered_fields.push(HirFieldInit {
                name: fname.name.clone(),
                value: Box::new(lowered),
                span: *fspan,
            });
        }

        let mut args = Vec::with_capacity(template.generics.len());
        for gp in &template.generics {
            match &gp.kind {
                ast_types::GenericParamKind::Const(_) => args.push(crate::MonoArg::Const(
                    const_subst.get(&gp.name.name).copied().unwrap_or(0),
                )),
                ast_types::GenericParamKind::Type => args.push(crate::MonoArg::Type(
                    subst.get(&gp.name.name).cloned().unwrap_or(HirType::Void),
                )),
            }
        }
        let mangled = self.instantiate_generic_struct(&name.name, &args)?;
        let struct_ty = HirType::Struct(mangled.clone());
        let base = match base {
            Some(b) => Some(Box::new(self.lower_expr(b, Some(&struct_ty))?)),
            None => None,
        };
        Ok(HirExpr::new(
            HirExprKind::StructLiteral {
                name: mangled,
                fields: lowered_fields,
                base,
            },
            struct_ty,
            span,
        ))
    }

    /// Lower a `start..end` / `start..=end` range. Ranges are not first-class values
    /// Only valid as a `string.slice` argument — so the node carries
    /// `void`; the slice lowering reads the bounds directly. Bounds are `u64`-typed.
    fn lower_range(
        &mut self,
        start: &Expr,
        end: &Expr,
        inclusive: bool,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let start = self.lower_expr(start, Some(&HirType::U64))?;
        let end = self.lower_expr(end, Some(&HirType::U64))?;
        Ok(HirExpr::new(
            HirExprKind::Range {
                start: Box::new(start),
                end: Box::new(end),
                inclusive,
            },
            HirType::Void,
            span,
        ))
    }

    fn lower_array_literal(
        &mut self,
        elements: &[Expr],
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let expected_element = match expected {
            Some(HirType::Array { element, .. }) => Some((**element).clone()),
            _ => None,
        };

        if elements.is_empty() {
            let ty = match expected {
                Some(HirType::Array { element, size }) => HirType::Array {
                    element: element.clone(),
                    size: *size,
                },
                _ => {
                    return Err(LoweringError::Malformed {
                        detail: "cannot infer element type of empty array literal".to_string(),
                    })
                }
            };
            return Ok(HirExpr::new(
                HirExprKind::ArrayLiteral { elements: vec![] },
                ty,
                span,
            ));
        }

        let first = self.lower_expr(&elements[0], expected_element.as_ref())?;
        let element_ty = first.ty.clone();
        let mut lowered = Vec::with_capacity(elements.len());
        lowered.push(first);
        for el in &elements[1..] {
            lowered.push(self.lower_expr(el, Some(&element_ty))?);
        }

        Ok(HirExpr::new(
            HirExprKind::ArrayLiteral { elements: lowered },
            HirType::Array {
                element: Box::new(element_ty),
                size: elements.len(),
            },
            span,
        ))
    }

    fn lower_tuple_literal(
        &mut self,
        elements: &[Expr],
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let expected_elems = match expected {
            Some(HirType::Tuple(es)) if es.len() == elements.len() => Some(es.clone()),
            _ => None,
        };
        let mut lowered = Vec::with_capacity(elements.len());
        let mut tys = Vec::with_capacity(elements.len());
        for (i, el) in elements.iter().enumerate() {
            let hint = expected_elems.as_ref().map(|es| &es[i]);
            let el = self.lower_expr(el, hint)?;
            tys.push(el.ty.clone());
            lowered.push(el);
        }
        Ok(HirExpr::new(
            HirExprKind::TupleLiteral { elements: lowered },
            HirType::Tuple(tys),
            span,
        ))
    }

    /// Lower a block in value position (a bare/`unsafe` block or an `if` arm),
    /// returning the lowered statements and the block's value type — the type of the
    /// trailing expression, or `void`. The tail is typed with no contextual hint,
    /// matching the checker's `check_block_expr_type`.
    fn lower_block_value(
        &mut self,
        stmts: &[ast_types::Stmt],
    ) -> Result<(Vec<HirStmt>, HirType), LoweringError> {
        self.push_scope();
        let result = self.lower_block_value_inner(stmts);
        self.pop_scope();
        result
    }

    fn lower_block_value_inner(
        &mut self,
        stmts: &[ast_types::Stmt],
    ) -> Result<(Vec<HirStmt>, HirType), LoweringError> {
        let mut out = Vec::with_capacity(stmts.len());
        let mut ty = HirType::Void;
        let last = stmts.len().saturating_sub(1);
        for (i, stmt) in stmts.iter().enumerate() {
            if i == last {
                if let ast_types::Stmt::Expr(expr) = stmt {
                    let tail = self.lower_expr(expr, None)?;
                    ty = tail.ty.clone();
                    out.push(HirStmt::Expr(tail));
                    return Ok((out, ty));
                }
            }
            out.push(self.lower_stmt(stmt)?);
        }
        Ok((out, ty))
    }

    /// The declared type of `field` on `struct_name`.
    fn struct_field_type(&self, struct_name: &str, field: &str) -> Result<HirType, LoweringError> {
        self.structs
            .get(struct_name)
            .and_then(|fields| fields.iter().find(|(n, _)| n == field))
            .map(|(_, t)| t.clone())
            .ok_or_else(|| LoweringError::Malformed {
                detail: format!("unknown field '{}' on struct '{}'", field, struct_name),
            })
    }

    /// The `(parameter types, return type)` of an associated function `Type::member`.
    fn assoc_signature(
        &self,
        type_name: &str,
        member: &str,
    ) -> Result<(Vec<HirType>, HirType), LoweringError> {
        let mangled = format!("{}__{}", type_name, member);
        self.functions
            .get(&mangled)
            .cloned()
            .ok_or(LoweringError::UnresolvedCall { target: mangled })
    }
}

/// Wrap a concrete reference in the unsizing coercion `&T` → `&dyn Trait` when
/// the context calls for a trait object and the value is not already one.
///
/// This is the sole implicit conversion in the language, so it is applied at exactly one
/// place: every context that supplies an expected type routes through here. The checker
/// has already verified that `T` implements the trait, so no impl lookup is repeated.
fn apply_dyn_coercion(expr: HirExpr, expected: Option<&HirType>) -> HirExpr {
    let Some(HirType::Reference {
        inner: expected_inner,
        mutable,
    }) = expected
    else {
        return expr;
    };
    if !matches!(expected_inner.as_ref(), HirType::DynObject(_)) {
        return expr;
    }
    // A value that is already a trait object needs no coercion; anything else must be a
    // concrete `&T` for the checker to have accepted it here.
    match &expr.ty {
        HirType::Reference { inner, .. } if matches!(inner.as_ref(), HirType::DynObject(_)) => expr,
        HirType::Reference { .. } => {
            let span = expr.span;
            HirExpr::new(
                HirExprKind::DynCoerce {
                    value: Box::new(expr),
                },
                HirType::Reference {
                    inner: expected_inner.clone(),
                    mutable: *mutable,
                },
                span,
            )
        }
        _ => expr,
    }
}

/// The resolved type of a literal under an optional contextual `expected` type,
/// mirroring the checker's literal inference (suffix wins; else the expected type
/// when it fits the literal's family; else the default `i32` / `f64`).
fn literal_type(lit: &Literal, expected: Option<&HirType>) -> HirType {
    match lit {
        Literal::Integer(_, Some(suffix)) => int_suffix_type(suffix),
        Literal::Integer(_, None) => match expected {
            Some(t) if is_integer(t) => t.clone(),
            _ => HirType::I32,
        },
        Literal::Float(_, Some(suffix)) => float_suffix_type(suffix),
        Literal::Float(_, None) => match expected {
            Some(t) if is_full_float(t) => t.clone(),
            _ => HirType::F64,
        },
        Literal::Boolean(_) => HirType::Bool,
        Literal::Char(_) => HirType::Char,
        Literal::String(_) => HirType::String,
    }
}

/// The scalar value a match-pattern literal denotes, as the low bits of an `i64`
/// Integers as-is, `bool` as 0/1, `char` as its Unicode scalar value. Float
/// and string literals are not matchable (the checker rejects them before lowering).
fn literal_scalar(lit: &Literal) -> Result<i64, LoweringError> {
    match lit {
        Literal::Integer(n, _) => Ok(*n),
        Literal::Boolean(b) => Ok(*b as i64),
        Literal::Char(c) => Ok(*c as i64),
        Literal::Float(_, _) | Literal::String(_) => Err(LoweringError::Malformed {
            detail: "float/string literal reached a match pattern".to_string(),
        }),
    }
}

/// Whether `t` is a numeric type usable with `-` / arithmetic (integer or
/// full-precision float). Half-precision is excluded.
fn is_numeric(t: &HirType) -> bool {
    is_integer(t) || is_full_float(t)
}

/// The result type of a binary operator given its operand types. Comparisons and
/// logical operators yield `bool`; `+` on two strings yields a new owned `string`
/// Other arithmetic and bitwise operators yield the left operand's type.
fn binary_result_type(
    op: BinaryOp,
    left: &HirType,
    right: &HirType,
) -> Result<HirType, LoweringError> {
    Ok(match op {
        BinaryOp::Equal
        | BinaryOp::NotEqual
        | BinaryOp::Less
        | BinaryOp::Greater
        | BinaryOp::LessEqual
        | BinaryOp::GreaterEqual
        | BinaryOp::And
        | BinaryOp::Or => HirType::Bool,
        BinaryOp::Add if peels_to_string(left) && peels_to_string(right) => HirType::String,
        BinaryOp::Add
        | BinaryOp::Subtract
        | BinaryOp::Multiply
        | BinaryOp::Divide
        | BinaryOp::Modulo
        | BinaryOp::BitAnd
        | BinaryOp::BitOr
        | BinaryOp::BitXor
        | BinaryOp::Shl => left.clone(),
        // `??` is rejected by the checker (Option/Result arrive in Phase 2), so it
        // never reaches a well-typed program's HIR.
        BinaryOp::NullCoalesce => {
            return Err(LoweringError::Malformed {
                detail: "`??` operator is not supported until Phase 2".to_string(),
            })
        }
    })
}
