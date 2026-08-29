//! Bind every call site's arguments to the callee's parameters in declaration order.
//!
//! Neuro lets a caller name an argument — `connect("localhost", port: 8080)` — and lets a
//! declaration *require* the name with an external label (`func clamp(_ v: f32, min lo: f32)`).
//! Both are pure surface syntax: this pass matches each label against the callee's
//! parameter list, permutes the arguments into declaration order, and clears the labels,
//! so type checking, HIR lowering, and both backends see the positional call they always
//! saw and a named argument costs nothing at runtime.
//!
//! Permuting the arguments also permutes the order they are evaluated in, which a program
//! whose arguments carry effects can tell apart from the order it wrote. Such a call is
//! rewritten into a block that binds its arguments to temporaries in source order and
//! passes them in declaration order instead — see `hoisting`.

mod binding;
mod errors;
mod hoisting;
mod signatures;
mod walk;

#[cfg(test)]
mod tests;

use ast_types::{Expr, Item};
use shared_types::Identifier;

pub use errors::ArgumentError;

use binding::Bound;
use signatures::{Lookup, SignatureTable};

/// Rewrite `items` so every call's arguments sit in the callee's declaration order with
/// no labels left, reporting every call that cannot be bound.
///
/// `items` is the whole program — module resolution has already merged every file and the
/// driver has prepended the prelude — because a call names its callee and the callee's
/// declaration may be anywhere in that list.
pub fn bind_arguments(items: &mut [Item]) -> Result<(), Vec<ArgumentError>> {
    let table = SignatureTable::build(items);
    let mut errors = Vec::new();
    let mut visit = |expr: &mut Expr, errors: &mut Vec<ArgumentError>| match bind_call(expr, &table)
    {
        Ok(bound) => bound,
        Err(error) => {
            errors.push(error);
            Bound::InPlace
        }
    };
    walk::walk_items(items, &mut visit, &mut errors);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Bind one `Expr::Call`, resolving its callee against the program's declarations.
fn bind_call(expr: &mut Expr, table: &SignatureTable) -> Result<Bound, ArgumentError> {
    let Expr::Call {
        func,
        arg_labels,
        span,
        ..
    } = expr
    else {
        return Ok(Bound::InPlace);
    };
    let (callee, lookup) = match func.as_ref() {
        Expr::Identifier(ident) => (ident.name.clone(), table.function(&ident.name)),
        Expr::Path {
            type_name, member, ..
        } => (
            format!("{}::{}", type_name.name, member.name),
            table.assoc_function(&type_name.name, &member.name),
        ),
        Expr::FieldAccess { field, .. } => (field.name.clone(), table.method(&field.name)),
        // Calling the result of an arbitrary expression — a returned closure, an element
        // of an array of functions. Nothing names its parameters.
        _ => ("this call".to_string(), Lookup::Unknown),
    };

    let span = *span;
    match lookup {
        Lookup::Known(sig) => binding::bind(expr, sig, &callee, span),
        Lookup::Unknown => match first_label(arg_labels) {
            Some(_) => Err(ArgumentError::LabelsUnsupported { callee, span }),
            None => Ok(Bound::InPlace),
        },
        Lookup::Ambiguous => match first_label(arg_labels) {
            Some(label) => Err(ArgumentError::AmbiguousMethodLabels {
                callee,
                label,
                span,
            }),
            None => Ok(Bound::InPlace),
        },
    }
}

fn first_label(labels: &[Option<Identifier>]) -> Option<String> {
    labels.iter().flatten().next().map(|l| l.name.clone())
}
