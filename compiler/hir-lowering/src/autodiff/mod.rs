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
//! does. The tape reads the receiver's fields as constants, except the tensors a `wrt:`
//! path names (`self.encoder.w`, `self.heads[1]`): each is copied once at the top of the
//! derivative and that copy is differentiated like a parameter, its gradient going in the
//! bundle field `self__encoder__w`.
//!
//! A call through a function value is inlined like any other call, so its target must be
//! known here. A function-typed parameter of the `@grad` function itself has no target of
//! its own: the function is derived once per distinct set of targets its `.backward()`
//! call sites pass ([`Specialization`]), and never on its own.
//!
//! Without `wrt:` every tensor parameter is differentiated; with it, what it lists ([`Wrt`]).
//! The checker has already required each differentiated parameter to be a `&mut` tensor,
//! each path to end in a tensor, and the loss to be rank-0 `f32`.

mod backward;
mod emit;
mod rules;
mod sweep;
mod tape;

use std::collections::HashMap;

use ast_types::{Attribute, Expr};
use neuro_hir::{
    HirCapture, HirExpr, HirExprKind, HirField, HirFieldInit, HirFunction, HirItem, HirMethod,
    HirParam, HirStmt, HirStruct, HirType,
};
use shared_types::{Literal, Span};

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

/// The `@grad` argument selecting what is differentiated.
const WRT_LABEL: &str = "wrt";

/// The receiver, the root of every `wrt:` field path.
const RECEIVER: &str = "self";

/// Joins the steps of a field path into its gradient's bundle field name, `self__w__1`.
/// No declared name contains it, so the name cannot clash with a parameter's.
const PATH_SEPARATOR: &str = "__";

/// Every lowered struct's fields in declaration order, by name: what a field path walks.
pub(crate) type StructFields = HashMap<String, Vec<(String, HirType)>>;

/// A `@grad` function's parameter names in order, and what it differentiates. A
/// `.backward()` reads the gradient bundle's fields by these names.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GradParams {
    pub(crate) names: Vec<String>,
    pub(crate) wrt: Wrt,
}

/// What a `@grad` function differentiates.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Wrt {
    /// No `wrt:`: every mutably borrowed tensor parameter, which the checker required
    /// every tensor parameter to be.
    Tensors,
    /// A `wrt:` list: parameters by name, and tensors reached from the receiver.
    Listed {
        params: Vec<String>,
        fields: Vec<FieldPath>,
    },
}

/// A path from the receiver to a tensor, one step per field or array position.
pub(crate) type FieldPath = Vec<PathStep>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PathStep {
    Field(String),
    Element(usize),
}

impl Wrt {
    /// The selection a `@grad` attribute among `attributes` makes. The checker has refused
    /// every `wrt:` entry that is not a parameter name or a field path rooted at `self`.
    pub(crate) fn of(attributes: &[Attribute]) -> Result<Self, LoweringError> {
        let list = attributes
            .iter()
            .filter(|attr| attr.name.name == GRAD_ATTRIBUTE)
            .flat_map(|attr| &attr.named)
            .find(|arg| arg.label.name == WRT_LABEL);
        let Some(list) = list else {
            return Ok(Wrt::Tensors);
        };
        let Expr::ArrayLiteral { elements, .. } = &list.value else {
            return Err(unchecked_wrt());
        };
        let (mut params, mut fields) = (Vec::new(), Vec::new());
        for entry in elements {
            match entry {
                Expr::Identifier(ident) if ident.name != RECEIVER => {
                    params.push(ident.name.clone())
                }
                _ => fields.push(field_path(entry).ok_or_else(unchecked_wrt)?),
            }
        }
        Ok(Wrt::Listed { params, fields })
    }

    /// Whether the parameter `name`, of type `ty`, is differentiated.
    pub(crate) fn selects(&self, name: &str, ty: &HirType) -> bool {
        match self {
            Wrt::Tensors => is_differentiated(ty),
            Wrt::Listed { params, .. } => params.iter().any(|param| param == name),
        }
    }

    pub(crate) fn fields(&self) -> &[FieldPath] {
        match self {
            Wrt::Tensors => &[],
            Wrt::Listed { fields, .. } => fields,
        }
    }
}

fn unchecked_wrt() -> LoweringError {
    LoweringError::Malformed {
        detail: "a `wrt:` entry the checker should have refused".to_string(),
    }
}

/// The steps of a field path rooted at `self`, `self.a.b[1]`.
fn field_path(entry: &Expr) -> Option<FieldPath> {
    match entry {
        Expr::Identifier(ident) if ident.name == RECEIVER => Some(Vec::new()),
        Expr::FieldAccess { object, field, .. } => {
            let mut path = field_path(object)?;
            path.push(PathStep::Field(field.name.clone()));
            Some(path)
        }
        Expr::Index { object, index, .. } => {
            let Expr::Literal(Literal::Integer(position, _), _) = index.as_ref() else {
                return None;
            };
            let mut path = field_path(object)?;
            path.push(PathStep::Element(usize::try_from(*position).ok()?));
            Some(path)
        }
        _ => None,
    }
}

/// The bundle field a field path's gradient goes in.
pub(crate) fn field_key(path: &[PathStep]) -> String {
    let mut key = RECEIVER.to_string();
    for step in path {
        key.push_str(PATH_SEPARATOR);
        match step {
            PathStep::Field(name) => key.push_str(name),
            PathStep::Element(position) => key.push_str(&position.to_string()),
        }
    }
    key
}

/// The place `path` names below `root`, typed step by step from the struct layouts.
pub(crate) fn field_place(
    root: HirExpr,
    path: &[PathStep],
    structs: &StructFields,
    span: Span,
) -> Result<HirExpr, LoweringError> {
    path.iter().try_fold(root, |place, step| {
        step_place(place, step, structs, span).ok_or_else(|| LoweringError::Malformed {
            detail: format!("the `wrt:` path '{}' reaches nothing", field_key(path)),
        })
    })
}

fn step_place(
    place: HirExpr,
    step: &PathStep,
    structs: &StructFields,
    span: Span,
) -> Option<HirExpr> {
    let ty = match (step, place.ty.referent()) {
        (PathStep::Field(field), HirType::Struct(owner)) => {
            let (_, ty) = structs.get(owner)?.iter().find(|(name, _)| name == field)?;
            ty.clone()
        }
        (PathStep::Element(_), HirType::Array { element, .. }) => (**element).clone(),
        _ => return None,
    };
    let object = Box::new(place);
    let kind = match step {
        PathStep::Field(field) => HirExprKind::FieldAccess {
            object,
            field: field.clone(),
        },
        PathStep::Element(position) => HirExprKind::Index {
            object,
            index: Box::new(HirExpr::new(
                HirExprKind::Literal(Literal::Integer(i128::try_from(*position).ok()?, None)),
                HirType::I64,
                span,
            )),
        },
    };
    Some(HirExpr::new(kind, ty, span))
}

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

/// A tensor a method's `wrt:` reaches through its receiver: the bundle field its gradient
/// goes in, its path, and the place the derivative reads it at.
pub(super) struct WrtField {
    pub(super) name: String,
    pub(super) path: FieldPath,
    pub(super) place: HirExpr,
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
    grad_params: &HashMap<String, GradParams>,
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
            wrt_of(grad_params, name)?,
            &[],
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
            wrt_of(grad_params, &specialization.function)?,
            &[],
        )?);
    }
    Ok(derived)
}

fn wrt_of<'g>(
    grad_params: &'g HashMap<String, GradParams>,
    key: &str,
) -> Result<&'g Wrt, LoweringError> {
    grad_params
        .get(key)
        .map(|params| &params.wrt)
        .ok_or_else(|| LoweringError::Malformed {
            detail: format!("`@grad` function '{key}' was never registered"),
        })
}

/// Give each `@grad` method in `methods`, named as (type, method), a `GradsOf_T__m`
/// struct and a derivative method `__m__rev` in the `impl` that declares it. The
/// derivative is a method so that it takes the receiver exactly as the primal does, and
/// every backend dispatches it like any other method of the type.
pub(crate) fn derive_method_reverses(
    items: &mut Vec<HirItem>,
    methods: &[(String, String)],
    grad_params: &HashMap<String, GradParams>,
    structs: &StructFields,
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
            let wrt = wrt_of(grad_params, &key)?;
            let receiver = HirExpr::new(
                HirExprKind::Variable(RECEIVER.to_string()),
                HirType::Struct(type_name.clone()),
                method.span,
            );
            let mut fields = Vec::with_capacity(wrt.fields().len());
            for path in wrt.fields() {
                fields.push(WrtField {
                    name: field_key(path),
                    path: path.clone(),
                    place: field_place(receiver.clone(), path, structs, method.span)?,
                });
            }
            let [bundle, reverse] = derive_reverse(
                &primal,
                &key,
                &functions,
                &HashMap::new(),
                Vec::new(),
                wrt,
                &fields,
            )?;
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
/// `primal`'s, for the captures those targets read. `wrt` picks the parameters
/// differentiated, and `fields` are the receiver's tensors it picks.
fn derive_reverse(
    primal: &HirFunction,
    key: &str,
    functions: &Functions<'_>,
    bound: &HashMap<String, Leaf>,
    extra: Vec<HirParam>,
    wrt: &Wrt,
    fields: &[WrtField],
) -> Result<[HirItem; 2], LoweringError> {
    let differentiated: Vec<_> = primal
        .params
        .iter()
        .filter(|param| wrt.selects(&param.name, &param.ty))
        .collect();
    let names: Vec<&str> = differentiated
        .iter()
        .map(|param| param.name.as_str())
        .collect();
    let tape = tape::linearize(primal, &names, functions, bound, fields)?;

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

    // Each gradient as (bundle field, the tape value it is the adjoint of, its tensor type,
    // where it was named).
    let mut targets: Vec<(&str, &str, HirType, Span)> = differentiated
        .iter()
        .map(|param| {
            let name = param.name.as_str();
            (name, name, param.ty.referent().clone(), param.span)
        })
        .collect();
    for (field, copy) in fields.iter().zip(&tape.fields) {
        let Some(copy) = copy.var_name() else {
            return Err(LoweringError::Malformed {
                detail: format!("the `wrt:` field '{}' was not copied", field.name),
            });
        };
        targets.push((&field.name, copy, field.place.ty.clone(), field.place.span));
    }

    let bundle_name = bundle_name(key);
    let mut bundle_fields = Vec::with_capacity(targets.len());
    let mut inits = Vec::with_capacity(targets.len());
    for (field, target, ty, span) in targets {
        let gradient = match adjoints.materialize(&mut em, target, &ty)? {
            Some(gradient) => gradient,
            None => em.zeros(&ty),
        };
        bundle_fields.push(HirField {
            name: field.to_string(),
            ty,
            span,
        });
        inits.push(HirFieldInit {
            name: field.to_string(),
            value: Box::new(emit::operand_owned(&gradient, span)),
            span,
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
            fields: bundle_fields,
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
