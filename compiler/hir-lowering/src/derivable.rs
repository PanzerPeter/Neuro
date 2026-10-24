//! The derivable traits' methods on implementors that have no `impl` block.
//!
//! A scalar, `string`, and a struct carrying a `@derive` implement `Clone`, `Debug` and
//! `Hashable` without methods of their own, so a call to `clone`, `fmt_debug` or `hash` on
//! one (written directly, or reached through a bound once a generic body is instantiated)
//! is lowered here to what the compiler generates in its place. A hand-written `Debug` is
//! the opposite case: `{x:?}` has to call it, through a helper that renders it to a string.

use ast_types::Expr;
use neuro_hir::{
    HirCollectionKind, HirExpr, HirExprKind, HirFieldInit, HirFunction, HirImpl, HirInterpPart,
    HirItem, HirMethod, HirParam, HirSelfParam, HirStmt, HirTarget, HirType,
};
use shared_types::{FormatKind, FormatSpec, Span};

use crate::{Lowerer, LoweringError};

const CLONE_METHOD: &str = "clone";
const FMT_DEBUG_METHOD: &str = "fmt_debug";
const HASH_METHOD: &str = "hash";
const EQ_METHOD: &str = "eq";
const DEBUG_TRAIT: &str = "Debug";
const CLONE_TRAIT: &str = "Clone";
const PUSH_STR_METHOD: &str = "push_str";
const TO_STRING_METHOD: &str = "to_string";
/// The method-shaped helper a `{x:?}` hole calls for a hand-written `Debug`. `$` cannot
/// appear in a source identifier, so no user method can collide with it.
const DEBUG_STRING_METHOD: &str = "fmt_debug$string";
/// The method-shaped helper a derived `.clone()` calls when a field's `clone` is written by
/// hand: the field-by-field impl the derive stands for. `$` keeps it apart from user names.
const DERIVED_CLONE_METHOD: &str = "clone$derived";
/// The suffix on a supplied method a `&dyn Trait` vtable slot calls; the backend's
/// vtable reads `<method>$dyn` as `<method>`.
const DYN_SUFFIX: &str = "$dyn";

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
        // A trait object's call goes through its vtable, whoever implemented the trait.
        if self.declares_method(object.ty.referent(), method)
            || matches!(object.ty.referent(), HirType::DynObject(_))
        {
            return Ok(None);
        }
        let lowered = match method {
            CLONE_METHOD if args.is_empty() && clone_copies(object.ty.referent()) => {
                read_through(object.clone())
            }
            CLONE_METHOD if args.is_empty() && self.clone_calls_fields(object.ty.referent()) => {
                self.derived_clone_call(object.clone(), span)
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
                self.emit_field_debug_helpers(object.ty.referent(), span);
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
        if spec.kind != FormatKind::Debug {
            return HirInterpPart::Formatted { expr, spec };
        }
        let Some(name) = self.debug_impl_name(&receiver) else {
            self.emit_field_debug_helpers(&receiver, expr.span);
            return HirInterpPart::Formatted { expr, spec };
        };
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

    /// Whether the derived `.clone()` of `ty` calls a hand-written `clone`: `ty` is a
    /// struct that derives `Clone`, is not `Copy` (a `Copy` value's clone is a copy, which
    /// the backend makes of the whole aggregate), and has a field with a `Clone` impl or a
    /// field that is such a struct itself. The checker's `derived_clone_calls` is the same
    /// rule.
    fn clone_calls_fields(&self, ty: &HirType) -> bool {
        let HirType::Struct(name) = ty else {
            return false;
        };
        if !self.clone_structs.contains(name) || self.is_copy(ty) {
            return false;
        }
        self.structs.get(name).is_some_and(|fields| {
            fields
                .iter()
                .any(|(_, field)| self.clone_impl(field) || self.clone_calls_fields(field))
        })
    }

    /// Whether `ty` has an `impl Clone`, whose `clone` a derive calls. An inherent method
    /// named `clone` is not one.
    fn clone_impl(&self, ty: &HirType) -> bool {
        nominal_name(ty).is_some_and(|name| {
            self.trait_impls
                .contains(&(CLONE_TRAIT.to_string(), name.to_string()))
        }) && self.declares_method(ty, CLONE_METHOD)
    }

    /// `object.clone()` on a struct [`Self::clone_calls_fields`] answers for: a call of
    /// `Type__clone$derived`, the impl a programmer would have written, emitted once per
    /// type. It is an ordinary `&self` method, so the receiver is lent to it exactly as to
    /// a hand-written `clone`, whatever expression it is.
    fn derived_clone_call(&mut self, object: HirExpr, span: Span) -> HirExpr {
        let receiver = object.ty.referent().clone();
        if let HirType::Struct(name) = &receiver {
            self.emit_derived_clone(name, span);
        }
        method_call(object, DERIVED_CLONE_METHOD, Vec::new(), receiver, span)
    }

    /// `impl Type { func clone$derived(&self) -> Type { Type { h: self.h.clone(), n: self.n } } }`.
    /// A field with an `impl Clone` takes its `.clone()`; a non-`Copy` struct deriving
    /// `Clone` calls its own helper; every other field, `Copy` by the derive's field rule,
    /// is read.
    fn emit_derived_clone(&mut self, name: &str, span: Span) {
        let helper = format!("{}__{}", name, DERIVED_CLONE_METHOD);
        // A struct lowering never registered emits no helper, and the call to it then
        // fails in the backend as an undefined function.
        let Some(fields) = self.structs.get(name).cloned() else {
            return;
        };
        if !self.derive_helpers.insert(helper) {
            return;
        }
        let ty = HirType::Struct(name.to_string());
        let mut inits = Vec::new();
        for (field, field_ty) in fields {
            let read = HirExpr::new(
                HirExprKind::FieldAccess {
                    object: Box::new(HirExpr::new(
                        HirExprKind::Variable(SELF_PARAM.to_string()),
                        ty.clone(),
                        span,
                    )),
                    field: field.clone(),
                },
                field_ty.clone(),
                span,
            );
            let derived = matches!(&field_ty, HirType::Struct(n) if self.clone_structs.contains(n))
                && !self.is_copy(&field_ty);
            let value = if self.clone_impl(&field_ty) {
                method_call(read, CLONE_METHOD, Vec::new(), field_ty, span)
            } else if derived {
                self.derived_clone_call(read, span)
            } else {
                read
            };
            inits.push(HirFieldInit {
                name: field,
                value: Box::new(value),
                span,
            });
        }
        let literal = HirExpr::new(
            HirExprKind::StructLiteral {
                name: name.to_string(),
                fields: inits,
                base: None,
            },
            ty.clone(),
            span,
        );
        // An ordinary `&self` method, so a call lends it the receiver exactly as it lends
        // one to a hand-written `clone`.
        self.mono_items.push(HirItem::Impl(HirImpl {
            type_name: name.to_string(),
            self_type: ty.clone(),
            trait_name: None,
            methods: vec![HirMethod {
                name: DERIVED_CLONE_METHOD.to_string(),
                self_param: Some(HirSelfParam::Ref),
                params: Vec::new(),
                return_type: ty,
                body: vec![HirStmt::Return {
                    value: Some(literal),
                    span,
                }],
                span,
            }],
            span,
        }));
    }

    /// The name of `ty` when its `Debug` is written by hand, which a rendering of it calls.
    fn debug_impl_name(&self, ty: &HirType) -> Option<String> {
        let name = nominal_name(ty)?;
        (self
            .trait_impls
            .contains(&(DEBUG_TRAIT.to_string(), name.to_string()))
            && self.declares_method(ty, FMT_DEBUG_METHOD))
        .then(|| name.to_string())
    }

    /// The helpers the backend's rendering of a `@derive(Debug)` struct `ty` calls: one
    /// for each field, at any depth through derived structs, whose `Debug` is written by
    /// hand. The rendering itself is generated field by field in the backend, which
    /// reaches a hand-written `fmt_debug` only through its helper.
    fn emit_field_debug_helpers(&mut self, ty: &HirType, span: Span) {
        let HirType::Struct(name) = ty else {
            return;
        };
        if !self.debug_structs.contains(name) {
            return;
        }
        for (_, mut field) in self.structs.get(name).cloned().unwrap_or_default() {
            // A newtype with a `Debug` of its own renders through it; any other renders
            // as its inner value.
            while let HirType::Newtype { inner, .. } = &field
                && self.debug_impl_name(&field).is_none()
            {
                field = (**inner).clone();
            }
            match self.debug_impl_name(&field) {
                Some(field_name) => self.emit_debug_string_helper(&field_name, &field, span),
                None => self.emit_field_debug_helpers(&field, span),
            }
        }
    }

    /// `Type__fmt_debug$string(&self) -> string`, emitted once per type: a fresh
    /// builder, the type's own `fmt_debug` into it, and the builder's text. Every return
    /// path allocates, so the hole that calls it owns and releases the result.
    fn emit_debug_string_helper(&mut self, name: &str, receiver: &HirType, span: Span) {
        let helper = format!("{}__{}", name, DEBUG_STRING_METHOD);
        if !self.derive_helpers.insert(helper.clone()) {
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
                init: Some(HirExpr::new(
                    HirExprKind::CollectionNew { capacity: None },
                    builder_ty,
                    span,
                )),
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

    /// The vtable a `&T` → `&dyn Trait` coercion needs when `T` implements `Trait` with
    /// no `impl` of its own (a derive, or a builtin): `impl Trait for T` with the methods
    /// the compiler supplies, emitted once per pair. Each is named `<method>$dyn`, which
    /// the backend's vtable accepts for `<method>`: a real `T__hash` would make the
    /// backend take `T` for a hand-written implementor, and `self.hash()` in the body
    /// would call itself.
    pub(crate) fn emit_supplied_vtable_impl(
        &mut self,
        trait_name: &str,
        concrete: &HirType,
        span: Span,
    ) {
        let type_name = match nominal_name(concrete) {
            Some(name) => name.to_string(),
            None => concrete.to_string(),
        };
        if self
            .trait_impls
            .contains(&(trait_name.to_string(), type_name.clone()))
            || self.declares_trait_methods(concrete, trait_name)
            || !self
                .derive_helpers
                .insert(format!("{type_name}__{trait_name}{DYN_SUFFIX}"))
        {
            return;
        }
        let method_names: Vec<String> = self
            .traits
            .get(trait_name)
            .map(|methods| methods.iter().map(|m| m.name.clone()).collect())
            .unwrap_or_default();
        let receiver = HirExpr::new(
            HirExprKind::Variable(SELF_PARAM.to_string()),
            concrete.clone(),
            span,
        );
        let mut methods = Vec::new();
        for method in method_names {
            let (params, return_type, body) = match method.as_str() {
                // `out.push_str("{self:?}")`, rendered as a `{x:?}` hole renders it.
                FMT_DEBUG_METHOD => {
                    let out_ty = HirType::Reference {
                        inner: Box::new(HirType::Collection {
                            kind: HirCollectionKind::StringBuilder,
                            args: Vec::new(),
                        }),
                        mutable: true,
                    };
                    let hole = self.debug_hole_through_impl(receiver.clone(), debug_spec());
                    let rendered = HirExpr::new(
                        HirExprKind::InterpString { parts: vec![hole] },
                        HirType::String,
                        span,
                    );
                    let out = HirExpr::new(
                        HirExprKind::Variable(BUILDER_LOCAL.to_string()),
                        out_ty.clone(),
                        span,
                    );
                    let params = vec![HirParam {
                        name: BUILDER_LOCAL.to_string(),
                        ty: out_ty,
                        span,
                    }];
                    let push =
                        method_call(out, PUSH_STR_METHOD, vec![rendered], HirType::Void, span);
                    (params, HirType::Void, vec![HirStmt::Expr(push)])
                }
                // The builtin hash the backend also keys maps with.
                HASH_METHOD => {
                    let hash = method_call(
                        receiver.clone(),
                        HASH_METHOD,
                        Vec::new(),
                        HirType::U64,
                        span,
                    );
                    let body = vec![HirStmt::Return {
                        value: Some(hash),
                        span,
                    }];
                    (Vec::new(), HirType::U64, body)
                }
                // The checker admits no other supplied trait with methods.
                _ => return,
            };
            methods.push(HirMethod {
                name: format!("{method}{DYN_SUFFIX}"),
                self_param: Some(HirSelfParam::Ref),
                params,
                return_type,
                body,
                span,
            });
        }
        self.mono_items.push(HirItem::Impl(HirImpl {
            type_name,
            self_type: concrete.clone(),
            trait_name: Some(trait_name.to_string()),
            methods,
            span,
        }));
    }

    /// Whether `ty` declares every method of `trait_name` itself, as a generic impl's
    /// instance does, so the coercion reaches a vtable without a supplied one.
    fn declares_trait_methods(&self, ty: &HirType, trait_name: &str) -> bool {
        self.traits.get(trait_name).is_some_and(|methods| {
            !methods.is_empty() && methods.iter().all(|m| self.declares_method(ty, &m.name))
        })
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
