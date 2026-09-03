//! Resolving a `for` head against the `IntoIterator` / `Iterator` protocol.
//!
//! The built-in sequence heads — a range, an array, a `Vec`, a borrowed slice — are
//! answered before this module is consulted and keep their counted-loop lowering. What
//! reaches here is a head whose type is a user nominal type, which is iterable exactly
//! when the protocol says so.

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

impl TypeChecker {
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

/// The declaration name behind a nominal type, which is the key both trait tables use.
fn nominal_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Struct(name) | Type::Enum(name) | Type::Newtype(name) => Some(name.clone()),
        _ => None,
    }
}
