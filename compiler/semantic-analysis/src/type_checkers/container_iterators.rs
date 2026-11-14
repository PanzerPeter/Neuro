//! `.iter()` on an array, a slice or a `Vec`, and `m.keys()` / `m.values()` on a map,
//! written as values.
//!
//! Each is a prelude struct holding a borrow of its container: `Iter<T>` yields `&T`, and
//! `HashKeys`, `HashValues`, `BTreeKeys` and `BTreeValues` yield `&K` or `&V`. The value
//! binds, passes, returns and takes the adapters like any `Iterator`, and the holder rules
//! keep the container frozen while it lives, as they do for `.chars()`.
//!
//! A map view written directly in a `for` head over a map place stays the head form
//! (`iteration.rs`), which walks the slots in place and builds nothing.

use ast_types::{Expr, PRELUDE_MODULE};
use shared_types::Span;

use super::TypeChecker;
use super::iteration::MAP_VIEWS;
use crate::errors::TypeError;
use crate::types::{CollectionKind, Type};

const ITER_METHOD: &str = "iter";
const ITER_STRUCT: &str = "Iter";
/// `string.chars()`, whose `Chars` borrows the text as `Iter` borrows its elements.
const CHARS_METHOD: &str = "chars";

/// The prelude-private map intrinsics a view's `next` steps with.
const SLOT_COUNT_INTRINSIC: &str = "__slot_count";
const SLOT_LIVE_INTRINSIC: &str = "__slot_live";
const KEY_AT_INTRINSIC: &str = "__key_at";
const VALUE_AT_INTRINSIC: &str = "__value_at";

/// The prelude struct a map view of `kind` builds, `keys` choosing the key half.
fn view_struct(kind: CollectionKind, keys: bool) -> Option<&'static str> {
    match (kind, keys) {
        (CollectionKind::HashMap, true) => Some("HashKeys"),
        (CollectionKind::HashMap, false) => Some("HashValues"),
        (CollectionKind::BTreeMap, true) => Some("BTreeKeys"),
        (CollectionKind::BTreeMap, false) => Some("BTreeValues"),
        _ => None,
    }
}

/// Whether `method` on an owned built-in `receiver` answers a value that points into it:
/// `.iter()`, a map view, or `.chars()`. Returning one over a local dangles.
pub(crate) fn views_builtin_receiver(receiver: &Type, method: &str) -> bool {
    match receiver {
        Type::String => method == CHARS_METHOD,
        Type::Array { .. }
        | Type::Collection {
            kind: CollectionKind::Vec,
            ..
        } => method == ITER_METHOD,
        Type::Collection {
            kind: CollectionKind::HashMap | CollectionKind::BTreeMap,
            ..
        } => MAP_VIEWS.contains(&method),
        _ => false,
    }
}

impl TypeChecker {
    /// Type `object.method()` as a borrowing iterator over the container `object`, or
    /// answer `None` when the call is not one, leaving the caller to look further.
    ///
    /// `obj_ty` is the receiver as method dispatch reads it, keeping one `&`. A receiver
    /// that is neither a place nor a borrow is a temporary, which would die under the
    /// iterator, so it is refused. A program that declares its own struct of the
    /// iterator's name withdraws the method, as its own `Rev` withdraws `.rev()`.
    pub(crate) fn check_container_iterator(
        &mut self,
        object: &Expr,
        obj_ty: &Type,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> Option<Type> {
        let (base, type_args) = match (obj_ty.referent(), method) {
            (Type::Array { element, .. } | Type::Slice(element), ITER_METHOD) => {
                (ITER_STRUCT, vec![(**element).clone()])
            }
            (
                Type::Collection {
                    kind: CollectionKind::Vec,
                    args,
                },
                ITER_METHOD,
            ) => (ITER_STRUCT, args.clone()),
            (Type::Collection { kind, args }, _) if MAP_VIEWS.contains(&method) => {
                (view_struct(*kind, method == MAP_VIEWS[0])?, args.clone())
            }
            _ => return None,
        };
        if self.struct_modules.get(base) != Some(&PRELUDE_MODULE) {
            return None;
        }
        self.check_call_args(args, &[], span);
        if !matches!(obj_ty, Type::Reference { .. }) && Self::place_root_name(object).is_none() {
            self.record_error(TypeError::CannotBorrowValue {
                span: object.span(),
            });
            return Some(Type::Unknown);
        }
        self.register_slice_borrow(object, span);
        Some(self.instantiate_generic(base, &type_args, span))
    }

    /// Type a call of one of the prelude's map intrinsics, or answer `None` when `method`
    /// is not one or the call is outside the prelude. `params` are the map's key and
    /// value types.
    pub(crate) fn check_map_intrinsic(
        &mut self,
        kind: CollectionKind,
        params: &[Type],
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> Option<Type> {
        if !self.in_prelude() {
            return None;
        }
        let half = match method {
            SLOT_COUNT_INTRINSIC => {
                self.check_call_args(args, &[], span);
                return Some(Type::U64);
            }
            SLOT_LIVE_INTRINSIC if kind == CollectionKind::HashMap => {
                self.check_call_args(args, &[Type::U64], span);
                return Some(Type::Bool);
            }
            KEY_AT_INTRINSIC => params.first(),
            VALUE_AT_INTRINSIC => params.last(),
            _ => return None,
        };
        self.check_call_args(args, &[Type::U64], span);
        Some(Type::Reference {
            inner: Box::new(half.cloned().unwrap_or(Type::Unknown)),
            mutable: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{view_struct, views_builtin_receiver};
    use crate::errors::TypeError;
    use crate::type_checkers::tests::semantic_errors;
    use crate::types::{ArrayLen, CollectionKind, Type};

    fn collection(kind: CollectionKind) -> Type {
        Type::Collection {
            kind,
            args: vec![Type::I32, Type::I32],
        }
    }

    #[test]
    fn each_map_view_names_its_own_struct() {
        assert_eq!(view_struct(CollectionKind::HashMap, true), Some("HashKeys"));
        assert_eq!(
            view_struct(CollectionKind::HashMap, false),
            Some("HashValues")
        );
        assert_eq!(
            view_struct(CollectionKind::BTreeMap, true),
            Some("BTreeKeys")
        );
        assert_eq!(
            view_struct(CollectionKind::BTreeMap, false),
            Some("BTreeValues")
        );
        assert_eq!(view_struct(CollectionKind::Vec, true), None);
    }

    /// The owned receivers whose iterator dangles once they are gone, and nothing else.
    #[test]
    fn only_a_container_view_borrows_an_owned_builtin_receiver() {
        let array = Type::Array {
            element: Box::new(Type::I32),
            size: ArrayLen::Fixed(2),
        };
        assert!(views_builtin_receiver(&array, "iter"));
        assert!(views_builtin_receiver(
            &collection(CollectionKind::Vec),
            "iter"
        ));
        assert!(views_builtin_receiver(
            &collection(CollectionKind::HashMap),
            "keys"
        ));
        assert!(views_builtin_receiver(
            &collection(CollectionKind::BTreeMap),
            "values"
        ));
        assert!(views_builtin_receiver(&Type::String, "chars"));
        assert!(!views_builtin_receiver(
            &collection(CollectionKind::Vec),
            "len"
        ));
        assert!(!views_builtin_receiver(
            &collection(CollectionKind::HashMap),
            "iter"
        ));
        assert!(!views_builtin_receiver(&Type::String, "iter"));
    }

    /// The iterator types are the standard library's, which these tests do not load, so
    /// `.iter()` is an unknown method rather than a value of a missing type.
    #[test]
    fn without_the_prelude_iter_is_no_method() {
        let errors = semantic_errors(
            "func main() -> i32 {\n    val a = [1, 2]\n    val it = a.iter()\n    0\n}\n",
        );
        assert!(
            matches!(errors.as_slice(), [TypeError::MethodNotFound { method_name, .. }] if method_name == "iter"),
            "got {errors:?}"
        );
    }
}
