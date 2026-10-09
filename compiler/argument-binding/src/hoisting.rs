// Evaluating a reordered named call's arguments in the order they were written.
//
// Binding permutes the arguments into the callee's declaration order, which is also the
// order every later stage evaluates them in. For a call whose arguments only read values
// that is invisible; for one whose arguments have effects it would run them in the wrong
// order, so such a call is rewritten into a block that binds each argument to a
// temporary in *source* order and then passes the temporaries in declaration order.

use ast_types::{Expr, Stmt, Type};
use shared_types::{Identifier, Span};

use crate::signatures::Signature;

/// The prefix of a hoisted argument temporary. `__` is reserved for compiler-generated
/// symbols, so a program cannot collide with one by accident.
const TEMP_PREFIX: &str = "__narg";

/// The prefix of a hoisted receiver index temporary: see [`hoist_receiver_indices`].
const RECEIVER_PREFIX: &str = "__nrecv";

/// Whether binding `args` in `order` would evaluate two effect-carrying arguments in an
/// order the program can tell apart from the one it wrote.
///
/// `order[declaration slot] = source index`, so the permutation's effect on evaluation is
/// exactly the source indices it visits: reading them in declaration order and finding a
/// descent means two arguments swapped places.
pub(crate) fn reorders_effects(order: &[usize], args: &[Expr]) -> bool {
    let observable: Vec<usize> = order
        .iter()
        .copied()
        .filter(|&i| args.get(i).map(|arg| !is_inert(arg)).unwrap_or(false))
        .collect();
    observable.windows(2).any(|pair| pair[0] > pair[1])
}

/// Whether an argument can neither cause an effect nor observe one, which makes moving it
/// past another argument unobservable.
///
/// A literal and a path (an enum's unit variant, an associated constant) are self-contained
/// values. Everything else (a call, an operator that can panic, and a plain variable read,
/// which another argument's `&mut` borrow can change under it) keeps its place.
fn is_inert(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(_, _) | Expr::Path { .. } => true,
        Expr::Paren(inner, _) => is_inert(inner),
        Expr::Cast { expr, .. } => is_inert(expr),
        _ => false,
    }
}

/// Whether a call's callee can be left where it is while its arguments are hoisted ahead
/// of it.
///
/// A free function and an associated function are named, not evaluated. A method's
/// receiver *is* evaluated, and it runs before the arguments do: hoisting them ahead of
/// an expression receiver would invert that pair to fix the argument pair, so a call with
/// one keeps the binding it has today. A place receiver (`obj.m(...)`, `a.b.m(...)`)
/// resolves to an address rather than a value, so nothing observable happens to it,
/// once `hoist` has bound any index it computes (`arr[next()].m(...)`) ahead of the
/// arguments.
pub(crate) fn callee_allows_hoisting(func: &Expr) -> bool {
    match func {
        Expr::Identifier(_) | Expr::Path { .. } => true,
        Expr::FieldAccess { object, .. } => is_place(object),
        _ => false,
    }
}

/// Whether an expression denotes a place (a variable, or a projection out of one) rather
/// than a computed value.
fn is_place(expr: &Expr) -> bool {
    match expr {
        Expr::Identifier(_) => true,
        Expr::Paren(inner, _) => is_place(inner),
        Expr::FieldAccess { object, .. }
        | Expr::TupleIndex { object, .. }
        | Expr::Index { object, .. } => is_place(object),
        Expr::Deref { operand, .. } => is_place(operand),
        _ => false,
    }
}

/// Rewrite `call` into `{ val __narg0 = <first written>; …; callee(<in declaration order>) }`.
///
/// `order[declaration slot] = source index`. The temporaries are declared in source order,
/// so the arguments run in the order they were written, and the call that follows them is
/// already bound: its arguments are the temporaries, in the callee's declaration order,
/// with no labels left. Inert arguments are left in the call rather than bound to a
/// temporary: nothing can observe when they are evaluated, and a literal that reaches its
/// parameter directly still takes its type from it.
pub(crate) fn hoist(call: &mut Expr, order: &[usize], sig: &Signature) {
    let Expr::Call {
        func,
        type_args,
        args,
        arg_labels,
        span,
    } = call
    else {
        return;
    };
    let call_span = *span;

    // The declaration slot each written argument binds to, so a temporary can carry the
    // parameter's own type annotation.
    let mut slot_of_source = vec![0usize; args.len()];
    for (slot, &source) in order.iter().enumerate() {
        slot_of_source[source] = slot;
    }

    let mut func = func.clone();
    let type_args = type_args.clone();
    arg_labels.clear();

    let mut stmts: Vec<Stmt> = Vec::with_capacity(args.len() + 1);
    if let Expr::FieldAccess { object, .. } = func.as_mut() {
        hoist_receiver_indices(object, &mut stmts);
    }
    let mut bound: Vec<Option<Expr>> = Vec::with_capacity(args.len());
    for (source, arg) in args.drain(..).enumerate() {
        if is_inert(&arg) {
            bound.push(Some(arg));
            continue;
        }
        let arg_span = arg.span();
        let name = temp_name(source, arg_span);
        stmts.push(Stmt::VarDecl {
            ty: annotation(sig, slot_of_source[source], &arg),
            init: Some(arg),
            mutable: false,
            span: arg_span,
            name: name.clone(),
        });
        bound.push(Some(Expr::Identifier(name)));
    }

    let mut ordered = Vec::with_capacity(order.len());
    for &source in order {
        if let Some(arg) = bound.get_mut(source).and_then(Option::take) {
            ordered.push(arg);
        }
    }

    stmts.push(Stmt::Expr(Expr::Call {
        func,
        type_args,
        args: ordered,
        arg_labels: Vec::new(),
        span: call_span,
    }));

    *call = Expr::Block {
        stmts,
        span: call_span,
    };
}

/// Bind each index a place receiver computes to a temporary, in the order the receiver
/// evaluates them, and index through the temporary instead.
///
/// The receiver runs before the arguments, so an index with an effect
/// (`arr[next()].m(b: f(), a: g())`) must too. Left in the trailing call it would run
/// after every hoisted argument, and the walk, which does not revisit that call, would
/// never bind a labelled call inside it. An inert index and a variable read stay put:
/// the index is an integer, and reading one is not an effect.
fn hoist_receiver_indices(place: &mut Expr, stmts: &mut Vec<Stmt>) {
    match place {
        Expr::Paren(inner, _) => hoist_receiver_indices(inner, stmts),
        Expr::FieldAccess { object, .. } | Expr::TupleIndex { object, .. } => {
            hoist_receiver_indices(object, stmts);
        }
        Expr::Deref { operand, .. } => hoist_receiver_indices(operand, stmts),
        Expr::Index { object, index, .. } => {
            hoist_receiver_indices(object, stmts);
            if is_inert(index) || matches!(index.as_ref(), Expr::Identifier(_)) {
                return;
            }
            let span = index.span();
            let name = Identifier {
                name: format!("{RECEIVER_PREFIX}{}", stmts.len()),
                span,
            };
            let init = std::mem::replace(index.as_mut(), Expr::Identifier(name.clone()));
            stmts.push(Stmt::VarDecl {
                ty: None,
                init: Some(init),
                mutable: false,
                span,
                name,
            });
        }
        _ => {}
    }
}

/// The name of the temporary holding the argument written at `source`.
fn temp_name(source: usize, span: Span) -> Identifier {
    Identifier {
        name: format!("{TEMP_PREFIX}{source}"),
        span,
    }
}

/// The type annotation a temporary carries, when restating the parameter's own annotation
/// at the call site is meaningful.
///
/// A binding infers its type from its initializer alone, while an argument is checked
/// against its parameter, so an expression that needs the parameter to type it
/// (`Vec::new()`, an integer literal narrower than its default) would lose that. Copying
/// the annotation gives it back. It is copied only when it names types a call site can
/// see: a type parameter of the callee, `Self`, or an `impl Trait` bound means nothing
/// here, and the temporary is left to inference.
///
/// A shared reference parameter is restated only over a shared borrow (`&x`). An
/// argument position reborrows a `&mut T` as the `&T` it expects and a binding does
/// not, so any other argument, `&mut x` or a variable holding one, is left to inference
/// and the call applies the reborrow as it does for the positional form.
fn annotation(sig: &Signature, slot: usize, arg: &Expr) -> Option<Type> {
    let ty = sig.params.get(slot)?.ty.as_ref()?;
    let shared_param = matches!(ty, Type::Reference { mutable: false, .. });
    let shared_borrow = matches!(arg, Expr::Reference { mutable: false, .. });
    (!shared_param || shared_borrow).then(|| ty.clone())
}
