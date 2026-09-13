// Binding one call site's arguments to one callee's parameters.

use ast_types::Expr;
use shared_types::Span;

use crate::errors::ArgumentError;
use crate::hoisting;
use crate::signatures::Signature;

/// What binding did to the call expression, which is what the walk needs to know to
/// carry on through it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bound {
    /// The call is still a call: its arguments were permuted in place, or left alone.
    InPlace,
    /// The call was replaced by a block binding its arguments to temporaries in source
    /// order. The trailing call inside it is already bound.
    Hoisted,
}

/// Rewrite `call`'s arguments into `sig`'s declaration order and empty its labels.
///
/// A call written entirely positionally against a signature that requires no name is
/// already in declaration order, so it is left exactly as it was: arity and types stay
/// the type checker's to report. Everything else is bound here, which is the only place
/// a label is ever matched against a parameter.
///
/// A permutation that would move two effect-carrying arguments past each other is not
/// applied to the arguments in place: the call is replaced by a block that binds them to
/// temporaries in source order (`hoisting`), which is reported as [`Bound::Hoisted`] so
/// the walk knows not to bind the finished call inside it a second time.
pub(crate) fn bind(
    call: &mut Expr,
    sig: &Signature,
    callee: &str,
    span: Span,
) -> Result<Bound, ArgumentError> {
    let Expr::Call {
        func,
        args,
        arg_labels: labels,
        ..
    } = call
    else {
        return Ok(Bound::InPlace);
    };
    let named_from = labels.iter().position(Option::is_some);
    if named_from.is_none() && !sig.has_required_label() {
        labels.clear();
        return Ok(Bound::InPlace);
    }

    if let Some(first_named) = named_from {
        if let Some(stray) = labels[first_named..].iter().position(Option::is_none) {
            let label = labels[first_named]
                .as_ref()
                .map(|l| l.name.clone())
                .unwrap_or_default();
            let at = first_named + stray;
            return Err(ArgumentError::PositionalAfterNamed {
                callee: callee.to_string(),
                label,
                span: args.get(at).map(Expr::span).unwrap_or(span),
            });
        }
    }

    // A permutation needs one argument per parameter, unless the callee is one of the
    // builtins whose omitted parameters have a specified default: those are filled in
    // below and only a surplus is a count error. Reporting the mismatch here rather than
    // deferring to the type checker keeps the failure on the call that caused it: this
    // pass returns before type checking runs at all.
    let defaulted = sig.has_defaults();
    if args.len() > sig.params.len() || (args.len() != sig.params.len() && !defaulted) {
        return Err(ArgumentError::ArgumentCountMismatch {
            callee: callee.to_string(),
            expected: sig.params.len(),
            found: args.len(),
            span,
        });
    }

    let positional = named_from.unwrap_or(args.len());
    let mut slots: Vec<Option<usize>> = vec![None; sig.params.len()];

    for (index, param) in sig.params.iter().enumerate().take(positional) {
        if param.required {
            return Err(ArgumentError::MissingArgumentLabel {
                callee: callee.to_string(),
                label: param.name.clone().unwrap_or_default(),
                span: args.get(index).map(Expr::span).unwrap_or(span),
            });
        }
        slots[index] = Some(index);
    }

    for (index, label) in labels.iter().enumerate().skip(positional) {
        let Some(label) = label.as_ref() else {
            continue;
        };
        let arg_span = args.get(index).map(Expr::span).unwrap_or(span);
        let Some(target) = sig.position_of(&label.name) else {
            if sig.is_suppressed(&label.name) {
                return Err(ArgumentError::SuppressedLabel {
                    callee: callee.to_string(),
                    label: label.name.clone(),
                    span: arg_span,
                });
            }
            return Err(ArgumentError::UnknownArgumentLabel {
                callee: callee.to_string(),
                label: label.name.clone(),
                span: arg_span,
            });
        };
        if slots[target].is_some() {
            return Err(ArgumentError::DuplicateArgumentLabel {
                callee: callee.to_string(),
                label: label.name.clone(),
                span: arg_span,
            });
        }
        slots[target] = Some(index);
    }

    // Equal counts plus distinct targets fill every slot; an unfilled one is the caller's
    // omission, which the parameter's own default covers when it has one and is otherwise
    // a parameter that went unmentioned while some other took two names. Naming it is
    // more useful than the count.
    let mut order = Vec::with_capacity(slots.len());
    let mut defaults = Vec::new();
    for (index, slot) in slots.iter().enumerate() {
        match slot {
            Some(from) => order.push(*from),
            None => match &sig.params[index].default {
                Some(default) => defaults.push((index, default.clone())),
                None => {
                    return Err(ArgumentError::MissingArgumentLabel {
                        callee: callee.to_string(),
                        label: sig.params[index].name.clone().unwrap_or_default(),
                        span,
                    })
                }
            },
        }
    }

    // Permuting the arguments also permutes the order they are evaluated in, because
    // every later stage evaluates them where it finds them. That is invisible while the
    // arguments only read values; when one of them carries an effect, the call is
    // rewritten instead into a block that runs them in the order they were written.
    //
    // A call with a filled-in default is never hoisted: a default is a constant the
    // specification writes, so it carries no effect to order against anything.
    if defaults.is_empty()
        && hoisting::reorders_effects(&order, args)
        && hoisting::callee_allows_hoisting(func)
    {
        hoisting::hoist(call, &order, sig);
        return Ok(Bound::Hoisted);
    }

    let mut taken: Vec<Option<Expr>> = args.drain(..).map(Some).collect();
    for from in order {
        if let Some(arg) = taken.get_mut(from).and_then(Option::take) {
            args.push(arg);
        }
    }
    // The slots the caller filled were collected in declaration order, so a default
    // belongs at its own declaration index once they are in place.
    for (index, default) in defaults {
        if index <= args.len() {
            args.insert(index, default);
        }
    }
    labels.clear();
    Ok(Bound::InPlace)
}
