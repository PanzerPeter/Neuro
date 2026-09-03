//! Resolving a `for` head against the `IntoIterator` / `Iterator` protocol.
//!
//! The built-in sequence heads — a range, an array, a `Vec`, a borrowed slice — are
//! answered before this module is consulted and keep their counted-loop lowering. What
//! reaches here is a head whose type is a user nominal type, which is iterable exactly
//! when the protocol says so.

use ast_types::Expr;
use shared_types::Span;

use crate::errors::TypeError;
use crate::types::Type;

use super::TypeChecker;

/// The prelude trait a container implements to produce an iterator.
const INTO_ITERATOR_TRAIT: &str = "IntoIterator";
/// The prelude trait an iterator itself implements.
const ITERATOR_TRAIT: &str = "Iterator";
/// The associated type naming what a step yields.
const ITEM_ASSOC: &str = "Item";
/// `IntoIterator`'s producing method.
const INTO_ITER_METHOD: &str = "into_iter";
/// The `for`-head form that drives a `Chars` iterator by byte offset.
const CHAR_INDICES_METHOD: &str = "char_indices";

impl TypeChecker {
    /// Type-check a `for` head of the form `text.char_indices()`, yielding the `Chars`
    /// iterator it drives.
    ///
    /// The call is not a method anywhere else in the language: it is a head form, like
    /// `.enumerate()`, and the position it binds is a byte offset read off the iterator
    /// rather than a payload it can yield — `Iterator::next` answers `Option<Self::Item>`,
    /// and an `Option` payload may only be a scalar, so a pair cannot travel through it.
    pub(crate) fn check_char_indices_head(&mut self, receiver: &Expr, span: Span) -> Type {
        let receiver_ty = self.check_expr(receiver, None).unwrap_or(Type::Unknown);
        if !matches!(receiver_ty.referent(), Type::String) {
            if !matches!(receiver_ty, Type::Unknown) {
                self.record_error(TypeError::MethodNotFound {
                    struct_name: receiver_ty.to_string(),
                    method_name: CHAR_INDICES_METHOD.to_string(),
                    span,
                });
            }
            return Type::Unknown;
        }
        self.chars_iterator(receiver, span)
    }

    /// What one step of iterating over `head` binds, or `None` when `head` does not
    /// implement the protocol.
    ///
    /// A type implementing `Iterator` is its own iterator — the blanket
    /// `impl<I: Iterator> IntoIterator for I` stated as a rule, since a blanket impl has
    /// no syntax yet. `IntoIterator` is consulted first, so a container that implements
    /// both still hands out its dedicated iterator.
    pub(crate) fn iteration_item(&self, head: &Type) -> Option<Type> {
        let name = nominal_name(head)?;

        if self
            .trait_impls
            .contains(&(INTO_ITERATOR_TRAIT.to_string(), name.clone()))
        {
            let iterator = self.method_return_type(&name, INTO_ITER_METHOD)?;
            return self.iterator_item(&iterator);
        }

        self.iterator_item(head)
    }

    /// What `ty`'s own `impl Iterator` yields, or `None` when it has no such impl.
    ///
    /// This is also what enforces `IntoIterator::Iter: Iterator`: the bound has no
    /// syntax on an associated-type declaration, so the requirement is checked here, on
    /// the type `into_iter` actually returned.
    fn iterator_item(&self, ty: &Type) -> Option<Type> {
        let name = nominal_name(ty)?;
        self.impl_assoc
            .get(&(ITERATOR_TRAIT.to_string(), name))
            .and_then(|bindings| bindings.get(ITEM_ASSOC))
            .cloned()
    }

    /// The declared return type of `type_name`'s `method_name`, resolved through the
    /// mangled key the impl registered.
    fn method_return_type(&self, type_name: &str, method_name: &str) -> Option<Type> {
        let mangled = self
            .impl_methods
            .get(type_name)
            .and_then(|methods| methods.get(method_name))?;
        match self.functions.get(mangled) {
            Some(Type::Function { ret, .. }) => Some((**ret).clone()),
            _ => None,
        }
    }
}

/// The receiver of a `text.char_indices()` `for` head, or `None` for any other iterable.
///
/// The parser has already rejected a decorated or single-bound one, so a match here is a
/// head the loop lowering will drive by byte offset.
pub(crate) fn char_indices_receiver(iterable: &Expr) -> Option<&Expr> {
    let Expr::Call { func, args, .. } = iterable else {
        return None;
    };
    let Expr::FieldAccess { object, field, .. } = func.as_ref() else {
        return None;
    };
    if field.name != CHAR_INDICES_METHOD || !args.is_empty() {
        return None;
    }
    Some(object.as_ref())
}

/// The declaration name behind a nominal type, which is the key both trait tables use.
fn nominal_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Struct(name) | Type::Enum(name) | Type::Newtype(name) => Some(name.clone()),
        _ => None,
    }
}
