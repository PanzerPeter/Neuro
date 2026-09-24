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
//! keeping `if` and `while` as nested tapes, and marks which values depend on a
//! differentiated parameter. [`sweep::forward`] emits those bindings again, reading every
//! tensor operand through a borrow. [`sweep::reverse`] walks the tape backwards from a
//! `1.0` seed at the loss, emitting each operation's adjoint rule ([`rules`]) and summing
//! the adjoints of values used more than once.
//!
//! At a point where the primal's control flow changes (a condition on the edge of
//! switching, a trip count about to change) the derivative is that of the path the primal
//! executes there, because the reverse pass follows exactly that path. This is the
//! language's rule, not an accident of the implementation.
//!
//! Every tensor parameter is differentiated: `wrt:` is a later item, and the checker has
//! already required each tensor parameter to be `&mut` and the loss to be rank-0 `f32`.

mod emit;
mod rules;
mod sweep;
mod tape;

use ast_types::Attribute;
use neuro_hir::{
    HirExpr, HirExprKind, HirField, HirFieldInit, HirFunction, HirItem, HirStmt, HirStruct, HirType,
};

use crate::LoweringError;
use emit::Emitter;
use rules::Adjoints;
use tape::Leaf;

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
    let tape = tape::linearize(&primal.name, &primal.body, &names, &primal.return_type)?;

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
            name: format!("{REVERSE_PREFIX}{}{REVERSE_SUFFIX}", primal.name),
            params: primal.params.clone(),
            return_type: result_ty,
            body,
            span,
        }),
    ])
}
