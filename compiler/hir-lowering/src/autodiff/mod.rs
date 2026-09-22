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
//! Three steps. [`tape::linearize`] flattens the body into single-operation bindings and
//! marks which depend on a differentiated parameter. The forward replay emits those
//! bindings again, reading every tensor operand through a borrow. The reverse sweep walks
//! the tape backwards from a `1.0` seed at the loss, emitting each operation's adjoint
//! rule ([`rules`]) and summing the adjoints of values used more than once.
//!
//! Every tensor parameter is differentiated: `wrt:` is a later item, and the checker has
//! already required each tensor parameter to be `&mut` and the loss to be rank-0 `f32`.

mod emit;
mod rules;
mod tape;

use ast_types::Attribute;
use neuro_hir::{
    HirExpr, HirExprKind, HirField, HirFieldInit, HirFunction, HirItem, HirStmt, HirStruct, HirType,
};

use crate::LoweringError;
use emit::Emitter;
use rules::Adjoints;
use tape::{Leaf, Op};

/// The attribute asking for a derivative.
const GRAD_ATTRIBUTE: &str = "grad";

/// The generated names. Duplicated in `semantic-analysis`, which rejects a program whose
/// own declarations would collide with them.
const BUNDLE_PREFIX: &str = "GradsOf_";
const REVERSE_PREFIX: &str = "__";
const REVERSE_SUFFIX: &str = "__rev";

pub(crate) fn is_grad(attributes: &[Attribute]) -> bool {
    attributes
        .iter()
        .any(|attr| attr.name.name == GRAD_ATTRIBUTE)
}

/// Whether a parameter of type `ty` is differentiated: a mutably borrowed tensor.
fn is_differentiated(ty: &HirType) -> bool {
    matches!(ty, HirType::Reference { inner, mutable: true } if matches!(**inner, HirType::Tensor { .. }))
}

/// Build `GradsOf_f` and `__f__rev` for the lowered `@grad` function `primal`.
pub(crate) fn derive_reverse(primal: &HirFunction) -> Result<[HirItem; 2], LoweringError> {
    let differentiated: Vec<_> = primal
        .params
        .iter()
        .filter(|param| is_differentiated(&param.ty))
        .collect();
    let names: Vec<&str> = differentiated
        .iter()
        .map(|param| param.name.as_str())
        .collect();
    let tape = tape::linearize(&primal.name, &primal.body, &names)?;

    let mut em = Emitter::new(primal.span);
    for entry in &tape.entries {
        let init = replay(entry, em.span());
        em.declare(entry.name.clone(), init);
    }

    let Leaf::Var {
        name: loss,
        ty: loss_ty,
    } = &tape.loss
    else {
        return Err(LoweringError::Malformed {
            detail: format!("`@grad` function '{}' returns a constant", primal.name),
        });
    };
    let mut adjoints = Adjoints::default();
    let one = em.float(1.0, emit::element_type(loss_ty));
    let seed = em.literal(&[one], loss_ty);
    adjoints.add(&tape.loss, seed);
    for entry in tape.entries.iter().rev().filter(|entry| entry.active) {
        let Some(adjoint) = adjoints.materialize(&mut em, &entry.name, &entry.ty)? else {
            continue;
        };
        rules::propagate(&mut em, entry, &adjoint, &tape.active, &mut adjoints)?;
    }

    let bundle_name = format!("{BUNDLE_PREFIX}{}", primal.name);
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
            elements: vec![
                HirExpr::new(HirExprKind::Variable(loss.clone()), loss_ty.clone(), span),
                bundle,
            ],
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
            name: format!("{REVERSE_PREFIX}{}{REVERSE_SUFFIX}", primal.name),
            params: primal.params.clone(),
            return_type: result_ty,
            body,
            span,
        }),
    ])
}

/// The forward computation of one tape entry, with every operand borrowed.
fn replay(entry: &tape::Entry, span: shared_types::Span) -> HirExpr {
    let read = |leaf: &Leaf| Box::new(emit::operand(leaf, span));
    let kind = match &entry.op {
        Op::Binary { op, left, right } => HirExprKind::Binary {
            op: *op,
            left: read(left),
            right: read(right),
        },
        Op::Negate(operand) => HirExprKind::Unary {
            op: ast_types::UnaryOp::Negate,
            operand: read(operand),
        },
        Op::Reduce { receiver, op, axis } => HirExprKind::TensorReduce {
            receiver: read(receiver),
            op: *op,
            axis: *axis,
        },
        Op::Literal(elements) => HirExprKind::TensorLiteral {
            elements: elements
                .iter()
                .map(|leaf| emit::operand(leaf, span))
                .collect(),
        },
        Op::Read { object, axes, .. } => HirExprKind::TensorIndex {
            object: read(object),
            axes: axes.clone(),
        },
        Op::Constant(expr) => return expr.clone(),
    };
    HirExpr::new(kind, entry.ty.clone(), entry.span)
}
