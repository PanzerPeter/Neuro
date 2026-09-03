//! The `for`-loop desugar against the `IntoIterator` / `Iterator` protocol.
//!
//! `for x in e { body }` over a nominal type becomes
//!
//! ```text
//! mut __iter_N = e.into_iter()          // omitted when `e` is already an Iterator
//! while true {
//!     match __iter_N.next() {
//!         Some(x) => { body }
//!         _       => { break }
//!     }
//! }
//! ```
//!
//! Every node it produces already exists, so no backend learns that the protocol is
//! there. The built-in sequence heads — a range, an array, a `Vec`, a borrowed slice —
//! never reach this module: they keep their direct counted-loop lowering, which is what
//! the spec's implementation note permits and what keeps their generated code unchanged.

use ast_types::{LoopAdapter, Stmt};
use neuro_hir::{
    HirBindingSource, HirExpr, HirExprKind, HirMatchArm, HirMatchBinding, HirMatchTest, HirStmt,
    HirType,
};
use shared_types::{Identifier, Literal, Span};

use crate::{Lowerer, LoweringError};

/// The prelude trait a container implements to produce an iterator.
const INTO_ITERATOR_TRAIT: &str = "IntoIterator";
/// The prelude trait an iterator itself implements.
const ITERATOR_TRAIT: &str = "Iterator";
/// `IntoIterator`'s producing method.
const INTO_ITER_METHOD: &str = "into_iter";
/// `Iterator`'s stepping method.
const NEXT_METHOD: &str = "next";
/// The type of an enumerated loop's position binding, matching the counted loops'.
const LOOP_INDEX_TYPE: HirType = HirType::U64;
/// The byte cursor a `Chars` iterator carries, which is what `.char_indices()` binds.
const CHARS_OFFSET_FIELD: &str = "offset";
/// The `for`-head form that drives a `Chars` iterator by that cursor.
const CHAR_INDICES_METHOD: &str = "char_indices";

/// Where a protocol loop's position binding takes its value.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoopPosition {
    /// `.enumerate()` — a counter over the steps the loop yielded.
    Step,
    /// `.char_indices()` — the iterator's own byte cursor, sampled before each step so
    /// the offset names the code point that step is about to yield.
    ByteOffset,
}

impl Lowerer {
    /// Whether `ty` iterates through the protocol rather than as a built-in sequence.
    pub(crate) fn iterates_by_protocol(&self, ty: &HirType) -> bool {
        nominal_name(ty).is_some_and(|name| {
            self.trait_impls
                .contains(&(INTO_ITERATOR_TRAIT.to_string(), name.clone()))
                || self
                    .trait_impls
                    .contains(&(ITERATOR_TRAIT.to_string(), name))
        })
    }

    /// Lower a `for` head that iterates through the protocol.
    ///
    /// `head` is the already-lowered iterable. The loop's own scope holds the
    /// generated iterator binding as well as the element and position bindings, so a
    /// `break` / `continue` inside `body` resolves against the emitted `while`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lower_protocol_for(
        &mut self,
        label: &Option<Identifier>,
        index: &Option<Identifier>,
        iterator: &Identifier,
        head: HirExpr,
        adapters: &[LoopAdapter],
        body: &[Stmt],
        span: Span,
        position: LoopPosition,
    ) -> Result<HirStmt, LoweringError> {
        let (iter_ty, iter_init) = self.iterator_of(head, span)?;
        let item_ty = self.protocol_item_type(&iter_ty)?;
        let next_ret = self.next_return_type(&iter_ty)?;
        let (some_tag, _) = self.success_variant(&next_ret)?;

        // An adapted head takes its element binding and its yielded-position cursor
        // from the plan, so the two are advanced together with the chain rather than
        // per protocol step.
        let plan = match adapters.is_empty() {
            true => None,
            false => Some(self.plan_loop_adapters(adapters, index, span)?),
        };

        self.protocol_counter += 1;
        let iter_binding = format!("__iter_{}", self.protocol_counter);
        let cursor_binding = match plan {
            Some(_) => None,
            None => index
                .as_ref()
                .map(|_| format!("__iter_pos_{}", self.protocol_counter)),
        };

        let mut prelude = vec![HirStmt::VarDecl {
            name: iter_binding.clone(),
            ty: iter_ty.clone(),
            init: Some(iter_init),
            mutable: true,
            span,
        }];
        if let Some(plan) = &plan {
            prelude.extend(plan.prelude.iter().cloned());
        }
        if let Some(cursor) = &cursor_binding {
            prelude.push(HirStmt::VarDecl {
                name: cursor.clone(),
                ty: LOOP_INDEX_TYPE,
                init: Some(int_literal(0, LOOP_INDEX_TYPE, span)),
                mutable: true,
                span,
            });
        }

        // The iterator and its cursor live in the loop's own scope: they are named by
        // the `while` body, and nothing outside the desugar may see them.
        self.push_scope();
        self.define(iter_binding.clone(), iter_ty.clone());
        if let Some(cursor) = &cursor_binding {
            self.define(cursor.clone(), LOOP_INDEX_TYPE);
        }

        let index_name = index.as_ref().map(|i| i.name.clone());
        let element_name = match &plan {
            Some(plan) => plan.element.clone(),
            None => iterator.name.clone(),
        };
        let element_ty = item_ty.clone();
        let arm_body = self.lower_loop_body_with(label, false, |lo: &mut Self| {
            if let Some(plan) = &plan {
                lo.define(element_name.clone(), element_ty.clone());
                return lo.apply_loop_adapters(plan, &element_ty, index, iterator, body, span);
            }
            if let Some(name) = &index_name {
                lo.define(name.clone(), LOOP_INDEX_TYPE);
            }
            lo.define(element_name.clone(), element_ty.clone());
            let mut stmts = Vec::new();
            // The position is read out and advanced *before* the user's statements so a
            // `continue` cannot skip the advance and repeat an index.
            if let (Some(name), Some(cursor)) = (&index_name, &cursor_binding) {
                stmts.push(HirStmt::VarDecl {
                    name: name.clone(),
                    ty: LOOP_INDEX_TYPE,
                    init: Some(variable(cursor, LOOP_INDEX_TYPE, span)),
                    mutable: false,
                    span,
                });
                // A byte cursor belongs to the iterator and is advanced by its own
                // `next`; only the step counter is the loop's to raise.
                if position == LoopPosition::Step {
                    stmts.push(HirStmt::Assignment {
                        target: cursor.clone(),
                        value: HirExpr::new(
                            HirExprKind::Binary {
                                op: ast_types::BinaryOp::Add,
                                left: Box::new(variable(cursor, LOOP_INDEX_TYPE, span)),
                                right: Box::new(int_literal(1, LOOP_INDEX_TYPE, span)),
                            },
                            LOOP_INDEX_TYPE,
                            span,
                        ),
                        span,
                    });
                }
            }
            stmts.extend(lo.lower_stmt_list(body)?);
            Ok(stmts)
        });
        self.pop_scope();
        let arm_body = arm_body?;

        let step = HirExpr::new(
            HirExprKind::Match {
                scrutinee: Box::new(Self::next_call(&iter_binding, &iter_ty, &next_ret, span)),
                arms: vec![
                    HirMatchArm {
                        tests: vec![HirMatchTest::Tag { tag: some_tag }],
                        bindings: vec![HirMatchBinding {
                            name: element_name.clone(),
                            ty: item_ty,
                            source: HirBindingSource::EnumPayload { slot: 0 },
                        }],
                        guard: None,
                        body: HirExpr::new(
                            HirExprKind::Block { stmts: arm_body },
                            HirType::Void,
                            span,
                        ),
                    },
                    HirMatchArm {
                        tests: vec![HirMatchTest::Wildcard],
                        bindings: Vec::new(),
                        guard: None,
                        body: HirExpr::new(
                            HirExprKind::Block {
                                stmts: vec![HirStmt::Break {
                                    label: None,
                                    value: None,
                                    span,
                                }],
                            },
                            HirType::Void,
                            span,
                        ),
                    },
                ],
            },
            HirType::Void,
            span,
        );

        // The sample sits ahead of the step because `next` advances the cursor past the
        // code point it returns: read afterwards, every offset would name the following
        // one. A `continue` cannot skip it — it is the first statement of the body.
        let mut loop_body = Vec::new();
        if let (LoopPosition::ByteOffset, Some(cursor)) = (position, &cursor_binding) {
            loop_body.push(HirStmt::Assignment {
                target: cursor.clone(),
                value: HirExpr::new(
                    HirExprKind::FieldAccess {
                        object: Box::new(variable(&iter_binding, iter_ty.clone(), span)),
                        field: CHARS_OFFSET_FIELD.to_string(),
                    },
                    LOOP_INDEX_TYPE,
                    span,
                ),
                span,
            });
        }
        loop_body.push(HirStmt::Expr(step));

        prelude.push(HirStmt::While {
            label: label.as_ref().map(|l| l.name.clone()),
            condition: HirExpr::new(
                HirExprKind::Literal(Literal::Boolean(true)),
                HirType::Bool,
                span,
            ),
            body: loop_body,
            span,
        });

        Ok(HirStmt::Expr(HirExpr::new(
            HirExprKind::Block { stmts: prelude },
            HirType::Void,
            span,
        )))
    }

    /// The iterator a head produces, and the expression that produces it.
    ///
    /// A type implementing `Iterator` is its own iterator — the blanket
    /// `impl<I: Iterator> IntoIterator for I` stated as a rule, since a blanket impl has
    /// no syntax yet. `IntoIterator` is consulted first, so a container implementing
    /// both still hands out its dedicated iterator.
    fn iterator_of(&self, head: HirExpr, span: Span) -> Result<(HirType, HirExpr), LoweringError> {
        let Some(name) = nominal_name(&head.ty) else {
            return Err(Self::not_iterable(&head.ty));
        };
        if !self
            .trait_impls
            .contains(&(INTO_ITERATOR_TRAIT.to_string(), name.clone()))
        {
            let ty = head.ty.clone();
            return Ok((ty, head));
        }

        let iter_ty = self.method_return_type(&name, INTO_ITER_METHOD)?;
        let call = HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(HirExpr::new(
                    HirExprKind::FieldAccess {
                        object: Box::new(head),
                        field: INTO_ITER_METHOD.to_string(),
                    },
                    iter_ty.clone(),
                    span,
                )),
                args: Vec::new(),
            },
            iter_ty.clone(),
            span,
        );
        Ok((iter_ty, call))
    }

    /// `__iter_N.next()`, typed as the `Option` instance the impl declared.
    ///
    /// The receiver carries the ITERATOR's type: the backend recovers the method symbol
    /// from it, so typing it as the call's result would send `next` looking for a
    /// builtin on `Option`.
    fn next_call(binding: &str, iter_ty: &HirType, next_ret: &HirType, span: Span) -> HirExpr {
        HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(HirExpr::new(
                    HirExprKind::FieldAccess {
                        object: Box::new(variable(binding, iter_ty.clone(), span)),
                        field: NEXT_METHOD.to_string(),
                    },
                    next_ret.clone(),
                    span,
                )),
                args: Vec::new(),
            },
            next_ret.clone(),
            span,
        )
    }

    /// The `Option` instance `iter_ty`'s `next` returns.
    fn next_return_type(&self, iter_ty: &HirType) -> Result<HirType, LoweringError> {
        let Some(name) = nominal_name(iter_ty) else {
            return Err(Self::not_iterable(iter_ty));
        };
        self.method_return_type(&name, NEXT_METHOD)
    }

    /// What one step of `iter_ty` binds, taken from the `Some` payload of its `next`.
    ///
    /// Reading the payload rather than the impl's `type Item` binding is deliberate:
    /// the payload is the type the backend will actually decode out of the enum, so the
    /// binding and the storage cannot drift apart.
    fn protocol_item_type(&self, iter_ty: &HirType) -> Result<HirType, LoweringError> {
        let next_ret = self.next_return_type(iter_ty)?;
        let (_, payload) = self.success_variant(&next_ret)?;
        Ok(payload)
    }

    /// The declared return type of `type_name`'s `method_name`.
    fn method_return_type(
        &self,
        type_name: &str,
        method_name: &str,
    ) -> Result<HirType, LoweringError> {
        self.impl_methods
            .get(type_name)
            .and_then(|methods| methods.get(method_name))
            .and_then(|mangled| self.functions.get(mangled))
            .map(|(_, ret)| ret.clone())
            .ok_or_else(|| LoweringError::UnresolvedCall {
                target: format!("{}::{}", type_name, method_name),
            })
    }

    fn not_iterable(ty: &HirType) -> LoweringError {
        LoweringError::Malformed {
            detail: format!("for-each over non-iterable type '{}'", ty),
        }
    }
}

/// The receiver of a `text.char_indices()` `for` head, or `None` for any other iterable.
///
/// The parser has already rejected a head that is decorated or bound to one variable, so
/// a match here is a loop the byte cursor drives.
pub(crate) fn char_indices_receiver(iterable: &ast_types::Expr) -> Option<&ast_types::Expr> {
    let ast_types::Expr::Call { func, args, .. } = iterable else {
        return None;
    };
    let ast_types::Expr::FieldAccess { object, field, .. } = func.as_ref() else {
        return None;
    };
    if field.name != CHAR_INDICES_METHOD || !args.is_empty() {
        return None;
    }
    Some(object.as_ref())
}

/// The declaration name behind a nominal type, which is the key the trait tables use.
fn nominal_name(ty: &HirType) -> Option<String> {
    match ty {
        HirType::Struct(name) | HirType::Enum(name) => Some(name.clone()),
        HirType::Newtype { name, .. } => Some(name.clone()),
        _ => None,
    }
}

fn variable(name: &str, ty: HirType, span: Span) -> HirExpr {
    HirExpr::new(HirExprKind::Variable(name.to_string()), ty, span)
}

fn int_literal(value: u64, ty: HirType, span: Span) -> HirExpr {
    HirExpr::new(
        HirExprKind::Literal(Literal::Integer(value as i64, None)),
        ty,
        span,
    )
}
