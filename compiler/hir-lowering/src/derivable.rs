//! The derivable traits' methods on implementors that have no `impl` block.
//!
//! A scalar, `string`, and a struct carrying a `@derive` implement `Clone`, `Debug` and
//! `Hashable` without methods of their own, so a call to `clone`, `fmt_debug` or `hash` on
//! one (written directly, or reached through a bound once a generic body is instantiated)
//! is lowered here to what the compiler generates in its place. A hand-written `Debug` is
//! the opposite case: `{x:?}` has to call it, through a helper that renders it to a string.

use ast_types::Expr;
use neuro_hir::{
    HirCollectionKind, HirExpr, HirExprKind, HirFunction, HirInterpPart, HirItem, HirParam,
    HirStmt, HirTarget, HirType,
};
use shared_types::{FormatKind, FormatSpec, Span};

use crate::{Lowerer, LoweringError};

const CLONE_METHOD: &str = "clone";
const FMT_DEBUG_METHOD: &str = "fmt_debug";
const HASH_METHOD: &str = "hash";
const EQ_METHOD: &str = "eq";
const DEBUG_TRAIT: &str = "Debug";
const PUSH_STR_METHOD: &str = "push_str";
const TO_STRING_METHOD: &str = "to_string";
/// The method-shaped helper a `{x:?}` hole calls for a hand-written `Debug`. `$` cannot
/// appear in a source identifier, so no user method can collide with it.
const DEBUG_STRING_METHOD: &str = "fmt_debug$string";

impl Lowerer {
    /// Lower `object.method(args)` when the receiver implements the method's derivable
    /// trait without an `impl`: `.clone()` on a `Copy` value is that value, `.hash()` is
    /// the builtin hash the backend also keys maps with, `.eq(&rhs)` is the built-in `==`,
    /// and `.fmt_debug(out)` appends
    /// the receiver's `{x:?}` rendering to `out`. `None` for every other call.
    pub(crate) fn lower_supplied_trait_method(
        &mut self,
        object: &HirExpr,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<Option<HirExpr>, LoweringError> {
        if self.declares_method(object.ty.referent(), method) {
            return Ok(None);
        }
        let lowered = match method {
            CLONE_METHOD if args.is_empty() && clone_copies(object.ty.referent()) => {
                read_through(object.clone())
            }
            HASH_METHOD if args.is_empty() => {
                method_call(object.clone(), HASH_METHOD, Vec::new(), HirType::U64, span)
            }
            // The built-in `==` the receiver's type already has, with both operands read
            // through their borrows as the operator reads them.
            EQ_METHOD if args.len() == 1 => {
                let rhs =
                    crate::expressions::coercion::auto_deref(self.lower_expr(&args[0], None)?);
                HirExpr::new(
                    HirExprKind::Binary {
                        op: ast_types::BinaryOp::Equal,
                        left: Box::new(crate::expressions::coercion::auto_deref(object.clone())),
                        right: Box::new(rhs),
                    },
                    HirType::Bool,
                    span,
                )
            }
            FMT_DEBUG_METHOD if args.len() == 1 => {
                let out = self.lower_expr(&args[0], None)?;
                let rendered = HirExpr::new(
                    HirExprKind::InterpString {
                        parts: vec![HirInterpPart::Formatted {
                            expr: object.clone(),
                            spec: debug_spec(),
                        }],
                    },
                    HirType::String,
                    span,
                );
                method_call(out, PUSH_STR_METHOD, vec![rendered], HirType::Void, span)
            }
            _ => return Ok(None),
        };
        Ok(Some(lowered))
    }

    /// A `{x:?}` hole whose value has a hand-written `Debug`, rewritten to call that
    /// `fmt_debug` through a helper returning the rendered `string`, which the hole then
    /// prints as plain text: under `:?` a `string` would be quoted. Every other hole is
    /// returned unchanged, for the backend to render.
    pub(crate) fn debug_hole_through_impl(
        &mut self,
        expr: HirExpr,
        spec: FormatSpec,
    ) -> HirInterpPart {
        let receiver = expr.ty.referent().clone();
        let through_impl = spec.kind == FormatKind::Debug
            && nominal_name(&receiver).is_some_and(|name| {
                self.trait_impls
                    .contains(&(DEBUG_TRAIT.to_string(), name.to_string()))
            })
            && self.declares_method(&receiver, FMT_DEBUG_METHOD);
        let Some(name) = nominal_name(&receiver).filter(|_| through_impl) else {
            return HirInterpPart::Formatted { expr, spec };
        };
        let name = name.to_string();
        self.emit_debug_string_helper(&name, &receiver, expr.span);
        let span = expr.span;
        HirInterpPart::Formatted {
            expr: method_call(expr, DEBUG_STRING_METHOD, Vec::new(), HirType::String, span),
            spec: FormatSpec {
                kind: FormatKind::Default,
                ..spec
            },
        }
    }

    /// `Type__fmt_debug$string(&self) -> string`, emitted once per type: a fresh
    /// builder, the type's own `fmt_debug` into it, and the builder's text. Every return
    /// path allocates, so the hole that calls it owns and releases the result.
    fn emit_debug_string_helper(&mut self, name: &str, receiver: &HirType, span: Span) {
        let helper = format!("{}__{}", name, DEBUG_STRING_METHOD);
        if !self.debug_string_helpers.insert(helper.clone()) {
            return;
        }
        let builder_ty = HirType::Collection {
            kind: HirCollectionKind::StringBuilder,
            args: Vec::new(),
        };
        let self_ty = HirType::Reference {
            inner: Box::new(receiver.clone()),
            mutable: false,
        };
        let variable = |name: &str, ty: HirType| {
            HirExpr::new(HirExprKind::Variable(name.to_string()), ty, span)
        };
        let builder = variable(BUILDER_LOCAL, builder_ty.clone());
        let lend = HirExpr::new(
            HirExprKind::Reference {
                operand: Box::new(builder.clone()),
                mutable: true,
            },
            HirType::Reference {
                inner: Box::new(builder_ty.clone()),
                mutable: true,
            },
            span,
        );
        let body = vec![
            HirStmt::VarDecl {
                name: BUILDER_LOCAL.to_string(),
                ty: builder_ty.clone(),
                init: Some(HirExpr::new(HirExprKind::CollectionNew, builder_ty, span)),
                mutable: true,
                span,
            },
            HirStmt::Expr(method_call(
                variable(SELF_PARAM, self_ty.clone()),
                FMT_DEBUG_METHOD,
                vec![lend],
                HirType::Void,
                span,
            )),
            HirStmt::Return {
                value: Some(method_call(
                    builder,
                    TO_STRING_METHOD,
                    Vec::new(),
                    HirType::String,
                    span,
                )),
                span,
            },
        ];
        self.mono_items.push(HirItem::Function(HirFunction {
            name: helper,
            params: vec![HirParam {
                name: SELF_PARAM.to_string(),
                ty: self_ty,
                span,
            }],
            return_type: HirType::String,
            body,
            target: HirTarget::Host,
            span,
        }));
    }

    /// Whether a struct, enum or newtype `ty` has a method `method` of its own.
    fn declares_method(&self, ty: &HirType, method: &str) -> bool {
        nominal_name(ty).is_some_and(|name| {
            self.impl_methods
                .get(name)
                .is_some_and(|methods| methods.contains_key(method))
        })
    }
}

const SELF_PARAM: &str = "self";
const BUILDER_LOCAL: &str = "out";

fn nominal_name(ty: &HirType) -> Option<&str> {
    match ty {
        HirType::Struct(name) | HirType::Enum(name) | HirType::Newtype { name, .. } => Some(name),
        _ => None,
    }
}

/// Whether `.clone()` on a value of `ty` is a plain copy. The checker admitted the call,
/// so a receiver with no clone of its own is `Copy`: `string`, a tensor and a cloning
/// struct have their own lowering, and a collection is never cloned.
fn clone_copies(ty: &HirType) -> bool {
    !matches!(
        ty,
        HirType::String
            | HirType::Struct(_)
            | HirType::Tensor { .. }
            | HirType::Collection { .. }
            | HirType::DynObject(_)
    )
}

/// The value a receiver denotes: a borrowed receiver is read through its borrow.
fn read_through(object: HirExpr) -> HirExpr {
    let HirType::Reference { inner, .. } = &object.ty else {
        return object;
    };
    let (ty, span) = ((**inner).clone(), object.span);
    HirExpr::new(
        HirExprKind::Deref {
            operand: Box::new(object),
        },
        ty,
        span,
    )
}

fn method_call(
    object: HirExpr,
    method: &str,
    args: Vec<HirExpr>,
    ret: HirType,
    span: Span,
) -> HirExpr {
    let callee = HirExpr::new(
        HirExprKind::FieldAccess {
            object: Box::new(object),
            field: method.to_string(),
        },
        ret.clone(),
        span,
    );
    HirExpr::new(
        HirExprKind::Call {
            callee: Box::new(callee),
            args,
        },
        ret,
        span,
    )
}

fn debug_spec() -> FormatSpec {
    FormatSpec {
        kind: FormatKind::Debug,
        ..FormatSpec::default()
    }
}
