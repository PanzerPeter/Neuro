//! Reverse-mode differentiation of a `@grad` function over its lowered HIR.
//!
//! `@grad func f` lowers to `f` itself plus two generated items: a `GradsOf_f` struct
//! holding one owned gradient per differentiated parameter, typed exactly as that
//! parameter's tensor, and a pure sibling
//!
//! ```text
//! __f__rev(<f's parameters>) -> (Tensor<f32, []>, GradsOf_f)
//! ```
//!
//! returning the loss and the bundle. The derivative is produced here, at compile time,
//! as ordinary HIR: nothing is recorded while the program runs, and every backend emits
//! `__f__rev` like any other function. The shapes are still in the types at this level,
//! which is the reason the transform lives here rather than over LLVM IR.
//!
//! Three steps. [`tape::linearize`] flattens the body into single-operation bindings,
//! keeping `if` and `while` as nested tapes, inlining every call to a user function, and
//! marks which values depend on a differentiated parameter. [`sweep::forward`] emits those bindings again, reading every
//! tensor operand through a borrow. [`sweep::reverse`] walks the tape backwards from a
//! `1.0` seed at the loss, emitting each operation's adjoint rule ([`rules`]) and summing
//! the adjoints of values used more than once.
//!
//! At a point where the primal's control flow changes (a condition on the edge of
//! switching, a trip count about to change) the derivative is that of the path the primal
//! executes there, because the reverse pass follows exactly that path. This is the
//! language's rule, not an accident of the implementation.
//!
//! A `@grad` method is derived the same way, keyed `Type__method`: `GradsOf_Type__method`
//! and a method `__method__rev` in the same `impl`, taking the receiver as the primal
//! does. The receiver is a constant, so the tape reads its fields and never differentiates
//! them.
//!
//! A call through a function value is inlined like any other call, so its target must be
//! known here. A function-typed parameter of the `@grad` function itself has no target of
//! its own: the function is derived once per distinct set of targets its `.backward()`
//! call sites pass ([`Specialization`]), and never on its own.
//!
//! Every tensor parameter is differentiated: `wrt:` is a later item, and the checker has
//! already required each tensor parameter to be `&mut` and the loss to be rank-0 `f32`.

mod backward;
mod emit;
mod rules;
mod sweep;
mod tape;

use std::collections::HashMap;

use ast_types::Attribute;
use neuro_hir::{
    HirCapture, HirExpr, HirExprKind, HirField, HirFieldInit, HirFunction, HirItem, HirMethod,
    HirParam, HirStmt, HirStruct, HirType,
};

use crate::LoweringError;
use emit::Emitter;
use rules::Adjoints;
use tape::{Functions, Leaf};

/// The attribute asking for a derivative.
const GRAD_ATTRIBUTE: &str = "grad";

/// The generated names. Duplicated in `semantic-analysis`, which rejects a program whose
/// own declarations would collide with them.
const BUNDLE_PREFIX: &str = "GradsOf_";
const REVERSE_PREFIX: &str = "__";
const REVERSE_SUFFIX: &str = "__rev";

/// The prefix of the parameters a specialized derivative takes a closure's captures by.
/// No user name contains `__`, and the tape's own names continue with a digit.
const CAPTURE_PREFIX: &str = "__ad_capture";

/// How a method's key joins its type and name, the mangling every slice uses for
/// `Type__method`.
pub(crate) const METHOD_SEPARATOR: &str = "__";

pub(crate) fn is_grad(attributes: &[Attribute]) -> bool {
    attributes
        .iter()
        .any(|attr| attr.name.name == GRAD_ATTRIBUTE)
}

/// The generated derivative of the lowered function `function`.
pub(crate) fn reverse_name(function: &str) -> String {
    format!("{REVERSE_PREFIX}{function}{REVERSE_SUFFIX}")
}

/// The generated struct holding `function`'s gradients, one field per differentiated
/// parameter, named like that parameter.
pub(crate) fn bundle_name(function: &str) -> String {
    format!("{BUNDLE_PREFIX}{function}")
}

/// Whether a parameter of type `ty` is differentiated: a mutably borrowed tensor.
fn is_differentiated(ty: &HirType) -> bool {
    matches!(ty, HirType::Reference { inner, mutable: true } if matches!(**inner, HirType::Tensor { .. }))
}

/// A `@grad` function taking function-typed parameters, derived for the targets one
/// `.backward()` call site passes them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Specialization {
    /// The lowered `@grad` function.
    pub(crate) function: String,
    /// What the derivative is named after: `GradsOf_<key>` and `__<key>__rev`.
    pub(crate) key: String,
    /// Each function-typed parameter, by name, with its target.
    pub(crate) targets: Vec<(String, Target)>,
}

/// A function value's target as a call site writes it: a function or a lifted closure by
/// name, and the closure's captures in its layout order. The derivative takes each capture
/// as one more parameter, which the call site passes by reading the captured binding.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Target {
    pub(crate) name: String,
    pub(crate) captures: Vec<HirCapture>,
}

/// Build `GradsOf_f` and `__f__rev` for each `@grad` function named in `grads`, and one
/// pair per entry of `specializations`, out of the fully lowered `items`. It runs once
/// every function is lowered, generic instances and closures included, because a `@grad`
/// body may call any of them. A function taking a function-typed parameter is derived
/// only through its specializations.
pub(crate) fn derive_reverses(
    items: &[HirItem],
    grads: &[String],
    specializations: &[Specialization],
) -> Result<Vec<HirItem>, LoweringError> {
    let functions = Functions::of(items);
    let primal = |name: &str| {
        functions
            .function(name)
            .ok_or_else(|| LoweringError::Malformed {
                detail: format!("`@grad` function '{name}' was never lowered"),
            })
    };
    let mut derived = Vec::with_capacity((grads.len() + specializations.len()) * 2);
    for name in grads {
        let primal = primal(name)?;
        // Only a call site knows what a function-typed parameter calls.
        if primal
            .params
            .iter()
            .any(|param| matches!(param.ty, HirType::Function { .. }))
        {
            continue;
        }
        derived.extend(derive_reverse(
            primal,
            name,
            &functions,
            &HashMap::new(),
            Vec::new(),
        )?);
    }
    for specialization in specializations {
        let primal = primal(&specialization.function)?;
        let mut bound = HashMap::with_capacity(specialization.targets.len());
        let mut extra = Vec::new();
        for (param, target) in &specialization.targets {
            let Some(declared) = primal.params.iter().find(|p| p.name == *param) else {
                return Err(LoweringError::Malformed {
                    detail: format!("'{}' has no parameter '{param}'", primal.name),
                });
            };
            let mut captures = Vec::with_capacity(target.captures.len());
            for capture in &target.captures {
                let name = format!("{CAPTURE_PREFIX}{}", extra.len());
                let leaf = Leaf::Var {
                    name: name.clone(),
                    ty: capture.ty.clone(),
                };
                captures.push((capture.name.clone(), leaf));
                extra.push(HirParam {
                    name,
                    ty: capture.ty.clone(),
                    span: primal.span,
                });
            }
            let leaf = Leaf::Function {
                target: target.name.clone(),
                captures,
                ty: declared.ty.clone(),
            };
            let _ = bound.insert(param.clone(), leaf);
        }
        derived.extend(derive_reverse(
            primal,
            &specialization.key,
            &functions,
            &bound,
            extra,
        )?);
    }
    Ok(derived)
}

/// Give each `@grad` method in `methods`, named as (type, method), a `GradsOf_T__m`
/// struct and a derivative method `__m__rev` in the `impl` that declares it. The
/// derivative is a method so that it takes the receiver exactly as the primal does, and
/// every backend dispatches it like any other method of the type.
pub(crate) fn derive_method_reverses(
    items: &mut Vec<HirItem>,
    methods: &[(String, String)],
) -> Result<(), LoweringError> {
    let mut derived = Vec::with_capacity(methods.len());
    {
        let functions = Functions::of(items);
        for (type_name, method_name) in methods {
            let Some((index, method)) = find_method(items, type_name, method_name) else {
                return Err(LoweringError::Malformed {
                    detail: format!("`@grad` method '{type_name}.{method_name}' was never lowered"),
                });
            };
            // The primal as the transform reads it: named for diagnostics as the program
            // spells it, with `self` left a free variable of the body, which the replay
            // re-reads exactly as the primal did.
            let primal = HirFunction {
                name: format!("{type_name}.{method_name}"),
                params: method.params.clone(),
                return_type: method.return_type.clone(),
                body: method.body.clone(),
                span: method.span,
            };
            let key = format!("{type_name}{METHOD_SEPARATOR}{method_name}");
            let [bundle, reverse] =
                derive_reverse(&primal, &key, &functions, &HashMap::new(), Vec::new())?;
            let HirItem::Function(reverse) = reverse else {
                return Err(LoweringError::Malformed {
                    detail: format!(
                        "the derivative of '{type_name}.{method_name}' is not a function"
                    ),
                });
            };
            let reverse = HirMethod {
                name: reverse_name(method_name),
                self_param: method.self_param.clone(),
                params: reverse.params,
                return_type: reverse.return_type,
                body: reverse.body,
                span: reverse.span,
            };
            derived.push((index, bundle, reverse));
        }
    }
    for (index, bundle, reverse) in derived {
        if let Some(HirItem::Impl(block)) = items.get_mut(index) {
            block.methods.push(reverse);
        }
        items.push(bundle);
    }
    Ok(())
}

/// The inherent `impl` of `type_name` declaring `method_name`, by position in `items`.
fn find_method<'i>(
    items: &'i [HirItem],
    type_name: &str,
    method_name: &str,
) -> Option<(usize, &'i HirMethod)> {
    items
        .iter()
        .enumerate()
        .find_map(|(index, item)| match item {
            HirItem::Impl(block) if block.type_name == type_name && block.trait_name.is_none() => {
                block
                    .methods
                    .iter()
                    .find(|method| method.name == method_name)
                    .map(|method| (index, method))
            }
            _ => None,
        })
}

/// Build `GradsOf_<key>` and `__<key>__rev` for the lowered `@grad` function `primal`,
/// where `key` is the name its generated items are derived from: the function's own name,
/// a specialization's key, or `Type__method` for a method. `bound` gives function-typed
/// parameters their targets, and `extra` are the parameters the derivative takes after
/// `primal`'s, for the captures those targets read.
fn derive_reverse(
    primal: &HirFunction,
    key: &str,
    functions: &Functions<'_>,
    bound: &HashMap<String, Leaf>,
    extra: Vec<HirParam>,
) -> Result<[HirItem; 2], LoweringError> {
    let differentiated: Vec<_> = primal
        .params
        .iter()
        .filter(|param| is_differentiated(&param.ty))
        .collect();
    let names: Vec<&str> = differentiated
        .iter()
        .map(|param| param.name.as_str())
        .collect();
    let tape = tape::linearize(primal, &names, functions, bound)?;

    let mut em = Emitter::new(primal.span);
    sweep::forward(&mut em, &tape.nodes)?;

    let Leaf::Var { name, ty: loss_ty } = &tape.loss else {
        return Err(LoweringError::Malformed {
            detail: format!("`@grad` function '{}' returns a constant", primal.name),
        });
    };
    // Taken before the sweep, which rebuilds a loop's carried values in place.
    let loss = if tape.slots.contains(name) {
        em.snapshot(&tape.loss)?
    } else {
        tape.loss.clone()
    };
    let mut adjoints = Adjoints::default();
    let one = em.float(1.0, emit::element_type(loss_ty));
    let seed = em.literal(&[one], loss_ty);
    adjoints.add(&tape.loss, seed);
    sweep::reverse(&mut em, &tape.nodes, &mut adjoints, &tape.active)?;

    let bundle_name = bundle_name(key);
    let mut fields = Vec::with_capacity(differentiated.len());
    let mut inits = Vec::with_capacity(differentiated.len());
    for param in &differentiated {
        let ty = param.ty.referent().clone();
        let gradient = match adjoints.materialize(&mut em, &param.name, &ty)? {
            Some(gradient) => gradient,
            None => em.zeros(&ty),
        };
        fields.push(HirField {
            name: param.name.clone(),
            ty: ty.clone(),
            span: param.span,
        });
        inits.push(HirFieldInit {
            name: param.name.clone(),
            value: Box::new(emit::operand_owned(&gradient, param.span)),
            span: param.span,
        });
    }

    let span = primal.span;
    let mut params = primal.params.clone();
    params.extend(extra);
    let bundle_ty = HirType::Struct(bundle_name.clone());
    let result_ty = HirType::Tuple(vec![loss_ty.clone(), bundle_ty.clone()]);
    let bundle = HirExpr::new(
        HirExprKind::StructLiteral {
            name: bundle_name.clone(),
            fields: inits,
            base: None,
        },
        bundle_ty,
        span,
    );
    let result = HirExpr::new(
        HirExprKind::TupleLiteral {
            elements: vec![emit::operand_owned(&loss, span), bundle],
        },
        result_ty.clone(),
        span,
    );
    let mut body = em.stmts;
    body.push(HirStmt::Return {
        value: Some(result),
        span,
    });

    Ok([
        HirItem::Struct(HirStruct {
            name: bundle_name.clone(),
            written_name: bundle_name,
            fields,
            span,
        }),
        HirItem::Function(HirFunction {
            name: reverse_name(key),
            params,
            return_type: result_ty,
            body,
            span,
        }),
    ])
}
